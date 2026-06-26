//! ThumbnailSpriteHandler -- assembles keyframe images into sprite sheets
//! and generates a WebVTT map with `#xywh=` coordinates for scrub bar
//! thumbnail previews.
//!
//! Queued as a downstream job by ExtractionHandler after video extraction
//! produces keyframe derived attachments (#525).

use async_trait::async_trait;
use image::imageops::FilterType;
use serde_json::json;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::JobType;
use matric_db::{Database, SchemaContext};

use crate::handler::{JobContext, JobHandler, JobResult};

/// Sprite sheet grid dimensions.
const SPRITE_COLS: u32 = 5;
const SPRITE_ROWS: u32 = 5;
const FRAMES_PER_SHEET: u32 = SPRITE_COLS * SPRITE_ROWS;

/// Individual thumbnail dimensions (16:9 at 1/12 scale of 1920x1080).
const THUMB_WIDTH: u32 = 160;
const THUMB_HEIGHT: u32 = 90;

/// JPEG quality for sprite sheets (0-100).
const JPEG_QUALITY: u8 = 75;

/// Extract the target schema from a job's payload.
fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema)
        .map_err(|_| JobResult::Failed("Invalid schema".into()))
}

fn sprite_text_len(text: &str) -> usize {
    text.chars().count()
}

fn sprite_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("not found") || text.contains("no such") || text.contains("missing") {
        "not_found"
    } else if text.contains("database") || text.contains("sql") || text.contains("postgres") {
        "database_error"
    } else if text.contains("image") || text.contains("decode") || text.contains("jpeg") {
        "invalid_image"
    } else {
        "operation_failed"
    }
}

/// Format seconds as a WebVTT timestamp (HH:MM:SS.mmm).
fn format_vtt_ts(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1_000;
    let ms = total_ms % 1_000;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// A keyframe ready for sprite assembly.
struct KeyframeEntry {
    attachment_id: Uuid,
    frame_index: u32,
    timestamp_secs: f64,
}

pub struct ThumbnailSpriteHandler {
    db: Database,
}

impl ThumbnailSpriteHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Load all keyframe derived attachments for a parent, sorted by frame index.
    async fn load_keyframes(
        &self,
        schema_ctx: &SchemaContext,
        parent_id: Uuid,
    ) -> Result<Vec<KeyframeEntry>, String> {
        let file_storage = self
            .db
            .file_storage
            .as_ref()
            .ok_or_else(|| "File storage not configured".to_string())?;

        let mut tx = schema_ctx
            .begin_tx()
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        let keyframes = file_storage
            .list_derived_by_type_tx(&mut tx, parent_id, "keyframe")
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        tx.commit()
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        let mut entries: Vec<KeyframeEntry> = keyframes
            .into_iter()
            .filter_map(|att| {
                let meta = att.extracted_metadata.as_ref()?;
                let frame_index = meta.get("frame_index")?.as_u64()? as u32;
                let timestamp_secs = meta.get("timestamp_secs")?.as_f64()?;
                Some(KeyframeEntry {
                    attachment_id: att.id,
                    frame_index,
                    timestamp_secs,
                })
            })
            .collect();

        entries.sort_by_key(|e| e.frame_index);
        Ok(entries)
    }

    /// Download a keyframe image and resize it to thumbnail dimensions.
    async fn load_thumbnail(
        &self,
        schema_ctx: &SchemaContext,
        attachment_id: Uuid,
    ) -> Result<image::RgbImage, String> {
        let file_storage = self
            .db
            .file_storage
            .as_ref()
            .ok_or_else(|| "File storage not configured".to_string())?;

        let mut tx = schema_ctx
            .begin_tx()
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        let (data, _ct, _filename) = file_storage
            .download_file_tx(&mut tx, attachment_id)
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        tx.commit()
            .await
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        let img = image::load_from_memory(&data)
            .map_err(|e| sprite_error_reason_code(&e.to_string()).to_string())?;

        // Resize to thumbnail dimensions, maintaining aspect ratio via exact fit
        let thumb = img.resize_exact(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Lanczos3);
        Ok(thumb.to_rgb8())
    }

    /// Assemble keyframe thumbnails into sprite sheets (5x5 grids).
    /// Returns (sprite_sheet_jpeg_bytes, sheet_index) pairs.
    async fn build_sprite_sheets(
        &self,
        schema_ctx: &SchemaContext,
        keyframes: &[KeyframeEntry],
        ctx: &JobContext,
    ) -> Vec<Vec<u8>> {
        let total_sheets = (keyframes.len() as u32).div_ceil(FRAMES_PER_SHEET);
        let mut sheets = Vec::new();

        for sheet_idx in 0..total_sheets {
            let start = (sheet_idx * FRAMES_PER_SHEET) as usize;
            let end = std::cmp::min(start + FRAMES_PER_SHEET as usize, keyframes.len());
            let chunk = &keyframes[start..end];

            let sheet_w = SPRITE_COLS * THUMB_WIDTH;
            let sheet_h = SPRITE_ROWS * THUMB_HEIGHT;
            let mut canvas = image::RgbImage::new(sheet_w, sheet_h);

            // Fill with black background
            for pixel in canvas.pixels_mut() {
                *pixel = image::Rgb([0, 0, 0]);
            }

            for (i, entry) in chunk.iter().enumerate() {
                let col = (i as u32) % SPRITE_COLS;
                let row = (i as u32) / SPRITE_COLS;
                let x = col * THUMB_WIDTH;
                let y = row * THUMB_HEIGHT;

                match self.load_thumbnail(schema_ctx, entry.attachment_id).await {
                    Ok(thumb) => {
                        image::imageops::overlay(&mut canvas, &thumb, x as i64, y as i64);
                    }
                    Err(e) => {
                        warn!(
                            attachment_id_present = true,
                            frame_index = entry.frame_index,
                            error_len = sprite_text_len(&e),
                            error_reason = sprite_error_reason_code(&e),
                            "Failed to load keyframe thumbnail"
                        );
                        // Leave black square for missing frames
                    }
                }
            }

            // Encode as JPEG
            let mut jpeg_buf = std::io::Cursor::new(Vec::new());
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, JPEG_QUALITY);
            if let Err(e) = canvas.write_with_encoder(encoder) {
                let error_text = e.to_string();
                error!(
                    sheet_idx,
                    error_len = sprite_text_len(&error_text),
                    error_reason = sprite_error_reason_code(&error_text),
                    "Failed to encode sprite sheet"
                );
                continue;
            }

            let progress = 30 + (50 * (sheet_idx + 1) / total_sheets) as i32;
            ctx.report_progress(
                progress,
                Some(&format!(
                    "Sprite sheet {}/{} assembled",
                    sheet_idx + 1,
                    total_sheets
                )),
            );

            sheets.push(jpeg_buf.into_inner());
        }

        sheets
    }

    /// Generate a WebVTT file mapping time ranges to sprite sheet coordinates.
    fn build_sprite_vtt(&self, keyframes: &[KeyframeEntry], attachment_id: Uuid) -> String {
        if keyframes.is_empty() {
            return String::new();
        }

        // Estimate interval from first two frames (fallback 10s)
        let interval = if keyframes.len() >= 2 {
            let diff = keyframes[1].timestamp_secs - keyframes[0].timestamp_secs;
            if diff > 0.0 {
                diff
            } else {
                10.0
            }
        } else {
            10.0
        };

        let mut vtt = String::from("WEBVTT\n\n");

        for (i, entry) in keyframes.iter().enumerate() {
            let start = entry.timestamp_secs;
            let end = keyframes
                .get(i + 1)
                .map(|next| next.timestamp_secs)
                .unwrap_or(start + interval);

            let sheet_idx = i as u32 / FRAMES_PER_SHEET;
            let local_idx = i as u32 % FRAMES_PER_SHEET;
            let col = local_idx % SPRITE_COLS;
            let row = local_idx / SPRITE_COLS;
            let x = col * THUMB_WIDTH;
            let y = row * THUMB_HEIGHT;

            // Sprite sheets are 1-indexed in the URL path
            vtt.push_str(&format!(
                "{} --> {}\n/api/v1/attachments/{}/sprites/{}.jpg#xywh={},{},{},{}\n\n",
                format_vtt_ts(start),
                format_vtt_ts(end),
                attachment_id,
                sheet_idx + 1,
                x,
                y,
                THUMB_WIDTH,
                THUMB_HEIGHT,
            ));
        }

        vtt
    }
}

#[async_trait]
impl JobHandler for ThumbnailSpriteHandler {
    fn job_type(&self) -> JobType {
        JobType::ThumbnailSprite
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing thumbnail_sprite job payload".into()),
        };

        let attachment_id: Uuid = match payload
            .get("attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid attachment_id".into()),
        };

        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("Missing note_id for thumbnail sprite".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Loading keyframe attachments"));

        // Load all keyframe derived attachments for this video
        let keyframes = match self.load_keyframes(&schema_ctx, attachment_id).await {
            Ok(kfs) => kfs,
            Err(reason) => {
                return JobResult::Failed(format!("Failed to load keyframes ({reason})"));
            }
        };

        if keyframes.is_empty() {
            info!(
                attachment_id_present = true,
                "No keyframes found, skipping sprite generation"
            );
            return JobResult::Success(Some(json!({
                "skipped": true,
                "reason": "No keyframe derived attachments found",
            })));
        }

        // Skip very short videos (<5s or single frame)
        if keyframes.len() < 2 {
            info!(
                attachment_id_present = true,
                frames = keyframes.len(),
                "Too few keyframes for sprite sheet"
            );
            return JobResult::Success(Some(json!({
                "skipped": true,
                "reason": "Too few keyframes for meaningful sprite sheet",
            })));
        }

        info!(
            attachment_id_present = true,
            frame_count = keyframes.len(),
            "Building sprite sheets from keyframes"
        );

        ctx.report_progress(10, Some("Assembling sprite sheets"));

        // Build sprite sheets from keyframe images
        let sheets = self
            .build_sprite_sheets(&schema_ctx, &keyframes, &ctx)
            .await;

        if sheets.is_empty() {
            return JobResult::Failed("Failed to generate any sprite sheets".into());
        }

        ctx.report_progress(80, Some("Generating VTT map"));

        // Generate sprite WebVTT
        let sprite_vtt = self.build_sprite_vtt(&keyframes, attachment_id);

        ctx.report_progress(85, Some("Storing sprite sheets"));

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Store each sprite sheet as a derived attachment
        let mut stored_count = 0;
        for (i, sheet_data) in sheets.iter().enumerate() {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(tx) => tx,
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        sheet_index = i + 1,
                        error_len = sprite_text_len(&error_text),
                        error_reason = sprite_error_reason_code(&error_text),
                        "Failed to begin transaction for sprite sheet"
                    );
                    continue;
                }
            };

            match file_storage
                .store_derived_attachment_tx(
                    &mut tx,
                    note_id,
                    attachment_id,
                    &format!("sprite_{}.jpg", i + 1),
                    "image/jpeg",
                    sheet_data,
                    "thumbnail_sprite",
                )
                .await
            {
                Ok(child) => {
                    // Merge sprite sheet index metadata
                    if let Err(e) = file_storage
                        .merge_extracted_metadata_tx(
                            &mut tx,
                            child.id,
                            &json!({
                                "sprite_index": i + 1,
                                "grid": format!("{}x{}", SPRITE_COLS, SPRITE_ROWS),
                                "thumb_width": THUMB_WIDTH,
                                "thumb_height": THUMB_HEIGHT,
                            }),
                        )
                        .await
                    {
                        let error_text = e.to_string();
                        warn!(
                            sheet_index = i + 1,
                            error_len = sprite_text_len(&error_text),
                            error_reason = sprite_error_reason_code(&error_text),
                            "Failed to merge sprite metadata"
                        );
                    }
                    if let Err(e) = tx.commit().await {
                        let error_text = e.to_string();
                        error!(
                            sheet_index = i + 1,
                            error_len = sprite_text_len(&error_text),
                            error_reason = sprite_error_reason_code(&error_text),
                            "Failed to commit sprite sheet"
                        );
                        continue;
                    }
                    stored_count += 1;
                    debug!(
                        sheet = i + 1,
                        size = sheet_data.len(),
                        "Sprite sheet stored"
                    );
                }
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        sheet_index = i + 1,
                        error_len = sprite_text_len(&error_text),
                        error_reason = sprite_error_reason_code(&error_text),
                        "Failed to store sprite sheet"
                    );
                    let _ = tx.rollback().await;
                }
            }
        }

        // Store the sprite VTT
        if !sprite_vtt.is_empty() {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(tx) => tx,
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        error_len = sprite_text_len(&error_text),
                        error_reason = sprite_error_reason_code(&error_text),
                        "Failed to begin transaction for sprite VTT"
                    );
                    return JobResult::Failed("Failed to store sprite VTT".into());
                }
            };

            match file_storage
                .store_derived_attachment_tx(
                    &mut tx,
                    note_id,
                    attachment_id,
                    "thumbnails.vtt",
                    "text/vtt",
                    sprite_vtt.as_bytes(),
                    "thumbnail_vtt",
                )
                .await
            {
                Ok(_) => {
                    if let Err(e) = tx.commit().await {
                        let error_text = e.to_string();
                        error!(
                            error_len = sprite_text_len(&error_text),
                            error_reason = sprite_error_reason_code(&error_text),
                            "Failed to commit sprite VTT"
                        );
                    }
                }
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        error_len = sprite_text_len(&error_text),
                        error_reason = sprite_error_reason_code(&error_text),
                        "Failed to store sprite VTT"
                    );
                    let _ = tx.rollback().await;
                }
            }
        }

        // Update parent attachment metadata to indicate sprites are available
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(tx) => tx,
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        error_len = sprite_text_len(&error_text),
                        error_reason = sprite_error_reason_code(&error_text),
                        "Failed to begin tx for parent metadata update"
                    );
                    // Non-fatal: sprites are stored, metadata is optional
                    return JobResult::Success(Some(json!({
                        "sprite_sheets": stored_count,
                        "keyframes": keyframes.len(),
                        "vtt_generated": !sprite_vtt.is_empty(),
                    })));
                }
            };

            if let Err(e) = file_storage
                .merge_extracted_metadata_tx(
                    &mut tx,
                    attachment_id,
                    &json!({
                        "has_thumbnail_sprites": true,
                        "thumbnail_sprite_count": stored_count,
                        "thumbnail_grid": format!("{}x{}", SPRITE_COLS, SPRITE_ROWS),
                        "thumbnail_dimensions": [THUMB_WIDTH, THUMB_HEIGHT],
                    }),
                )
                .await
            {
                let error_text = e.to_string();
                warn!(
                    error_len = sprite_text_len(&error_text),
                    error_reason = sprite_error_reason_code(&error_text),
                    "Failed to update parent metadata with sprite info"
                );
            }

            let _ = tx.commit().await;
        }

        ctx.report_progress(100, Some("Sprite sheets complete"));

        info!(
            attachment_id_present = true,
            sprite_sheets = stored_count,
            keyframes = keyframes.len(),
            "Thumbnail sprite generation complete"
        );

        JobResult::Success(Some(json!({
            "sprite_sheets": stored_count,
            "keyframes": keyframes.len(),
            "vtt_generated": !sprite_vtt.is_empty(),
            "grid": format!("{}x{}", SPRITE_COLS, SPRITE_ROWS),
            "thumb_dimensions": [THUMB_WIDTH, THUMB_HEIGHT],
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_runtime_telemetry_helpers_redact_private_values() {
        let raw_error =
            "postgres://user:pass@db.internal failed for /srv/private/mm_key_sprite.jpg";
        let rendered = format!(
            "attachment_id_present=true; error_len={}; error_reason={}",
            sprite_text_len(raw_error),
            sprite_error_reason_code(raw_error)
        );

        assert!(rendered.contains("attachment_id_present=true"));
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason=database_error"));
        assert!(!rendered.contains("postgres://user:pass"));
        assert!(!rendered.contains("db.internal"));
        assert!(!rendered.contains("/srv/private"));
        assert!(!rendered.contains("mm_key_sprite"));
    }
}

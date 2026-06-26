//! KeyframeCharacterVisionHandler — identifies characters/people in a video keyframe.
//!
//! Specialized vision pass that identifies and describes people visible in each
//! keyframe. Stores results in extracted_metadata.character_analysis rather than
//! ai_description (which is used by the general KeyframeVision scene pass).
//!
//! Fan-in: when expected_vision_passes >= 3, uses count_fully_analyzed_keyframes_tx
//! to check if all 3 passes (scene + character + setting) are complete before
//! queuing KeyframeAssembly.
//!
//! Issue #550

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType};
use matric_db::{Database, SchemaContext};
use matric_inference::VisionBackend;

use crate::handler::{JobContext, JobHandler, JobResult};

fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema)
        .map_err(|e| JobResult::Failed(format!("Invalid schema '{}': {}", schema, e)))
}

fn keyframe_character_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("not found")
        || text.contains("no such")
        || text.contains("missing")
        || text.contains("unknown")
    {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else if text.contains("connection refused")
        || text.contains("cannot connect")
        || text.contains("connection")
    {
        "connection_failed"
    } else if text.contains("database") || text.contains("sql") || text.contains("postgres") {
        "database_error"
    } else if text.contains("model") || text.contains("vision") || text.contains("inference") {
        "model_backend_error"
    } else {
        "operation_failed"
    }
}

pub struct KeyframeCharacterVisionHandler {
    db: Database,
    vision: Option<Arc<dyn VisionBackend>>,
}

impl KeyframeCharacterVisionHandler {
    pub fn new(db: Database, vision: Option<Arc<dyn VisionBackend>>) -> Self {
        Self { db, vision }
    }
}

#[async_trait]
impl JobHandler for KeyframeCharacterVisionHandler {
    fn job_type(&self) -> JobType {
        JobType::KeyframeCharacterVision
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => {
                return JobResult::Failed("Missing keyframe character vision job payload".into())
            }
        };

        let parent_attachment_id: Uuid = match payload
            .get("parent_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid parent_attachment_id".into()),
        };

        let keyframe_attachment_id: Uuid = match payload
            .get("keyframe_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid keyframe_attachment_id".into()),
        };

        let frame_index = payload
            .get("frame_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let timestamp_secs = payload
            .get("timestamp_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let total_frames = payload
            .get("total_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as i64;

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let vision = match self.vision.as_ref() {
            Some(v) => v,
            None => {
                warn!(
                    frame_index,
                    keyframe = %keyframe_attachment_id,
                    "KeyframeCharacterVision job deferred — vision backend unavailable"
                );
                return JobResult::Retry(
                    "Vision backend unavailable — job will retry when configured".into(),
                );
            }
        };

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Step 1: Download keyframe JPEG
        ctx.report_progress(10, Some("Downloading keyframe image"));
        let image_data = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let result = file_storage
                .download_file_tx(&mut tx, keyframe_attachment_id)
                .await;
            let _ = tx.commit().await;
            match result {
                Ok((data, _content_type, _filename)) => data,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to download keyframe {}: {}",
                        keyframe_attachment_id, e
                    ))
                }
            }
        };

        if image_data.is_empty() {
            return JobResult::Failed(format!(
                "Empty image data for keyframe {}",
                keyframe_attachment_id
            ));
        }

        // Step 2: Call vision LLM with character-specific prompt
        ctx.report_progress(
            30,
            Some(&format!(
                "Analyzing characters in frame {}/{} ({:.0}s)",
                frame_index + 1,
                total_frames,
                timestamp_secs
            )),
        );

        let prompt = "Identify and describe the people or characters visible in this video frame. \
            For each person: appearance (age, gender, build, hair), clothing and \
            distinctive features, apparent role (speaker, presenter, background). \
            If no people are visible, state 'No characters visible.'";

        let analysis = match vision
            .describe_image(&image_data, "image/jpeg", Some(prompt))
            .await
        {
            Ok(desc) => desc,
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    frame_index,
                    parent = %parent_attachment_id,
                    keyframe = %keyframe_attachment_id,
                    model = vision.model_name(),
                    error_len = error_text.len(),
                    error_reason = keyframe_character_error_reason_code(&error_text),
                    "Vision LLM failed for character analysis — will retry"
                );
                return JobResult::Retry(format!(
                    "Vision LLM failed for character analysis frame {}: {}",
                    frame_index, e
                ));
            }
        };

        // Step 3: Store character analysis in extracted_metadata
        ctx.report_progress(80, Some("Storing character analysis"));
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            if let Err(e) = file_storage
                .merge_extracted_metadata_tx(
                    &mut tx,
                    keyframe_attachment_id,
                    &json!({
                        "character_analysis": analysis,
                        "character_analysis_complete": true,
                    }),
                )
                .await
            {
                return JobResult::Failed(format!("Failed to store character analysis: {}", e));
            }
            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
            }
        }

        info!(
            frame_index,
            parent = %parent_attachment_id,
            keyframe = %keyframe_attachment_id,
            "Keyframe {} character analysis complete ({} chars)",
            frame_index,
            analysis.len()
        );

        // Step 4: Fan-in check
        ctx.report_progress(90, Some("Checking fan-in"));
        if total_frames > 0 {
            self.check_fan_in(
                &ctx,
                &schema_ctx,
                file_storage,
                parent_attachment_id,
                total_frames,
                schema,
            )
            .await;
        }

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(json!({
            "frame_index": frame_index,
            "analysis_length": analysis.len(),
        })))
    }
}

impl KeyframeCharacterVisionHandler {
    async fn check_fan_in(
        &self,
        ctx: &JobContext,
        schema_ctx: &SchemaContext,
        file_storage: &matric_db::PgFileStorageRepository,
        parent_attachment_id: Uuid,
        total_frames: i64,
        schema: &str,
    ) {
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    error_len = error_text.len(),
                    error_reason = keyframe_character_error_reason_code(&error_text),
                    "Fan-in count failed, assembly may be delayed"
                );
                return;
            }
        };

        // Read expected_vision_passes and transcript_complete from parent metadata
        let row: Option<(Option<serde_json::Value>,)> =
            sqlx::query_as("SELECT extracted_metadata FROM attachment WHERE id = $1")
                .bind(parent_attachment_id)
                .fetch_optional(&mut *tx)
                .await
                .ok()
                .flatten();
        let parent_meta = row.and_then(|(em,)| em).unwrap_or_else(|| json!({}));

        let expected_passes = parent_meta
            .get("expected_vision_passes")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as i64;

        let transcript_complete = parent_meta
            .get("transcript_complete")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let vision_ready = if expected_passes >= 3 {
            let fully_analyzed = file_storage
                .count_fully_analyzed_keyframes_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            debug!(
                fully_analyzed,
                total = total_frames,
                transcript_complete,
                "Character fan-in: {}/{} fully analyzed + transcript={}",
                fully_analyzed,
                total_frames,
                transcript_complete
            );
            fully_analyzed >= total_frames
        } else {
            // Standard mode — this handler shouldn't run, but be safe
            let described = file_storage
                .count_described_keyframes_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            described >= total_frames
        };

        let _ = tx.commit().await;

        if vision_ready && transcript_complete {
            let mut assembly_payload = serde_json::Map::new();
            assembly_payload.insert(
                "attachment_id".into(),
                json!(parent_attachment_id.to_string()),
            );
            if schema != "public" {
                assembly_payload.insert("schema".into(), json!(schema));
            }

            match self
                .db
                .jobs
                .queue_deduplicated(
                    ctx.note_id(),
                    JobType::KeyframeAssembly,
                    JobType::KeyframeAssembly.default_priority(),
                    Some(serde_json::Value::Object(assembly_payload)),
                    JobType::KeyframeAssembly.default_cost_tier(),
                )
                .await
            {
                Ok(Some(job_id)) => {
                    ctx.emit_job_queued(job_id, JobType::KeyframeAssembly, ctx.note_id());
                    info!(
                        "All {} keyframes fully analyzed + transcript complete, KeyframeAssembly queued (job {})",
                        total_frames, job_id
                    );
                }
                Ok(None) => {
                    debug!("KeyframeAssembly already queued (deduplicated)");
                }
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        error_len = error_text.len(),
                        error_reason = keyframe_character_error_reason_code(&error_text),
                        "Failed to queue KeyframeAssembly"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_character_error_reason_code_uses_stable_classes() {
        assert_eq!(
            keyframe_character_error_reason_code(
                "vision model failed for /home/operator/mm_key_secret"
            ),
            "model_backend_error"
        );
        assert_eq!(
            keyframe_character_error_reason_code("postgres://user:secret@db/app sql failed"),
            "database_error"
        );
        assert_eq!(
            keyframe_character_error_reason_code("Cannot connect to inference backend"),
            "connection_failed"
        );
        assert_eq!(
            keyframe_character_error_reason_code("opaque backend text with token mm_key_secret"),
            "operation_failed"
        );
    }
}

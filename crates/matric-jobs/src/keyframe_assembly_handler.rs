//! KeyframeAssemblyHandler — fan-in aggregation of keyframe descriptions.
//!
//! Reads all described keyframes for a parent video attachment, rebuilds
//! the full video markdown with visual content, regenerates the keyframe
//! manifest and VTT, and updates the parent attachment's extracted_text.
//!
//! Triggered by the last KeyframeVision job to complete (fan-in via
//! queue_deduplicated). Also propagates content to the note and queues
//! downstream NLP jobs (Embedding, Linking, ConceptTagging, TitleGeneration).
//!
//! Issue #526

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tracing::{error, info, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType};
use matric_db::{Database, SchemaContext};

use crate::adapters::video_multimodal::{build_keyframe_vtt, format_video_markdown};
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

fn keyframe_assembly_error_reason_code(error: &str) -> &'static str {
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
    } else if text.contains("storage") || text.contains("file") {
        "storage_error"
    } else {
        "operation_failed"
    }
}

pub struct KeyframeAssemblyHandler {
    db: Database,
}

impl KeyframeAssemblyHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for KeyframeAssemblyHandler {
    fn job_type(&self) -> JobType {
        JobType::KeyframeAssembly
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing keyframe assembly job payload".into()),
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
            None => return JobResult::Failed("Missing note_id for assembly".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Step 1: Load parent attachment metadata (duration, transcript)
        ctx.report_progress(10, Some("Loading parent metadata"));
        let (duration_secs, transcript_segments_json, transcript_text, existing_metadata) = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let row: Option<(Option<String>, Option<JsonValue>)> = sqlx::query_as(
                "SELECT extracted_text, extracted_metadata FROM attachment WHERE id = $1",
            )
            .bind(attachment_id)
            .fetch_optional(&mut *tx)
            .await
            .ok()
            .flatten();
            let _ = tx.commit().await;

            match row {
                Some((text, meta)) => {
                    let meta = meta.unwrap_or(json!({}));
                    let duration = meta.get("duration_secs").and_then(|v| v.as_f64());
                    let segments = meta.get("transcript_segments").cloned();
                    // Reconstruct transcript text from segments if not in extracted_text
                    let transcript = text.clone().or_else(|| {
                        segments.as_ref().and_then(|s| {
                            let segs: Vec<JsonValue> = serde_json::from_value(s.clone()).ok()?;
                            let parts: Vec<String> = segs
                                .iter()
                                .filter_map(|seg| {
                                    seg.get("text").and_then(|t| t.as_str()).map(String::from)
                                })
                                .collect();
                            if parts.is_empty() {
                                None
                            } else {
                                Some(parts.join(" "))
                            }
                        })
                    });
                    (duration, segments, transcript, meta)
                }
                None => {
                    return JobResult::Failed(format!(
                        "Parent attachment {} not found",
                        attachment_id
                    ))
                }
            }
        };

        // Step 2: Load all keyframe derived attachments sorted by frame_index
        ctx.report_progress(20, Some("Loading keyframe descriptions"));
        let keyframe_descriptions: Vec<JsonValue> = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let keyframes = file_storage
                .list_derived_by_type_tx(&mut tx, attachment_id, "keyframe")
                .await
                .unwrap_or_default();
            let _ = tx.commit().await;

            let mut descriptions: Vec<(u64, JsonValue)> = keyframes
                .iter()
                .filter_map(|att| {
                    let meta = att.extracted_metadata.as_ref()?;
                    let frame_index = meta.get("frame_index")?.as_u64()?;
                    let timestamp_secs = meta.get("timestamp_secs")?.as_f64()?;
                    let description = att.ai_description.as_deref().unwrap_or("");
                    // Read character/setting analysis from extracted_metadata (#550)
                    let character_analysis = meta
                        .get("character_analysis")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let setting_analysis = meta
                        .get("setting_analysis")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    Some((
                        frame_index,
                        json!({
                            "frame_index": frame_index,
                            "timestamp_secs": timestamp_secs,
                            "description": description,
                            "character_analysis": character_analysis,
                            "setting_analysis": setting_analysis,
                        }),
                    ))
                })
                .collect();
            descriptions.sort_by_key(|(idx, _)| *idx);
            descriptions.into_iter().map(|(_, v)| v).collect()
        };

        if keyframe_descriptions.is_empty() {
            return JobResult::Failed("No keyframe descriptions found for assembly".into());
        }

        info!(
            attachment = %attachment_id,
            frames = keyframe_descriptions.len(),
            "Assembling {} keyframe descriptions into video markdown",
            keyframe_descriptions.len()
        );

        // Step 3: Rebuild full video markdown
        ctx.report_progress(40, Some("Generating video markdown"));
        let transcript_language = existing_metadata
            .get("transcript_language")
            .and_then(|v| v.as_str());
        // Parse transcript segments from parent metadata for interleaved output
        let transcript_segments: Option<Vec<JsonValue>> =
            transcript_segments_json.and_then(|v| serde_json::from_value(v).ok());
        let full_text = format_video_markdown(
            transcript_text.as_deref(),
            &keyframe_descriptions,
            duration_secs,
            transcript_language,
            transcript_segments.as_deref(),
        );

        // Step 4: Update parent attachment's extracted_text and metadata
        ctx.report_progress(60, Some("Updating attachment"));
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };

            // Merge frame_count into existing metadata
            let mut updated_meta = existing_metadata.clone();
            if let Some(obj) = updated_meta.as_object_mut() {
                obj.insert(
                    "frame_count".to_string(),
                    json!(keyframe_descriptions.len()),
                );
                obj.insert("keyframe_assembly_complete".to_string(), json!(true));
            }

            if let Err(e) = file_storage
                .update_extracted_content_tx(
                    &mut tx,
                    attachment_id,
                    full_text.as_deref(),
                    Some(updated_meta),
                )
                .await
            {
                return JobResult::Failed(format!("Failed to update extracted content: {}", e));
            }

            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
            }
        }

        // Step 5: Store keyframe manifest as derived attachment
        ctx.report_progress(70, Some("Storing keyframe manifest"));
        {
            let manifest_data =
                serde_json::to_vec_pretty(&keyframe_descriptions).unwrap_or_default();
            let base_name = payload
                .get("base_name")
                .and_then(|v| v.as_str())
                .unwrap_or("video");

            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        error_len = error_text.len(),
                        error_reason = keyframe_assembly_error_reason_code(&error_text),
                        "Failed to store manifest, continuing"
                    );
                    // Non-fatal — the markdown is already saved
                    return finish_propagation(
                        &self.db,
                        &schema_ctx,
                        schema,
                        &ctx,
                        note_id,
                        full_text.as_deref(),
                        keyframe_descriptions.len(),
                    )
                    .await;
                }
            };

            // Store manifest
            let _ = file_storage
                .store_derived_attachment_tx(
                    &mut tx,
                    note_id,
                    attachment_id,
                    &format!("{}_keyframes.json", base_name),
                    "application/json",
                    &manifest_data,
                    "keyframe_manifest",
                )
                .await;

            // Store updated VTT
            let vtt = build_keyframe_vtt(&keyframe_descriptions);
            if !vtt.is_empty() {
                let _ = file_storage
                    .store_derived_attachment_tx(
                        &mut tx,
                        note_id,
                        attachment_id,
                        &format!("{}_keyframes.vtt", base_name),
                        "text/vtt",
                        vtt.as_bytes(),
                        "keyframe_vtt",
                    )
                    .await;
            }

            let _ = tx.commit().await;
        }

        // Step 6: Propagate to note + queue downstream NLP
        ctx.report_progress(80, Some("Propagating content"));
        finish_propagation(
            &self.db,
            &schema_ctx,
            schema,
            &ctx,
            note_id,
            full_text.as_deref(),
            keyframe_descriptions.len(),
        )
        .await
    }
}

/// Propagate assembled content to the note and queue downstream NLP jobs.
async fn finish_propagation(
    db: &Database,
    schema_ctx: &SchemaContext,
    schema: &str,
    ctx: &JobContext,
    note_id: Uuid,
    full_text: Option<&str>,
    frame_count: usize,
) -> JobResult {
    if let Some(content) = full_text {
        // Always propagate assembled video content to the note — the keyframe
        // assembly produces the authoritative representation of the video
        // (interleaved scenes + dialog) which is far richer than whatever stub
        // or speaker-config content may already be there.
        let mut content_updated = false;
        match schema_ctx.begin_tx().await {
            Ok(mut tx) => {
                match db.notes.update_original_tx(&mut tx, note_id, content).await {
                    Ok(()) => content_updated = true,
                    Err(e) => {
                        let error_text = e.to_string();
                        error!(
                            note_present = true,
                            error_len = error_text.len(),
                            error_reason = keyframe_assembly_error_reason_code(&error_text),
                            "Failed to propagate assembly content to note"
                        );
                    }
                }

                if let Err(e) = tx.commit().await {
                    let error_text = e.to_string();
                    error!(
                        error_len = error_text.len(),
                        error_reason = keyframe_assembly_error_reason_code(&error_text),
                        "Failed to commit note propagation"
                    );
                    content_updated = false;
                }
            }
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    error_len = error_text.len(),
                    error_reason = keyframe_assembly_error_reason_code(&error_text),
                    "Failed to begin tx for note propagation"
                );
            }
        }

        // Queue downstream NLP if content was updated
        if content_updated {
            // ConceptTagging removed — chained from AiRevision after revision completes.
            // Embedding + Linking removed — chained from ConceptTagging → RelatedConceptInference.
            // Pipeline: AiRevision → ConceptTagging → RelatedConceptInference → Embedding → Linking.
            let downstream_types = [JobType::TitleGeneration];

            let mut schema_payload = serde_json::Map::new();
            if schema != "public" {
                schema_payload.insert("schema".to_string(), json!(schema));
            }
            let job_payload = if schema_payload.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(schema_payload))
            };

            // Queue AI revision so the assembled keyframe descriptions get
            // polished content. ConceptTagging chains from AiRevision completion.
            // Mark as post_extraction so the handler skips the media-deferral check.
            let mut revision_payload = serde_json::Map::new();
            revision_payload.insert("revision_mode".to_string(), json!("standard"));
            revision_payload.insert("post_extraction".to_string(), json!(true));
            if schema != "public" {
                revision_payload.insert("schema".to_string(), json!(schema));
            }
            match db
                .jobs
                .queue_deduplicated(
                    Some(note_id),
                    JobType::AiRevision,
                    JobType::AiRevision.default_priority(),
                    Some(serde_json::Value::Object(revision_payload)),
                    JobType::AiRevision.default_cost_tier(),
                )
                .await
            {
                Ok(Some(job_id)) => {
                    ctx.emit_job_queued(job_id, JobType::AiRevision, Some(note_id));
                }
                Ok(None) => {} // deduplicated
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        error_len = error_text.len(),
                        error_reason = keyframe_assembly_error_reason_code(&error_text),
                        "Failed to queue AiRevision after keyframe assembly"
                    );
                }
            }

            for job_type in &downstream_types {
                match db
                    .jobs
                    .queue_deduplicated(
                        Some(note_id),
                        *job_type,
                        job_type.default_priority(),
                        job_payload.clone(),
                        job_type.default_cost_tier(),
                    )
                    .await
                {
                    Ok(Some(job_id)) => {
                        ctx.emit_job_queued(job_id, *job_type, Some(note_id));
                    }
                    Ok(None) => {} // deduplicated
                    Err(e) => {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = keyframe_assembly_error_reason_code(&error_text),
                            job_type = ?job_type,
                            "Failed to queue downstream job"
                        );
                    }
                }
            }

            info!(
                note_present = true,
                "Queued AiRevision + downstream NLP after keyframe assembly"
            );
        }
    }

    ctx.report_progress(100, Some("Done"));
    JobResult::Success(Some(json!({
        "frame_count": frame_count,
        "content_updated": full_text.is_some(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_assembly_error_reason_code_uses_stable_classes() {
        assert_eq!(
            keyframe_assembly_error_reason_code("database sql failed while applying manifest"),
            "database_error"
        );
        assert_eq!(
            keyframe_assembly_error_reason_code("file storage denied during write"),
            "permission_denied"
        );
        assert_eq!(
            keyframe_assembly_error_reason_code("Cannot connect to queue backend"),
            "connection_failed"
        );
        assert_eq!(
            keyframe_assembly_error_reason_code("opaque backend diagnostic text"),
            "operation_failed"
        );
    }
}

//! KeyframeVisionHandler — describes a single video keyframe via vision LLM.
//!
//! Each instance processes exactly one keyframe: downloads the JPEG from
//! derived attachment storage, calls the vision backend, and updates the
//! attachment's ai_description. After completion, checks if all keyframes
//! for the parent video are described; if so, queues KeyframeAssembly.
//!
//! Fan-in: count(described) == total_frames → queue_deduplicated(KeyframeAssembly)
//! Race safety: queue_deduplicated prevents duplicate assembly jobs.
//!
//! Issue #526

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType};
use matric_db::{Database, SchemaContext};
use matric_inference::{transcription::TranscriptionSegment, VisionBackend};

use crate::adapters::video_multimodal::get_transcript_context_for_frame;
use crate::handler::{JobContext, JobHandler, JobResult};

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
        .map_err(|e| JobResult::Failed(format!("Invalid schema '{}': {}", schema, e)))
}

pub struct KeyframeVisionHandler {
    db: Database,
    vision: Option<Arc<dyn VisionBackend>>,
}

impl KeyframeVisionHandler {
    pub fn new(db: Database, vision: Option<Arc<dyn VisionBackend>>) -> Self {
        Self { db, vision }
    }
}

#[async_trait]
impl JobHandler for KeyframeVisionHandler {
    fn job_type(&self) -> JobType {
        JobType::KeyframeVision
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing keyframe vision job payload".into()),
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

        // Bail early if vision backend is unavailable — the job stays in the
        // queue and will be retried once the backend is configured (#529).
        let vision = match self.vision.as_ref() {
            Some(v) => v,
            None => {
                warn!(
                    frame_index,
                    keyframe = %keyframe_attachment_id,
                    "KeyframeVision job deferred — vision backend unavailable"
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

        // Step 2: Get transcript context from parent's extracted_metadata
        ctx.report_progress(20, Some("Loading transcript context"));
        let transcript_context: Option<String> = 'tc: {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    warn!(error = %e, "Failed to begin tx for transcript context");
                    break 'tc None;
                }
            };
            let row: Option<(Option<serde_json::Value>,)> =
                sqlx::query_as("SELECT extracted_metadata FROM attachment WHERE id = $1")
                    .bind(parent_attachment_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()
                    .flatten();
            let _ = tx.commit().await;

            row.and_then(|(em,)| em).and_then(|em| {
                let segments: Vec<TranscriptionSegment> =
                    serde_json::from_value(em.get("transcript_segments")?.clone()).ok()?;
                get_transcript_context_for_frame(timestamp_secs, &segments)
            })
        };

        // Step 3: Call vision LLM
        ctx.report_progress(
            30,
            Some(&format!(
                "Describing frame {}/{} ({:.0}s)",
                frame_index + 1,
                total_frames,
                timestamp_secs
            )),
        );

        let mut prompt =
            "Describe this video frame in detail. What is happening in this scene?".to_string();
        if let Some(ref tc) = transcript_context {
            prompt.push_str(&format!(
                "\n\nNearby audio/speech: \"{}\"\nAlign your visual description with the spoken content where relevant.",
                tc
            ));
        }

        let description = match vision
            .describe_image(&image_data, "image/jpeg", Some(&prompt))
            .await
        {
            Ok(desc) => desc,
            Err(e) => {
                warn!(
                    frame_index,
                    parent = %parent_attachment_id,
                    keyframe = %keyframe_attachment_id,
                    model = vision.model_name(),
                    image_bytes = image_data.len(),
                    error = %e,
                    "Vision LLM failed for keyframe — will retry"
                );
                return JobResult::Retry(format!(
                    "Vision LLM failed for frame {}: {}",
                    frame_index, e
                ));
            }
        };

        // Step 4: Update derived attachment with ai_description
        ctx.report_progress(80, Some("Storing description"));
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            if let Err(e) = file_storage
                .update_ai_description_tx(
                    &mut tx,
                    keyframe_attachment_id,
                    &description,
                    Some(vision.model_name()),
                )
                .await
            {
                return JobResult::Failed(format!("Failed to store description: {}", e));
            }
            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
            }
        }

        info!(
            frame_index,
            parent = %parent_attachment_id,
            keyframe = %keyframe_attachment_id,
            "Keyframe {} described ({} chars)",
            frame_index,
            description.len()
        );

        // Step 5: Fan-in check — are all sibling keyframes described AND transcript complete?
        // Both keyframe descriptions and audio transcription must finish before assembly.
        ctx.report_progress(90, Some("Checking fan-in"));
        if total_frames > 0 {
            let (described_count, transcript_complete) = {
                let mut tx = match schema_ctx.begin_tx().await {
                    Ok(t) => t,
                    Err(e) => {
                        warn!(error = %e, "Fan-in count failed, assembly may be delayed");
                        return JobResult::Success(Some(json!({
                            "frame_index": frame_index,
                            "description_length": description.len(),
                        })));
                    }
                };
                let count = file_storage
                    .count_described_keyframes_tx(&mut tx, parent_attachment_id)
                    .await
                    .unwrap_or(0);
                // Check transcript_complete flag from parent's extracted_metadata
                let tc_row: Option<(Option<serde_json::Value>,)> =
                    sqlx::query_as("SELECT extracted_metadata FROM attachment WHERE id = $1")
                        .bind(parent_attachment_id)
                        .fetch_optional(&mut *tx)
                        .await
                        .ok()
                        .flatten();
                let tc = tc_row
                    .and_then(|(em,)| em)
                    .and_then(|em| em.get("transcript_complete")?.as_bool())
                    .unwrap_or(false);
                let _ = tx.commit().await;
                (count, tc)
            };

            debug!(
                described = described_count,
                total = total_frames,
                transcript_complete,
                "Fan-in: {}/{} keyframes + transcript={}",
                described_count,
                total_frames,
                transcript_complete
            );

            if described_count >= total_frames && transcript_complete {
                // All frames described AND transcript complete — queue assembly
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
                            "All {} keyframes + transcript complete, KeyframeAssembly queued (job {})",
                            total_frames, job_id
                        );
                    }
                    Ok(None) => {
                        debug!("KeyframeAssembly already queued (deduplicated)");
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to queue KeyframeAssembly");
                    }
                }
            }
        }

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(json!({
            "frame_index": frame_index,
            "description_length": description.len(),
        })))
    }
}

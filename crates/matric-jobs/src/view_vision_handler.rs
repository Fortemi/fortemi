//! ViewVisionHandler — describes a single 3D model rendered view via vision LLM.
//!
//! Each instance processes exactly one rendered view: downloads the PNG from
//! derived attachment storage, calls the vision backend, and updates the
//! attachment's ai_description. After completion, checks if all views
//! for the parent 3D model are described; if so, queues ViewAssembly.
//!
//! Fan-in: count(described) == total_views → queue_deduplicated(ViewAssembly)
//! Race safety: queue_deduplicated prevents duplicate assembly jobs.
//!
//! Issue #533

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType};
use matric_db::{Database, SchemaContext};
use matric_inference::VisionBackend;

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

fn view_vision_error_reason_code(error: &str) -> &'static str {
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

pub struct ViewVisionHandler {
    db: Database,
    vision: Option<Arc<dyn VisionBackend>>,
}

impl ViewVisionHandler {
    pub fn new(db: Database, vision: Option<Arc<dyn VisionBackend>>) -> Self {
        Self { db, vision }
    }
}

#[async_trait]
impl JobHandler for ViewVisionHandler {
    fn job_type(&self) -> JobType {
        JobType::ViewVision
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing view vision job payload".into()),
        };

        let parent_attachment_id: Uuid = match payload
            .get("parent_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid parent_attachment_id".into()),
        };

        let view_attachment_id: Uuid = match payload
            .get("view_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid view_attachment_id".into()),
        };

        let view_index = payload
            .get("view_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let angle_degrees = payload
            .get("angle_degrees")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let elevation = payload
            .get("elevation")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let total_views = payload
            .get("total_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as i64;

        let filename = payload
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("model.glb");

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Bail early if vision backend is unavailable — the job stays in the
        // queue and will be retried once the backend is configured.
        let vision = match self.vision.as_ref() {
            Some(v) => v,
            None => {
                warn!(
                    view_index,
                    view = %view_attachment_id,
                    "ViewVision job deferred — vision backend unavailable"
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

        // Step 1: Download rendered view PNG
        ctx.report_progress(10, Some("Downloading rendered view"));
        let image_data = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let result = file_storage
                .download_file_tx(&mut tx, view_attachment_id)
                .await;
            let _ = tx.commit().await;
            match result {
                Ok((data, _content_type, _filename)) => data,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to download view {}: {}",
                        view_attachment_id, e
                    ))
                }
            }
        };

        if image_data.is_empty() {
            return JobResult::Failed(format!("Empty image data for view {}", view_attachment_id));
        }

        // Step 2: Call vision LLM
        ctx.report_progress(
            30,
            Some(&format!(
                "Describing view {}/{} ({:.0}°, {})",
                view_index + 1,
                total_views,
                angle_degrees,
                elevation
            )),
        );

        let prompt = format!(
            "Describe this rendered view of a 3D model in detail. \
             This is view {} of {} (camera angle: {:.0}°, elevation: {}). \
             The model file is '{}'. \
             Describe the shape, materials, textures, colors, and any notable features visible from this angle.",
            view_index + 1, total_views, angle_degrees, elevation, filename
        );

        let description = match vision
            .describe_image(&image_data, "image/png", Some(&prompt))
            .await
        {
            Ok(desc) => desc,
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    view_index,
                    parent = %parent_attachment_id,
                    view = %view_attachment_id,
                    model = vision.model_name(),
                    image_bytes = image_data.len(),
                    error_len = error_text.len(),
                    error_reason = view_vision_error_reason_code(&error_text),
                    "Vision LLM failed for view — will retry"
                );
                return JobResult::Retry(format!(
                    "Vision LLM failed for view {}: {}",
                    view_index, e
                ));
            }
        };

        // Step 3: Update derived attachment with ai_description
        ctx.report_progress(80, Some("Storing description"));
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            if let Err(e) = file_storage
                .update_ai_description_tx(
                    &mut tx,
                    view_attachment_id,
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
            view_index,
            parent = %parent_attachment_id,
            view = %view_attachment_id,
            "View {} described ({} chars)",
            view_index,
            description.len()
        );

        // Step 4: Fan-in check — are all sibling views described?
        ctx.report_progress(90, Some("Checking fan-in"));
        if total_views > 0 {
            let described_count = {
                let mut tx = match schema_ctx.begin_tx().await {
                    Ok(t) => t,
                    Err(e) => {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = view_vision_error_reason_code(&error_text),
                            "Fan-in count failed, assembly may be delayed"
                        );
                        return JobResult::Success(Some(json!({
                            "view_index": view_index,
                            "description_length": description.len(),
                        })));
                    }
                };
                let count = file_storage
                    .count_described_derived_tx(&mut tx, parent_attachment_id, "3d_rendering")
                    .await
                    .unwrap_or(0);
                let _ = tx.commit().await;
                count
            };

            debug!(
                described = described_count,
                total = total_views,
                "Fan-in: {}/{} views described",
                described_count,
                total_views
            );

            if described_count >= total_views {
                // All views done — queue assembly
                let mut assembly_payload = serde_json::Map::new();
                assembly_payload.insert(
                    "attachment_id".into(),
                    json!(parent_attachment_id.to_string()),
                );
                assembly_payload.insert("filename".into(), json!(filename));
                if schema != "public" {
                    assembly_payload.insert("schema".into(), json!(schema));
                }

                match self
                    .db
                    .jobs
                    .queue_deduplicated(
                        ctx.note_id(),
                        JobType::ViewAssembly,
                        JobType::ViewAssembly.default_priority(),
                        Some(serde_json::Value::Object(assembly_payload)),
                        JobType::ViewAssembly.default_cost_tier(),
                    )
                    .await
                {
                    Ok(Some(job_id)) => {
                        ctx.emit_job_queued(job_id, JobType::ViewAssembly, ctx.note_id());
                        info!(
                            "All {} views described, ViewAssembly queued (job {})",
                            total_views, job_id
                        );
                    }
                    Ok(None) => {
                        debug!("ViewAssembly already queued (deduplicated)");
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        error!(
                            error_len = error_text.len(),
                            error_reason = view_vision_error_reason_code(&error_text),
                            "Failed to queue ViewAssembly"
                        );
                    }
                }
            }
        }

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(json!({
            "view_index": view_index,
            "description_length": description.len(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_vision_error_reason_code_uses_stable_classes() {
        assert_eq!(
            view_vision_error_reason_code("vision model failed for /home/operator/mm_key_secret"),
            "model_backend_error"
        );
        assert_eq!(
            view_vision_error_reason_code("postgres://user:secret@db/app sql failed"),
            "database_error"
        );
        assert_eq!(
            view_vision_error_reason_code("Cannot connect to inference backend"),
            "connection_failed"
        );
        assert_eq!(
            view_vision_error_reason_code("opaque backend text with token mm_key_secret"),
            "operation_failed"
        );
    }
}

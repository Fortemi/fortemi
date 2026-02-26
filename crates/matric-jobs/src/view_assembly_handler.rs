//! ViewAssemblyHandler — fan-in aggregation of 3D model view descriptions.
//!
//! Reads all described rendered views for a parent 3D model attachment,
//! synthesizes a composite description from all views, updates the parent
//! attachment's ai_description and extracted_metadata, and propagates content
//! to the note. Queues downstream NLP jobs (Embedding, Linking,
//! ConceptTagging, TitleGeneration).
//!
//! Triggered by the last ViewVision job to complete (fan-in via
//! queue_deduplicated).
//!
//! Issue #533

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tracing::{error, info, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType};
use matric_db::{Database, SchemaContext};

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

pub struct ViewAssemblyHandler {
    db: Database,
}

impl ViewAssemblyHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for ViewAssemblyHandler {
    fn job_type(&self) -> JobType {
        JobType::ViewAssembly
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing view assembly job payload".into()),
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

        let filename = payload
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("model.glb");

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Step 1: Load parent attachment metadata
        ctx.report_progress(10, Some("Loading parent metadata"));
        let existing_metadata = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let row: Option<(Option<JsonValue>,)> =
                sqlx::query_as("SELECT extracted_metadata FROM attachment WHERE id = $1")
                    .bind(attachment_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()
                    .flatten();
            let _ = tx.commit().await;

            match row {
                Some((meta,)) => meta.unwrap_or(json!({})),
                None => {
                    return JobResult::Failed(format!(
                        "Parent attachment {} not found",
                        attachment_id
                    ))
                }
            }
        };

        // Step 2: Load all 3d_rendering derived attachments sorted by view_index
        ctx.report_progress(20, Some("Loading view descriptions"));
        let view_descriptions: Vec<JsonValue> = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let views = file_storage
                .list_derived_by_type_tx(&mut tx, attachment_id, "3d_rendering")
                .await
                .unwrap_or_default();
            let _ = tx.commit().await;

            let mut descriptions: Vec<(u64, JsonValue)> = views
                .iter()
                .filter_map(|att| {
                    let meta = att.extracted_metadata.as_ref()?;
                    let view_index = meta.get("view_index")?.as_u64()?;
                    let angle_degrees = meta.get("angle_degrees")?.as_f64().unwrap_or(0.0);
                    let elevation = meta
                        .get("elevation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let description = att.ai_description.as_deref().unwrap_or("");
                    Some((
                        view_index,
                        json!({
                            "view_index": view_index,
                            "angle_degrees": angle_degrees,
                            "elevation": elevation,
                            "description": description,
                        }),
                    ))
                })
                .collect();
            descriptions.sort_by_key(|(idx, _)| *idx);
            descriptions.into_iter().map(|(_, v)| v).collect()
        };

        if view_descriptions.is_empty() {
            return JobResult::Failed("No view descriptions found for assembly".into());
        }

        info!(
            attachment = %attachment_id,
            views = view_descriptions.len(),
            "Assembling {} view descriptions into 3D model description",
            view_descriptions.len()
        );

        // Step 3: Build composite description from all views
        ctx.report_progress(40, Some("Synthesizing composite description"));
        let composite_description = build_composite_description(filename, &view_descriptions);

        // Step 4: Build markdown content for the 3D model
        ctx.report_progress(50, Some("Building model markdown"));
        let model_markdown =
            format_3d_model_markdown(filename, &view_descriptions, &composite_description);

        // Step 5: Update parent attachment's ai_description and metadata
        ctx.report_progress(60, Some("Updating attachment"));
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };

            // Merge view_count into existing metadata
            let mut updated_meta = existing_metadata.clone();
            if let Some(obj) = updated_meta.as_object_mut() {
                obj.insert("view_count".to_string(), json!(view_descriptions.len()));
                obj.insert("view_assembly_complete".to_string(), json!(true));
                obj.insert("view_descriptions".to_string(), json!(view_descriptions));
            }

            if let Err(e) = file_storage
                .update_extracted_content_tx(
                    &mut tx,
                    attachment_id,
                    Some(&model_markdown),
                    Some(updated_meta),
                )
                .await
            {
                return JobResult::Failed(format!("Failed to update extracted content: {}", e));
            }

            // Also update ai_description with the composite
            if let Err(e) = file_storage
                .update_ai_description_tx(&mut tx, attachment_id, &composite_description, None)
                .await
            {
                warn!(error = %e, "Failed to update ai_description (non-fatal)");
            }

            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
            }
        }

        // Step 6: Store view manifest as derived attachment
        ctx.report_progress(70, Some("Storing view manifest"));
        {
            let manifest_data = serde_json::to_vec_pretty(&view_descriptions).unwrap_or_default();
            let base_name = filename
                .trim_end_matches(".glb")
                .trim_end_matches(".gltf")
                .trim_end_matches(".GLB")
                .trim_end_matches(".GLTF");

            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                let _ = file_storage
                    .store_derived_attachment_tx(
                        &mut tx,
                        note_id,
                        attachment_id,
                        &format!("{}_views.json", base_name),
                        "application/json",
                        &manifest_data,
                        "view_manifest",
                    )
                    .await;
                let _ = tx.commit().await;
            }
        }

        // Step 7: Propagate to note + queue downstream NLP
        ctx.report_progress(80, Some("Propagating content"));
        finish_propagation(
            &self.db,
            &schema_ctx,
            schema,
            &ctx,
            note_id,
            &model_markdown,
            view_descriptions.len(),
        )
        .await
    }
}

/// Build a composite description by combining individual view descriptions.
fn build_composite_description(filename: &str, views: &[JsonValue]) -> String {
    let views_text: Vec<String> = views
        .iter()
        .map(|v| {
            format!(
                "View {} ({:.0}°, {}): {}",
                v["view_index"], v["angle_degrees"], v["elevation"], v["description"]
            )
        })
        .collect();

    format!(
        "3D model '{}' — {} views rendered.\n\n{}",
        filename,
        views.len(),
        views_text.join("\n\n")
    )
}

/// Format view descriptions into a structured markdown document.
fn format_3d_model_markdown(filename: &str, views: &[JsonValue], composite: &str) -> String {
    let mut md = format!("# 3D Model: {}\n\n", filename);
    md.push_str(&format!("**Views rendered:** {}\n\n", views.len()));
    md.push_str("## Composite Description\n\n");
    md.push_str(composite);
    md.push_str("\n\n## Individual Views\n\n");

    for v in views {
        let idx = v["view_index"].as_u64().unwrap_or(0);
        let angle = v["angle_degrees"].as_f64().unwrap_or(0.0);
        let elev = v["elevation"].as_str().unwrap_or("unknown");
        let desc = v["description"].as_str().unwrap_or("");
        md.push_str(&format!(
            "### View {} ({:.0}°, {})\n\n{}\n\n",
            idx + 1,
            angle,
            elev,
            desc
        ));
    }

    md
}

/// Propagate assembled content to the note and queue downstream NLP jobs.
async fn finish_propagation(
    db: &Database,
    schema_ctx: &SchemaContext,
    schema: &str,
    ctx: &JobContext,
    note_id: Uuid,
    content: &str,
    view_count: usize,
) -> JobResult {
    // Always propagate assembled content to the note — the view assembly
    // produces the authoritative multi-view representation which is far
    // richer than whatever stub content may already be there.
    let mut content_updated = false;
    match schema_ctx.begin_tx().await {
        Ok(mut tx) => {
            match db.notes.update_original_tx(&mut tx, note_id, content).await {
                Ok(()) => content_updated = true,
                Err(e) => {
                    error!(
                        note_id = %note_id,
                        error = %e,
                        "Failed to propagate assembly content to note"
                    );
                }
            }

            if let Err(e) = tx.commit().await {
                error!(error = %e, "Failed to commit note propagation");
                content_updated = false;
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to begin tx for note propagation");
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

        // Queue AI revision so the assembled view descriptions get a polished
        // title and revised content (mirrors extraction_handler.rs pattern).
        let mut revision_payload = serde_json::Map::new();
        revision_payload.insert("revision_mode".to_string(), json!("standard"));
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
                warn!(error = %e, "Failed to queue AiRevision after view assembly");
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
                    warn!(error = %e, job_type = ?job_type, "Failed to queue downstream job");
                }
            }
        }

        info!(
            note_id = %note_id,
            "Queued AiRevision + downstream NLP after view assembly"
        );
    }

    ctx.report_progress(100, Some("Done"));
    JobResult::Success(Some(json!({
        "view_count": view_count,
        "content_updated": content_updated,
    })))
}

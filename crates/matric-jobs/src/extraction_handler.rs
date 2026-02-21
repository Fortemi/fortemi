//! ExtractionHandler — dispatches upload → extract → chunk → embed pipeline.

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use matric_core::{AttachmentStatus, ExtractionStrategy, JobRepository, JobType, ProgressFn};
use matric_db::{Database, SchemaContext};

/// Minimum note content length (in chars) below which extraction results
/// replace the note content.  Notes auto-created from attachment uploads
/// typically have only the filename as content — these should be enriched.
const MIN_CONTENT_LEN: usize = 50;

use crate::extraction::ExtractionRegistry;
use crate::handler::{JobContext, JobHandler, JobResult};

/// Extract the target schema from a job's payload.
///
/// Returns the schema name for multi-memory archive support (Issue #426).
/// Defaults to "public" for backward compatibility with jobs queued before
/// the multi-memory feature.
fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

/// Create a SchemaContext for the given schema, returning a JobResult error on failure.
fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema)
        .map_err(|e| JobResult::Failed(format!("Invalid schema '{}': {}", schema, e)))
}

pub struct ExtractionHandler {
    db: Database,
    registry: Arc<ExtractionRegistry>,
}

impl ExtractionHandler {
    pub fn new(db: Database, registry: Arc<ExtractionRegistry>) -> Self {
        Self { db, registry }
    }
}

#[async_trait]
impl JobHandler for ExtractionHandler {
    fn job_type(&self) -> JobType {
        JobType::Extraction
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        // Parse payload: { strategy, filename, mime_type, data, config }
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing extraction job payload".into()),
        };

        let strategy_str = payload
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("text_native");
        let strategy: ExtractionStrategy = match strategy_str.parse() {
            Ok(s) => s,
            Err(e) => return JobResult::Failed(format!("Invalid extraction strategy: {}", e)),
        };

        let filename = payload
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let mime_type = payload
            .get("mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        let config = payload.get("config").cloned().unwrap_or_else(|| json!({}));

        // Parse optional attachment_id (used later for persisting results)
        let attachment_id: Option<Uuid> = if let Some(id_str) =
            payload.get("attachment_id").and_then(|v| v.as_str())
        {
            match id_str.parse() {
                Ok(id) => Some(id),
                Err(e) => return JobResult::Failed(format!("Invalid attachment_id UUID: {}", e)),
            }
        } else {
            None
        };

        // Schema context for multi-memory archive support (Issue #426)
        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Resolving attachment and strategy"));

        // Get data: prefer attachment_id (fetch from file storage), fall back to inline data
        let data = if let Some(att_id) = attachment_id {
            let file_storage = match self.db.file_storage.as_ref() {
                Some(fs) => fs,
                None => return JobResult::Failed("File storage not configured".into()),
            };

            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let result = file_storage.download_file_tx(&mut tx, att_id).await;
            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
            }
            match result {
                Ok((file_data, _content_type, _filename)) => file_data,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to download attachment {}: {}",
                        att_id, e
                    ))
                }
            }
        } else if let Some(data_str) = payload.get("data").and_then(|v| v.as_str()) {
            data_str.as_bytes().to_vec()
        } else {
            return JobResult::Failed(
                "No data provided (expected 'attachment_id' or 'data' field)".into(),
            );
        };

        ctx.report_progress(10, Some("Starting extraction"));

        // Check adapter availability
        if !self.registry.has_adapter(strategy) {
            let available: Vec<String> = self
                .registry
                .available_strategies()
                .iter()
                .map(|s| s.to_string())
                .collect();
            return JobResult::Failed(format!(
                "No adapter registered for strategy: {:?}. Available strategies: {:?}",
                strategy, available
            ));
        }

        ctx.report_progress(20, Some("Extracting content"));

        // Create a scoped progress callback that maps adapter progress (0-100%)
        // into the extraction phase range (20-80% of overall job progress).
        let progress: ProgressFn = {
            let progress_cb = ctx.progress_callback_arc();
            Arc::new(move |adapter_pct: i32, message: Option<&str>| {
                // Map adapter's 0-100 to job's 20-80
                let job_pct = 20 + (adapter_pct.clamp(0, 100) as i64 * 60 / 100) as i32;
                if let Some(ref cb) = progress_cb {
                    cb(job_pct, message);
                }
            })
        };

        // Run extraction with progress reporting
        match self
            .registry
            .extract_with_progress(strategy, &data, filename, mime_type, &config, progress)
            .await
        {
            Ok(result) => {
                ctx.report_progress(80, Some("Extraction complete"));

                // Persist extraction results to the attachment record (schema-aware)
                if let Some(att_id) = attachment_id {
                    if let Some(file_storage) = self.db.file_storage.as_ref() {
                        match schema_ctx.begin_tx().await {
                            Ok(mut tx) => {
                                if let Err(e) = file_storage
                                    .update_extracted_content_tx(
                                        &mut tx,
                                        att_id,
                                        result.extracted_text.as_deref(),
                                        Some(result.metadata.clone()),
                                    )
                                    .await
                                {
                                    error!(
                                        attachment_id = %att_id,
                                        error = %e,
                                        "Failed to persist extracted content"
                                    );
                                }

                                // Persist ai_description if present (Issue #492, Bug 1).
                                // This is the primary useful output for Vision and
                                // Glb3DModel adapters.
                                if let Some(ref description) = result.ai_description {
                                    if let Err(e) = file_storage
                                        .update_ai_description_tx(
                                            &mut tx,
                                            att_id,
                                            description,
                                            None, // TODO: pass model name from adapter
                                        )
                                        .await
                                    {
                                        error!(
                                            attachment_id = %att_id,
                                            error = %e,
                                            "Failed to persist ai_description"
                                        );
                                    }
                                }

                                if let Err(e) = file_storage
                                    .update_status_tx(
                                        &mut tx,
                                        att_id,
                                        AttachmentStatus::Completed,
                                        None,
                                    )
                                    .await
                                {
                                    error!(
                                        attachment_id = %att_id,
                                        error = %e,
                                        "Failed to update attachment status"
                                    );
                                }

                                if let Err(e) = tx.commit().await {
                                    error!(
                                        attachment_id = %att_id,
                                        error = %e,
                                        "Failed to commit extraction results"
                                    );
                                }
                            }
                            Err(e) => {
                                error!(
                                    attachment_id = %att_id,
                                    error = %e,
                                    "Failed to begin schema tx for persisting results"
                                );
                            }
                        }
                    }
                }

                // Run MP4 faststart optimization if applicable (#503)
                if let Some(att_id) = attachment_id {
                    if mime_type == "video/mp4" || mime_type == "video/quicktime" {
                        if let Some(file_storage) = self.db.file_storage.as_ref() {
                            ctx.report_progress(82, Some("Optimizing video for streaming"));
                            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                if let Ok(info) =
                                    file_storage.get_file_metadata_tx(&mut tx, att_id).await
                                {
                                    drop(tx);
                                    if let matric_db::FileSource::Filesystem(ref storage_path) =
                                        info.source
                                    {
                                        if let Some(fs_path) =
                                            file_storage.resolve_storage_path(storage_path)
                                        {
                                            let fs_str = fs_path.to_string_lossy().to_string();
                                            let work_dir = match tempfile::TempDir::new() {
                                                Ok(d) => d,
                                                Err(_) => {
                                                    warn!(
                                                        "Failed to create temp dir for faststart"
                                                    );
                                                    // skip optimization
                                                    tempfile::TempDir::new().unwrap()
                                                }
                                            };
                                            match crate::adapters::video_multimodal::optimize_faststart(&fs_str, &work_dir).await {
                                                Ok(optimized) if optimized != fs_str => {
                                                    // Replace original file with optimized version
                                                    if let Err(e) = tokio::fs::copy(&optimized, &fs_path).await {
                                                        warn!(error = %e, "Failed to copy faststart-optimized file");
                                                    } else {
                                                        info!(attachment_id = %att_id, "MP4 faststart optimization applied");
                                                    }
                                                }
                                                _ => {} // Already optimized or failed gracefully
                                            }
                                        }
                                    }
                                } else {
                                    drop(tx);
                                }
                            }
                        }
                    }
                }

                // Persist thumbnail as derived attachment if available (#502)
                if let (Some(att_id), Some(note_id), Some(thumbnail_bytes)) =
                    (attachment_id, ctx.note_id(), result.preview_data.as_ref())
                {
                    if let Some(file_storage) = self.db.file_storage.as_ref() {
                        ctx.report_progress(83, Some("Persisting thumbnail"));
                        if let Ok(mut tx) = schema_ctx.begin_tx().await {
                            let thumb_filename = format!("{}_thumbnail.jpg", att_id);
                            match file_storage
                                .store_derived_attachment_tx(
                                    &mut tx,
                                    note_id,
                                    att_id,
                                    &thumb_filename,
                                    "image/jpeg",
                                    thumbnail_bytes,
                                    "thumbnail",
                                )
                                .await
                            {
                                Ok(thumb_att) => {
                                    // Mark parent as having a preview
                                    if let Err(e) =
                                        file_storage.set_has_preview_tx(&mut tx, att_id, true).await
                                    {
                                        warn!(error = %e, "Failed to set has_preview on parent");
                                    }
                                    if let Err(e) = tx.commit().await {
                                        error!(error = %e, "Failed to commit thumbnail");
                                    } else {
                                        info!(
                                            parent = %att_id,
                                            thumbnail = %thumb_att.id,
                                            "Thumbnail persisted as derived attachment"
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to store thumbnail attachment");
                                    drop(tx);
                                }
                            }
                        }
                    }
                }

                // Persist transcript files as derived attachments if available (#498)
                if let (Some(att_id), Some(note_id)) = (attachment_id, ctx.note_id()) {
                    if let Some(file_storage) = self.db.file_storage.as_ref() {
                        // Check for transcript segments in extraction metadata
                        let segments_json = result
                            .metadata
                            .get("transcript_segments")
                            .or_else(|| result.metadata.get("segments"))
                            .and_then(|v| v.as_array());

                        if let Some(segs) = segments_json {
                            let caption_segments: Vec<matric_core::captions::CaptionSegment> = segs
                                .iter()
                                .filter_map(|seg| {
                                    let start = seg.get("start_secs")?.as_f64()?;
                                    let end = seg.get("end_secs")?.as_f64()?;
                                    let text = seg.get("text")?.as_str()?.to_string();
                                    let speaker = seg
                                        .get("speaker_id")
                                        .and_then(|s| s.as_str())
                                        .map(|s| s.to_string());
                                    Some(matric_core::captions::CaptionSegment {
                                        start_secs: start,
                                        end_secs: end,
                                        text,
                                        speaker,
                                    })
                                })
                                .collect();

                            if !caption_segments.is_empty() {
                                ctx.report_progress(84, Some("Persisting transcript files"));
                                let base_name = filename
                                    .rsplit_once('.')
                                    .map(|(name, _)| name)
                                    .unwrap_or(filename);

                                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                    // VTT file
                                    let vtt =
                                        matric_core::captions::render_webvtt(&caption_segments);
                                    if let Err(e) = file_storage
                                        .store_derived_attachment_tx(
                                            &mut tx,
                                            note_id,
                                            att_id,
                                            &format!("{}.vtt", base_name),
                                            "text/vtt",
                                            vtt.as_bytes(),
                                            "caption",
                                        )
                                        .await
                                    {
                                        warn!(error = %e, "Failed to store VTT attachment");
                                    }

                                    // SRT file
                                    let srt = matric_core::captions::render_srt(&caption_segments);
                                    if let Err(e) = file_storage
                                        .store_derived_attachment_tx(
                                            &mut tx,
                                            note_id,
                                            att_id,
                                            &format!("{}.srt", base_name),
                                            "application/x-subrip",
                                            srt.as_bytes(),
                                            "caption",
                                        )
                                        .await
                                    {
                                        warn!(error = %e, "Failed to store SRT attachment");
                                    }

                                    // Plain text transcript
                                    let plain_text: String = caption_segments
                                        .iter()
                                        .map(|s| s.text.trim().to_string())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    if let Err(e) = file_storage
                                        .store_derived_attachment_tx(
                                            &mut tx,
                                            note_id,
                                            att_id,
                                            &format!("{}.transcript.txt", base_name),
                                            "text/plain",
                                            plain_text.as_bytes(),
                                            "transcript",
                                        )
                                        .await
                                    {
                                        warn!(error = %e, "Failed to store transcript attachment");
                                    }

                                    if let Err(e) = tx.commit().await {
                                        error!(error = %e, "Failed to commit transcript attachments");
                                    } else {
                                        info!(
                                            parent = %att_id,
                                            "Transcript files persisted as derived attachments (VTT, SRT, TXT)"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Persist derived files as child attachments (email attachments, etc.)
                // NOTE: This 84% report is mutually exclusive with transcript persistence above.
                // Transcripts come from audio/video adapters; derived files from email/archive adapters.
                if let (Some(att_id), Some(note_id)) = (attachment_id, ctx.note_id()) {
                    if !result.derived_files.is_empty() {
                        if let Some(file_storage) = self.db.file_storage.as_ref() {
                            let count = result.derived_files.len();
                            ctx.report_progress(
                                84,
                                Some(&format!("Persisting {} extracted files", count)),
                            );
                            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                let mut stored = 0usize;
                                for df in &result.derived_files {
                                    match file_storage
                                        .store_derived_attachment_tx(
                                            &mut tx,
                                            note_id,
                                            att_id,
                                            &df.filename,
                                            &df.content_type,
                                            &df.data,
                                            &df.derivation_type,
                                        )
                                        .await
                                    {
                                        Ok(_) => stored += 1,
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                filename = %df.filename,
                                                "Failed to store derived file"
                                            );
                                        }
                                    }
                                }
                                if let Err(e) = tx.commit().await {
                                    error!(error = %e, "Failed to commit derived files");
                                } else if stored > 0 {
                                    info!(
                                        parent = %att_id,
                                        count = stored,
                                        "Derived files persisted as child attachments"
                                    );
                                }
                            }
                        }
                    }
                }

                ctx.report_progress(85, Some("Results persisted"));

                // Queue speaker diarization if transcript segments are available (#497).
                // Only for audio/video strategies when diarization backend is configured.
                let has_transcript_segments = result
                    .metadata
                    .get("transcript_segments")
                    .or_else(|| result.metadata.get("segments"))
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                let is_audio_video = matches!(
                    strategy,
                    ExtractionStrategy::AudioTranscribe | ExtractionStrategy::VideoMultimodal
                );

                let diarization_available =
                    std::env::var(matric_core::defaults::ENV_DIARIZATION_BASE_URL)
                        .ok()
                        .filter(|s| !s.is_empty())
                        .is_some();

                if let (Some(att_id), Some(note_id)) = (attachment_id, ctx.note_id()) {
                    if is_audio_video && has_transcript_segments && diarization_available {
                        let mut diar_payload = serde_json::Map::new();
                        diar_payload.insert("attachment_id".to_string(), json!(att_id.to_string()));
                        if schema != "public" {
                            diar_payload.insert("schema".to_string(), json!(&schema));
                        }
                        match self
                            .db
                            .jobs
                            .queue_deduplicated(
                                Some(note_id),
                                JobType::SpeakerDiarization,
                                JobType::SpeakerDiarization.default_priority(),
                                Some(serde_json::Value::Object(diar_payload)),
                                JobType::SpeakerDiarization.default_cost_tier(),
                            )
                            .await
                        {
                            Ok(Some(job_id)) => {
                                ctx.emit_job_queued(
                                    job_id,
                                    JobType::SpeakerDiarization,
                                    Some(note_id),
                                );
                                info!(
                                    note_id = %note_id,
                                    attachment_id = %att_id,
                                    "Speaker diarization job queued"
                                );
                            }
                            Ok(None) => {} // Deduplicated
                            Err(e) => {
                                warn!(
                                    note_id = %note_id,
                                    error = %e,
                                    "Failed to queue speaker diarization job"
                                );
                            }
                        }
                    }
                }

                // --- Bug 1b (Issue #492): propagate extraction content to note
                // and re-queue downstream NLP jobs so they operate on real text
                // instead of a bare filename stub. ---
                let effective_content = result
                    .ai_description
                    .as_deref()
                    .or(result.extracted_text.as_deref());

                if let (Some(content), Some(note_id)) = (effective_content, ctx.note_id()) {
                    // Update note content if it is currently minimal (< MIN_CONTENT_LEN).
                    // Track whether we actually updated so we only re-queue downstream
                    // jobs when note content changed (avoids wasted reprocessing).
                    let mut content_updated = false;

                    match schema_ctx.begin_tx().await {
                        Ok(mut tx) => {
                            let should_update = match self.db.notes.fetch_tx(&mut tx, note_id).await
                            {
                                Ok(note) => note.original.content.len() < MIN_CONTENT_LEN,
                                Err(e) => {
                                    warn!(
                                        note_id = %note_id,
                                        error = %e,
                                        "Could not fetch note for content propagation"
                                    );
                                    false
                                }
                            };

                            if should_update {
                                match self
                                    .db
                                    .notes
                                    .update_original_tx(&mut tx, note_id, content)
                                    .await
                                {
                                    Ok(()) => content_updated = true,
                                    Err(e) => {
                                        error!(
                                            note_id = %note_id,
                                            error = %e,
                                            "Failed to propagate extraction content to note"
                                        );
                                    }
                                }
                            }

                            if let Err(e) = tx.commit().await {
                                error!(
                                    note_id = %note_id,
                                    error = %e,
                                    "Failed to commit note content propagation"
                                );
                                content_updated = false;
                            }
                        }
                        Err(e) => {
                            error!(
                                note_id = %note_id,
                                error = %e,
                                "Failed to begin tx for note content propagation"
                            );
                        }
                    }

                    // Re-queue downstream NLP jobs only when note content was
                    // actually updated — otherwise they'd reprocess identical text.
                    if content_updated {
                        let downstream_types = [
                            JobType::Embedding,
                            JobType::Linking,
                            JobType::ConceptTagging,
                            JobType::TitleGeneration,
                        ];

                        let mut schema_payload = serde_json::Map::new();
                        if schema != "public" {
                            schema_payload.insert("schema".to_string(), json!(&schema));
                        }
                        let job_payload = if schema_payload.is_empty() {
                            None
                        } else {
                            Some(serde_json::Value::Object(schema_payload))
                        };

                        // Re-queue AI revision with Standard mode so it operates on
                        // the actual extracted content (not the filename stub). (#494)
                        let mut revision_payload = serde_json::Map::new();
                        revision_payload.insert("revision_mode".to_string(), json!("standard"));
                        if schema != "public" {
                            revision_payload.insert("schema".to_string(), json!(&schema));
                        }
                        match self
                            .db
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
                            Ok(None) => {} // Deduplicated
                            Err(e) => {
                                error!(
                                    note_id = %note_id,
                                    error = %e,
                                    "Failed to re-queue AiRevision after extraction"
                                );
                            }
                        }

                        for job_type in &downstream_types {
                            match self
                                .db
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
                                Ok(None) => {} // Deduplicated
                                Err(e) => {
                                    error!(
                                        note_id = %note_id,
                                        job_type = ?job_type,
                                        error = %e,
                                        "Failed to re-queue downstream job after extraction"
                                    );
                                }
                            }
                        }

                        info!(
                            note_id = %note_id,
                            content_len = content.len(),
                            "Propagated extraction content and re-queued downstream jobs"
                        );
                    }
                }

                ctx.report_progress(95, Some("Downstream jobs queued"));

                let result_json = json!({
                    "strategy": strategy.to_string(),
                    "has_text": result.extracted_text.is_some(),
                    "text_length": result.extracted_text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "has_description": result.ai_description.is_some(),
                    "metadata": result.metadata,
                });

                info!(
                    strategy = %strategy,
                    filename,
                    text_len = result.extracted_text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "Extraction completed successfully"
                );

                ctx.report_progress(100, Some("Done"));
                JobResult::Success(Some(result_json))
            }
            Err(e) => {
                let error_msg = format!("Extraction failed: {}", e);
                error!(strategy = %strategy, filename, error = %e, "Extraction failed");

                // Update attachment status to Failed so it doesn't stay stuck at "uploaded"
                if let Some(att_id) = attachment_id {
                    if let Some(file_storage) = self.db.file_storage.as_ref() {
                        if let Ok(mut tx) = schema_ctx.begin_tx().await {
                            if let Err(status_err) = file_storage
                                .update_status_tx(
                                    &mut tx,
                                    att_id,
                                    AttachmentStatus::Failed,
                                    Some(&error_msg),
                                )
                                .await
                            {
                                error!(
                                    attachment_id = %att_id,
                                    error = %status_err,
                                    "Failed to update attachment status to Failed"
                                );
                            }
                            if let Err(commit_err) = tx.commit().await {
                                error!(
                                    attachment_id = %att_id,
                                    error = %commit_err,
                                    "Failed to commit attachment failure status"
                                );
                            }
                        }
                    }
                }

                JobResult::Failed(error_msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TextNativeAdapter;
    use chrono::Utc;
    use matric_core::{Job, JobStatus};
    use serde_json::json;
    use uuid::Uuid;

    fn test_db() -> Database {
        let pool =
            sqlx::Pool::<sqlx::Postgres>::connect_lazy("postgres://test:test@localhost/test")
                .expect("lazy pool");
        Database::new(pool)
    }

    fn create_test_job(payload: Option<serde_json::Value>) -> Job {
        Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::Extraction,
            status: JobStatus::Pending,
            priority: 7,
            payload,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_job_type() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);
        assert_eq!(handler.job_type(), JobType::Extraction);
    }

    #[tokio::test]
    async fn test_extraction_handler_can_handle() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);
        assert!(handler.can_handle(JobType::Extraction));
        assert!(!handler.can_handle(JobType::Embedding));
        assert!(!handler.can_handle(JobType::Linking));
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_payload() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let job = create_test_job(None);
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("Missing extraction job payload"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_invalid_strategy() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let payload = json!({
            "strategy": "invalid_strategy_name",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("Invalid extraction strategy"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_adapter() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("No adapter registered for strategy"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_data() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain"
            // Missing "data" field
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("No data provided"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_success() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "hello world",
            "config": {}
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Success(Some(result_json)) => {
                assert_eq!(result_json["strategy"], "text_native");
                assert_eq!(result_json["has_text"], true);
                assert_eq!(result_json["text_length"], 11);
            }
            _ => panic!("Expected Success result with data"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_with_progress_tracking() {
        use std::sync::{Arc as StdArc, Mutex};

        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));

        let progress_log = StdArc::new(Mutex::new(Vec::new()));
        let progress_log_clone = progress_log.clone();

        let ctx = JobContext::new(job).with_progress_callback(move |percent, message| {
            progress_log_clone
                .lock()
                .unwrap()
                .push((percent, message.map(String::from)));
        });

        let result = handler.execute(ctx).await;
        assert!(matches!(result, JobResult::Success(_)));

        let log = progress_log.lock().unwrap();
        assert!(log.len() >= 4); // At least: 10%, 20%, 80%, 100%
        assert!(log.iter().any(|(p, _)| *p == 10));
        assert!(log.iter().any(|(p, _)| *p == 20));
        assert!(log.iter().any(|(p, _)| *p == 80));
        assert!(log.iter().any(|(p, _)| *p == 100));
    }

    #[tokio::test]
    async fn test_extraction_handler_default_values() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        // Minimal payload with defaults
        let payload = json!({
            "data": "test"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        // Should use default strategy "text_native", filename "unknown", etc.
        assert!(matches!(result, JobResult::Success(_)));
    }
}

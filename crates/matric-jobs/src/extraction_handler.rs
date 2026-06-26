//! ExtractionHandler — dispatches upload → extract → chunk → embed pipeline.

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tracing::{debug, error, info, warn};
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

/// Stream-copy a file to a temp location using a windowed buffer.
///
/// Reads from `source` in `buffer_size`-byte chunks and writes to a new
/// temp file. Peak memory usage is bounded to `buffer_size` regardless
/// of file size. The returned `NamedTempFile` must be kept alive for as
/// long as the temp path is needed.
fn stream_copy_to_temp(
    source: &std::path::Path,
    buffer_size: usize,
) -> std::io::Result<NamedTempFile> {
    use std::io::{BufReader, BufWriter, Read, Write};

    let src = std::fs::File::open(source)?;
    let tmpfile = NamedTempFile::new()?;

    let mut reader = BufReader::with_capacity(buffer_size, src);
    let mut writer = BufWriter::with_capacity(buffer_size, tmpfile.reopen()?);

    let mut buf = vec![0u8; buffer_size];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
    }
    writer.flush()?;

    Ok(tmpfile)
}

fn telemetry_text_len(text: &str) -> usize {
    text.len()
}

fn telemetry_path_len(path: &std::path::Path) -> usize {
    path.display().to_string().len()
}

fn extraction_error_reason_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access denied") {
        "permission_denied"
    } else if lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
    {
        "not_found"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timed_out"
    } else if lower.contains("invalid") || lower.contains("parse") || lower.contains("decode") {
        "invalid_input"
    } else if lower.contains("too large") || lower.contains("limit") {
        "limit_exceeded"
    } else {
        "operation_failed"
    }
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
        let mut config = payload.get("config").cloned().unwrap_or_else(|| json!({}));

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

        // For strategies that benefit from direct filesystem access (video, audio),
        // resolve the on-disk path instead of loading the entire file into memory.
        //
        // Two modes controlled by VIDEO_FILE_ACCESS env var:
        //   "direct" (default): inject the storage path directly into config;
        //     adapter reads from the original file (zero-copy, fastest).
        //   "stream": stream-copy from storage to temp file using a windowed
        //     buffer (VIDEO_STREAM_BUFFER_BYTES); never holds more than the
        //     buffer size in memory. Use when workers need filesystem isolation.
        //
        // Both modes fall back to full download for inline (database) storage.
        let supports_path_access = matches!(
            strategy,
            ExtractionStrategy::VideoMultimodal | ExtractionStrategy::AudioTranscribe
        );
        let file_access_mode = matric_core::defaults::video_file_access_mode();

        // _stream_tmpfile keeps the temp file alive for the duration of the job.
        // Dropped at end of scope, cleaning up the temp file automatically.
        let mut _stream_tmpfile: Option<NamedTempFile> = None;

        // Get data: prefer attachment_id (fetch from file storage), fall back to inline data
        let data = if let Some(att_id) = attachment_id {
            let file_storage = match self.db.file_storage.as_ref() {
                Some(fs) => fs,
                None => return JobResult::Failed("File storage not configured".into()),
            };

            // Try path-based access for supported strategies
            if supports_path_access {
                let mut tx = match schema_ctx.begin_tx().await {
                    Ok(t) => t,
                    Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
                };
                let path_result = file_storage.get_file_metadata_tx(&mut tx, att_id).await;
                if let Err(e) = tx.commit().await {
                    return JobResult::Failed(format!("Commit failed: {}", e));
                }
                match path_result {
                    Ok(info) => {
                        if let matric_db::FileSource::Filesystem(ref storage_path) = info.source {
                            if let Some(fs_path) = file_storage.resolve_storage_path(storage_path) {
                                let source_path = if file_access_mode
                                    == matric_core::defaults::VIDEO_FILE_ACCESS_DIRECT
                                {
                                    // Direct mode: use the storage path as-is
                                    debug!(
                                        mode = "direct",
                                        path = %fs_path.display(),
                                        "Using direct file access for extraction"
                                    );
                                    fs_path.to_string_lossy().to_string()
                                } else {
                                    // Stream mode: windowed copy to temp file
                                    let buffer_size =
                                        matric_core::defaults::video_stream_buffer_bytes();
                                    debug!(
                                        mode = "stream",
                                        buffer_bytes = buffer_size,
                                        source = %fs_path.display(),
                                        "Streaming file to temp location for extraction"
                                    );
                                    match stream_copy_to_temp(&fs_path, buffer_size) {
                                        Ok(tmpfile) => {
                                            let path = tmpfile.path().to_string_lossy().to_string();
                                            _stream_tmpfile = Some(tmpfile);
                                            path
                                        }
                                        Err(e) => {
                                            return JobResult::Failed(format!(
                                                "Stream copy failed for {}: {}",
                                                fs_path.display(),
                                                e
                                            ));
                                        }
                                    }
                                };

                                // Inject path into config for the adapter
                                if let Some(obj) = config.as_object_mut() {
                                    obj.insert("_source_path".to_string(), json!(source_path));
                                }
                                Vec::new() // Empty data — adapter uses _source_path
                            } else {
                                // Path couldn't be resolved: download into memory
                                let mut tx2 = match schema_ctx.begin_tx().await {
                                    Ok(t) => t,
                                    Err(e) => {
                                        return JobResult::Failed(format!(
                                            "Schema tx failed: {}",
                                            e
                                        ))
                                    }
                                };
                                let result = file_storage.download_file_tx(&mut tx2, att_id).await;
                                if let Err(e) = tx2.commit().await {
                                    return JobResult::Failed(format!("Commit failed: {}", e));
                                }
                                match result {
                                    Ok((d, _, _)) => d,
                                    Err(e) => {
                                        return JobResult::Failed(format!(
                                            "Failed to download attachment {}: {}",
                                            att_id, e
                                        ))
                                    }
                                }
                            }
                        } else {
                            // Inline storage: extract the data directly
                            match info.source {
                                matric_db::FileSource::Inline(d) => d,
                                _ => unreachable!(),
                            }
                        }
                    }
                    Err(e) => {
                        return JobResult::Failed(format!(
                            "Failed to get attachment metadata {}: {}",
                            att_id, e
                        ))
                    }
                }
            } else {
                // Non-path strategies: download into memory as before
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
            }
        } else if let Some(data_str) = payload.get("data").and_then(|v| v.as_str()) {
            data_str.as_bytes().to_vec()
        } else {
            return JobResult::Failed(
                "No data provided (expected 'attachment_id' or 'data' field)".into(),
            );
        };

        // Checkpoint/resume: for video extraction, query existing keyframe derived
        // attachments. If any exist (from a previous partial run), inject their frame
        // indices so the adapter can skip already-completed frames.
        if matches!(strategy, ExtractionStrategy::VideoMultimodal) {
            if let Some(att_id) = attachment_id {
                if let Some(file_storage) = self.db.file_storage.as_ref() {
                    if let Ok(mut tx) = schema_ctx.begin_tx().await {
                        match file_storage
                            .list_derived_by_type_tx(&mut tx, att_id, "keyframe")
                            .await
                        {
                            Ok(existing) if !existing.is_empty() => {
                                // Extract frame indices from existing keyframes' metadata
                                let completed_indices: Vec<JsonValue> = existing
                                    .iter()
                                    .filter_map(|att| {
                                        att.extracted_metadata
                                            .as_ref()
                                            .and_then(|m| m.get("frame_index").cloned())
                                    })
                                    .collect();
                                if !completed_indices.is_empty() {
                                    debug!(
                                        attachment_id = %att_id,
                                        completed = completed_indices.len(),
                                        "Checkpoint: found existing keyframes, injecting skip list"
                                    );
                                    if let Some(obj) = config.as_object_mut() {
                                        obj.insert(
                                            "_checkpoint".to_string(),
                                            json!({ "completed_frames": completed_indices }),
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                        let _ = tx.commit().await;
                    }
                }
            }
        }

        // Inject _skip_vision for VideoMultimodal and Glb3DModel: defer vision
        // LLM calls to atomic per-item vision jobs instead of running them inline.
        // - VideoMultimodal → KeyframeVision jobs (#526)
        // - Glb3DModel → ViewVision jobs (#533)
        if matches!(
            strategy,
            ExtractionStrategy::VideoMultimodal | ExtractionStrategy::Glb3DModel
        ) {
            if let Some(obj) = config.as_object_mut() {
                obj.insert("_skip_vision".to_string(), json!(true));
            }
            debug!(
                strategy = ?strategy,
                filename,
                "_skip_vision injected — vision LLM calls deferred to atomic jobs"
            );
        }

        // Inject _skip_transcription for VideoMultimodal: defer Whisper transcription
        // to an atomic AudioTranscription job for fan-in coordination with keyframes. (#542)
        if matches!(strategy, ExtractionStrategy::VideoMultimodal) {
            if let Some(obj) = config.as_object_mut() {
                obj.insert("_skip_transcription".to_string(), json!(true));
            }
            debug!(
                strategy = ?strategy,
                filename,
                "_skip_transcription injected — transcription deferred to AudioTranscription job"
            );
        }

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
                                // When the adapter didn't produce extracted_text but
                                // did produce an ai_description, use the description
                                // as extracted_text so it's indexed for FTS search.
                                let effective_text = result
                                    .extracted_text
                                    .as_deref()
                                    .or(result.ai_description.as_deref());

                                if let Err(e) = file_storage
                                    .update_extracted_content_tx(
                                        &mut tx,
                                        att_id,
                                        effective_text,
                                        Some(result.metadata.clone()),
                                    )
                                    .await
                                {
                                    let error_text = e.to_string();
                                    error!(
                                        attachment_present = true,
                                        error_len = telemetry_text_len(&error_text),
                                        error_reason = extraction_error_reason_code(&error_text),
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
                                        let error_text = e.to_string();
                                        error!(
                                            attachment_present = true,
                                            error_len = telemetry_text_len(&error_text),
                                            error_reason =
                                                extraction_error_reason_code(&error_text),
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
                                    let error_text = e.to_string();
                                    error!(
                                        attachment_present = true,
                                        error_len = telemetry_text_len(&error_text),
                                        error_reason = extraction_error_reason_code(&error_text),
                                        "Failed to update attachment status"
                                    );
                                }

                                if let Err(e) = tx.commit().await {
                                    let error_text = e.to_string();
                                    error!(
                                        attachment_present = true,
                                        error_len = telemetry_text_len(&error_text),
                                        error_reason = extraction_error_reason_code(&error_text),
                                        "Failed to commit extraction results"
                                    );
                                }
                            }
                            Err(e) => {
                                let error_text = e.to_string();
                                error!(
                                    attachment_present = true,
                                    error_len = telemetry_text_len(&error_text),
                                    error_reason = extraction_error_reason_code(&error_text),
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
                                                        let error_text = e.to_string();
                                                        warn!(
                                                            attachment_present = true,
                                                            error_len = telemetry_text_len(&error_text),
                                                            error_reason = extraction_error_reason_code(&error_text),
                                                            "Failed to copy faststart-optimized file"
                                                        );
                                                    } else {
                                                        info!(
                                                            attachment_present = true,
                                                            "MP4 faststart optimization applied"
                                                        );
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
                            let thumb_filename = format!("{}_thumbnail.png", att_id);
                            match file_storage
                                .store_derived_attachment_tx(
                                    &mut tx,
                                    note_id,
                                    att_id,
                                    &thumb_filename,
                                    "image/png",
                                    thumbnail_bytes,
                                    "thumbnail",
                                )
                                .await
                            {
                                Ok(_thumb_att) => {
                                    // Mark parent as having a preview
                                    if let Err(e) =
                                        file_storage.set_has_preview_tx(&mut tx, att_id, true).await
                                    {
                                        let error_text = e.to_string();
                                        warn!(
                                            attachment_present = true,
                                            note_present = true,
                                            error_len = telemetry_text_len(&error_text),
                                            error_reason =
                                                extraction_error_reason_code(&error_text),
                                            "Failed to set has_preview on parent"
                                        );
                                    }
                                    if let Err(e) = tx.commit().await {
                                        let error_text = e.to_string();
                                        error!(
                                            attachment_present = true,
                                            note_present = true,
                                            error_len = telemetry_text_len(&error_text),
                                            error_reason =
                                                extraction_error_reason_code(&error_text),
                                            "Failed to commit thumbnail"
                                        );
                                    } else {
                                        info!(
                                            parent_attachment_present = true,
                                            thumbnail_attachment_present = true,
                                            "Thumbnail persisted as derived attachment"
                                        );
                                    }
                                }
                                Err(e) => {
                                    let error_text = e.to_string();
                                    warn!(
                                        attachment_present = true,
                                        note_present = true,
                                        error_len = telemetry_text_len(&error_text),
                                        error_reason = extraction_error_reason_code(&error_text),
                                        "Failed to store thumbnail attachment"
                                    );
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
                                    // Plain text for all caption files (shared for extracted_text)
                                    let plain_text: String = caption_segments
                                        .iter()
                                        .map(|s| s.text.trim().to_string())
                                        .collect::<Vec<_>>()
                                        .join("\n");

                                    // VTT file
                                    let vtt =
                                        matric_core::captions::render_webvtt(&caption_segments);
                                    match file_storage
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
                                        Ok(child) => {
                                            let _ = file_storage
                                                .update_extracted_content_tx(
                                                    &mut tx,
                                                    child.id,
                                                    Some(&plain_text),
                                                    None,
                                                )
                                                .await;
                                        }
                                        Err(e) => {
                                            let error_text = e.to_string();
                                            warn!(
                                                attachment_present = true,
                                                note_present = true,
                                                error_len = telemetry_text_len(&error_text),
                                                error_reason =
                                                    extraction_error_reason_code(&error_text),
                                                "Failed to store VTT attachment"
                                            )
                                        }
                                    }

                                    // SRT file
                                    let srt = matric_core::captions::render_srt(&caption_segments);
                                    match file_storage
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
                                        Ok(child) => {
                                            let _ = file_storage
                                                .update_extracted_content_tx(
                                                    &mut tx,
                                                    child.id,
                                                    Some(&plain_text),
                                                    None,
                                                )
                                                .await;
                                        }
                                        Err(e) => {
                                            let error_text = e.to_string();
                                            warn!(
                                                attachment_present = true,
                                                note_present = true,
                                                error_len = telemetry_text_len(&error_text),
                                                error_reason =
                                                    extraction_error_reason_code(&error_text),
                                                "Failed to store SRT attachment"
                                            )
                                        }
                                    }

                                    // Plain text transcript
                                    match file_storage
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
                                        Ok(child) => {
                                            let _ = file_storage
                                                .update_extracted_content_tx(
                                                    &mut tx,
                                                    child.id,
                                                    Some(&plain_text),
                                                    None,
                                                )
                                                .await;
                                        }
                                        Err(e) => {
                                            let error_text = e.to_string();
                                            warn!(
                                                attachment_present = true,
                                                note_present = true,
                                                error_len = telemetry_text_len(&error_text),
                                                error_reason =
                                                    extraction_error_reason_code(&error_text),
                                                "Failed to store transcript attachment"
                                            )
                                        }
                                    }

                                    if let Err(e) = tx.commit().await {
                                        let error_text = e.to_string();
                                        error!(
                                            attachment_present = true,
                                            note_present = true,
                                            error_len = telemetry_text_len(&error_text),
                                            error_reason =
                                                extraction_error_reason_code(&error_text),
                                            "Failed to commit transcript attachments"
                                        );
                                    } else {
                                        info!(
                                            parent_attachment_present = true,
                                            note_present = true,
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
                // Track audio_track attachment ID across derived file persistence
                // and downstream job queuing (#542).
                let mut audio_track_attachment_id: Option<Uuid> = None;

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
                                    // Resolve file data: use source_path if data is empty
                                    let file_data = if df.data.is_empty() {
                                        if let Some(ref path) = df.source_path {
                                            match std::fs::read(path) {
                                                Ok(d) => d,
                                                Err(e) => {
                                                    let error_text = e.to_string();
                                                    warn!(
                                                        error_len = telemetry_text_len(&error_text),
                                                        error_reason = extraction_error_reason_code(&error_text),
                                                        source_path_len = telemetry_path_len(path),
                                                        filename_len = telemetry_text_len(&df.filename),
                                                        "Failed to read derived file from source_path"
                                                    );
                                                    continue;
                                                }
                                            }
                                        } else {
                                            // Empty data and no source_path — skip
                                            warn!(
                                                filename_len = telemetry_text_len(&df.filename),
                                                "Derived file has empty data and no source_path"
                                            );
                                            continue;
                                        }
                                    } else {
                                        df.data.clone()
                                    };

                                    match file_storage
                                        .store_derived_attachment_tx(
                                            &mut tx,
                                            note_id,
                                            att_id,
                                            &df.filename,
                                            &df.content_type,
                                            &file_data,
                                            &df.derivation_type,
                                        )
                                        .await
                                    {
                                        Ok(child_att) => {
                                            stored += 1;

                                            // Track audio_track attachment ID for AudioTranscription job (#542)
                                            if df.derivation_type == "audio_track" {
                                                audio_track_attachment_id = Some(child_att.id);
                                            }

                                            // Merge DerivedFile.metadata into extracted_metadata
                                            if let Some(ref extra_meta) = df.metadata {
                                                if let Err(e) = file_storage
                                                    .merge_extracted_metadata_tx(
                                                        &mut tx,
                                                        child_att.id,
                                                        extra_meta,
                                                    )
                                                    .await
                                                {
                                                    let error_text = e.to_string();
                                                    warn!(
                                                        child_attachment_present = true,
                                                        error_len = telemetry_text_len(&error_text),
                                                        error_reason = extraction_error_reason_code(
                                                            &error_text
                                                        ),
                                                        "Failed to merge derived file metadata"
                                                    );
                                                }
                                            }

                                            // Persist AI description and extracted_text
                                            if let Some(ref desc) = df.ai_description {
                                                if let Err(e) = file_storage
                                                    .update_ai_description_tx(
                                                        &mut tx,
                                                        child_att.id,
                                                        desc,
                                                        None,
                                                    )
                                                    .await
                                                {
                                                    let error_text = e.to_string();
                                                    warn!(
                                                        child_attachment_present = true,
                                                        error_len = telemetry_text_len(&error_text),
                                                        error_reason = extraction_error_reason_code(&error_text),
                                                        "Failed to persist derived file ai_description"
                                                    );
                                                }
                                                // Also set extracted_text for FTS indexing
                                                let _ = file_storage
                                                    .update_extracted_content_tx(
                                                        &mut tx,
                                                        child_att.id,
                                                        Some(desc),
                                                        None,
                                                    )
                                                    .await;
                                            }
                                        }
                                        Err(e) => {
                                            let error_text = e.to_string();
                                            warn!(
                                                error_len = telemetry_text_len(&error_text),
                                                error_reason =
                                                    extraction_error_reason_code(&error_text),
                                                filename_len = telemetry_text_len(&df.filename),
                                                "Failed to store derived file"
                                            );
                                        }
                                    }
                                }
                                if let Err(e) = tx.commit().await {
                                    let error_text = e.to_string();
                                    error!(
                                        attachment_present = true,
                                        stored_count = stored,
                                        error_len = telemetry_text_len(&error_text),
                                        error_reason = extraction_error_reason_code(&error_text),
                                        "Failed to commit derived files"
                                    );
                                } else if stored > 0 {
                                    info!(
                                        parent_attachment_present = true,
                                        count = stored,
                                        "Derived files persisted as child attachments"
                                    );
                                }
                            }
                        }
                    }
                }

                ctx.report_progress(85, Some("Results persisted"));

                // Queue downstream jobs for audio/video content.
                let has_transcript_segments = result
                    .metadata
                    .get("transcript_segments")
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                let diarization_available =
                    std::env::var(matric_core::defaults::ENV_DIARIZATION_BASE_URL)
                        .ok()
                        .filter(|s| !s.is_empty())
                        .is_some();

                if let (Some(att_id), Some(note_id)) = (attachment_id, ctx.note_id()) {
                    // VideoMultimodal: queue AudioTranscription job instead of inline diarization.
                    // AudioTranscriptionHandler will transcribe, persist captions, queue diarization,
                    // and participate in fan-in with keyframes. (#542)
                    if matches!(strategy, ExtractionStrategy::VideoMultimodal) {
                        let has_audio = result
                            .metadata
                            .get("has_audio")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        if has_audio {
                            if let Some(audio_att_id) = audio_track_attachment_id {
                                let mut at_payload = serde_json::Map::new();
                                at_payload.insert(
                                    "parent_attachment_id".into(),
                                    json!(att_id.to_string()),
                                );
                                at_payload.insert(
                                    "audio_attachment_id".into(),
                                    json!(audio_att_id.to_string()),
                                );
                                at_payload.insert("is_video".into(), json!(true));
                                if schema != "public" {
                                    at_payload.insert("schema".into(), json!(&schema));
                                }
                                match self
                                    .db
                                    .jobs
                                    .queue_deduplicated(
                                        Some(note_id),
                                        JobType::AudioTranscription,
                                        JobType::AudioTranscription.default_priority(),
                                        Some(serde_json::Value::Object(at_payload)),
                                        JobType::AudioTranscription.default_cost_tier(),
                                    )
                                    .await
                                {
                                    Ok(Some(job_id)) => {
                                        ctx.emit_job_queued(
                                            job_id,
                                            JobType::AudioTranscription,
                                            Some(note_id),
                                        );
                                        info!(
                                            note_id = %note_id,
                                            attachment_id = %att_id,
                                            audio_attachment = %audio_att_id,
                                            "AudioTranscription job queued for video"
                                        );
                                    }
                                    Ok(None) => {} // Deduplicated
                                    Err(e) => {
                                        warn!(
                                            note_id = %note_id,
                                            error = %e,
                                            "Failed to queue AudioTranscription job"
                                        );
                                    }
                                }
                            } else {
                                warn!(
                                    note_id = %note_id,
                                    attachment_id = %att_id,
                                    "Video has audio but audio_track attachment ID not found — \
                                     cannot queue AudioTranscription"
                                );
                            }
                        } else {
                            // Video without audio: set transcript_complete=true so fan-in
                            // isn't blocked waiting for transcription that will never come.
                            if let Some(fs) = self.db.file_storage.as_ref() {
                                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                    let _ = fs
                                        .merge_extracted_metadata_tx(
                                            &mut tx,
                                            att_id,
                                            &json!({"transcript_complete": true}),
                                        )
                                        .await;
                                    let _ = tx.commit().await;
                                }
                            }
                            debug!(
                                note_id = %note_id,
                                attachment_id = %att_id,
                                "Video has no audio — transcript_complete set to true for fan-in"
                            );
                        }
                    } else if matches!(strategy, ExtractionStrategy::AudioTranscribe) {
                        // Standalone audio: queue diarization inline (transcription already done)
                        if has_transcript_segments && diarization_available {
                            let mut diar_payload = serde_json::Map::new();
                            diar_payload
                                .insert("attachment_id".to_string(), json!(att_id.to_string()));
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
                        } else if !diarization_available {
                            info!(
                                note_id = %note_id,
                                attachment_id = %att_id,
                                strategy = ?strategy,
                                "Diarization skipped: DIARIZATION_BASE_URL not set"
                            );
                        } else if !has_transcript_segments {
                            info!(
                                note_id = %note_id,
                                attachment_id = %att_id,
                                strategy = ?strategy,
                                "Diarization skipped: no transcript segments"
                            );
                        }
                    }

                    // Queue thumbnail sprite sheet generation for video extraction (#525).
                    // Only for VideoMultimodal strategy when keyframes were persisted.
                    let has_keyframes = result
                        .derived_files
                        .iter()
                        .any(|f| f.derivation_type == "keyframe");

                    if matches!(strategy, ExtractionStrategy::VideoMultimodal) && has_keyframes {
                        let mut sprite_payload = serde_json::Map::new();
                        sprite_payload
                            .insert("attachment_id".to_string(), json!(att_id.to_string()));
                        if schema != "public" {
                            sprite_payload.insert("schema".to_string(), json!(&schema));
                        }
                        match self
                            .db
                            .jobs
                            .queue_deduplicated(
                                Some(note_id),
                                JobType::ThumbnailSprite,
                                JobType::ThumbnailSprite.default_priority(),
                                Some(serde_json::Value::Object(sprite_payload)),
                                JobType::ThumbnailSprite.default_cost_tier(),
                            )
                            .await
                        {
                            Ok(Some(job_id)) => {
                                ctx.emit_job_queued(
                                    job_id,
                                    JobType::ThumbnailSprite,
                                    Some(note_id),
                                );
                                info!(
                                    note_id = %note_id,
                                    attachment_id = %att_id,
                                    "Thumbnail sprite job queued"
                                );
                            }
                            Ok(None) => {} // Deduplicated
                            Err(e) => {
                                warn!(
                                    note_id = %note_id,
                                    error = %e,
                                    "Failed to queue thumbnail sprite job"
                                );
                            }
                        }

                        // Queue atomic KeyframeVision jobs — one per keyframe (#526).
                        // Each job describes a single frame via vision LLM. The last
                        // to complete triggers KeyframeAssembly for markdown rebuild.
                        if let Some(fs) = self.db.file_storage.as_ref() {
                            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                let keyframes: Vec<matric_core::Attachment> = fs
                                    .list_derived_by_type_tx(&mut tx, att_id, "keyframe")
                                    .await
                                    .unwrap_or_default();
                                let _ = tx.commit().await;

                                let total_frames = keyframes.len();
                                if total_frames == 0 {
                                    warn!(
                                        note_id = %note_id,
                                        attachment_id = %att_id,
                                        in_memory_keyframes = has_keyframes,
                                        "No keyframe attachments found in DB — \
                                         KeyframeVision jobs will not be queued. \
                                         This may indicate derived files were not \
                                         persisted before this check ran."
                                    );
                                }
                                if total_frames > 0 {
                                    // Read vision_mode from config (#550)
                                    let vision_mode = config
                                        .get("vision_mode")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("standard");
                                    let expected_vision_passes: u64 =
                                        if vision_mode == "full" { 3 } else { 1 };

                                    // Store expected count + vision passes in parent metadata for fan-in
                                    if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                        let _ = fs
                                            .merge_extracted_metadata_tx(
                                                &mut tx,
                                                att_id,
                                                &json!({
                                                    "expected_frame_count": total_frames,
                                                    "expected_vision_passes": expected_vision_passes,
                                                }),
                                            )
                                            .await;
                                        let _ = tx.commit().await;
                                    }

                                    let mut queued = 0usize;
                                    for kf in &keyframes {
                                        let frame_index: u64 = kf
                                            .extracted_metadata
                                            .as_ref()
                                            .and_then(|m| m.get("frame_index"))
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        let timestamp_secs: f64 = kf
                                            .extracted_metadata
                                            .as_ref()
                                            .and_then(|m| m.get("timestamp_secs"))
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0);

                                        let mut vision_payload = serde_json::Map::new();
                                        vision_payload.insert(
                                            "parent_attachment_id".into(),
                                            json!(att_id.to_string()),
                                        );
                                        vision_payload.insert(
                                            "keyframe_attachment_id".into(),
                                            json!(kf.id.to_string()),
                                        );
                                        vision_payload
                                            .insert("frame_index".into(), json!(frame_index));
                                        vision_payload
                                            .insert("timestamp_secs".into(), json!(timestamp_secs));
                                        vision_payload
                                            .insert("total_frames".into(), json!(total_frames));
                                        if schema != "public" {
                                            vision_payload.insert("schema".into(), json!(&schema));
                                        }

                                        // Use queue() not queue_deduplicated() — each frame
                                        // is a distinct job sharing the same (note_id, job_type).
                                        match self
                                            .db
                                            .jobs
                                            .queue(
                                                Some(note_id),
                                                JobType::KeyframeVision,
                                                JobType::KeyframeVision.default_priority(),
                                                Some(serde_json::Value::Object(
                                                    vision_payload.clone(),
                                                )),
                                                JobType::KeyframeVision.default_cost_tier(),
                                            )
                                            .await
                                        {
                                            Ok(job_id) => {
                                                ctx.emit_job_queued(
                                                    job_id,
                                                    JobType::KeyframeVision,
                                                    Some(note_id),
                                                );
                                                queued += 1;
                                            }
                                            Err(e) => {
                                                warn!(
                                                    note_id = %note_id,
                                                    frame_index,
                                                    error = %e,
                                                    "Failed to queue KeyframeVision job"
                                                );
                                            }
                                        }

                                        // Queue character + setting vision jobs if full mode (#550)
                                        if vision_mode == "full" {
                                            if let Ok(job_id) = self
                                                .db
                                                .jobs
                                                .queue(
                                                    Some(note_id),
                                                    JobType::KeyframeCharacterVision,
                                                    JobType::KeyframeCharacterVision
                                                        .default_priority(),
                                                    Some(serde_json::Value::Object(
                                                        vision_payload.clone(),
                                                    )),
                                                    JobType::KeyframeCharacterVision
                                                        .default_cost_tier(),
                                                )
                                                .await
                                            {
                                                ctx.emit_job_queued(
                                                    job_id,
                                                    JobType::KeyframeCharacterVision,
                                                    Some(note_id),
                                                );
                                                queued += 1;
                                            }

                                            if let Ok(job_id) = self
                                                .db
                                                .jobs
                                                .queue(
                                                    Some(note_id),
                                                    JobType::KeyframeSettingVision,
                                                    JobType::KeyframeSettingVision
                                                        .default_priority(),
                                                    Some(serde_json::Value::Object(vision_payload)),
                                                    JobType::KeyframeSettingVision
                                                        .default_cost_tier(),
                                                )
                                                .await
                                            {
                                                ctx.emit_job_queued(
                                                    job_id,
                                                    JobType::KeyframeSettingVision,
                                                    Some(note_id),
                                                );
                                                queued += 1;
                                            }
                                        }
                                    }

                                    if vision_mode == "full" {
                                        info!(
                                            note_id = %note_id,
                                            attachment_id = %att_id,
                                            total_frames,
                                            queued,
                                            "Queued {} vision jobs (full mode: scene + character + setting)",
                                            queued
                                        );
                                    } else {
                                        info!(
                                            note_id = %note_id,
                                            attachment_id = %att_id,
                                            total_frames,
                                            queued,
                                            "Queued {} KeyframeVision jobs",
                                            queued
                                        );
                                    }
                                }
                            }
                        } // if let Some(fs)
                    } else if matches!(strategy, ExtractionStrategy::VideoMultimodal) {
                        info!(
                            note_id = %note_id,
                            attachment_id = %att_id,
                            derived_count = result.derived_files.len(),
                            has_keyframes,
                            "VideoMultimodal extraction produced no keyframe derived files — \
                             KeyframeVision and ThumbnailSprite jobs will not be queued"
                        );
                    }

                    // Queue atomic ViewVision jobs for 3D model views (#533).
                    // Mirrors the KeyframeVision fan-out pattern: one job per rendered view,
                    // last to complete triggers ViewAssembly for composite description.
                    let has_3d_views = result
                        .derived_files
                        .iter()
                        .any(|f| f.derivation_type == "3d_rendering");

                    if matches!(strategy, ExtractionStrategy::Glb3DModel) && has_3d_views {
                        if let Some(fs) = self.db.file_storage.as_ref() {
                            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                let views: Vec<matric_core::Attachment> = fs
                                    .list_derived_by_type_tx(&mut tx, att_id, "3d_rendering")
                                    .await
                                    .unwrap_or_default();
                                let _ = tx.commit().await;

                                let total_views = views.len();
                                if total_views == 0 {
                                    warn!(
                                        note_id = %note_id,
                                        attachment_id = %att_id,
                                        in_memory_views = has_3d_views,
                                        "No 3d_rendering attachments found in DB — \
                                         ViewVision jobs will not be queued. \
                                         This may indicate derived files were not \
                                         persisted before this check ran."
                                    );
                                }
                                if total_views > 0 {
                                    // Store expected count in parent metadata for fan-in
                                    if let Ok(mut tx) = schema_ctx.begin_tx().await {
                                        let _ = fs
                                            .merge_extracted_metadata_tx(
                                                &mut tx,
                                                att_id,
                                                &json!({"expected_view_count": total_views}),
                                            )
                                            .await;
                                        let _ = tx.commit().await;
                                    }

                                    // Get filename from parent for view prompts
                                    let parent_filename = result
                                        .metadata
                                        .get("filename")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("model.glb");

                                    let mut queued = 0usize;
                                    for view_att in &views {
                                        let meta = view_att
                                            .extracted_metadata
                                            .as_ref()
                                            .cloned()
                                            .unwrap_or(json!({}));
                                        let view_index = meta
                                            .get("view_index")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        let angle_degrees = meta
                                            .get("angle_degrees")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0);
                                        let elevation = meta
                                            .get("elevation")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");

                                        let mut vision_payload = serde_json::Map::new();
                                        vision_payload.insert(
                                            "parent_attachment_id".into(),
                                            json!(att_id.to_string()),
                                        );
                                        vision_payload.insert(
                                            "view_attachment_id".into(),
                                            json!(view_att.id.to_string()),
                                        );
                                        vision_payload
                                            .insert("view_index".into(), json!(view_index));
                                        vision_payload
                                            .insert("angle_degrees".into(), json!(angle_degrees));
                                        vision_payload.insert("elevation".into(), json!(elevation));
                                        vision_payload
                                            .insert("total_views".into(), json!(total_views));
                                        vision_payload
                                            .insert("filename".into(), json!(parent_filename));
                                        if schema != "public" {
                                            vision_payload.insert("schema".into(), json!(&schema));
                                        }

                                        match self
                                            .db
                                            .jobs
                                            .queue(
                                                Some(note_id),
                                                JobType::ViewVision,
                                                JobType::ViewVision.default_priority(),
                                                Some(serde_json::Value::Object(vision_payload)),
                                                JobType::ViewVision.default_cost_tier(),
                                            )
                                            .await
                                        {
                                            Ok(job_id) => {
                                                ctx.emit_job_queued(
                                                    job_id,
                                                    JobType::ViewVision,
                                                    Some(note_id),
                                                );
                                                queued += 1;
                                            }
                                            Err(e) => {
                                                warn!(
                                                    note_id = %note_id,
                                                    view_index,
                                                    error = %e,
                                                    "Failed to queue ViewVision job"
                                                );
                                            }
                                        }
                                    }

                                    info!(
                                        note_id = %note_id,
                                        attachment_id = %att_id,
                                        total_views,
                                        queued,
                                        "Queued {} ViewVision jobs",
                                        queued
                                    );
                                }
                            }
                        }
                    } else if matches!(strategy, ExtractionStrategy::Glb3DModel) && !has_3d_views {
                        info!(
                            note_id = %note_id,
                            attachment_id = %att_id,
                            derived_count = result.derived_files.len(),
                            "Glb3DModel extraction produced no 3d_rendering derived files — \
                             ViewVision jobs will not be queued"
                        );
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
                    //
                    // Skip for fan-out strategies (Glb3DModel, VideoMultimodal):
                    // their assembly handlers (ViewAssembly, KeyframeAssembly) queue
                    // downstream NLP after vision analysis completes, producing much
                    // richer content for NER, concept extraction, and embeddings.
                    // Queueing here would race ahead on minimal content, and
                    // queue_deduplicated() would then silently discard the assembly
                    // handler's re-queue attempt. (#534)
                    let uses_fanout = matches!(
                        strategy,
                        ExtractionStrategy::Glb3DModel | ExtractionStrategy::VideoMultimodal
                    );
                    if content_updated && !uses_fanout {
                        // ConceptTagging removed — chained from AiRevision after revision completes.
                        // Embedding + Linking removed — chained from ConceptTagging → RelatedConceptInference.
                        // Pipeline: AiRevision → ConceptTagging → RelatedConceptInference → Embedding → Linking.
                        let downstream_types = [JobType::TitleGeneration];

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
                        // Mark as post_extraction so the handler skips the media-deferral check.
                        let mut revision_payload = serde_json::Map::new();
                        revision_payload.insert("revision_mode".to_string(), json!("standard"));
                        revision_payload.insert("post_extraction".to_string(), json!(true));
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
                    } else if content_updated && uses_fanout {
                        info!(
                            note_id = %note_id,
                            strategy = %strategy,
                            content_len = content.len(),
                            "Propagated extraction content but deferred downstream NLP \
                             to assembly handler (fan-out strategy)"
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
                    filename_len = telemetry_text_len(filename),
                    text_len = result.extracted_text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "Extraction completed successfully"
                );

                ctx.report_progress(100, Some("Done"));
                JobResult::Success(Some(result_json))
            }
            Err(e) => {
                let error_msg = format!("Extraction failed: {}", e);
                let error_text = e.to_string();
                error!(
                    strategy = %strategy,
                    filename_len = telemetry_text_len(filename),
                    error_len = telemetry_text_len(&error_text),
                    error_reason = extraction_error_reason_code(&error_text),
                    "Extraction failed"
                );

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

    #[test]
    fn extraction_error_reason_code_uses_stable_classes() {
        assert_eq!(
            extraction_error_reason_code("permission denied reading /srv/private/input.pdf"),
            "permission_denied"
        );
        assert_eq!(
            extraction_error_reason_code("No such file token=mm_key_secret"),
            "not_found"
        );
        assert_eq!(
            extraction_error_reason_code("decode failed for generated output"),
            "invalid_input"
        );
        assert_eq!(
            extraction_error_reason_code("opaque backend text /srv/private/input.pdf"),
            "operation_failed"
        );
    }

    #[test]
    fn extraction_telemetry_lengths_do_not_render_raw_values() {
        let filename = "secret-customer-token-mm_key_secret.pdf";
        let path =
            std::path::Path::new("/srv/fortemi/private/secret-customer-token-mm_key_secret.pdf");
        let detail = format!(
            "filename_len={}; source_path_len={}; error_reason={}",
            telemetry_text_len(filename),
            telemetry_path_len(path),
            extraction_error_reason_code("permission denied reading secret path")
        );

        assert!(detail.contains("filename_len="));
        assert!(detail.contains("source_path_len="));
        assert!(detail.contains("permission_denied"));
        assert!(!detail.contains("secret-customer-token"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("/srv/fortemi"));
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

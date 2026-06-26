//! AudioTranscriptionHandler — orchestrates audio transcription from a derived attachment.
//!
//! For short audio (below AUDIO_CHUNK_THRESHOLD_SECS): transcribes inline in a
//! single Whisper pass, stores results, and triggers downstream work.
//!
//! For long audio (above threshold): splits into chunks, stores each chunk as a
//! derived attachment, and fans out N AudioChunkTranscription jobs that run
//! independently. The last chunk to finish merges results and triggers downstream.
//!
//! Fan-in: after completing (or after chunk merge), checks if all keyframe
//! descriptions AND transcription are done. When both are met, queues
//! KeyframeAssembly for final markdown rebuild.
//!
//! Downstream: queues SpeakerDiarization if pyannote backend is configured.
//!
//! Issue #542, #543

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::{captions, JobRepository, JobType};
use matric_db::{Database, PgFileStorageRepository, SchemaContext};
use matric_inference::transcription::{TranscriptionBackend, TranscriptionResult};

use crate::handler::{JobContext, JobHandler, JobResult};

/// Extract the target schema from a job's payload.
pub(crate) fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

pub(crate) fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema)
        .map_err(|e| JobResult::Failed(format!("Invalid schema '{}': {}", schema, e)))
}

fn audio_transcription_error_reason_code(error: &str) -> &'static str {
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

// ---------------------------------------------------------------------------
// Shared helpers used by both AudioTranscriptionHandler (short path) and
// AudioChunkTranscriptionHandler (fan-in merge path).
// ---------------------------------------------------------------------------

/// Store transcript segments and metadata in the parent attachment's
/// `extracted_metadata`, then persist VTT/SRT/TXT caption files as derived
/// attachments.
pub(crate) async fn store_transcript_and_captions(
    schema_ctx: &SchemaContext,
    file_storage: &PgFileStorageRepository,
    parent_attachment_id: Uuid,
    note_id: Option<Uuid>,
    transcription: &TranscriptionResult,
) -> Result<(), String> {
    // Build segment JSON
    let segments_json: Vec<serde_json::Value> = transcription
        .segments
        .iter()
        .map(|seg| {
            json!({
                "start_secs": seg.start_secs,
                "end_secs": seg.end_secs,
                "text": seg.text,
            })
        })
        .collect();

    let mut transcript_metadata = json!({
        "transcript_segments": segments_json,
        "transcript_complete": true,
    });
    if let Some(ref lang) = transcription.language {
        transcript_metadata["transcript_language"] = json!(lang);
    }
    if let Some(duration) = transcription.duration_secs {
        transcript_metadata["audio_duration_secs"] = json!(duration);
    }

    // Store in parent metadata
    {
        let mut tx = schema_ctx
            .begin_tx()
            .await
            .map_err(|e| format!("Schema tx failed: {}", e))?;
        file_storage
            .merge_extracted_metadata_tx(&mut tx, parent_attachment_id, &transcript_metadata)
            .await
            .map_err(|e| format!("Failed to store transcript metadata: {}", e))?;
        let _ = tx.commit().await;
    }

    // Persist caption files
    if !transcription.segments.is_empty() {
        if let Some(note_id) = note_id {
            let caption_segments: Vec<captions::CaptionSegment> = transcription
                .segments
                .iter()
                .map(|seg| captions::CaptionSegment {
                    start_secs: seg.start_secs,
                    end_secs: seg.end_secs,
                    text: seg.text.clone(),
                    speaker: seg.speaker_id.clone(),
                })
                .collect();

            let vtt_content = captions::render_webvtt(&caption_segments);
            let srt_content = captions::render_srt(&caption_segments);
            let txt_content = transcription.full_text.clone();

            let caption_files: Vec<(&str, &str, Vec<u8>)> = vec![
                ("transcript.vtt", "text/vtt", vtt_content.into_bytes()),
                (
                    "transcript.srt",
                    "application/x-subrip",
                    srt_content.into_bytes(),
                ),
                ("transcript.txt", "text/plain", txt_content.into_bytes()),
            ];

            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                for (filename, content_type, data) in &caption_files {
                    match file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            parent_attachment_id,
                            filename,
                            content_type,
                            data,
                            "caption",
                        )
                        .await
                    {
                        Ok(_) => {
                            debug!(
                                parent = %parent_attachment_id,
                                filename_len = filename.len(),
                                "Caption file persisted"
                            );
                        }
                        Err(e) => {
                            let error_text = e.to_string();
                            warn!(
                                error_len = error_text.len(),
                                error_reason = audio_transcription_error_reason_code(&error_text),
                                filename_len = filename.len(),
                                "Failed to store caption file"
                            );
                        }
                    }
                }
                let _ = tx.commit().await;
            }
        } else {
            warn!(
                parent = %parent_attachment_id,
                "Cannot persist caption files — no note_id in job context"
            );
        }
    }

    Ok(())
}

/// Queue SpeakerDiarization if pyannote backend is configured.
pub(crate) async fn queue_diarization(
    db: &Database,
    ctx: &JobContext,
    note_id: Uuid,
    parent_attachment_id: Uuid,
    schema: &str,
    has_segments: bool,
) {
    let diarization_available = std::env::var(matric_core::defaults::ENV_DIARIZATION_BASE_URL)
        .ok()
        .filter(|s| !s.is_empty())
        .is_some();

    if diarization_available && has_segments {
        let mut diar_payload = serde_json::Map::new();
        diar_payload.insert(
            "attachment_id".to_string(),
            json!(parent_attachment_id.to_string()),
        );
        if schema != "public" {
            diar_payload.insert("schema".to_string(), json!(schema));
        }
        match db
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
                ctx.emit_job_queued(job_id, JobType::SpeakerDiarization, Some(note_id));
                info!(
                    note_present = true,
                    parent = %parent_attachment_id,
                    "SpeakerDiarization queued"
                );
            }
            Ok(None) => {} // Deduplicated
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    error_len = error_text.len(),
                    error_reason = audio_transcription_error_reason_code(&error_text),
                    "Failed to queue SpeakerDiarization"
                );
            }
        }
    }
}

/// Check if both keyframe descriptions and transcription are complete for a
/// video attachment. If so, queue KeyframeAssembly. Race-safe via
/// `queue_deduplicated`.
pub(crate) async fn check_video_fan_in(
    db: &Database,
    ctx: &JobContext,
    schema_ctx: &SchemaContext,
    file_storage: &PgFileStorageRepository,
    parent_attachment_id: Uuid,
    schema: &str,
) {
    let (expected_frames, transcript_done, expected_passes) = {
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    error_len = error_text.len(),
                    error_reason = audio_transcription_error_reason_code(&error_text),
                    "Fan-in metadata read failed"
                );
                return;
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

        let att_meta = row.and_then(|(em,)| em).unwrap_or(json!({}));
        let expected = att_meta
            .get("expected_frame_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let tc = att_meta
            .get("transcript_complete")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let passes = att_meta
            .get("expected_vision_passes")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as i64;
        (expected, tc, passes)
    };

    if expected_frames == 0 {
        debug!("No keyframes expected — skipping video fan-in check");
        return;
    }

    if !transcript_done {
        debug!("Transcript not yet complete — skipping fan-in");
        return;
    }

    // Choose count method based on expected_vision_passes (#550)
    let vision_ready: bool = {
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(_) => return,
        };
        let ready = if expected_passes >= 3 {
            let fully_analyzed = file_storage
                .count_fully_analyzed_keyframes_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            debug!(
                fully_analyzed,
                expected = expected_frames,
                transcript_done,
                "Video fan-in (full): {}/{} fully analyzed + transcript={}",
                fully_analyzed,
                expected_frames,
                transcript_done
            );
            fully_analyzed >= expected_frames as i64
        } else {
            let described_count = file_storage
                .count_described_keyframes_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            debug!(
                described = described_count,
                expected = expected_frames,
                transcript_done,
                "Video fan-in: {}/{} keyframes + transcript={}",
                described_count,
                expected_frames,
                transcript_done
            );
            described_count >= expected_frames as i64
        };
        let _ = tx.commit().await;
        ready
    };

    if vision_ready {
        let mut assembly_payload = serde_json::Map::new();
        assembly_payload.insert(
            "attachment_id".into(),
            json!(parent_attachment_id.to_string()),
        );
        if schema != "public" {
            assembly_payload.insert("schema".into(), json!(schema));
        }

        match db
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
                    expected_frames, job_id
                );
            }
            Ok(None) => {
                debug!("KeyframeAssembly already queued (deduplicated)");
            }
            Err(e) => {
                let error_text = e.to_string();
                error!(
                    error_len = error_text.len(),
                    error_reason = audio_transcription_error_reason_code(&error_text),
                    "Failed to queue KeyframeAssembly"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AudioTranscriptionHandler
// ---------------------------------------------------------------------------

pub struct AudioTranscriptionHandler {
    db: Database,
    transcription: Arc<dyn TranscriptionBackend>,
}

impl AudioTranscriptionHandler {
    pub fn new(db: Database, transcription: Arc<dyn TranscriptionBackend>) -> Self {
        Self { db, transcription }
    }
}

#[async_trait]
impl JobHandler for AudioTranscriptionHandler {
    fn job_type(&self) -> JobType {
        JobType::AudioTranscription
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing audio transcription job payload".into()),
        };

        let parent_attachment_id: Uuid = match payload
            .get("parent_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing parent_attachment_id".into()),
        };

        let audio_attachment_id: Uuid = match payload
            .get("audio_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing audio_attachment_id".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(sc) => sc,
            Err(e) => return e,
        };

        let is_video = payload
            .get("is_video")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ctx.report_progress(5, Some("Checking backend availability"));

        match self.transcription.health_check().await {
            Ok(true) => {}
            Ok(false) => {
                return JobResult::Retry("Whisper backend not ready — will retry".into());
            }
            Err(e) => {
                return JobResult::Retry(format!(
                    "Whisper backend health check failed: {} — will retry",
                    e
                ));
            }
        }

        ctx.report_progress(10, Some("Downloading audio file"));

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        let audio_data = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let result = file_storage
                .download_file_tx(&mut tx, audio_attachment_id)
                .await;
            let _ = tx.commit().await;

            match result {
                Ok((data, _, _)) => data,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to download audio attachment {}: {}",
                        audio_attachment_id, e
                    ))
                }
            }
        };

        if audio_data.is_empty() {
            return JobResult::Failed("Audio attachment is empty".into());
        }

        ctx.report_progress(20, Some("Transcoding audio"));

        let work_dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => return JobResult::Failed(format!("Failed to create work dir: {}", e)),
        };

        let input_path = work_dir.path().join("input.wav");
        if let Err(e) = std::fs::write(&input_path, &audio_data) {
            return JobResult::Failed(format!("Failed to write audio temp file: {}", e));
        }
        // Free the in-memory copy now that it's on disk
        drop(audio_data);

        // Transcode to 16kHz mono PCM WAV
        let wav_path = match crate::adapters::audio_util::transcode_to_speech_wav(
            &input_path,
            work_dir.path(),
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    error_len = error_text.len(),
                    error_reason = audio_transcription_error_reason_code(&error_text),
                    "Audio transcode failed, using original input"
                );
                input_path.clone()
            }
        };

        // Probe duration to decide: single-pass vs fan-out
        let chunk_threshold = matric_core::defaults::audio_chunk_threshold_secs();
        let duration = crate::adapters::audio_util::probe_duration(&wav_path)
            .await
            .unwrap_or(0.0);

        if duration > chunk_threshold as f64 {
            // Long audio — fan out to AudioChunkTranscription jobs
            self.fan_out_chunks(
                &ctx,
                &schema_ctx,
                file_storage,
                parent_attachment_id,
                audio_attachment_id,
                &wav_path,
                work_dir.path(),
                duration,
                is_video,
                schema,
            )
            .await
        } else {
            // Short audio — transcribe inline
            self.transcribe_inline(
                &ctx,
                &schema_ctx,
                file_storage,
                parent_attachment_id,
                &wav_path,
                is_video,
                schema,
            )
            .await
        }
    }
}

impl AudioTranscriptionHandler {
    /// Short-audio path: transcribe inline in a single Whisper pass.
    #[allow(clippy::too_many_arguments)]
    async fn transcribe_inline(
        &self,
        ctx: &JobContext,
        schema_ctx: &SchemaContext,
        file_storage: &PgFileStorageRepository,
        parent_attachment_id: Uuid,
        wav_path: &std::path::Path,
        is_video: bool,
        schema: &str,
    ) -> JobResult {
        ctx.report_progress(30, Some("Transcribing audio"));

        let wav_data = match std::fs::read(wav_path) {
            Ok(d) => d,
            Err(e) => return JobResult::Failed(format!("Failed to read WAV: {}", e)),
        };

        let transcription = match self
            .transcription
            .transcribe(&wav_data, "audio/wav", None)
            .await
        {
            Ok(r) => r,
            Err(e) => return JobResult::Retry(format!("Whisper transcription failed: {}", e)),
        };

        let segment_count = transcription.segments.len();
        info!(
            parent = %parent_attachment_id,
            segments = segment_count,
            duration = ?transcription.duration_secs,
            "Audio transcription complete (inline)"
        );

        ctx.report_progress(60, Some("Storing transcript"));

        if let Err(e) = store_transcript_and_captions(
            schema_ctx,
            file_storage,
            parent_attachment_id,
            ctx.note_id(),
            &transcription,
        )
        .await
        {
            return JobResult::Failed(e);
        }

        ctx.report_progress(80, Some("Queuing downstream jobs"));

        if let Some(note_id) = ctx.note_id() {
            queue_diarization(
                &self.db,
                ctx,
                note_id,
                parent_attachment_id,
                schema,
                !transcription.segments.is_empty(),
            )
            .await;
        }

        if is_video {
            check_video_fan_in(
                &self.db,
                ctx,
                schema_ctx,
                file_storage,
                parent_attachment_id,
                schema,
            )
            .await;
        }

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(json!({
            "segment_count": segment_count,
            "duration_secs": transcription.duration_secs,
            "language": transcription.language,
            "transcript_complete": true,
        })))
    }

    /// Long-audio path: split into chunks, store as derived attachments, and
    /// queue N AudioChunkTranscription jobs.
    #[allow(clippy::too_many_arguments)]
    async fn fan_out_chunks(
        &self,
        ctx: &JobContext,
        schema_ctx: &SchemaContext,
        file_storage: &PgFileStorageRepository,
        parent_attachment_id: Uuid,
        audio_attachment_id: Uuid,
        wav_path: &std::path::Path,
        work_dir: &std::path::Path,
        duration: f64,
        is_video: bool,
        schema: &str,
    ) -> JobResult {
        let chunk_duration = matric_core::defaults::audio_chunk_duration_secs();
        let num_chunks = (duration / chunk_duration as f64).ceil() as usize;

        info!(
            parent = %parent_attachment_id,
            duration_secs = duration,
            chunk_duration_secs = chunk_duration,
            num_chunks,
            "Long audio detected — splitting into chunks for fan-out"
        );

        ctx.report_progress(
            30,
            Some(&format!("Splitting audio into {} chunks", num_chunks)),
        );

        let chunks = match crate::adapters::audio_util::split_audio_chunks(
            wav_path,
            work_dir,
            duration,
            chunk_duration,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                return JobResult::Failed(format!("Failed to split audio into chunks: {}", e))
            }
        };

        if chunks.is_empty() {
            return JobResult::Failed("Audio split produced no chunks".into());
        }

        let total_chunks = chunks.len();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => {
                return JobResult::Failed(
                    "Cannot fan out audio chunks — no note_id in job context".into(),
                )
            }
        };

        ctx.report_progress(50, Some("Storing chunk attachments"));

        // Store expected_chunk_count in parent metadata for fan-in
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let _ = file_storage
                .merge_extracted_metadata_tx(
                    &mut tx,
                    parent_attachment_id,
                    &json!({"expected_chunk_count": total_chunks}),
                )
                .await;
            let _ = tx.commit().await;
        }

        // Store each chunk as a derived attachment and queue a job
        let mut queued = 0usize;
        for (i, (offset_secs, chunk_path)) in chunks.iter().enumerate() {
            let chunk_data = match std::fs::read(chunk_path) {
                Ok(d) => d,
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        chunk = i,
                        error_len = error_text.len(),
                        error_reason = audio_transcription_error_reason_code(&error_text),
                        "Failed to read chunk file, skipping"
                    );
                    continue;
                }
            };

            // Store chunk WAV as derived attachment
            let chunk_att_id = {
                let mut tx = match schema_ctx.begin_tx().await {
                    Ok(t) => t,
                    Err(e) => {
                        let error_text = e.to_string();
                        warn!(
                            chunk = i,
                            error_len = error_text.len(),
                            error_reason = audio_transcription_error_reason_code(&error_text),
                            "Schema tx failed for chunk storage"
                        );
                        continue;
                    }
                };
                let result = file_storage
                    .store_derived_attachment_tx(
                        &mut tx,
                        note_id,
                        parent_attachment_id,
                        &format!("chunk_{:04}.wav", i),
                        "audio/wav",
                        &chunk_data,
                        "audio_chunk",
                    )
                    .await;
                let _ = tx.commit().await;
                match result {
                    Ok(att) => att.id,
                    Err(e) => {
                        let error_text = e.to_string();
                        warn!(
                            chunk = i,
                            error_len = error_text.len(),
                            error_reason = audio_transcription_error_reason_code(&error_text),
                            "Failed to store chunk attachment"
                        );
                        continue;
                    }
                }
            };

            // Store chunk metadata
            {
                let mut tx = match schema_ctx.begin_tx().await {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let _ = file_storage
                    .merge_extracted_metadata_tx(
                        &mut tx,
                        chunk_att_id,
                        &json!({
                            "chunk_index": i,
                            "chunk_offset_secs": offset_secs,
                        }),
                    )
                    .await;
                let _ = tx.commit().await;
            }

            // Queue AudioChunkTranscription job
            let chunk_payload = json!({
                "parent_attachment_id": parent_attachment_id.to_string(),
                "audio_attachment_id": audio_attachment_id.to_string(),
                "chunk_attachment_id": chunk_att_id.to_string(),
                "chunk_index": i,
                "chunk_offset_secs": offset_secs,
                "total_chunks": total_chunks,
                "is_video": is_video,
                "schema": schema,
            });

            // Use queue() — each chunk is a distinct job sharing (note_id, job_type)
            match self
                .db
                .jobs
                .queue(
                    Some(note_id),
                    JobType::AudioChunkTranscription,
                    JobType::AudioChunkTranscription.default_priority(),
                    Some(chunk_payload),
                    JobType::AudioChunkTranscription.default_cost_tier(),
                )
                .await
            {
                Ok(job_id) => {
                    ctx.emit_job_queued(job_id, JobType::AudioChunkTranscription, Some(note_id));
                    queued += 1;
                }
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        chunk = i,
                        error_len = error_text.len(),
                        error_reason = audio_transcription_error_reason_code(&error_text),
                        "Failed to queue AudioChunkTranscription job"
                    );
                }
            }
        }

        info!(
            parent = %parent_attachment_id,
            total_chunks,
            queued,
            "Queued {} AudioChunkTranscription jobs",
            queued
        );

        if queued == 0 {
            return JobResult::Failed("Failed to queue any AudioChunkTranscription jobs".into());
        }

        ctx.report_progress(100, Some("Fan-out complete"));
        JobResult::Success(Some(json!({
            "fan_out": true,
            "total_chunks": total_chunks,
            "queued": queued,
            "duration_secs": duration,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_transcription_error_reason_code_uses_stable_classes() {
        assert_eq!(
            audio_transcription_error_reason_code("database sql failed while writing chunk"),
            "database_error"
        );
        assert_eq!(
            audio_transcription_error_reason_code("file storage denied during caption write"),
            "permission_denied"
        );
        assert_eq!(
            audio_transcription_error_reason_code("Cannot connect to transcription queue"),
            "connection_failed"
        );
        assert_eq!(
            audio_transcription_error_reason_code("opaque backend diagnostic text"),
            "operation_failed"
        );
    }
}

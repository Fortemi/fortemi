//! AudioChunkTranscriptionHandler — transcribes a single audio chunk via Whisper.
//!
//! Atomic job type for the audio pipeline: processes one chunk of a split audio
//! file, calls the Whisper backend, adjusts timestamps by chunk offset, and stores
//! results in the chunk attachment's metadata. Each chunk is independent and
//! parallelizable.
//!
//! Fan-in: after completing, checks if all sibling chunks are done. The last to
//! finish merges all chunk transcriptions, stores the final transcript in the
//! parent attachment, persists caption files, and triggers downstream work
//! (diarization, video assembly fan-in).
//!
//! Issue #543

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use matric_core::{Attachment, JobType};
use matric_db::Database;
use matric_inference::transcription::{
    TranscriptionBackend, TranscriptionResult, TranscriptionSegment,
};

use crate::audio_transcription_handler::{
    check_video_fan_in, extract_schema, queue_diarization, schema_context,
    store_transcript_and_captions,
};
use crate::handler::{JobContext, JobHandler, JobResult};

fn audio_chunk_error_reason_code(error: &str) -> &'static str {
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

fn audio_chunk_text_len(text: &str) -> usize {
    text.chars().count()
}

pub struct AudioChunkTranscriptionHandler {
    db: Database,
    transcription: Arc<dyn TranscriptionBackend>,
}

impl AudioChunkTranscriptionHandler {
    pub fn new(db: Database, transcription: Arc<dyn TranscriptionBackend>) -> Self {
        Self { db, transcription }
    }
}

#[async_trait]
impl JobHandler for AudioChunkTranscriptionHandler {
    fn job_type(&self) -> JobType {
        JobType::AudioChunkTranscription
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => {
                return JobResult::Failed("Missing audio chunk transcription job payload".into())
            }
        };

        // Parse payload
        let parent_attachment_id: Uuid = match payload
            .get("parent_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing parent_attachment_id".into()),
        };

        let chunk_attachment_id: Uuid = match payload
            .get("chunk_attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing chunk_attachment_id".into()),
        };

        let chunk_index = payload
            .get("chunk_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let chunk_offset_secs = payload
            .get("chunk_offset_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let total_chunks = payload
            .get("total_chunks")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;

        let is_video = payload
            .get("is_video")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(sc) => sc,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Checking backend availability"));

        // Health check
        match self.transcription.health_check().await {
            Ok(true) => {}
            Ok(false) => {
                return JobResult::Retry("Whisper backend not ready — will retry".into());
            }
            Err(e) => {
                let error_text = e.to_string();
                return JobResult::Retry(format!(
                    "Whisper backend health check failed ({}) — will retry",
                    audio_chunk_error_reason_code(&error_text)
                ));
            }
        }

        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Step 1: Download chunk WAV
        ctx.report_progress(
            10,
            Some(&format!(
                "Downloading chunk {}/{}",
                chunk_index + 1,
                total_chunks
            )),
        );

        let chunk_data = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Schema tx failed ({})",
                        audio_chunk_error_reason_code(&error_text)
                    ));
                }
            };
            let result = file_storage
                .download_file_tx(&mut tx, chunk_attachment_id)
                .await;
            let _ = tx.commit().await;

            match result {
                Ok((data, _, _)) => data,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Failed to download chunk attachment ({})",
                        audio_chunk_error_reason_code(&error_text)
                    ));
                }
            }
        };

        if chunk_data.is_empty() {
            return JobResult::Failed("Chunk attachment is empty".into());
        }

        // Step 2: Transcribe via Whisper (single pass — chunk is sized appropriately)
        ctx.report_progress(
            20,
            Some(&format!(
                "Transcribing chunk {}/{} (offset {:.0}s)",
                chunk_index + 1,
                total_chunks,
                chunk_offset_secs
            )),
        );

        let mut transcription = match self
            .transcription
            .transcribe(&chunk_data, "audio/wav", None)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let error_text = e.to_string();
                return JobResult::Retry(format!(
                    "Whisper transcription failed for chunk {} ({})",
                    chunk_index,
                    audio_chunk_error_reason_code(&error_text)
                ));
            }
        };

        // Step 3: Adjust timestamps by chunk offset
        for seg in &mut transcription.segments {
            seg.start_secs += chunk_offset_secs;
            seg.end_secs += chunk_offset_secs;
            if let Some(ref mut words) = seg.words {
                for w in words.iter_mut() {
                    w.start_secs += chunk_offset_secs;
                    w.end_secs += chunk_offset_secs;
                }
            }
        }

        let segment_count = transcription.segments.len();
        info!(
            chunk = chunk_index,
            total = total_chunks,
            offset = chunk_offset_secs,
            segments = segment_count,
            "Chunk {}/{} transcription complete",
            chunk_index + 1,
            total_chunks
        );

        // Step 4: Store chunk segments in chunk attachment metadata
        ctx.report_progress(60, Some("Storing chunk segments"));

        let chunk_segments_json: Vec<serde_json::Value> = transcription
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

        let chunk_meta = json!({
            "chunk_segments": chunk_segments_json,
            "chunk_offset_secs": chunk_offset_secs,
            "chunk_index": chunk_index,
            "chunk_transcription_complete": true,
            "chunk_segment_count": segment_count,
            "chunk_full_text": transcription.full_text,
            "chunk_language": transcription.language,
            "chunk_duration_secs": transcription.duration_secs,
        });

        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Schema tx failed ({})",
                        audio_chunk_error_reason_code(&error_text)
                    ));
                }
            };
            if let Err(e) = file_storage
                .merge_extracted_metadata_tx(&mut tx, chunk_attachment_id, &chunk_meta)
                .await
            {
                let _ = tx.commit().await;
                let error_text = e.to_string();
                return JobResult::Failed(format!(
                    "Failed to store chunk metadata ({})",
                    audio_chunk_error_reason_code(&error_text)
                ));
            }
            let _ = tx.commit().await;
        }

        // Step 5: Fan-in check — are all sibling chunks transcribed?
        ctx.report_progress(80, Some("Checking chunk fan-in"));

        let completed_count: i64 = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        error_len = error_text.len(),
                        error_reason = audio_chunk_error_reason_code(&error_text),
                        "Fan-in count query failed"
                    );
                    return JobResult::Success(Some(json!({
                        "chunk_index": chunk_index,
                        "segment_count": segment_count,
                    })));
                }
            };
            let count = file_storage
                .count_completed_chunks_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            let _ = tx.commit().await;
            count
        };

        debug!(
            completed = completed_count,
            total = total_chunks,
            "Chunk fan-in: {}/{} chunks complete",
            completed_count,
            total_chunks
        );

        if completed_count >= total_chunks as i64 {
            // All chunks complete — merge and trigger downstream
            ctx.report_progress(85, Some("All chunks complete — merging"));

            match self
                .merge_and_finalize(
                    &ctx,
                    &schema_ctx,
                    file_storage,
                    parent_attachment_id,
                    is_video,
                    schema,
                )
                .await
            {
                Ok(merged_count) => {
                    ctx.report_progress(100, Some("Done (merge complete)"));
                    JobResult::Success(Some(json!({
                        "chunk_index": chunk_index,
                        "segment_count": segment_count,
                        "fan_in_trigger": true,
                        "merged_segment_count": merged_count,
                    })))
                }
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        error_len = error_text.len(),
                        error_reason = audio_chunk_error_reason_code(&error_text),
                        "Merge failed but chunk transcription succeeded"
                    );
                    // The chunk itself succeeded — don't retry. The merge can be
                    // triggered by a re-run or manual assembly job.
                    JobResult::Success(Some(json!({
                        "chunk_index": chunk_index,
                        "segment_count": segment_count,
                        "merge_error_len": audio_chunk_text_len(&error_text),
                        "merge_error_reason": audio_chunk_error_reason_code(&error_text),
                    })))
                }
            }
        } else {
            ctx.report_progress(100, Some("Done"));
            JobResult::Success(Some(json!({
                "chunk_index": chunk_index,
                "segment_count": segment_count,
            })))
        }
    }
}

impl AudioChunkTranscriptionHandler {
    /// Merge all chunk transcriptions, store the final transcript, persist
    /// captions, and trigger downstream work.
    async fn merge_and_finalize(
        &self,
        ctx: &JobContext,
        schema_ctx: &matric_db::SchemaContext,
        file_storage: &matric_db::PgFileStorageRepository,
        parent_attachment_id: Uuid,
        is_video: bool,
        schema: &str,
    ) -> Result<usize, String> {
        // Load all audio_chunk derived attachments
        let chunk_attachments: Vec<Attachment> = {
            let mut tx = schema_ctx.begin_tx().await.map_err(|e| {
                let error_text = e.to_string();
                format!(
                    "Schema tx failed ({})",
                    audio_chunk_error_reason_code(&error_text)
                )
            })?;
            let atts = file_storage
                .list_derived_by_type_tx(&mut tx, parent_attachment_id, "audio_chunk")
                .await
                .map_err(|e| {
                    let error_text = e.to_string();
                    format!(
                        "Failed to list chunk attachments ({})",
                        audio_chunk_error_reason_code(&error_text)
                    )
                })?;
            let _ = tx.commit().await;
            atts
        };

        // Sort by chunk_index and merge segments
        let mut indexed_chunks: Vec<(usize, &Attachment)> = chunk_attachments
            .iter()
            .filter_map(|att| {
                let idx = att
                    .extracted_metadata
                    .as_ref()?
                    .get("chunk_index")?
                    .as_u64()? as usize;
                Some((idx, att))
            })
            .collect();
        indexed_chunks.sort_by_key(|(idx, _)| *idx);

        let mut all_segments: Vec<TranscriptionSegment> = Vec::new();
        let mut all_text_parts: Vec<String> = Vec::new();
        let mut total_duration: f64 = 0.0;
        let mut language: Option<String> = None;

        for (idx, att) in &indexed_chunks {
            let meta = match att.extracted_metadata.as_ref() {
                Some(m) => m,
                None => {
                    warn!(chunk = idx, "Chunk attachment missing metadata, skipping");
                    continue;
                }
            };

            // Extract chunk segments
            if let Some(segs) = meta.get("chunk_segments") {
                if let Ok(parsed) = serde_json::from_value::<Vec<serde_json::Value>>(segs.clone()) {
                    for seg_val in parsed {
                        let start = seg_val
                            .get("start_secs")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let end = seg_val
                            .get("end_secs")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let text = seg_val
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        all_segments.push(TranscriptionSegment {
                            start_secs: start,
                            end_secs: end,
                            text,
                            speaker_id: None,
                            words: None,
                        });
                    }
                }
            }

            if let Some(text) = meta.get("chunk_full_text").and_then(|v| v.as_str()) {
                all_text_parts.push(text.to_string());
            }
            if let Some(dur) = meta.get("chunk_duration_secs").and_then(|v| v.as_f64()) {
                let offset = meta
                    .get("chunk_offset_secs")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let end = offset + dur;
                if end > total_duration {
                    total_duration = end;
                }
            }
            if language.is_none() {
                language = meta
                    .get("chunk_language")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
        }

        let merged = TranscriptionResult {
            full_text: all_text_parts.join(" "),
            segments: all_segments,
            language,
            duration_secs: if total_duration > 0.0 {
                Some(total_duration)
            } else {
                None
            },
        };

        let merged_count = merged.segments.len();
        info!(
            parent_attachment_id_present = true,
            chunks = indexed_chunks.len(),
            segments = merged_count,
            duration = ?merged.duration_secs,
            "Chunk merge complete"
        );

        // Store merged transcript and captions
        store_transcript_and_captions(
            schema_ctx,
            file_storage,
            parent_attachment_id,
            ctx.note_id(),
            &merged,
        )
        .await?;

        // Queue diarization
        if let Some(note_id) = ctx.note_id() {
            queue_diarization(
                &self.db,
                ctx,
                note_id,
                parent_attachment_id,
                schema,
                !merged.segments.is_empty(),
            )
            .await;
        }

        // Video fan-in check
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

        Ok(merged_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_chunk_error_reason_code_uses_stable_classes() {
        assert_eq!(
            audio_chunk_error_reason_code("database sql failed during fan-in"),
            "database_error"
        );
        assert_eq!(
            audio_chunk_error_reason_code("file storage denied during merge"),
            "permission_denied"
        );
        assert_eq!(
            audio_chunk_error_reason_code("Cannot connect to transcription queue"),
            "connection_failed"
        );
        assert_eq!(
            audio_chunk_error_reason_code("opaque backend diagnostic text"),
            "operation_failed"
        );
    }

    #[test]
    fn audio_chunk_runtime_telemetry_helpers_redact_private_values() {
        let raw_error =
            "postgres://user:mm_key_secret@db.internal/app failed at /srv/private/audio.wav";
        let rendered = format!(
            "parent_attachment_id_present=true; error_len={}; error_reason={}; merge_error_len={}; merge_error_reason={}",
            audio_chunk_text_len(raw_error),
            audio_chunk_error_reason_code(raw_error),
            audio_chunk_text_len(raw_error),
            audio_chunk_error_reason_code(raw_error)
        );

        assert!(rendered.contains("parent_attachment_id_present=true"));
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason="));
        assert!(rendered.contains("merge_error_len="));
        assert!(rendered.contains("merge_error_reason="));
        assert!(!rendered.contains("mm_key_secret"));
        assert!(!rendered.contains("postgres://"));
        assert!(!rendered.contains("db.internal"));
        assert!(!rendered.contains("/srv/private"));
    }
}

//! SpeakerDiarizationHandler — runs speaker diarization on audio/video
//! attachments after transcription, aligns speakers with transcript segments,
//! then re-renders caption files (VTT/SRT/TXT) with speaker labels.
//!
//! Queued as a downstream job by ExtractionHandler when transcript segments
//! are present and a diarization backend is available.

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use matric_core::{captions, JobType};
use matric_db::{Database, FileSource, SchemaContext};
use matric_inference::{
    align_speakers, DiarizationBackend, DiarizationResult, TranscriptionSegment,
};

use crate::handler::{JobContext, JobHandler, JobResult};
use crate::relabel_handler::{SpeakerConfig, SpeakerMapping};

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

pub struct SpeakerDiarizationHandler {
    db: Database,
    backend: Arc<dyn DiarizationBackend>,
}

impl SpeakerDiarizationHandler {
    pub fn new(db: Database, backend: Arc<dyn DiarizationBackend>) -> Self {
        Self { db, backend }
    }
}

#[async_trait]
impl JobHandler for SpeakerDiarizationHandler {
    fn job_type(&self) -> JobType {
        JobType::SpeakerDiarization
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing diarization job payload".into()),
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
            None => return JobResult::Failed("Missing note_id for diarization".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Loading attachment metadata"));

        // 1. Fetch the attachment to get transcript segments and file path
        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        let attachment = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let att = match file_storage.get_tx(&mut tx, attachment_id).await {
                Ok(a) => a,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to fetch attachment {}: {}",
                        attachment_id, e
                    ))
                }
            };
            let _ = tx.commit().await;
            att
        };

        // 2. Parse transcript segments from extracted_metadata
        let segments_json = attachment
            .extracted_metadata
            .as_ref()
            .and_then(|m| m.get("transcript_segments"))
            .and_then(|v| v.as_array());

        let mut transcript_segments: Vec<TranscriptionSegment> = match segments_json {
            Some(segs) => segs
                .iter()
                .filter_map(|seg| {
                    let start = seg.get("start_secs")?.as_f64()?;
                    let end = seg.get("end_secs")?.as_f64()?;
                    let text = seg.get("text")?.as_str()?.to_string();
                    let words = seg
                        .get("words")
                        .and_then(|w| serde_json::from_value(w.clone()).ok());
                    Some(TranscriptionSegment {
                        start_secs: start,
                        end_secs: end,
                        text,
                        speaker_id: None,
                        words,
                    })
                })
                .collect(),
            None => {
                return JobResult::Failed(
                    "No transcript segments found in attachment metadata".into(),
                )
            }
        };

        if transcript_segments.is_empty() {
            return JobResult::Failed("Transcript segments are empty".into());
        }

        ctx.report_progress(10, Some("Retrieving audio file"));

        // 3. Get the audio file path for diarization
        let audio_path = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let info = match file_storage
                .get_file_metadata_tx(&mut tx, attachment_id)
                .await
            {
                Ok(i) => i,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to get file metadata for {}: {}",
                        attachment_id, e
                    ))
                }
            };
            let _ = tx.commit().await;

            match info.source {
                FileSource::Filesystem(ref storage_path) => {
                    match file_storage.resolve_storage_path(storage_path) {
                        Some(p) => p,
                        None => {
                            return JobResult::Failed(format!(
                                "Cannot resolve storage path: {}",
                                storage_path
                            ))
                        }
                    }
                }
                FileSource::Inline(_) => {
                    // For inline storage, write to a temp file
                    let data = match file_storage.download_file(attachment_id).await {
                        Ok((data, _, _)) => data,
                        Err(e) => {
                            return JobResult::Failed(format!(
                                "Failed to download attachment: {}",
                                e
                            ))
                        }
                    };
                    let suffix = attachment
                        .filename
                        .rsplit_once('.')
                        .map(|(_, ext)| format!(".{}", ext))
                        .unwrap_or_else(|| ".wav".to_string());
                    let tmp = match tempfile::Builder::new().suffix(&suffix).tempfile() {
                        Ok(t) => t,
                        Err(e) => {
                            return JobResult::Failed(format!("Failed to create temp file: {}", e))
                        }
                    };
                    let path = tmp.path().to_path_buf();
                    if let Err(e) = tokio::fs::write(&path, &data).await {
                        return JobResult::Failed(format!(
                            "Failed to write temp audio file: {}",
                            e
                        ));
                    }
                    // Leak the tempfile so it persists until this handler completes
                    let _ = tmp.into_temp_path();
                    path
                }
            }
        };

        ctx.report_progress(20, Some("Running speaker diarization"));

        // 4. Run diarization
        let min_speakers = payload
            .get("min_speakers")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let max_speakers = payload
            .get("max_speakers")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let diarization_result: DiarizationResult = match self
            .backend
            .diarize(&audio_path, min_speakers, max_speakers)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                return JobResult::Retry(format!("Diarization failed: {}", e));
            }
        };

        if diarization_result.segments.is_empty() {
            info!(
                attachment_id = %attachment_id,
                "Diarization returned no segments — single-speaker audio likely"
            );
            return JobResult::Success(Some(json!({
                "num_speakers": 0,
                "segments": 0,
                "status": "no_segments"
            })));
        }

        ctx.report_progress(60, Some("Aligning speakers with transcript"));

        // 5. Align speakers with transcript segments
        align_speakers(&mut transcript_segments, &diarization_result);

        ctx.report_progress(70, Some("Updating attachment metadata"));

        // 6. Update the attachment's extracted_metadata with speaker-labeled segments
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Schema tx failed for metadata update: {}",
                        e
                    ))
                }
            };

            // Merge speaker data into existing metadata
            let mut metadata = attachment
                .extracted_metadata
                .clone()
                .unwrap_or_else(|| json!({}));

            // Serialize the speaker-labeled segments back
            let labeled_segments: Vec<serde_json::Value> = transcript_segments
                .iter()
                .map(|seg| {
                    let mut obj = json!({
                        "start_secs": seg.start_secs,
                        "end_secs": seg.end_secs,
                        "text": seg.text,
                    });
                    if let Some(ref speaker) = seg.speaker_id {
                        obj["speaker_id"] = json!(speaker);
                    }
                    if let Some(ref words) = seg.words {
                        obj["words"] = serde_json::to_value(words).unwrap_or_default();
                    }
                    obj
                })
                .collect();

            metadata["transcript_segments"] = json!(labeled_segments);

            // Add diarization metadata
            metadata["diarization"] = json!({
                "num_speakers": diarization_result.num_speakers,
                "segment_count": diarization_result.segments.len(),
                "model": self.backend.model_name(),
            });

            if let Err(e) = file_storage
                .update_extracted_content_tx(
                    &mut tx,
                    attachment_id,
                    attachment.extracted_text.as_deref(),
                    Some(metadata),
                )
                .await
            {
                error!(
                    attachment_id = %attachment_id,
                    error = %e,
                    "Failed to update attachment metadata with diarization"
                );
                return JobResult::Failed(format!("Failed to update metadata: {}", e));
            }

            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Failed to commit diarization metadata: {}", e));
            }
        }

        ctx.report_progress(80, Some("Re-rendering caption files"));

        // 7. Re-render caption files with speaker labels
        {
            let caption_segments: Vec<captions::CaptionSegment> = transcript_segments
                .iter()
                .map(|seg| captions::CaptionSegment {
                    start_secs: seg.start_secs,
                    end_secs: seg.end_secs,
                    text: seg.text.clone(),
                    speaker: seg.speaker_id.clone(),
                })
                .collect();

            if !caption_segments.is_empty() {
                let filename = &attachment.filename;
                let base_name = filename
                    .rsplit_once('.')
                    .map(|(name, _)| name)
                    .unwrap_or(filename);

                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                    // Delete existing derived caption/transcript attachments to avoid duplicates
                    if let Err(e) = file_storage
                        .delete_derived_captions_tx(&mut tx, attachment_id)
                        .await
                    {
                        warn!(error = %e, "Failed to delete existing caption attachments");
                    }

                    // VTT file
                    let vtt = captions::render_webvtt(&caption_segments);
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.vtt", base_name),
                            "text/vtt",
                            vtt.as_bytes(),
                            "caption",
                        )
                        .await
                    {
                        warn!(error = %e, "Failed to store diarized VTT attachment");
                    }

                    // SRT file
                    let srt = captions::render_srt(&caption_segments);
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.srt", base_name),
                            "application/x-subrip",
                            srt.as_bytes(),
                            "caption",
                        )
                        .await
                    {
                        warn!(error = %e, "Failed to store diarized SRT attachment");
                    }

                    // Plain text transcript with speaker labels
                    let plain_text: String = caption_segments
                        .iter()
                        .map(|s| {
                            if let Some(ref speaker) = s.speaker {
                                format!("[{}] {}", speaker, s.text.trim())
                            } else {
                                s.text.trim().to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.transcript.txt", base_name),
                            "text/plain",
                            plain_text.as_bytes(),
                            "transcript",
                        )
                        .await
                    {
                        warn!(error = %e, "Failed to store diarized transcript attachment");
                    }

                    if let Err(e) = tx.commit().await {
                        error!(error = %e, "Failed to commit diarized caption files");
                    } else {
                        info!(
                            attachment_id = %attachment_id,
                            num_speakers = diarization_result.num_speakers,
                            "Caption files re-rendered with speaker labels"
                        );
                    }
                }
            }
        }

        // 8. Inject speaker config block into note content for user editing
        {
            ctx.report_progress(90, Some("Adding speaker config to note"));

            // Collect unique speaker IDs
            let mut speaker_ids: Vec<String> = transcript_segments
                .iter()
                .filter_map(|s| s.speaker_id.clone())
                .collect();
            speaker_ids.sort();
            speaker_ids.dedup();

            let config = SpeakerConfig {
                speakers: speaker_ids
                    .iter()
                    .map(|id| SpeakerMapping {
                        id: id.clone(),
                        name: id.clone(), // Default: same as ID until user edits
                        role: None,
                    })
                    .collect(),
            };

            let config_block = config.render_block();

            if let Ok(mut tx) = schema_ctx.begin_tx().await {
                match self.db.notes.fetch_tx(&mut tx, note_id).await {
                    Ok(note) => {
                        // Only add if not already present
                        if !note.original.content.contains("```json:speakers") {
                            let updated_content =
                                format!("{}\n\n{}", note.original.content, config_block);
                            if let Err(e) = self
                                .db
                                .notes
                                .update_original_tx(&mut tx, note_id, &updated_content)
                                .await
                            {
                                warn!(error = %e, "Failed to inject speaker config into note");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to fetch note for speaker config injection");
                    }
                }
                let _ = tx.commit().await;
            }
        }

        ctx.report_progress(95, Some("Diarization complete"));

        let result_json = json!({
            "num_speakers": diarization_result.num_speakers,
            "diarization_segments": diarization_result.segments.len(),
            "transcript_segments": transcript_segments.len(),
            "model": self.backend.model_name(),
        });

        info!(
            attachment_id = %attachment_id,
            num_speakers = diarization_result.num_speakers,
            "Speaker diarization completed"
        );

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(result_json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_schema_default() {
        let job = matric_core::Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::SpeakerDiarization,
            status: matric_core::JobStatus::Pending,
            priority: 5,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert_eq!(extract_schema(&ctx), "public");
    }

    #[test]
    fn test_extract_schema_from_payload() {
        let job = matric_core::Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::SpeakerDiarization,
            status: matric_core::JobStatus::Pending,
            priority: 5,
            payload: Some(json!({
                "attachment_id": Uuid::new_v4().to_string(),
                "schema": "archive_photos"
            })),
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert_eq!(extract_schema(&ctx), "archive_photos");
    }
}

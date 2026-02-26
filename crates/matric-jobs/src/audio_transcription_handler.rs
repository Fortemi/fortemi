//! AudioTranscriptionHandler — transcribes audio from a derived attachment via Whisper.
//!
//! Atomic job type for the video/audio pipeline: processes a single audio file
//! (or chunk for feature-length content), calls the Whisper backend, stores
//! transcript segments in parent metadata, and persists caption files (VTT/SRT/TXT)
//! as derived attachments.
//!
//! Fan-in: after completing, checks if all keyframe descriptions AND transcription
//! are done. When both prerequisites are met, queues KeyframeAssembly for final
//! markdown rebuild.
//!
//! Downstream: queues SpeakerDiarization if pyannote backend is configured.
//!
//! Issue #542

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::{captions, JobRepository, JobType};
use matric_db::{Database, PgFileStorageRepository, SchemaContext};
use matric_inference::transcription::TranscriptionBackend;

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

        // Parse payload
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

        ctx.report_progress(5, Some("Checking backend availability"));

        // Check backend availability — retry if unavailable
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

        // Download audio from derived attachment storage
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

        // Transcode to 16kHz mono PCM WAV for reliable Whisper compatibility.
        // The audio is already a WAV from the video pipeline's extract_audio_track(),
        // but re-transcode ensures consistent format (and handles any edge cases).
        let work_dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => return JobResult::Failed(format!("Failed to create work dir: {}", e)),
        };

        let input_path = work_dir.path().join("input.wav");
        if let Err(e) = std::fs::write(&input_path, &audio_data) {
            return JobResult::Failed(format!("Failed to write audio temp file: {}", e));
        }

        let wav_data = match crate::adapters::audio_util::transcode_to_speech_wav(
            &input_path,
            work_dir.path(),
        )
        .await
        {
            Ok(wav_path) => match std::fs::read(&wav_path) {
                Ok(data) => data,
                Err(e) => {
                    return JobResult::Failed(format!("Failed to read transcoded WAV: {}", e))
                }
            },
            Err(e) => {
                // If transcode fails, try original data directly
                warn!(error = %e, "Audio transcode failed, using original data");
                audio_data
            }
        };

        ctx.report_progress(30, Some("Transcribing audio"));

        // Call Whisper backend
        let transcription = match self
            .transcription
            .transcribe(&wav_data, "audio/wav", None)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                return JobResult::Retry(format!("Whisper transcription failed: {}", e));
            }
        };

        let segment_count = transcription.segments.len();
        info!(
            parent = %parent_attachment_id,
            audio = %audio_attachment_id,
            segments = segment_count,
            duration = ?transcription.duration_secs,
            language = ?transcription.language,
            "Audio transcription complete"
        );

        ctx.report_progress(60, Some("Storing transcript segments"));

        // Build segment JSON for metadata storage
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

        // Store transcript_segments in parent attachment's extracted_metadata
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

        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            if let Err(e) = file_storage
                .merge_extracted_metadata_tx(&mut tx, parent_attachment_id, &transcript_metadata)
                .await
            {
                warn!(error = %e, "Failed to store transcript in parent metadata");
                let _ = tx.commit().await;
                return JobResult::Failed(format!("Failed to store transcript metadata: {}", e));
            }
            let _ = tx.commit().await;
        }

        ctx.report_progress(70, Some("Persisting caption files"));

        // Generate and persist caption files as derived attachments
        if !transcription.segments.is_empty() {
            if let Some(note_id) = ctx.note_id() {
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
                                    filename = %filename,
                                    "Caption file persisted"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    filename = %filename,
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

        ctx.report_progress(80, Some("Queuing downstream jobs"));

        // Queue SpeakerDiarization if pyannote backend is configured
        let diarization_available = std::env::var(matric_core::defaults::ENV_DIARIZATION_BASE_URL)
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();

        if let Some(note_id) = ctx.note_id() {
            if diarization_available && !transcription.segments.is_empty() {
                let mut diar_payload = serde_json::Map::new();
                diar_payload.insert(
                    "attachment_id".to_string(),
                    json!(parent_attachment_id.to_string()),
                );
                if schema != "public" {
                    diar_payload.insert("schema".to_string(), json!(schema));
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
                        ctx.emit_job_queued(job_id, JobType::SpeakerDiarization, Some(note_id));
                        info!(
                            note_id = %note_id,
                            parent = %parent_attachment_id,
                            "SpeakerDiarization queued from AudioTranscription"
                        );
                    }
                    Ok(None) => {} // Deduplicated
                    Err(e) => {
                        warn!(error = %e, "Failed to queue SpeakerDiarization");
                    }
                }
            }
        }

        ctx.report_progress(90, Some("Checking fan-in"));

        // Fan-in check: for video content, check if all keyframe descriptions
        // AND transcription are complete. The last to finish triggers assembly.
        let is_video = payload
            .get("is_video")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_video {
            self.check_video_fan_in(
                &ctx,
                &schema_ctx,
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
}

impl AudioTranscriptionHandler {
    /// Check if both keyframe descriptions and transcription are complete.
    /// If so, queue KeyframeAssembly.
    async fn check_video_fan_in(
        &self,
        ctx: &JobContext,
        schema_ctx: &SchemaContext,
        file_storage: &PgFileStorageRepository,
        parent_attachment_id: Uuid,
        schema: &str,
    ) {
        // Read parent's extracted_metadata for expected frame count and transcript status
        let (expected_frames, transcript_done) = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    warn!(error = %e, "Fan-in metadata read failed");
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
            (expected, tc)
        };

        if expected_frames == 0 {
            debug!("No keyframes expected — skipping video fan-in check");
            return;
        }

        if !transcript_done {
            debug!("Transcript not yet complete — skipping fan-in");
            return;
        }

        // Count described keyframes
        let described_count: i64 = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(_) => return,
            };
            let count = file_storage
                .count_described_keyframes_tx(&mut tx, parent_attachment_id)
                .await
                .unwrap_or(0);
            let _ = tx.commit().await;
            count
        };

        debug!(
            described = described_count,
            expected = expected_frames,
            transcript_done,
            "Video fan-in: {}/{} keyframes + transcript={}",
            described_count,
            expected_frames,
            transcript_done
        );

        if described_count >= expected_frames as i64 {
            // All prerequisites met — queue assembly
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
                        expected_frames, job_id
                    );
                }
                Ok(None) => {
                    debug!("KeyframeAssembly already queued (deduplicated)");
                }
                Err(e) => {
                    error!(error = %e, "Failed to queue KeyframeAssembly from AudioTranscription");
                }
            }
        }
    }
}

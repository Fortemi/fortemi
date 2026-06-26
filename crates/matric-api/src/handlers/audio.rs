//! Audio transcription HTTP handlers.
//!
//! Provides ad-hoc audio transcription via Whisper-compatible backend without
//! requiring attachment creation. Useful for preview, inline analysis, and MCP tooling.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::fmt;
use tracing::warn;

use crate::{ApiError, AppState};
use matric_inference::transcription::{TranscriptionSegment, WhisperBackend};

const AUDIO_TRANSCRIPTION_PROVIDER_DETAIL: &str =
    "Audio transcription backend failed. Check server logs for diagnostics.";

/// Response from audio transcription.
#[derive(Serialize)]
pub struct TranscribeAudioResponse {
    /// Full transcribed text.
    pub text: String,
    /// Timestamped segments.
    pub segments: Vec<TranscriptionSegment>,
    /// Detected language (ISO 639-1 code).
    pub language: Option<String>,
    /// Total audio duration in seconds.
    pub duration_secs: Option<f64>,
    /// Whisper model used for transcription.
    pub model: String,
    /// Size of the uploaded audio in bytes.
    pub audio_size: usize,
}

impl fmt::Debug for TranscribeAudioResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let segment_text_lens: Vec<usize> = self
            .segments
            .iter()
            .map(|segment| segment.text.len())
            .collect();
        let speaker_id_lens: Vec<Option<usize>> = self
            .segments
            .iter()
            .map(|segment| segment.speaker_id.as_ref().map(String::len))
            .collect();
        let word_counts: Vec<usize> = self
            .segments
            .iter()
            .map(|segment| segment.words.as_ref().map_or(0, Vec::len))
            .collect();

        f.debug_struct("TranscribeAudioResponse")
            .field("text_len", &self.text.len())
            .field("segments_count", &self.segments.len())
            .field("segment_text_lens", &segment_text_lens)
            .field("speaker_id_lens", &speaker_id_lens)
            .field("segment_word_counts", &word_counts)
            .field("language_len", &self.language.as_ref().map(String::len))
            .field("duration_secs", &self.duration_secs)
            .field("model_len", &self.model.len())
            .field("audio_size", &self.audio_size)
            .finish()
    }
}

/// Transcribe audio using the configured Whisper-compatible backend.
///
/// Accepts multipart/form-data with an audio file and returns a transcription with timestamps.
/// Requires `WHISPER_BASE_URL` to be configured.
///
/// # Multipart Fields
/// - `file`: Audio file (required)
/// - `language`: ISO 639-1 language hint, e.g. "en", "es" (optional, auto-detect if omitted)
/// - `model`: Whisper model slug override (optional, e.g. "Systran/faster-whisper-large-v3")
///
/// # Returns
/// - 200 OK with transcription text, segments, language, duration, model, and audio size
/// - 400 Bad Request if file is missing or empty
/// - 503 Service Unavailable if transcription backend is not configured
#[utoipa::path(post, path = "/api/v1/audio/transcribe", tag = "Audio",
    responses((status = 200, description = "Transcription result")))]
pub async fn transcribe_audio(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<TranscribeAudioResponse>, ApiError> {
    let default_backend = state.transcription_backend.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Audio transcription backend is not configured.".into())
    })?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut language: Option<String> = None;
    let mut model_override: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid multipart audio request.".to_string()))?
    {
        let field_name = field.name().map(|n| n.to_string());
        match field_name.as_deref() {
            Some("file") => {
                content_type = field.content_type().map(|c| c.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| {
                            ApiError::BadRequest("Invalid uploaded audio file.".to_string())
                        })?
                        .to_vec(),
                );
            }
            Some("language") => {
                language =
                    Some(field.text().await.map_err(|_| {
                        ApiError::BadRequest("Invalid language field.".to_string())
                    })?);
            }
            Some("model") => {
                let val = field
                    .text()
                    .await
                    .map_err(|_| ApiError::BadRequest("Invalid model field.".to_string()))?;
                if !val.trim().is_empty() {
                    model_override = Some(val.trim().to_string());
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    let audio_bytes = file_data
        .ok_or_else(|| ApiError::BadRequest("Missing file in multipart form".to_string()))?;

    if audio_bytes.is_empty() {
        return Err(ApiError::BadRequest("Audio file is empty".into()));
    }

    let mime_type = content_type.as_deref().unwrap_or("audio/wav");

    // Use model override if specified, otherwise fall back to configured default
    let overridden_backend = model_override.map(|m| {
        let base_url = std::env::var(matric_core::defaults::ENV_WHISPER_BASE_URL)
            .unwrap_or_else(|_| matric_core::defaults::DEFAULT_WHISPER_BASE_URL.to_string());
        WhisperBackend::new(base_url, m)
    });
    let backend: &dyn matric_inference::transcription::TranscriptionBackend =
        match &overridden_backend {
            Some(b) => b,
            None => default_backend.as_ref(),
        };

    let result = backend
        .transcribe(&audio_bytes, mime_type, language.as_deref())
        .await
        .map_err(|e| {
            let diagnostic = e.to_string();
            warn!(
                error_len = diagnostic.chars().count(),
                "Audio transcription backend failed"
            );
            ApiError::ProviderFailure {
                capability: "Audio transcription",
                detail: AUDIO_TRANSCRIPTION_PROVIDER_DETAIL.to_string(),
            }
        })?;

    Ok(Json(TranscribeAudioResponse {
        text: result.full_text,
        segments: result.segments,
        language: result.language,
        duration_secs: result.duration_secs,
        model: backend.model_name().to_string(),
        audio_size: audio_bytes.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_provider_detail_is_fixed_and_redacted() {
        assert_eq!(
            AUDIO_TRANSCRIPTION_PROVIDER_DETAIL,
            "Audio transcription backend failed. Check server logs for diagnostics."
        );
        assert!(!AUDIO_TRANSCRIPTION_PROVIDER_DETAIL.contains("https://"));
        assert!(!AUDIO_TRANSCRIPTION_PROVIDER_DETAIL.contains("token"));
        assert!(!AUDIO_TRANSCRIPTION_PROVIDER_DETAIL.contains("/srv/fortemi"));
    }

    #[test]
    fn transcribe_audio_response_debug_redacts_transcript_text_and_model() {
        let response = TranscribeAudioResponse {
            text: "customer@example.com transcript postgres://user:pass@db.internal/app"
                .to_string(),
            segments: vec![TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "segment text has /srv/private/audio.wav and sk-live-audio".to_string(),
                speaker_id: Some("speaker-customer@example.com".to_string()),
                words: Some(vec![matric_inference::transcription::WordTimestamp {
                    word: "secret-word-token".to_string(),
                    start_secs: 0.1,
                    end_secs: 0.2,
                    confidence: Some(0.9),
                }]),
            }],
            language: Some("en-private-customer".to_string()),
            duration_secs: Some(2.5),
            model: "whisper-private-model-db.internal".to_string(),
            audio_size: 4096,
        };

        let rendered = format!("{response:?}");

        assert!(rendered.contains("TranscribeAudioResponse"));
        assert!(rendered.contains("text_len"));
        assert!(rendered.contains("segments_count"));
        assert!(rendered.contains("segment_text_lens"));
        assert!(rendered.contains("speaker_id_lens"));
        assert!(rendered.contains("segment_word_counts"));
        assert!(rendered.contains("language_len"));
        assert!(rendered.contains("model_len"));

        for raw in [
            "customer@example.com",
            "postgres://user:pass",
            "db.internal",
            "segment text",
            "/srv/private/audio.wav",
            "sk-live-audio",
            "speaker-customer",
            "secret-word-token",
            "en-private-customer",
            "whisper-private-model",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }
}

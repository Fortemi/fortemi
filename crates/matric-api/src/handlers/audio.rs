//! Audio transcription HTTP handlers.
//!
//! Provides ad-hoc audio transcription via Whisper-compatible backend without
//! requiring attachment creation. Useful for preview, inline analysis, and MCP tooling.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::{ApiError, AppState};
use matric_inference::transcription::{TranscriptionSegment, WhisperBackend};

/// Response from audio transcription.
#[derive(Debug, Serialize)]
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
        ApiError::ServiceUnavailable(
            "Transcription backend not configured. Set WHISPER_BASE_URL environment variable."
                .into(),
        )
    })?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut language: Option<String> = None;
    let mut model_override: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let field_name = field.name().map(|n| n.to_string());
        match field_name.as_deref() {
            Some("file") => {
                content_type = field.content_type().map(|c| c.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?
                        .to_vec(),
                );
            }
            Some("language") => {
                language = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?,
                );
            }
            Some("model") => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?;
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
        .map_err(|e| ApiError::Internal(format!("Transcription error: {}", e)))?;

    Ok(Json(TranscribeAudioResponse {
        text: result.full_text,
        segments: result.segments,
        language: result.language,
        duration_secs: result.duration_secs,
        model: backend.model_name().to_string(),
        audio_size: audio_bytes.len(),
    }))
}

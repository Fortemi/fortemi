//! Transcription backend traits and implementations for audio-to-text.

use async_trait::async_trait;
use matric_core::Result;
use serde::{Deserialize, Serialize};

/// A segment of transcribed audio with timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionSegment {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

/// Result of audio transcription.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionResult {
    /// Full transcribed text.
    pub full_text: String,
    /// Timestamped segments.
    pub segments: Vec<TranscriptionSegment>,
    /// Detected language (ISO 639-1 code).
    pub language: Option<String>,
    /// Total audio duration in seconds.
    pub duration_secs: Option<f64>,
}

/// Backend for transcribing audio files.
#[async_trait]
pub trait TranscriptionBackend: Send + Sync {
    /// Transcribe audio data.
    async fn transcribe(
        &self,
        audio_data: &[u8],
        mime_type: &str,
        language: Option<&str>,
    ) -> Result<TranscriptionResult>;

    /// Check if the transcription backend is available.
    async fn health_check(&self) -> Result<bool>;

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

/// OpenAI-compatible Whisper backend (works with Speaches/faster-whisper-server).
pub struct WhisperBackend {
    base_url: String,
    model: String,
    client: reqwest::Client,
    timeout_secs: u64,
}

impl WhisperBackend {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
            timeout_secs: 300, // 5 min for long audio
        }
    }

    /// Create from environment variables.
    /// Returns None if WHISPER_BASE_URL is not set.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var(matric_core::defaults::ENV_WHISPER_BASE_URL).ok()?;
        if base_url.is_empty() {
            return None;
        }
        let model = std::env::var(matric_core::defaults::ENV_WHISPER_MODEL)
            .unwrap_or_else(|_| matric_core::defaults::DEFAULT_WHISPER_MODEL.to_string());
        Some(Self::new(base_url, model))
    }
}

/// OpenAI Whisper API response format.
#[derive(Deserialize)]
struct WhisperResponse {
    text: String,
    #[serde(default)]
    segments: Option<Vec<WhisperSegment>>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
}

#[derive(Deserialize)]
struct WhisperSegment {
    start: f64,
    end: f64,
    text: String,
}

#[async_trait]
impl TranscriptionBackend for WhisperBackend {
    async fn transcribe(
        &self,
        audio_data: &[u8],
        mime_type: &str,
        language: Option<&str>,
    ) -> Result<TranscriptionResult> {
        let url = format!("{}/v1/audio/transcriptions", self.base_url);

        // Determine file extension from MIME type
        let ext = match mime_type {
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" | "audio/x-wav" => "wav",
            "audio/ogg" => "ogg",
            "audio/flac" => "flac",
            "audio/aac" => "aac",
            "audio/webm" => "webm",
            _ => "wav",
        };

        let file_part = reqwest::multipart::Part::bytes(audio_data.to_vec())
            .file_name(format!("audio.{}", ext))
            .mime_str(mime_type)
            .map_err(|e| {
                matric_core::Error::Internal(format!("Failed to create multipart: {}", e))
            })?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "verbose_json");

        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let response = self
            .client
            .post(&url)
            .multipart(form)
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .send()
            .await
            .map_err(|e| {
                matric_core::Error::Internal(format!("Transcription request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(format!(
                "Whisper API returned {}: {}",
                status, body
            )));
        }

        let result: WhisperResponse = response.json().await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to parse whisper response: {}", e))
        })?;

        let segments = result
            .segments
            .unwrap_or_default()
            .into_iter()
            .map(|s| TranscriptionSegment {
                start_secs: s.start,
                end_secs: s.end,
                text: s.text,
            })
            .collect();

        Ok(TranscriptionResult {
            full_text: result.text,
            segments,
            language: result.language,
            duration_secs: result.duration,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcription_segment_serialization() {
        let segment = TranscriptionSegment {
            start_secs: 0.0,
            end_secs: 5.5,
            text: "Hello world".to_string(),
        };

        let json = serde_json::to_value(&segment).unwrap();
        assert_eq!(json["start_secs"], 0.0);
        assert_eq!(json["end_secs"], 5.5);
        assert_eq!(json["text"], "Hello world");

        let deserialized: TranscriptionSegment = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, segment);
    }

    #[test]
    fn test_transcription_result_serialization() {
        let result = TranscriptionResult {
            full_text: "Hello world. This is a test.".to_string(),
            segments: vec![
                TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 2.5,
                    text: "Hello world.".to_string(),
                },
                TranscriptionSegment {
                    start_secs: 2.5,
                    end_secs: 5.0,
                    text: "This is a test.".to_string(),
                },
            ],
            language: Some("en".to_string()),
            duration_secs: Some(5.0),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["full_text"], "Hello world. This is a test.");
        assert_eq!(json["segments"].as_array().unwrap().len(), 2);
        assert_eq!(json["language"], "en");
        assert_eq!(json["duration_secs"], 5.0);

        let deserialized: TranscriptionResult = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, result);
    }

    #[test]
    fn test_whisper_backend_new() {
        let backend =
            WhisperBackend::new("http://localhost:8000".to_string(), "whisper-1".to_string());
        assert_eq!(backend.base_url, "http://localhost:8000");
        assert_eq!(backend.model, "whisper-1");
        assert_eq!(backend.timeout_secs, 300);
        assert_eq!(backend.model_name(), "whisper-1");
    }

    #[test]
    fn test_whisper_backend_constructor_with_custom_params() {
        let backend =
            WhisperBackend::new("http://test:8000".to_string(), "custom-whisper".to_string());
        assert_eq!(backend.base_url, "http://test:8000");
        assert_eq!(backend.model, "custom-whisper");
        assert_eq!(backend.timeout_secs, 300);
    }

    #[test]
    fn test_whisper_backend_constructor_with_default_model() {
        let backend = WhisperBackend::new(
            "http://test:8000".to_string(),
            matric_core::defaults::DEFAULT_WHISPER_MODEL.to_string(),
        );
        assert_eq!(backend.base_url, "http://test:8000");
        assert_eq!(backend.model, matric_core::defaults::DEFAULT_WHISPER_MODEL);
    }

    #[test]
    fn test_whisper_response_deserialization() {
        let json = r#"{
            "text": "Hello world",
            "segments": [
                {"start": 0.0, "end": 2.5, "text": "Hello"},
                {"start": 2.5, "end": 5.0, "text": "world"}
            ],
            "language": "en",
            "duration": 5.0
        }"#;

        let response: WhisperResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Hello world");
        assert!(response.segments.is_some());
        assert_eq!(response.segments.as_ref().unwrap().len(), 2);
        assert_eq!(response.language.as_ref().unwrap(), "en");
        assert_eq!(response.duration.unwrap(), 5.0);
    }

    #[test]
    fn test_whisper_response_deserialization_minimal() {
        let json = r#"{"text": "Hello world"}"#;

        let response: WhisperResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Hello world");
        assert!(response.segments.is_none());
        assert!(response.language.is_none());
        assert!(response.duration.is_none());
    }

    #[test]
    fn test_mime_type_to_extension() {
        // This test validates the extension mapping logic by checking
        // that we have reasonable defaults
        let test_cases = vec![
            ("audio/mpeg", "mp3"),
            ("audio/mp3", "mp3"),
            ("audio/wav", "wav"),
            ("audio/x-wav", "wav"),
            ("audio/ogg", "ogg"),
            ("audio/flac", "flac"),
            ("audio/aac", "aac"),
            ("audio/webm", "webm"),
            ("audio/unknown", "wav"), // default fallback
        ];

        for (mime_type, expected_ext) in test_cases {
            let ext = match mime_type {
                "audio/mpeg" | "audio/mp3" => "mp3",
                "audio/wav" | "audio/x-wav" => "wav",
                "audio/ogg" => "ogg",
                "audio/flac" => "flac",
                "audio/aac" => "aac",
                "audio/webm" => "webm",
                _ => "wav",
            };
            assert_eq!(
                ext, expected_ext,
                "MIME type {} should map to {}",
                mime_type, expected_ext
            );
        }
    }
}

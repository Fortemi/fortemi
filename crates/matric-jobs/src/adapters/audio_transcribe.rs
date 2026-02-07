//! AudioTranscribe extraction adapter - handles audio transcription via Whisper.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::transcription::TranscriptionBackend;

/// Adapter for extracting text from audio files via transcription.
///
/// Uses a TranscriptionBackend (typically WhisperBackend) to transcribe
/// audio content into text. Returns the full transcription text along
/// with segment-level timestamps and metadata.
///
/// Configuration options:
/// - `language`: Optional ISO 639-1 language code hint (e.g., "en", "es")
pub struct AudioTranscribeAdapter {
    backend: Arc<dyn TranscriptionBackend>,
}

impl AudioTranscribeAdapter {
    /// Create a new adapter with the given transcription backend.
    pub fn new(backend: Arc<dyn TranscriptionBackend>) -> Self {
        Self { backend }
    }

    /// Create from environment variables using WhisperBackend.
    ///
    /// Returns None if WHISPER_BASE_URL is not set or empty.
    pub fn from_env() -> Option<Self> {
        use matric_inference::transcription::WhisperBackend;

        WhisperBackend::from_env().map(|backend| Self::new(Arc::new(backend)))
    }
}

#[async_trait]
impl ExtractionAdapter for AudioTranscribeAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::AudioTranscribe
    }

    async fn extract(
        &self,
        data: &[u8],
        _filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        // Extract optional language hint from config
        let language = config.get("language").and_then(|v| v.as_str());

        // Perform transcription
        let transcription = self.backend.transcribe(data, mime_type, language).await?;

        // Build metadata with segments and transcription info
        let segments_json: Vec<JsonValue> = transcription
            .segments
            .iter()
            .map(|seg| {
                serde_json::json!({
                    "start_secs": seg.start_secs,
                    "end_secs": seg.end_secs,
                    "text": seg.text,
                })
            })
            .collect();

        let mut metadata = serde_json::json!({
            "segment_count": transcription.segments.len(),
            "segments": segments_json,
        });

        if let Some(lang) = &transcription.language {
            metadata["detected_language"] = serde_json::json!(lang);
        }

        if let Some(duration) = transcription.duration_secs {
            metadata["duration_secs"] = serde_json::json!(duration);
        }

        Ok(ExtractionResult {
            extracted_text: Some(transcription.full_text),
            metadata,
            ai_description: None, // Transcription is the text itself
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        self.backend.health_check().await
    }

    fn name(&self) -> &str {
        "audio_transcribe"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_inference::transcription::{TranscriptionResult, TranscriptionSegment};

    /// Mock transcription backend for testing.
    struct MockTranscriptionBackend {
        result: TranscriptionResult,
        health_ok: bool,
    }

    #[async_trait]
    impl TranscriptionBackend for MockTranscriptionBackend {
        async fn transcribe(
            &self,
            _audio_data: &[u8],
            _mime_type: &str,
            _language: Option<&str>,
        ) -> Result<TranscriptionResult> {
            Ok(self.result.clone())
        }

        async fn health_check(&self) -> Result<bool> {
            Ok(self.health_ok)
        }

        fn model_name(&self) -> &str {
            "mock-whisper"
        }
    }

    #[test]
    fn test_adapter_name() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };
        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        assert_eq!(adapter.name(), "audio_transcribe");
    }

    #[test]
    fn test_adapter_strategy() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };
        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        assert_eq!(adapter.strategy(), ExtractionStrategy::AudioTranscribe);
    }

    #[tokio::test]
    async fn test_health_check_ok() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };
        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_health_check_fail() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: false,
        };
        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_extract_basic_transcription() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "Hello, this is a test transcription.".to_string(),
                segments: vec![
                    TranscriptionSegment {
                        start_secs: 0.0,
                        end_secs: 2.5,
                        text: "Hello, this is".to_string(),
                    },
                    TranscriptionSegment {
                        start_secs: 2.5,
                        end_secs: 5.0,
                        text: "a test transcription.".to_string(),
                    },
                ],
                language: Some("en".to_string()),
                duration_secs: Some(5.0),
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(
                b"fake_audio_data",
                "test.mp3",
                "audio/mpeg",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(
            result.extracted_text.as_deref(),
            Some("Hello, this is a test transcription.")
        );
        assert_eq!(result.metadata["segment_count"], 2);
        assert_eq!(result.metadata["detected_language"], "en");
        assert_eq!(result.metadata["duration_secs"], 5.0);
        assert!(result.ai_description.is_none());
        assert!(result.preview_data.is_none());

        // Verify segments structure
        let segments = result.metadata["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0]["start_secs"], 0.0);
        assert_eq!(segments[0]["end_secs"], 2.5);
        assert_eq!(segments[0]["text"], "Hello, this is");
        assert_eq!(segments[1]["start_secs"], 2.5);
        assert_eq!(segments[1]["end_secs"], 5.0);
        assert_eq!(segments[1]["text"], "a test transcription.");
    }

    #[tokio::test]
    async fn test_extract_minimal_metadata() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "Short audio.".to_string(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(
                b"audio_data",
                "short.wav",
                "audio/wav",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.extracted_text.as_deref(), Some("Short audio."));
        assert_eq!(result.metadata["segment_count"], 0);
        assert!(result.metadata.get("detected_language").is_none());
        assert!(result.metadata.get("duration_secs").is_none());
    }

    #[tokio::test]
    async fn test_extract_with_language_config() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "Hola mundo.".to_string(),
                segments: vec![TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 1.5,
                    text: "Hola mundo.".to_string(),
                }],
                language: Some("es".to_string()),
                duration_secs: Some(1.5),
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(
                b"audio_data",
                "spanish.ogg",
                "audio/ogg",
                &serde_json::json!({ "language": "es" }),
            )
            .await
            .unwrap();

        assert_eq!(result.extracted_text.as_deref(), Some("Hola mundo."));
        assert_eq!(result.metadata["detected_language"], "es");
        assert_eq!(result.metadata["segment_count"], 1);
    }

    #[tokio::test]
    async fn test_extract_empty_transcription() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: Some(0.0),
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(
                b"silent_audio",
                "silence.flac",
                "audio/flac",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.extracted_text.as_deref(), Some(""));
        assert_eq!(result.metadata["segment_count"], 0);
        assert_eq!(result.metadata["duration_secs"], 0.0);
    }

    #[test]
    fn test_adapter_constructor() {
        // Test that AudioTranscribeAdapter can be constructed with a mock backend
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: String::new(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };
        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        assert_eq!(adapter.name(), "audio_transcribe");
        assert_eq!(adapter.strategy(), ExtractionStrategy::AudioTranscribe);
    }

    #[tokio::test]
    async fn test_extract_with_invalid_config() {
        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "Test".to_string(),
                segments: vec![],
                language: None,
                duration_secs: None,
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));

        // Invalid language type (number instead of string) should be ignored
        let result = adapter
            .extract(
                b"data",
                "test.mp3",
                "audio/mpeg",
                &serde_json::json!({ "language": 123 }),
            )
            .await
            .unwrap();

        assert_eq!(result.extracted_text.as_deref(), Some("Test"));
    }

    #[tokio::test]
    async fn test_multiple_segments_metadata() {
        let segments = vec![
            TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 1.0,
                text: "First".to_string(),
            },
            TranscriptionSegment {
                start_secs: 1.0,
                end_secs: 2.0,
                text: "Second".to_string(),
            },
            TranscriptionSegment {
                start_secs: 2.0,
                end_secs: 3.5,
                text: "Third".to_string(),
            },
        ];

        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "First Second Third".to_string(),
                segments: segments.clone(),
                language: Some("en".to_string()),
                duration_secs: Some(3.5),
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(b"audio", "multi.wav", "audio/wav", &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["segment_count"], 3);
        let segments_json = result.metadata["segments"].as_array().unwrap();
        assert_eq!(segments_json.len(), 3);

        for (i, seg) in segments.iter().enumerate() {
            assert_eq!(segments_json[i]["start_secs"], seg.start_secs);
            assert_eq!(segments_json[i]["end_secs"], seg.end_secs);
            assert_eq!(segments_json[i]["text"], seg.text);
        }
    }
}

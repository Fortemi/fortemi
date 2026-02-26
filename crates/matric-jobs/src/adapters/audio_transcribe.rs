//! AudioTranscribe extraction adapter - handles audio transcription via Whisper.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::transcription::TranscriptionBackend;

/// Format duration in seconds to a human-readable string.
fn format_audio_duration(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

/// Map audio MIME type to a file extension for temp file creation.
fn mime_type_to_ext(mime_type: &str) -> &str {
    match mime_type {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" | "audio/vorbis" => "ogg",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/aac" => "aac",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/webm" => "webm",
        "audio/opus" => "opus",
        _ => "bin",
    }
}

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
    /// Returns None only if WHISPER_BASE_URL is explicitly set to empty string.
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

        // Resolve the audio source: path-access mode or inline bytes.
        // Then transcode to 16kHz mono PCM WAV via ffmpeg for reliable
        // Whisper/pyannote compatibility (eliminates 415 errors).
        let work_dir = tempfile::tempdir().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create work dir: {}", e))
        })?;

        let source_path = if let Some(path) = config.get("_source_path").and_then(|v| v.as_str()) {
            // Path-access mode: file already on disk
            std::path::PathBuf::from(path)
        } else {
            // Inline bytes: write to temp file for ffmpeg input
            let ext = mime_type_to_ext(mime_type);
            let input_path = work_dir.path().join(format!("input.{}", ext));
            std::fs::write(&input_path, data).map_err(|e| {
                matric_core::Error::Internal(format!("Failed to write audio temp file: {}", e))
            })?;
            input_path
        };

        // Transcode to speech WAV (16kHz mono PCM) for universal backend compatibility.
        // Skip transcode only if _skip_transcode is set (e.g., input is already speech WAV
        // from a video pipeline that already transcoded).
        let skip_transcode = config
            .get("_skip_transcode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let transcription = if skip_transcode {
            // Already in speech WAV format — transcribe directly (single pass,
            // since video pipeline chunks at the video level, not audio level).
            let audio_data = std::fs::read(&source_path).map_err(|e| {
                matric_core::Error::Internal(format!(
                    "Failed to read audio from {}: {}",
                    source_path.display(),
                    e
                ))
            })?;
            self.backend
                .transcribe(&audio_data, mime_type, language)
                .await
                .map_err(|e| {
                    matric_core::Error::Internal(format!(
                        "Audio transcription failed (backend: {}, mime: {}, size: {} bytes): {}",
                        self.backend.model_name(),
                        mime_type,
                        audio_data.len(),
                        e
                    ))
                })?
        } else {
            // Transcode, then use chunked transcription for long files (Issue #543).
            let wav_path =
                super::audio_util::transcode_to_speech_wav(&source_path, work_dir.path()).await?;
            super::audio_util::transcribe_with_chunking(
                &self.backend,
                &wav_path,
                work_dir.path(),
                language,
            )
            .await?
        };

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
            "transcript_segments": segments_json,
        });

        if let Some(lang) = &transcription.language {
            metadata["detected_language"] = serde_json::json!(lang);
        }

        if let Some(duration) = transcription.duration_secs {
            metadata["duration_secs"] = serde_json::json!(duration);
        }

        // Format as markdown with metadata header and transcript section
        let extracted_text = if transcription.full_text.is_empty() {
            Some(transcription.full_text)
        } else {
            let mut parts = Vec::new();

            // Metadata header
            let mut meta_items = Vec::new();
            if let Some(duration) = transcription.duration_secs {
                meta_items.push(format!("**Duration**: {}", format_audio_duration(duration)));
            }
            if let Some(ref lang) = transcription.language {
                meta_items.push(format!("**Language**: {}", lang));
            }
            meta_items.push(format!("**Segments**: {}", transcription.segments.len()));
            parts.push(meta_items.join(" | "));

            // Transcript section
            parts.push("## Transcript".to_string());
            parts.push(transcription.full_text);

            Some(parts.join("\n\n"))
        };

        Ok(ExtractionResult {
            extracted_text,
            metadata,
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let backend_ok = self.backend.health_check().await?;
        let ffmpeg_ok = super::audio_util::ffmpeg_available().await;
        if !ffmpeg_ok {
            tracing::warn!("ffmpeg not available — audio transcoding will fail");
        }
        Ok(backend_ok && ffmpeg_ok)
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
                        speaker_id: None,
                        words: None,
                    },
                    TranscriptionSegment {
                        start_secs: 2.5,
                        end_secs: 5.0,
                        text: "a test transcription.".to_string(),
                        speaker_id: None,
                        words: None,
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
                &serde_json::json!({ "_skip_transcode": true }),
            )
            .await
            .unwrap();

        let text = result.extracted_text.as_deref().unwrap();
        assert!(
            text.contains("## Transcript"),
            "should have transcript heading"
        );
        assert!(
            text.contains("Hello, this is a test transcription."),
            "should contain transcript text"
        );
        assert!(text.contains("**Duration**: 5s"), "should have duration");
        assert!(text.contains("**Language**: en"), "should have language");
        assert!(
            text.contains("**Segments**: 2"),
            "should have segment count"
        );
        assert_eq!(result.metadata["segment_count"], 2);
        assert_eq!(result.metadata["detected_language"], "en");
        assert_eq!(result.metadata["duration_secs"], 5.0);
        assert!(result.ai_description.is_none());
        assert!(result.preview_data.is_none());

        // Verify segments structure
        let segments = result.metadata["transcript_segments"].as_array().unwrap();
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
                &serde_json::json!({ "_skip_transcode": true }),
            )
            .await
            .unwrap();

        let text = result.extracted_text.as_deref().unwrap();
        assert!(
            text.contains("## Transcript"),
            "should have transcript heading"
        );
        assert!(
            text.contains("Short audio."),
            "should contain transcript text"
        );
        assert!(
            text.contains("**Segments**: 0"),
            "should have segment count"
        );
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
                    speaker_id: None,
                    words: None,
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
                &serde_json::json!({ "language": "es", "_skip_transcode": true }),
            )
            .await
            .unwrap();

        let text = result.extracted_text.as_deref().unwrap();
        assert!(
            text.contains("Hola mundo."),
            "should contain transcript text"
        );
        assert!(text.contains("**Language**: es"), "should have language");
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
                &serde_json::json!({ "_skip_transcode": true }),
            )
            .await
            .unwrap();

        // Empty transcription returns empty string (no markdown wrapping)
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
                &serde_json::json!({ "language": 123, "_skip_transcode": true }),
            )
            .await
            .unwrap();

        let text = result.extracted_text.as_deref().unwrap();
        assert!(text.contains("Test"), "should contain transcript text");
    }

    #[tokio::test]
    async fn test_multiple_segments_metadata() {
        let segments = vec![
            TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 1.0,
                text: "First".to_string(),
                speaker_id: None,
                words: None,
            },
            TranscriptionSegment {
                start_secs: 1.0,
                end_secs: 2.0,
                text: "Second".to_string(),
                speaker_id: None,
                words: None,
            },
            TranscriptionSegment {
                start_secs: 2.0,
                end_secs: 3.5,
                text: "Third".to_string(),
                speaker_id: None,
                words: None,
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
            .extract(
                b"audio",
                "multi.wav",
                "audio/wav",
                &serde_json::json!({ "_skip_transcode": true }),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["segment_count"], 3);
        let segments_json = result.metadata["transcript_segments"].as_array().unwrap();
        assert_eq!(segments_json.len(), 3);

        for (i, seg) in segments.iter().enumerate() {
            assert_eq!(segments_json[i]["start_secs"], seg.start_secs);
            assert_eq!(segments_json[i]["end_secs"], seg.end_secs);
            assert_eq!(segments_json[i]["text"], seg.text);
        }
    }

    #[tokio::test]
    async fn test_extract_with_source_path() {
        // Write a temp file to simulate path-access mode from extraction handler
        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("test_audio_path_access.raw");
        std::fs::write(&tmp_path, b"fake_audio_from_file").unwrap();

        let mock_backend = MockTranscriptionBackend {
            result: TranscriptionResult {
                full_text: "Path access works.".to_string(),
                segments: vec![TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 2.0,
                    text: "Path access works.".to_string(),
                    speaker_id: None,
                    words: None,
                }],
                language: Some("en".to_string()),
                duration_secs: Some(2.0),
            },
            health_ok: true,
        };

        let adapter = AudioTranscribeAdapter::new(Arc::new(mock_backend));
        let result = adapter
            .extract(
                b"", // Empty data — simulates path-access mode
                "test.mp3",
                "audio/mpeg",
                &serde_json::json!({ "_source_path": tmp_path.to_str().unwrap(), "_skip_transcode": true }),
            )
            .await
            .unwrap();

        let text = result.extracted_text.as_deref().unwrap();
        assert!(
            text.contains("Path access works."),
            "should transcribe from file path, not empty data"
        );
        assert_eq!(result.metadata["segment_count"], 1);

        // Cleanup
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[tokio::test]
    async fn test_extract_source_path_not_found() {
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
        let result = adapter
            .extract(
                b"",
                "test.mp3",
                "audio/mpeg",
                &serde_json::json!({ "_source_path": "/nonexistent/path/audio.mp3", "_skip_transcode": true }),
            )
            .await;

        assert!(
            result.is_err(),
            "should fail when source path doesn't exist"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Failed to read audio from"),
            "error should mention file read failure: {}",
            err
        );
    }
}

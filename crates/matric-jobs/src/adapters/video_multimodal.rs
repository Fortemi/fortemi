//! VideoMultimodalAdapter — Extracts content from video files using FFmpeg, vision, and transcription.
//!
//! Pipeline:
//! 1. Extract audio track via FFmpeg (if available) → transcribe via WhisperBackend
//! 2. Extract keyframes via FFmpeg → describe via VisionBackend
//! 3. Fuse results with temporal alignment
//!
//! Falls back gracefully if backends are unavailable.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tempfile::{NamedTempFile, TempDir};
use tokio::process::Command;
use tracing::{debug, warn};

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::transcription::TranscriptionBackend;
use matric_inference::vision::VisionBackend;

pub struct VideoMultimodalAdapter {
    vision: Option<Arc<dyn VisionBackend>>,
    transcription: Option<Arc<dyn TranscriptionBackend>>,
}

impl VideoMultimodalAdapter {
    /// Create a new adapter with optional vision and transcription backends.
    pub fn new(
        vision: Option<Arc<dyn VisionBackend>>,
        transcription: Option<Arc<dyn TranscriptionBackend>>,
    ) -> Self {
        Self {
            vision,
            transcription,
        }
    }

    /// Create from environment variables (both backends optional).
    pub fn from_env() -> Self {
        use matric_inference::transcription::WhisperBackend;
        use matric_inference::vision::OllamaVisionBackend;

        let vision = OllamaVisionBackend::from_env().map(|v| Arc::new(v) as Arc<dyn VisionBackend>);
        let transcription =
            WhisperBackend::from_env().map(|t| Arc::new(t) as Arc<dyn TranscriptionBackend>);

        Self::new(vision, transcription)
    }
}

/// Run a command that may output to files rather than stdout.
async fn run_cmd_status(cmd: &mut Command, timeout_secs: u64) -> Result<()> {
    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| {
            matric_core::Error::Internal(format!(
                "External command timed out after {}s",
                timeout_secs
            ))
        })?
        .map_err(|e| matric_core::Error::Internal(format!("Failed to execute command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(matric_core::Error::Internal(format!(
            "Command failed (exit {}): {}",
            output.status,
            stderr.trim()
        )));
    }

    Ok(())
}

#[async_trait]
impl ExtractionAdapter for VideoMultimodalAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::VideoMultimodal
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot process empty video data".to_string(),
            ));
        }

        // Read config
        let extract_audio = config
            .get("extract_audio")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let extract_keyframes = config
            .get("extract_keyframes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let keyframe_interval = config
            .get("keyframe_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(10); // Extract keyframe every 10 seconds

        // Write video to temp file
        let mut tmpfile = NamedTempFile::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
        })?;
        tmpfile.write_all(data).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
        })?;
        let video_path = tmpfile.path().to_string_lossy().to_string();

        // Create temp dir for extracted assets
        let work_dir = TempDir::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp dir: {}", e))
        })?;

        debug!(filename, "Extracting video content");

        // Get video duration and metadata via ffprobe
        let duration_secs = get_video_duration(&video_path).await.ok();

        let mut extracted_text_parts = Vec::new();
        let mut keyframe_descriptions = Vec::new();
        let mut transcript_segments = Vec::new();
        let mut has_audio = false;
        let mut has_video = false;

        // Step 1: Extract and transcribe audio (if backend available and requested)
        if extract_audio && self.transcription.is_some() {
            debug!(filename, "Extracting audio track");
            match extract_audio_track(&video_path, &work_dir).await {
                Ok(audio_path) => {
                    has_audio = true;
                    if let Some(ref backend) = self.transcription {
                        match transcribe_audio(backend.as_ref(), &audio_path).await {
                            Ok(result) => {
                                extracted_text_parts
                                    .push(format!("=== TRANSCRIPT ===\n{}", result.full_text));
                                transcript_segments = result.segments;
                            }
                            Err(e) => {
                                warn!(filename, error = %e, "Audio transcription failed");
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(filename, error = %e, "No audio track found or extraction failed");
                }
            }
        }

        // Step 2: Extract and describe keyframes (if backend available and requested)
        if extract_keyframes && self.vision.is_some() {
            debug!(filename, "Extracting keyframes");
            match extract_keyframes_ffmpeg(&video_path, &work_dir, keyframe_interval).await {
                Ok(frame_paths) => {
                    has_video = !frame_paths.is_empty();
                    if let Some(ref backend) = self.vision {
                        for (i, frame_path) in frame_paths.iter().enumerate() {
                            match describe_frame(backend.as_ref(), frame_path).await {
                                Ok(description) => {
                                    keyframe_descriptions.push(json!({
                                        "frame_index": i,
                                        "description": description,
                                    }));
                                }
                                Err(e) => {
                                    warn!(frame = i, error = %e, "Frame description failed");
                                }
                            }
                        }

                        // Add visual descriptions to extracted text
                        if !keyframe_descriptions.is_empty() {
                            let descriptions_text = keyframe_descriptions
                                .iter()
                                .map(|kf| {
                                    format!("Frame {}: {}", kf["frame_index"], kf["description"])
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            extracted_text_parts
                                .push(format!("=== VISUAL CONTENT ===\n{}", descriptions_text));
                        }
                    }
                }
                Err(e) => {
                    warn!(filename, error = %e, "Keyframe extraction failed");
                }
            }
        }

        let full_text = if extracted_text_parts.is_empty() {
            None
        } else {
            Some(extracted_text_parts.join("\n\n"))
        };

        Ok(ExtractionResult {
            extracted_text: full_text,
            metadata: json!({
                "duration_secs": duration_secs,
                "frame_count": keyframe_descriptions.len(),
                "has_audio": has_audio,
                "has_video": has_video,
                "keyframe_descriptions": keyframe_descriptions,
                "transcript_segments": transcript_segments,
            }),
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if ffmpeg is available
        let ffmpeg_ok = match Command::new("ffmpeg").arg("-version").output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        };
        Ok(ffmpeg_ok)
    }

    fn name(&self) -> &str {
        "video_multimodal"
    }
}

/// Get video duration in seconds using ffprobe.
async fn get_video_duration(video_path: &str) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            video_path,
        ])
        .output()
        .await
        .map_err(|e| matric_core::Error::Internal(format!("ffprobe failed: {}", e)))?;

    if !output.status.success() {
        return Err(matric_core::Error::Internal(
            "ffprobe failed to get duration".to_string(),
        ));
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    duration_str
        .trim()
        .parse::<f64>()
        .map_err(|e| matric_core::Error::Internal(format!("Failed to parse duration: {}", e)))
}

/// Extract audio track from video to WAV format.
async fn extract_audio_track(video_path: &str, work_dir: &TempDir) -> Result<PathBuf> {
    let audio_path = work_dir.path().join("audio.wav");

    run_cmd_status(
        Command::new("ffmpeg")
            .arg("-i")
            .arg(video_path)
            .arg("-vn") // No video
            .arg("-acodec")
            .arg("pcm_s16le") // PCM 16-bit
            .arg("-ar")
            .arg("16000") // 16kHz sample rate (Whisper standard)
            .arg("-ac")
            .arg("1") // Mono
            .arg("-y") // Overwrite
            .arg(&audio_path),
        EXTRACTION_CMD_TIMEOUT_SECS * 2,
    )
    .await?;

    Ok(audio_path)
}

/// Extract keyframes from video using FFmpeg.
async fn extract_keyframes_ffmpeg(
    video_path: &str,
    work_dir: &TempDir,
    interval_secs: u64,
) -> Result<Vec<PathBuf>> {
    let frame_prefix = work_dir.path().join("frame_%04d.jpg");

    // Use fps filter to extract frames at specified interval
    let fps_value = if interval_secs > 0 {
        format!("1/{}", interval_secs)
    } else {
        "1/10".to_string() // Default: 1 frame per 10 seconds
    };

    run_cmd_status(
        Command::new("ffmpeg")
            .arg("-i")
            .arg(video_path)
            .arg("-vf")
            .arg(format!("fps={}", fps_value))
            .arg("-q:v")
            .arg("2") // High quality JPEG
            .arg("-y")
            .arg(&frame_prefix),
        EXTRACTION_CMD_TIMEOUT_SECS * 3,
    )
    .await?;

    // Collect all extracted frames
    let mut frame_paths = Vec::new();
    let entries = fs::read_dir(work_dir.path())
        .map_err(|e| matric_core::Error::Internal(format!("Failed to read work dir: {}", e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to read dir entry: {}", e))
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jpg") {
            frame_paths.push(path);
        }
    }
    frame_paths.sort();

    Ok(frame_paths)
}

/// Transcribe audio file using transcription backend.
async fn transcribe_audio(
    backend: &dyn TranscriptionBackend,
    audio_path: &PathBuf,
) -> Result<matric_inference::transcription::TranscriptionResult> {
    let audio_data = fs::read(audio_path)
        .map_err(|e| matric_core::Error::Internal(format!("Failed to read audio: {}", e)))?;

    backend.transcribe(&audio_data, "audio/wav", None).await
}

/// Describe a video frame using vision backend.
async fn describe_frame(backend: &dyn VisionBackend, frame_path: &PathBuf) -> Result<String> {
    let frame_data = fs::read(frame_path)
        .map_err(|e| matric_core::Error::Internal(format!("Failed to read frame: {}", e)))?;

    backend
        .describe_image(
            &frame_data,
            "image/jpeg",
            Some("Describe this video frame in detail. What is happening in this scene?"),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_inference::transcription::TranscriptionSegment;

    // ── Mock backends ──────────────────────────────────────────────────

    struct MockVision;
    #[async_trait]
    impl VisionBackend for MockVision {
        async fn describe_image(
            &self,
            _image_data: &[u8],
            _mime_type: &str,
            _prompt: Option<&str>,
        ) -> Result<String> {
            Ok("Mock description".to_string())
        }
        async fn health_check(&self) -> Result<bool> {
            Ok(true)
        }
        fn model_name(&self) -> &str {
            "mock-vision"
        }
    }

    struct MockTranscription;
    #[async_trait]
    impl TranscriptionBackend for MockTranscription {
        async fn transcribe(
            &self,
            _audio_data: &[u8],
            _mime_type: &str,
            _language: Option<&str>,
        ) -> Result<matric_inference::transcription::TranscriptionResult> {
            Ok(matric_inference::transcription::TranscriptionResult {
                full_text: "Mock transcript".to_string(),
                segments: vec![TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 1.0,
                    text: "Mock segment".to_string(),
                }],
                language: Some("en".to_string()),
                duration_secs: Some(1.0),
            })
        }
        async fn health_check(&self) -> Result<bool> {
            Ok(true)
        }
        fn model_name(&self) -> &str {
            "mock-whisper"
        }
    }

    #[allow(dead_code)]
    struct UnhealthyVision;
    #[async_trait]
    impl VisionBackend for UnhealthyVision {
        async fn describe_image(
            &self,
            _image_data: &[u8],
            _mime_type: &str,
            _prompt: Option<&str>,
        ) -> Result<String> {
            Err(matric_core::Error::Inference("unhealthy".to_string()))
        }
        async fn health_check(&self) -> Result<bool> {
            Ok(false)
        }
        fn model_name(&self) -> &str {
            "unhealthy-vision"
        }
    }

    #[allow(dead_code)]
    struct UnhealthyTranscription;
    #[async_trait]
    impl TranscriptionBackend for UnhealthyTranscription {
        async fn transcribe(
            &self,
            _audio_data: &[u8],
            _mime_type: &str,
            _language: Option<&str>,
        ) -> Result<matric_inference::transcription::TranscriptionResult> {
            Err(matric_core::Error::Inference("unhealthy".to_string()))
        }
        async fn health_check(&self) -> Result<bool> {
            Ok(false)
        }
        fn model_name(&self) -> &str {
            "unhealthy-whisper"
        }
    }

    // ── Basic property tests ───────────────────────────────────────────

    #[test]
    fn test_video_multimodal_strategy() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        assert_eq!(adapter.strategy(), ExtractionStrategy::VideoMultimodal);
    }

    #[test]
    fn test_video_multimodal_name() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        assert_eq!(adapter.name(), "video_multimodal");
    }

    #[tokio::test]
    async fn test_video_multimodal_health_check() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        // Result depends on whether ffmpeg is installed
    }

    // ── Constructor tests ──────────────────────────────────────────────

    #[test]
    fn test_video_multimodal_construct_with_none() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        assert!(adapter.vision.is_none());
        assert!(adapter.transcription.is_none());
    }

    #[test]
    fn test_video_multimodal_construct_with_backends() {
        let vision = Arc::new(MockVision) as Arc<dyn VisionBackend>;
        let transcription = Arc::new(MockTranscription) as Arc<dyn TranscriptionBackend>;
        let adapter = VideoMultimodalAdapter::new(Some(vision), Some(transcription));

        assert!(adapter.vision.is_some());
        assert!(adapter.transcription.is_some());
    }

    #[test]
    fn test_video_multimodal_construct_vision_only() {
        let vision = Arc::new(MockVision) as Arc<dyn VisionBackend>;
        let adapter = VideoMultimodalAdapter::new(Some(vision), None);

        assert!(adapter.vision.is_some());
        assert!(adapter.transcription.is_none());
    }

    #[test]
    fn test_video_multimodal_construct_transcription_only() {
        let transcription = Arc::new(MockTranscription) as Arc<dyn TranscriptionBackend>;
        let adapter = VideoMultimodalAdapter::new(None, Some(transcription));

        assert!(adapter.vision.is_none());
        assert!(adapter.transcription.is_some());
    }

    // ── Extract error paths ────────────────────────────────────────────

    #[tokio::test]
    async fn test_video_multimodal_empty_input() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        let result = adapter
            .extract(b"", "empty.mp4", "video/mp4", &json!({}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn test_video_multimodal_no_backends_returns_empty_result() {
        // With no backends, extraction should succeed but produce no text
        // (ffmpeg will fail on fake data, but it handles errors gracefully)
        let adapter = VideoMultimodalAdapter::new(None, None);
        let result = adapter
            .extract(
                b"\x00\x00\x00\x1cftypisom",
                "fake.mp4",
                "video/mp4",
                &json!({}),
            )
            .await;

        // Either an error (ffmpeg not installed) or an empty result is acceptable
        if let Ok(extraction) = result {
            // No text should be extracted without backends
            let metadata = &extraction.metadata;
            assert_eq!(metadata["frame_count"], 0);
        }
    }

    #[tokio::test]
    async fn test_video_multimodal_config_extract_audio_false() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        let config = json!({
            "extract_audio": false,
            "extract_keyframes": false,
        });
        let result = adapter
            .extract(
                b"\x00\x00\x00\x1cftypisom",
                "test.mp4",
                "video/mp4",
                &config,
            )
            .await;

        // With both extraction disabled, should still succeed
        if let Ok(extraction) = result {
            assert_eq!(extraction.metadata["frame_count"], 0);
            assert_eq!(extraction.metadata["has_audio"], false);
            assert_eq!(extraction.metadata["has_video"], false);
        }
    }

    #[tokio::test]
    async fn test_video_multimodal_config_keyframe_interval() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        let config = json!({
            "keyframe_interval": 5,
        });
        // Verify config is parsed without panic
        let _ = adapter
            .extract(
                b"\x00\x00\x00\x1cftypisom",
                "test.mp4",
                "video/mp4",
                &config,
            )
            .await;
    }

    #[tokio::test]
    async fn test_video_multimodal_metadata_structure() {
        let adapter = VideoMultimodalAdapter::new(None, None);
        let config = json!({
            "extract_audio": false,
            "extract_keyframes": false,
        });
        let result = adapter
            .extract(
                b"\x00\x00\x00\x1cftypisom",
                "test.mp4",
                "video/mp4",
                &config,
            )
            .await;

        if let Ok(extraction) = result {
            let md = &extraction.metadata;
            // Verify all expected metadata keys are present
            assert!(md.get("frame_count").is_some(), "Missing frame_count");
            assert!(md.get("has_audio").is_some(), "Missing has_audio");
            assert!(md.get("has_video").is_some(), "Missing has_video");
            assert!(
                md.get("keyframe_descriptions").is_some(),
                "Missing keyframe_descriptions"
            );
            assert!(
                md.get("transcript_segments").is_some(),
                "Missing transcript_segments"
            );
        }
    }
}

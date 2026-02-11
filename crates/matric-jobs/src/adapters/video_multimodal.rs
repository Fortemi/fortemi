//! VideoMultimodalAdapter — Extracts content from video files using FFmpeg, vision, and transcription.
//!
//! Pipeline:
//! 1. Extract audio track via FFmpeg (if available) → transcribe via WhisperBackend
//! 2. Extract keyframes via FFmpeg (interval, scene detection, or hybrid) → describe via VisionBackend
//! 3. Frame-to-frame temporal context: each frame description includes previous frames for continuity
//! 4. Align transcript segments with keyframe timestamps for coherent multimodal output
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
use matric_core::{
    ExtractionAdapter, ExtractionResult, ExtractionStrategy, KeyframeStrategy, Result,
};
use matric_inference::transcription::TranscriptionBackend;
use matric_inference::vision::VisionBackend;

/// Maximum number of previous frame descriptions to include as temporal context.
const TEMPORAL_CONTEXT_WINDOW: usize = 3;

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

/// Parse KeyframeStrategy from extraction config JSON.
fn parse_keyframe_strategy(config: &JsonValue) -> KeyframeStrategy {
    // Try structured strategy first
    if let Some(strategy_val) = config.get("keyframe_strategy") {
        if let Ok(strategy) = serde_json::from_value::<KeyframeStrategy>(strategy_val.clone()) {
            return strategy;
        }
    }

    // Fall back to legacy keyframe_interval field
    let interval = config
        .get("keyframe_interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(10);

    KeyframeStrategy::Interval {
        every_n_secs: interval,
    }
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
        let keyframe_strategy = parse_keyframe_strategy(config);

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

        debug!(filename, ?keyframe_strategy, "Extracting video content");

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

        // Step 2: Extract and describe keyframes with temporal context
        if extract_keyframes && self.vision.is_some() {
            debug!(filename, "Extracting keyframes");
            match extract_keyframes_ffmpeg(&video_path, &work_dir, &keyframe_strategy).await {
                Ok(frame_entries) => {
                    has_video = !frame_entries.is_empty();
                    if let Some(ref backend) = self.vision {
                        // Build descriptions with sliding temporal context window
                        let mut prev_descriptions: Vec<String> = Vec::new();

                        for (i, entry) in frame_entries.iter().enumerate() {
                            // Build context from transcript segments near this frame's timestamp
                            let transcript_context = get_transcript_context_for_frame(
                                entry.timestamp_secs,
                                &transcript_segments,
                            );

                            match describe_frame_with_context(
                                backend.as_ref(),
                                &entry.path,
                                &prev_descriptions,
                                transcript_context.as_deref(),
                            )
                            .await
                            {
                                Ok(description) => {
                                    keyframe_descriptions.push(json!({
                                        "frame_index": i,
                                        "timestamp_secs": entry.timestamp_secs,
                                        "description": description,
                                    }));

                                    // Update sliding window
                                    prev_descriptions.push(description.clone());
                                    if prev_descriptions.len() > TEMPORAL_CONTEXT_WINDOW {
                                        prev_descriptions.remove(0);
                                    }
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
                                    let ts = kf["timestamp_secs"]
                                        .as_f64()
                                        .map(|t| format!(" [{:.1}s]", t))
                                        .unwrap_or_default();
                                    format!(
                                        "Frame {}{}: {}",
                                        kf["frame_index"], ts, kf["description"]
                                    )
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
                "keyframe_strategy": serde_json::to_value(&keyframe_strategy).ok(),
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

/// A keyframe extracted from video with its timestamp.
struct FrameEntry {
    path: PathBuf,
    timestamp_secs: f64,
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

/// Extract keyframes from video using the configured strategy.
///
/// Returns a list of frame entries with paths and approximate timestamps.
async fn extract_keyframes_ffmpeg(
    video_path: &str,
    work_dir: &TempDir,
    strategy: &KeyframeStrategy,
) -> Result<Vec<FrameEntry>> {
    let frame_prefix = work_dir.path().join("frame_%04d.jpg");
    let showinfo_log = work_dir.path().join("showinfo.log");

    // Build FFmpeg filter based on strategy
    let vf_filter = match strategy {
        KeyframeStrategy::Interval { every_n_secs } => {
            let interval = if *every_n_secs > 0 { *every_n_secs } else { 10 };
            format!("fps=1/{},showinfo", interval)
        }
        KeyframeStrategy::SceneDetection { threshold } => {
            let t = threshold.clamp(0.01, 1.0);
            format!("select='gt(scene\\,{})',showinfo", t)
        }
        KeyframeStrategy::Hybrid {
            scene_threshold,
            min_interval_secs,
        } => {
            let t = scene_threshold.clamp(0.01, 1.0);
            let min = if *min_interval_secs > 0 {
                *min_interval_secs
            } else {
                2
            };
            // Select scene changes but throttle to min_interval_secs apart
            format!("select='gt(scene\\,{t})*gte(t-prev_selected_t\\,{min})',showinfo")
        }
    };

    // Run ffmpeg with showinfo to capture timestamps
    // stderr contains showinfo output, redirect it to a log file
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(&vf_filter)
        .arg("-vsync")
        .arg("vfr") // Variable frame rate for scene detection
        .arg("-q:v")
        .arg("2") // High quality JPEG
        .arg("-y")
        .arg(&frame_prefix);

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS * 3),
        cmd.output(),
    )
    .await
    .map_err(|_| {
        matric_core::Error::Internal(format!(
            "FFmpeg timed out after {}s",
            EXTRACTION_CMD_TIMEOUT_SECS * 3
        ))
    })?
    .map_err(|e| matric_core::Error::Internal(format!("Failed to execute ffmpeg: {}", e)))?;

    // Parse showinfo timestamps from stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Save for debugging
    let _ = fs::write(&showinfo_log, stderr.as_bytes());
    let timestamps = parse_showinfo_timestamps(&stderr);

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

    // Pair frames with timestamps
    let frame_entries: Vec<FrameEntry> = frame_paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let timestamp_secs = timestamps.get(i).copied().unwrap_or_else(|| {
                // Fallback: estimate from interval if timestamps unavailable
                match strategy {
                    KeyframeStrategy::Interval { every_n_secs } => {
                        (i as f64) * (*every_n_secs as f64)
                    }
                    _ => i as f64, // Best guess for scene detection
                }
            });
            FrameEntry {
                path,
                timestamp_secs,
            }
        })
        .collect();

    Ok(frame_entries)
}

/// Parse timestamps from FFmpeg showinfo filter output.
///
/// Looks for lines like: `[Parsed_showinfo_1 ...] n:   0 pts:   1234 pts_time:1.234`
fn parse_showinfo_timestamps(stderr: &str) -> Vec<f64> {
    let mut timestamps = Vec::new();
    for line in stderr.lines() {
        if line.contains("pts_time:") {
            // Extract pts_time value
            if let Some(pos) = line.find("pts_time:") {
                let after = &line[pos + 9..];
                let value_str: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                if let Ok(ts) = value_str.parse::<f64>() {
                    timestamps.push(ts);
                }
            }
        }
    }
    timestamps
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

/// Find transcript segments that overlap with a keyframe timestamp.
///
/// Returns a short context string of nearby dialogue/speech.
fn get_transcript_context_for_frame(
    frame_timestamp: f64,
    segments: &[matric_inference::transcription::TranscriptionSegment],
) -> Option<String> {
    if segments.is_empty() {
        return None;
    }

    // Find segments within +/- 5 seconds of the frame
    let window = 5.0;
    let nearby: Vec<&str> = segments
        .iter()
        .filter(|s| {
            s.start_secs <= frame_timestamp + window && s.end_secs >= frame_timestamp - window
        })
        .map(|s| s.text.as_str())
        .collect();

    if nearby.is_empty() {
        None
    } else {
        Some(nearby.join(" "))
    }
}

/// Describe a video frame using vision backend with temporal context.
///
/// Includes descriptions of previous frames (sliding window) and nearby
/// transcript text to help the vision model produce continuity-aware descriptions.
async fn describe_frame_with_context(
    backend: &dyn VisionBackend,
    frame_path: &PathBuf,
    previous_descriptions: &[String],
    transcript_context: Option<&str>,
) -> Result<String> {
    let frame_data = fs::read(frame_path)
        .map_err(|e| matric_core::Error::Internal(format!("Failed to read frame: {}", e)))?;

    // Build a context-rich prompt
    let mut prompt_parts = Vec::new();
    prompt_parts
        .push("Describe this video frame in detail. What is happening in this scene?".to_string());

    if !previous_descriptions.is_empty() {
        prompt_parts.push("\nPrevious frames for continuity:".to_string());
        for (i, desc) in previous_descriptions.iter().enumerate() {
            let offset = previous_descriptions.len() - i;
            prompt_parts.push(format!("  [{} frame(s) ago]: {}", offset, desc));
        }
        prompt_parts
            .push("Describe what has changed or progressed since the previous frames.".to_string());
    }

    if let Some(transcript) = transcript_context {
        prompt_parts.push(format!("\nNearby audio/speech: \"{}\"", transcript));
        prompt_parts.push(
            "Align your visual description with the spoken content where relevant.".to_string(),
        );
    }

    let prompt = prompt_parts.join("\n");

    backend
        .describe_image(&frame_data, "image/jpeg", Some(&prompt))
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
            assert!(
                md.get("keyframe_strategy").is_some(),
                "Missing keyframe_strategy"
            );
        }
    }

    // ── KeyframeStrategy parsing tests ─────────────────────────────────

    #[test]
    fn test_parse_keyframe_strategy_default() {
        let config = json!({});
        let strategy = parse_keyframe_strategy(&config);
        assert!(matches!(
            strategy,
            KeyframeStrategy::Interval { every_n_secs: 10 }
        ));
    }

    #[test]
    fn test_parse_keyframe_strategy_legacy_interval() {
        let config = json!({ "keyframe_interval": 5 });
        let strategy = parse_keyframe_strategy(&config);
        assert!(matches!(
            strategy,
            KeyframeStrategy::Interval { every_n_secs: 5 }
        ));
    }

    #[test]
    fn test_parse_keyframe_strategy_scene_detection() {
        let config = json!({
            "keyframe_strategy": {
                "mode": "scene_detection",
                "threshold": 0.4
            }
        });
        let strategy = parse_keyframe_strategy(&config);
        match strategy {
            KeyframeStrategy::SceneDetection { threshold } => {
                assert!((threshold - 0.4).abs() < f64::EPSILON);
            }
            _ => panic!("Expected SceneDetection"),
        }
    }

    #[test]
    fn test_parse_keyframe_strategy_hybrid() {
        let config = json!({
            "keyframe_strategy": {
                "mode": "hybrid",
                "scene_threshold": 0.35,
                "min_interval_secs": 3
            }
        });
        let strategy = parse_keyframe_strategy(&config);
        match strategy {
            KeyframeStrategy::Hybrid {
                scene_threshold,
                min_interval_secs,
            } => {
                assert!((scene_threshold - 0.35).abs() < f64::EPSILON);
                assert_eq!(min_interval_secs, 3);
            }
            _ => panic!("Expected Hybrid"),
        }
    }

    // ── Temporal context tests ─────────────────────────────────────────

    #[test]
    fn test_get_transcript_context_for_frame_empty() {
        let segments: Vec<TranscriptionSegment> = vec![];
        let result = get_transcript_context_for_frame(5.0, &segments);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_transcript_context_for_frame_in_range() {
        let segments = vec![
            TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 3.0,
                text: "Hello world".to_string(),
            },
            TranscriptionSegment {
                start_secs: 3.0,
                end_secs: 6.0,
                text: "Second segment".to_string(),
            },
            TranscriptionSegment {
                start_secs: 20.0,
                end_secs: 25.0,
                text: "Far away".to_string(),
            },
        ];
        // Frame at 4.0s should pick up both first and second segments
        let result = get_transcript_context_for_frame(4.0, &segments);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("Hello world"));
        assert!(text.contains("Second segment"));
        assert!(!text.contains("Far away"));
    }

    #[test]
    fn test_get_transcript_context_for_frame_out_of_range() {
        let segments = vec![TranscriptionSegment {
            start_secs: 0.0,
            end_secs: 1.0,
            text: "Early".to_string(),
        }];
        // Frame at 100s should find nothing
        let result = get_transcript_context_for_frame(100.0, &segments);
        assert!(result.is_none());
    }

    // ── showinfo timestamp parsing tests ───────────────────────────────

    #[test]
    fn test_parse_showinfo_timestamps_valid() {
        let stderr = r#"
[Parsed_showinfo_1 @ 0x55a] n:   0 pts:      0 pts_time:0.000000
[Parsed_showinfo_1 @ 0x55a] n:   1 pts:  10000 pts_time:10.000000
[Parsed_showinfo_1 @ 0x55a] n:   2 pts:  20000 pts_time:20.000000
"#;
        let timestamps = parse_showinfo_timestamps(stderr);
        assert_eq!(timestamps.len(), 3);
        assert!((timestamps[0] - 0.0).abs() < 0.001);
        assert!((timestamps[1] - 10.0).abs() < 0.001);
        assert!((timestamps[2] - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_showinfo_timestamps_empty() {
        let timestamps = parse_showinfo_timestamps("nothing useful here");
        assert!(timestamps.is_empty());
    }
}

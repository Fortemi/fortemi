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

use matric_core::defaults::{EXTRACTION_CMD_TIMEOUT_SECS, VIDEO_MAX_KEYFRAMES};
use matric_core::{
    DerivedFile, ExtractionAdapter, ExtractionResult, ExtractionStrategy, KeyframeStrategy,
    ProgressFn, Result,
};
use matric_inference::transcription::TranscriptionBackend;
use matric_inference::vision::VisionBackend;

/// Maximum number of previous frame descriptions to include as temporal context.
const TEMPORAL_CONTEXT_WINDOW: usize = 3;

/// Writes frame descriptions to individual files in a work directory,
/// avoiding accumulation in memory. Each description is stored as a JSON
/// file (`desc_NNNN.json`) and read back at assembly time.
struct FrameDescriptionWriter {
    dir: PathBuf,
    count: usize,
}

impl FrameDescriptionWriter {
    fn new(work_dir: &std::path::Path) -> std::io::Result<Self> {
        let dir = work_dir.join("descriptions");
        fs::create_dir_all(&dir)?;
        Ok(Self { dir, count: 0 })
    }

    /// Write a single frame description to disk and return the count so far.
    fn write(&mut self, desc: &JsonValue) -> std::io::Result<usize> {
        let path = self.dir.join(format!("desc_{:04}.json", self.count));
        let data = serde_json::to_vec(desc)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, &data)?;
        self.count += 1;
        Ok(self.count)
    }

    /// Read all descriptions back from disk in order.
    fn read_all(&self) -> Vec<JsonValue> {
        let mut results = Vec::with_capacity(self.count);
        for i in 0..self.count {
            let path = self.dir.join(format!("desc_{:04}.json", i));
            if let Ok(data) = fs::read(&path) {
                if let Ok(val) = serde_json::from_slice(&data) {
                    results.push(val);
                }
            }
        }
        results
    }

    /// Number of descriptions written.
    fn len(&self) -> usize {
        self.count
    }
}

/// Parse checkpoint data from config, returning the set of already-completed frame indices.
///
/// The extraction handler injects `_checkpoint.completed_frames` when resuming a
/// partially-completed extraction job (Issue 6). Frames in this set are skipped.
fn parse_checkpoint(config: &JsonValue) -> std::collections::HashSet<u64> {
    config
        .get("_checkpoint")
        .and_then(|cp| cp.get("completed_frames"))
        .and_then(|arr| arr.as_array())
        .map(|indices| indices.iter().filter_map(|v| v.as_u64()).collect())
        .unwrap_or_default()
}

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

/// Read max_keyframes from config JSON, env var, or default.
fn read_max_keyframes(config: &JsonValue) -> u32 {
    // 1. Per-job config override
    if let Some(v) = config.get("max_keyframes").and_then(|v| v.as_u64()) {
        return (v as u32).max(1);
    }
    // 2. Environment variable
    if let Ok(v) = std::env::var(matric_core::defaults::ENV_VIDEO_MAX_KEYFRAMES) {
        if let Ok(n) = v.parse::<u32>() {
            return n.max(1);
        }
    }
    // 3. Default
    VIDEO_MAX_KEYFRAMES
}

/// Adjust keyframe strategy so it produces at most `max_keyframes` for the given duration.
///
/// For Interval/Hybrid strategies, increases the interval to `ceil(duration / max)`.
/// For SceneDetection, the budget is enforced after extraction by truncating.
fn apply_keyframe_budget(
    strategy: KeyframeStrategy,
    duration_secs: f64,
    max_keyframes: u32,
) -> KeyframeStrategy {
    if max_keyframes == 0 || duration_secs <= 0.0 {
        return strategy;
    }

    let estimated_frames = match &strategy {
        KeyframeStrategy::Interval { every_n_secs } => {
            let interval = if *every_n_secs > 0 { *every_n_secs } else { 10 };
            (duration_secs / interval as f64).ceil() as u32
        }
        KeyframeStrategy::Hybrid {
            min_interval_secs, ..
        } => {
            let interval = if *min_interval_secs > 0 {
                *min_interval_secs
            } else {
                2
            };
            // Worst case: scene change at every min_interval
            (duration_secs / interval as f64).ceil() as u32
        }
        KeyframeStrategy::SceneDetection { .. } => {
            // Can't predict scene detection count; budget enforced post-extraction
            return strategy;
        }
    };

    if estimated_frames <= max_keyframes {
        return strategy;
    }

    // Increase interval to fit within budget
    let new_interval = (duration_secs / max_keyframes as f64).ceil() as u64;

    match strategy {
        KeyframeStrategy::Interval { every_n_secs } => {
            debug!(
                original_interval = every_n_secs,
                new_interval, max_keyframes, duration_secs, "Keyframe budget: clamped interval"
            );
            KeyframeStrategy::Interval {
                every_n_secs: new_interval,
            }
        }
        KeyframeStrategy::Hybrid {
            scene_threshold,
            min_interval_secs,
        } => {
            debug!(
                original_min_interval = min_interval_secs,
                new_min_interval = new_interval,
                max_keyframes,
                duration_secs,
                "Keyframe budget: clamped hybrid min_interval"
            );
            KeyframeStrategy::Hybrid {
                scene_threshold,
                min_interval_secs: new_interval,
            }
        }
        other => other,
    }
}

/// Truncate a list of frame entries to at most `max` by evenly sampling.
fn truncate_frames(frames: Vec<FrameEntry>, max: usize) -> Vec<FrameEntry> {
    if frames.len() <= max {
        return frames;
    }
    let total = frames.len();
    let step = total as f64 / max as f64;
    (0..max)
        .map(|i| {
            let idx = (i as f64 * step).floor() as usize;
            idx.min(total - 1)
        })
        .map(|idx| {
            // We need to move out of the vec, so collect indices first
            FrameEntry {
                path: frames[idx].path.clone(),
                timestamp_secs: frames[idx].timestamp_secs,
            }
        })
        .collect()
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
        let has_source_path = config
            .get("_source_path")
            .and_then(|v| v.as_str())
            .is_some();
        if data.is_empty() && !has_source_path {
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
        let persist_keyframes = config
            .get("persist_keyframes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        // When true, skip vision LLM calls and store keyframes without descriptions.
        // KeyframeVision jobs will describe each frame atomically. (#526)
        let skip_vision = config
            .get("_skip_vision")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let keyframe_strategy = parse_keyframe_strategy(config);

        // Write video to temp file (unless _source_path is provided).
        // _tmpfile_guard keeps NamedTempFile alive so the OS doesn't delete it.
        let mut _tmpfile_guard: Option<NamedTempFile> = None;
        let video_path = if let Some(src) = config.get("_source_path").and_then(|v| v.as_str()) {
            src.to_string()
        } else if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "No video data and no _source_path provided".to_string(),
            ));
        } else {
            let mut tmpfile = NamedTempFile::new().map_err(|e| {
                matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
            })?;
            tmpfile.write_all(data).map_err(|e| {
                matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
            })?;
            let path = tmpfile.path().to_string_lossy().to_string();
            _tmpfile_guard = Some(tmpfile);
            path
        };

        // Create temp dir for extracted assets
        let work_dir = TempDir::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp dir: {}", e))
        })?;

        debug!(filename, ?keyframe_strategy, "Extracting video content");

        // Get video duration and metadata via ffprobe
        let duration_secs = get_video_duration(&video_path).await.ok();

        // Apply keyframe budget to prevent runaway processing on long videos
        let max_keyframes = read_max_keyframes(config);
        let keyframe_strategy = if let Some(dur) = duration_secs {
            apply_keyframe_budget(keyframe_strategy, dur, max_keyframes)
        } else {
            keyframe_strategy
        };

        // Write descriptions to disk incrementally to cap memory usage
        let mut desc_writer = FrameDescriptionWriter::new(work_dir.path()).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create description writer: {}", e))
        })?;
        let mut transcript_segments = Vec::new();
        let mut has_audio = false;
        let mut has_video = false;
        let mut derived_files: Vec<DerivedFile> = Vec::new();

        let mut transcript_text: Option<String> = None;
        let mut transcript_language: Option<String> = None;

        // Step 1: Extract and transcribe audio (if backend available and requested)
        if extract_audio && self.transcription.is_some() {
            debug!(filename, "Extracting audio track");
            match extract_audio_track(&video_path, &work_dir).await {
                Ok(audio_path) => {
                    has_audio = true;

                    // Persist extracted audio as a derived file
                    if let Ok(audio_bytes) = fs::read(&audio_path) {
                        let base_name = filename
                            .rsplit('/')
                            .next()
                            .unwrap_or(filename)
                            .rsplit_once('.')
                            .map(|(n, _)| n)
                            .unwrap_or(filename);
                        derived_files.push(DerivedFile {
                            filename: format!("{}_audio.wav", base_name),
                            content_type: "audio/wav".to_string(),
                            data: audio_bytes,
                            derivation_type: "audio_track".to_string(),
                            ai_description: None,
                            metadata: None,
                            source_path: None,
                        });
                    }

                    if let Some(ref backend) = self.transcription {
                        match transcribe_audio(backend.as_ref(), &audio_path).await {
                            Ok(result) => {
                                transcript_text = Some(result.full_text);
                                transcript_language = result.language;
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
            let completed_frames = parse_checkpoint(config);
            if !completed_frames.is_empty() {
                debug!(
                    completed = completed_frames.len(),
                    "Checkpoint: resuming with {} completed frames",
                    completed_frames.len()
                );
            }
            let base_name = filename
                .rsplit('/')
                .next()
                .unwrap_or(filename)
                .rsplit_once('.')
                .map(|(n, _)| n)
                .unwrap_or(filename);
            match extract_keyframes_ffmpeg(&video_path, &work_dir, &keyframe_strategy).await {
                Ok(frame_entries) => {
                    // Apply budget truncation for scene detection (interval already adjusted above)
                    let frame_entries = truncate_frames(frame_entries, max_keyframes as usize);
                    has_video = !frame_entries.is_empty();

                    if skip_vision {
                        // Atomic pipeline (#526): persist keyframe JPEGs without vision descriptions.
                        // KeyframeVision jobs will describe each frame independently.
                        if persist_keyframes {
                            for (i, entry) in frame_entries.iter().enumerate() {
                                derived_files.push(DerivedFile {
                                    filename: format!("{}_keyframe_{:04}.jpg", base_name, i),
                                    content_type: "image/jpeg".to_string(),
                                    data: Vec::new(),
                                    derivation_type: "keyframe".to_string(),
                                    ai_description: None,
                                    metadata: Some(json!({
                                        "frame_index": i,
                                        "timestamp_secs": entry.timestamp_secs,
                                    })),
                                    source_path: Some(entry.path.clone()),
                                });
                            }
                            debug!(
                                frame_count = frame_entries.len(),
                                "Persisted {} keyframes without vision (skip_vision=true)",
                                frame_entries.len()
                            );
                        }
                    } else if let Some(ref backend) = self.vision {
                        // Build descriptions with sliding temporal context window
                        let mut prev_descriptions: Vec<String> = Vec::new();

                        for (i, entry) in frame_entries.iter().enumerate() {
                            // Skip frames already completed in a previous run
                            if completed_frames.contains(&(i as u64)) {
                                debug!(frame = i, "Checkpoint: skipping completed frame");
                                continue;
                            }

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
                                    // Write description to disk (not memory)
                                    let desc_json = json!({
                                        "frame_index": i,
                                        "timestamp_secs": entry.timestamp_secs,
                                        "description": description,
                                    });
                                    if let Err(e) = desc_writer.write(&desc_json) {
                                        warn!(frame = i, error = %e, "Failed to write description to disk");
                                    }

                                    // Persist keyframe JPEG as derived attachment
                                    if persist_keyframes {
                                        derived_files.push(DerivedFile {
                                            filename: format!(
                                                "{}_keyframe_{:04}.jpg",
                                                base_name, i
                                            ),
                                            content_type: "image/jpeg".to_string(),
                                            data: Vec::new(), // read from source_path
                                            derivation_type: "keyframe".to_string(),
                                            ai_description: Some(description.clone()),
                                            metadata: Some(json!({
                                                "frame_index": i,
                                                "timestamp_secs": entry.timestamp_secs,
                                            })),
                                            source_path: Some(entry.path.clone()),
                                        });
                                    }

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
                    }
                }
                Err(e) => {
                    warn!(filename, error = %e, "Keyframe extraction failed");
                }
            }
        }

        // Probe media info for resolution, codec, bitrate
        let media_info = probe_media_info(&video_path).await.unwrap_or(json!({}));

        // Extract content-aware thumbnail
        let thumbnail_data = match extract_thumbnail(&video_path, &work_dir).await {
            Some(thumb_path) => fs::read(&thumb_path).ok(),
            None => None,
        };

        // Read all descriptions back from disk for assembly
        let keyframe_descriptions = desc_writer.read_all();

        let full_text = format_video_markdown(
            transcript_text.as_deref(),
            &keyframe_descriptions,
            duration_secs,
            transcript_language.as_deref(),
        );

        // Store bulk keyframe descriptions as a derived manifest file
        // instead of bloating the parent's extracted_metadata JSONB.
        if !keyframe_descriptions.is_empty() {
            let base_name = filename
                .rsplit('/')
                .next()
                .unwrap_or(filename)
                .rsplit_once('.')
                .map(|(n, _)| n)
                .unwrap_or(filename);
            let manifest = serde_json::to_vec_pretty(&json!({
                "keyframe_descriptions": keyframe_descriptions,
            }))
            .unwrap_or_default();
            derived_files.push(DerivedFile {
                filename: format!("{}_keyframes.json", base_name),
                content_type: "application/json".to_string(),
                data: manifest,
                derivation_type: "keyframe_manifest".to_string(),
                ai_description: None,
                metadata: None,
                source_path: None,
            });

            // Generate keyframe VTT mapping timestamps for sprite assembly (#525)
            push_keyframe_vtt(&mut derived_files, &keyframe_descriptions, base_name);
        }

        Ok(ExtractionResult {
            extracted_text: full_text,
            metadata: json!({
                "duration_secs": duration_secs,
                "frame_count": desc_writer.len(),
                "has_audio": has_audio,
                "has_video": has_video,
                "has_thumbnail": thumbnail_data.is_some(),
                "keyframe_strategy": serde_json::to_value(&keyframe_strategy).ok(),
                "transcript_segments": transcript_segments,
                "media_info": media_info,
            }),
            ai_description: None,
            preview_data: thumbnail_data,
            derived_files,
        })
    }

    async fn extract_with_progress(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        config: &JsonValue,
        progress: ProgressFn,
    ) -> Result<ExtractionResult> {
        let has_source_path = config
            .get("_source_path")
            .and_then(|v| v.as_str())
            .is_some();
        if data.is_empty() && !has_source_path {
            return Err(matric_core::Error::InvalidInput(
                "Cannot process empty video data".to_string(),
            ));
        }

        let extract_audio = config
            .get("extract_audio")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let extract_keyframes = config
            .get("extract_keyframes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let persist_keyframes = config
            .get("persist_keyframes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let skip_vision = config
            .get("_skip_vision")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let keyframe_strategy = parse_keyframe_strategy(config);

        // Use _source_path if provided (avoid RAM copy), otherwise write to temp file.
        // _tmpfile_guard keeps NamedTempFile alive so the OS doesn't delete it.
        let mut _tmpfile_guard: Option<NamedTempFile> = None;
        let video_path = if let Some(src) = config.get("_source_path").and_then(|v| v.as_str()) {
            src.to_string()
        } else {
            let mut tmpfile = NamedTempFile::new().map_err(|e| {
                matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
            })?;
            tmpfile.write_all(data).map_err(|e| {
                matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
            })?;
            let path = tmpfile.path().to_string_lossy().to_string();
            _tmpfile_guard = Some(tmpfile);
            path
        };
        let work_dir = TempDir::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp dir: {}", e))
        })?;

        progress(0, Some("Analyzing video"));
        let duration_secs = get_video_duration(&video_path).await.ok();

        // Apply keyframe budget to prevent runaway processing on long videos
        let max_keyframes = read_max_keyframes(config);
        let keyframe_strategy = if let Some(dur) = duration_secs {
            apply_keyframe_budget(keyframe_strategy, dur, max_keyframes)
        } else {
            keyframe_strategy
        };

        // Write descriptions to disk incrementally to cap memory usage
        let mut desc_writer = FrameDescriptionWriter::new(work_dir.path()).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create description writer: {}", e))
        })?;
        let mut transcript_segments = Vec::new();
        let mut has_audio = false;
        let mut has_video = false;
        let mut derived_files: Vec<DerivedFile> = Vec::new();
        let mut transcript_text: Option<String> = None;
        let mut transcript_language: Option<String> = None;

        // Phase 1: Audio extraction + transcription (0-20%)
        if extract_audio && self.transcription.is_some() {
            progress(5, Some("Extracting audio track"));
            match extract_audio_track(&video_path, &work_dir).await {
                Ok(audio_path) => {
                    has_audio = true;

                    // Persist extracted audio as a derived file
                    if let Ok(audio_bytes) = fs::read(&audio_path) {
                        let base_name = filename
                            .rsplit('/')
                            .next()
                            .unwrap_or(filename)
                            .rsplit_once('.')
                            .map(|(n, _)| n)
                            .unwrap_or(filename);
                        derived_files.push(DerivedFile {
                            filename: format!("{}_audio.wav", base_name),
                            content_type: "audio/wav".to_string(),
                            data: audio_bytes,
                            derivation_type: "audio_track".to_string(),
                            ai_description: None,
                            metadata: None,
                            source_path: None,
                        });
                    }

                    progress(10, Some("Transcribing audio"));
                    if let Some(ref backend) = self.transcription {
                        match transcribe_audio(backend.as_ref(), &audio_path).await {
                            Ok(result) => {
                                transcript_text = Some(result.full_text);
                                transcript_language = result.language;
                                transcript_segments = result.segments;
                                progress(20, Some("Transcription complete"));
                            }
                            Err(e) => {
                                warn!(filename, error = %e, "Audio transcription failed");
                                progress(20, Some("Transcription failed, continuing"));
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(filename, error = %e, "No audio track found");
                    progress(20, Some("No audio track"));
                }
            }
        } else {
            progress(20, Some("Audio extraction skipped"));
        }

        // Phase 2: Keyframe extraction + description (20-95%)
        if extract_keyframes && (skip_vision || self.vision.is_some()) {
            progress(22, Some("Extracting keyframes"));
            let completed_frames = parse_checkpoint(config);
            if !completed_frames.is_empty() {
                debug!(
                    completed = completed_frames.len(),
                    "Checkpoint: resuming with {} completed frames",
                    completed_frames.len()
                );
            }
            let base_name = filename
                .rsplit('/')
                .next()
                .unwrap_or(filename)
                .rsplit_once('.')
                .map(|(n, _)| n)
                .unwrap_or(filename);
            match extract_keyframes_ffmpeg(&video_path, &work_dir, &keyframe_strategy).await {
                Ok(frame_entries) => {
                    // Apply budget truncation for scene detection (interval already adjusted above)
                    let frame_entries = truncate_frames(frame_entries, max_keyframes as usize);
                    let total_frames = frame_entries.len();
                    has_video = total_frames > 0;

                    if skip_vision {
                        // Atomic pipeline (#526): persist keyframe JPEGs without vision.
                        // KeyframeVision jobs will describe each frame independently.
                        if persist_keyframes {
                            for (i, entry) in frame_entries.iter().enumerate() {
                                derived_files.push(DerivedFile {
                                    filename: format!("{}_keyframe_{:04}.jpg", base_name, i),
                                    content_type: "image/jpeg".to_string(),
                                    data: Vec::new(),
                                    derivation_type: "keyframe".to_string(),
                                    ai_description: None,
                                    metadata: Some(json!({
                                        "frame_index": i,
                                        "timestamp_secs": entry.timestamp_secs,
                                    })),
                                    source_path: Some(entry.path.clone()),
                                });
                            }
                            progress(
                                90,
                                Some(&format!(
                                    "Persisted {} keyframes (vision deferred to atomic jobs)",
                                    total_frames
                                )),
                            );
                        }
                    } else {
                        if total_frames > 0 {
                            let remaining = total_frames - completed_frames.len().min(total_frames);
                            progress(
                                25,
                                Some(&format!(
                                    "Describing {} keyframes ({} cached)",
                                    remaining,
                                    completed_frames.len()
                                )),
                            );
                        }

                        if let Some(ref backend) = self.vision {
                            let mut prev_descriptions: Vec<String> = Vec::new();

                            for (i, entry) in frame_entries.iter().enumerate() {
                                // Skip frames already completed in a previous run
                                if completed_frames.contains(&(i as u64)) {
                                    debug!(frame = i, "Checkpoint: skipping completed frame");
                                    continue;
                                }

                                // Report per-frame progress: map frame i/total to 25-95%
                                let frame_pct =
                                    25 + ((i as i64) * 70 / total_frames.max(1) as i64) as i32;
                                progress(
                                    frame_pct,
                                    Some(&format!(
                                        "Frame {}/{} ({:.0}s)",
                                        i + 1,
                                        total_frames,
                                        entry.timestamp_secs
                                    )),
                                );

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
                                        // Write description to disk (not memory)
                                        let desc_json = json!({
                                            "frame_index": i,
                                            "timestamp_secs": entry.timestamp_secs,
                                            "description": description,
                                        });
                                        if let Err(e) = desc_writer.write(&desc_json) {
                                            warn!(frame = i, error = %e, "Failed to write description to disk");
                                        }

                                        // Persist keyframe JPEG as derived attachment
                                        if persist_keyframes {
                                            derived_files.push(DerivedFile {
                                                filename: format!(
                                                    "{}_keyframe_{:04}.jpg",
                                                    base_name, i
                                                ),
                                                content_type: "image/jpeg".to_string(),
                                                data: Vec::new(),
                                                derivation_type: "keyframe".to_string(),
                                                ai_description: Some(description.clone()),
                                                metadata: Some(json!({
                                                    "frame_index": i,
                                                    "timestamp_secs": entry.timestamp_secs,
                                                })),
                                                source_path: Some(entry.path.clone()),
                                            });
                                        }

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
                        }
                    }
                }
                Err(e) => {
                    warn!(filename, error = %e, "Keyframe extraction failed");
                }
            }
        }

        progress(90, Some("Probing media info"));
        let media_info = probe_media_info(&video_path).await.unwrap_or(json!({}));

        progress(93, Some("Generating thumbnail"));
        let thumbnail_data = match extract_thumbnail(&video_path, &work_dir).await {
            Some(thumb_path) => fs::read(&thumb_path).ok(),
            None => None,
        };

        progress(97, Some("Assembling results"));

        // Read all descriptions back from disk for assembly
        let keyframe_descriptions = desc_writer.read_all();

        let full_text = format_video_markdown(
            transcript_text.as_deref(),
            &keyframe_descriptions,
            duration_secs,
            transcript_language.as_deref(),
        );

        // Store bulk keyframe descriptions as a derived manifest file
        // instead of bloating the parent's extracted_metadata JSONB.
        if !keyframe_descriptions.is_empty() {
            let base_name = filename
                .rsplit('/')
                .next()
                .unwrap_or(filename)
                .rsplit_once('.')
                .map(|(n, _)| n)
                .unwrap_or(filename);
            let manifest = serde_json::to_vec_pretty(&json!({
                "keyframe_descriptions": keyframe_descriptions,
            }))
            .unwrap_or_default();
            derived_files.push(DerivedFile {
                filename: format!("{}_keyframes.json", base_name),
                content_type: "application/json".to_string(),
                data: manifest,
                derivation_type: "keyframe_manifest".to_string(),
                ai_description: None,
                metadata: None,
                source_path: None,
            });

            // Generate keyframe VTT mapping timestamps for sprite assembly (#525)
            push_keyframe_vtt(&mut derived_files, &keyframe_descriptions, base_name);
        }

        progress(100, Some("Complete"));

        Ok(ExtractionResult {
            extracted_text: full_text,
            metadata: json!({
                "duration_secs": duration_secs,
                "frame_count": desc_writer.len(),
                "has_audio": has_audio,
                "has_video": has_video,
                "has_thumbnail": thumbnail_data.is_some(),
                "keyframe_strategy": serde_json::to_value(&keyframe_strategy).ok(),
                "transcript_segments": transcript_segments,
                "media_info": media_info,
            }),
            ai_description: None,
            preview_data: thumbnail_data,
            derived_files,
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

/// Format seconds as human-readable duration (e.g., "1m 30s", "2h 15m 42s").
pub(crate) fn format_duration(secs: f64) -> String {
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

/// Format seconds as timestamp (e.g., "0:00", "1:30", "1:05:42").
pub(crate) fn format_timestamp(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

/// Format a seconds value as a WebVTT timestamp (HH:MM:SS.mmm).
pub(crate) fn format_vtt_timestamp(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1_000;
    let ms = total_ms % 1_000;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// Build a WebVTT file mapping keyframe indices to their time ranges.
///
/// Each cue covers from the keyframe's timestamp to the next keyframe's
/// timestamp (or +interval for the last frame). The cue payload is the
/// keyframe filename so the ThumbnailSprite job can correlate frames.
pub(crate) fn build_keyframe_vtt(keyframe_descriptions: &[serde_json::Value]) -> String {
    if keyframe_descriptions.is_empty() {
        return String::new();
    }

    // Estimate interval from first two keyframes (fallback 10s)
    let interval = keyframe_descriptions
        .windows(2)
        .filter_map(|w| {
            let t0 = w[0].get("timestamp_secs").and_then(|v| v.as_f64());
            let t1 = w[1].get("timestamp_secs").and_then(|v| v.as_f64());
            match (t0, t1) {
                (Some(a), Some(b)) if b > a => Some(b - a),
                _ => None,
            }
        })
        .next()
        .unwrap_or(10.0);

    let mut vtt = String::from("WEBVTT\n\n");
    for (i, desc) in keyframe_descriptions.iter().enumerate() {
        let ts = desc
            .get("timestamp_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let end_ts = keyframe_descriptions
            .get(i + 1)
            .and_then(|d| d.get("timestamp_secs"))
            .and_then(|v| v.as_f64())
            .unwrap_or(ts + interval);

        vtt.push_str(&format!(
            "{}\n{} --> {}\nkeyframe_{:04}.jpg\n\n",
            i + 1,
            format_vtt_timestamp(ts),
            format_vtt_timestamp(end_ts),
            i,
        ));
    }
    vtt
}

/// Push a keyframe VTT derived file into the derived_files list.
fn push_keyframe_vtt(
    derived_files: &mut Vec<DerivedFile>,
    keyframe_descriptions: &[serde_json::Value],
    base_name: &str,
) {
    let vtt = build_keyframe_vtt(keyframe_descriptions);
    if vtt.is_empty() {
        return;
    }
    derived_files.push(DerivedFile {
        filename: format!("{}_keyframes.vtt", base_name),
        content_type: "text/vtt".to_string(),
        data: vtt.into_bytes(),
        derivation_type: "keyframe_vtt".to_string(),
        ai_description: None,
        metadata: None,
        source_path: None,
    });
}

/// Assemble extraction results into properly formatted markdown.
pub(crate) fn format_video_markdown(
    transcript_text: Option<&str>,
    keyframe_descriptions: &[JsonValue],
    duration_secs: Option<f64>,
    language: Option<&str>,
) -> Option<String> {
    let has_transcript = transcript_text.is_some();
    let has_frames = !keyframe_descriptions.is_empty();

    if !has_transcript && !has_frames {
        return None;
    }

    let mut parts = Vec::new();

    // Metadata header
    let mut meta_items = Vec::new();
    if let Some(d) = duration_secs {
        meta_items.push(format!("**Duration**: {}", format_duration(d)));
    }
    if has_frames {
        meta_items.push(format!("**Frames**: {}", keyframe_descriptions.len()));
    }
    if let Some(lang) = language {
        meta_items.push(format!("**Language**: {}", lang));
    }
    if !meta_items.is_empty() {
        parts.push(meta_items.join(" | "));
    }

    // Transcript section
    if let Some(text) = transcript_text {
        parts.push("## Transcript".to_string());
        parts.push(text.to_string());
    }

    // Visual content section
    if has_frames {
        parts.push("## Visual Content".to_string());
        for (i, kf) in keyframe_descriptions.iter().enumerate() {
            let ts = kf["timestamp_secs"].as_f64().unwrap_or(0.0);
            let desc = kf["description"].as_str().unwrap_or("");
            parts.push(format!(
                "### Scene {} \u{2014} {}",
                i + 1,
                format_timestamp(ts)
            ));
            parts.push(desc.to_string());
        }
    }

    Some(parts.join("\n\n"))
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
            let timestamp_secs = timestamps.get(i).copied().unwrap_or({
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
pub(crate) fn get_transcript_context_for_frame(
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

/// Check if an MP4 file has the moov atom before the mdata atom (faststart-optimized).
///
/// Returns `true` if the file is already optimized or not an MP4.
pub async fn is_faststart(video_path: &str) -> Result<bool> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "format_tags=",
                "-of",
                "json",
                video_path,
            ])
            .output(),
    )
    .await
    .map_err(|_| matric_core::Error::Internal("ffprobe timed out".to_string()))?
    .map_err(|e| matric_core::Error::Internal(format!("ffprobe failed: {}", e)))?;

    if !output.status.success() {
        return Ok(true); // If we can't probe, assume it's fine
    }

    // Quick heuristic: use ffprobe atoms. If we can't determine, assume not optimized.
    // A more robust check reads the first bytes for moov vs mdat atom ordering,
    // but ffmpeg's -movflags +faststart is idempotent and fast, so we just run it.
    Ok(false)
}

/// Run MP4 faststart optimization on a file (moves moov atom to beginning).
///
/// Uses `ffmpeg -c copy -movflags +faststart` which only rearranges metadata
/// without re-encoding, making it extremely fast (seconds, not minutes).
///
/// Returns the path to the optimized file, or the original path if optimization
/// was not needed or failed.
pub async fn optimize_faststart(video_path: &str, work_dir: &TempDir) -> Result<String> {
    let output_path = work_dir
        .path()
        .join("faststart.mp4")
        .to_string_lossy()
        .to_string();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS),
        Command::new("ffmpeg")
            .args([
                "-i",
                video_path,
                "-c",
                "copy",
                "-movflags",
                "+faststart",
                "-y",
                &output_path,
            ])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            debug!(video_path, "MP4 faststart optimization succeeded");
            Ok(output_path)
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                video_path,
                error = %stderr.trim(),
                "MP4 faststart optimization failed, using original"
            );
            Ok(video_path.to_string())
        }
        Ok(Err(e)) => {
            warn!(video_path, error = %e, "Failed to run ffmpeg for faststart");
            Ok(video_path.to_string())
        }
        Err(_) => {
            warn!(video_path, "ffmpeg faststart timed out, using original");
            Ok(video_path.to_string())
        }
    }
}

/// Probe video file for detailed media info using ffprobe.
///
/// Returns metadata including resolution, codec, bitrate, and audio codec.
pub async fn probe_media_info(video_path: &str) -> Result<JsonValue> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                video_path,
            ])
            .output(),
    )
    .await
    .map_err(|_| matric_core::Error::Internal("ffprobe timed out".to_string()))?
    .map_err(|e| matric_core::Error::Internal(format!("ffprobe failed: {}", e)))?;

    if !output.status.success() {
        return Ok(json!({}));
    }

    let probe_data: JsonValue = serde_json::from_slice(&output.stdout).unwrap_or(json!({}));

    // Extract key fields from ffprobe output
    let streams = probe_data.get("streams").and_then(|s| s.as_array());
    let format = probe_data.get("format");

    let mut info = json!({});

    // Find video stream
    if let Some(streams) = streams {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|v| v.as_str());
            match codec_type {
                Some("video") => {
                    info["width"] = stream.get("width").cloned().unwrap_or(json!(null));
                    info["height"] = stream.get("height").cloned().unwrap_or(json!(null));
                    info["codec"] = stream.get("codec_name").cloned().unwrap_or(json!(null));
                    info["frame_rate"] = stream
                        .get("r_frame_rate")
                        .and_then(|v| v.as_str())
                        .and_then(|s| {
                            let parts: Vec<&str> = s.split('/').collect();
                            if parts.len() == 2 {
                                let num: f64 = parts[0].parse().ok()?;
                                let den: f64 = parts[1].parse().ok()?;
                                if den > 0.0 {
                                    Some(json!((num / den * 100.0).round() / 100.0))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or(json!(null));
                }
                Some("audio") => {
                    info["audio_codec"] = stream.get("codec_name").cloned().unwrap_or(json!(null));
                    info["audio_sample_rate"] =
                        stream.get("sample_rate").cloned().unwrap_or(json!(null));
                }
                _ => {}
            }
        }
    }

    // Extract format-level info
    if let Some(format) = format {
        if let Some(bitrate) = format.get("bit_rate").and_then(|v| v.as_str()) {
            if let Ok(bps) = bitrate.parse::<u64>() {
                info["bitrate_kbps"] = json!(bps / 1000);
            }
        }
    }

    Ok(info)
}

/// Extract a content-aware thumbnail from a video using FFmpeg's `thumbnail` filter.
///
/// The `thumbnail` filter analyzes N frames and selects the most visually
/// diverse frame based on histogram variance across color channels. Combined
/// with scene detection, this avoids black frames, transitions, and motion blur.
///
/// Returns the path to the best thumbnail JPEG, or `None` if extraction fails.
pub async fn extract_thumbnail(video_path: &str, work_dir: &TempDir) -> Option<PathBuf> {
    let output_path = work_dir.path().join("thumbnail.jpg");

    // Use scene detection + thumbnail filter for best results.
    // select='gt(scene,0.15)' filters to scene-change frames only,
    // thumbnail=50 picks the most visually interesting from each 50-frame group.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS),
        Command::new("ffmpeg")
            .args([
                "-i",
                video_path,
                "-vf",
                "select='gt(scene\\,0.15)',thumbnail=50",
                "-frames:v",
                "1",
                "-q:v",
                "2",
                "-y",
            ])
            .arg(&output_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() && output_path.exists() => {
            // Verify the thumbnail is not empty / corrupt
            if let Ok(meta) = fs::metadata(&output_path) {
                if meta.len() > 100 {
                    debug!(video_path, "Thumbnail extracted via scene+thumbnail filter");
                    return Some(output_path);
                }
            }
            // Fall through to simpler approach
            warn!(
                video_path,
                "Thumbnail filter produced empty file, trying fallback"
            );
        }
        _ => {
            debug!(
                video_path,
                "Scene+thumbnail filter failed, trying simple thumbnail"
            );
        }
    }

    // Fallback: just use the thumbnail filter without scene detection
    let fallback_path = work_dir.path().join("thumbnail_fallback.jpg");
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS),
        Command::new("ffmpeg")
            .args([
                "-i",
                video_path,
                "-vf",
                "thumbnail=100",
                "-frames:v",
                "1",
                "-q:v",
                "2",
                "-y",
            ])
            .arg(&fallback_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() && fallback_path.exists() => {
            if let Ok(meta) = fs::metadata(&fallback_path) {
                if meta.len() > 100 {
                    debug!(
                        video_path,
                        "Thumbnail extracted via fallback thumbnail filter"
                    );
                    return Some(fallback_path);
                }
            }
        }
        _ => {}
    }

    warn!(video_path, "Could not extract thumbnail from video");
    None
}

/// Generate a waveform visualization thumbnail from an audio file.
///
/// Creates a PNG image showing the audio waveform, suitable for use as
/// a visual thumbnail for audio-only content.
///
/// Returns the path to the waveform PNG, or `None` if generation fails.
pub async fn generate_audio_waveform(audio_path: &str, work_dir: &TempDir) -> Option<PathBuf> {
    let output_path = work_dir.path().join("waveform.png");

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS),
        Command::new("ffmpeg")
            .args([
                "-i",
                audio_path,
                "-filter_complex",
                "showwavespic=s=640x120:colors=0x3B82F6",
                "-frames:v",
                "1",
                "-y",
            ])
            .arg(&output_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() && output_path.exists() => {
            if let Ok(meta) = fs::metadata(&output_path) {
                if meta.len() > 100 {
                    debug!(audio_path, "Audio waveform thumbnail generated");
                    return Some(output_path);
                }
            }
        }
        _ => {}
    }

    warn!(audio_path, "Could not generate audio waveform thumbnail");
    None
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
                    speaker_id: None,
                    words: None,
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
                md.get("transcript_segments").is_some(),
                "Missing transcript_segments"
            );
            assert!(
                md.get("keyframe_strategy").is_some(),
                "Missing keyframe_strategy"
            );
            // keyframe_descriptions moved to derived keyframe_manifest file (Issue 5)
            assert!(
                md.get("keyframe_descriptions").is_none(),
                "keyframe_descriptions should be in derived manifest, not metadata"
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
                speaker_id: None,
                words: None,
            },
            TranscriptionSegment {
                start_secs: 3.0,
                end_secs: 6.0,
                text: "Second segment".to_string(),
                speaker_id: None,
                words: None,
            },
            TranscriptionSegment {
                start_secs: 20.0,
                end_secs: 25.0,
                text: "Far away".to_string(),
                speaker_id: None,
                words: None,
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
            speaker_id: None,
            words: None,
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

    // ── Keyframe budget tests ───────────────────────────────────────────

    #[test]
    fn test_apply_keyframe_budget_no_clamp_needed() {
        // 60s video at 10s interval = 6 frames, well under budget of 60
        let strategy = KeyframeStrategy::Interval { every_n_secs: 10 };
        let result = apply_keyframe_budget(strategy, 60.0, 60);
        match result {
            KeyframeStrategy::Interval { every_n_secs } => {
                assert_eq!(every_n_secs, 10);
            }
            _ => panic!("Expected Interval"),
        }
    }

    #[test]
    fn test_apply_keyframe_budget_clamps_interval() {
        // 7200s (2h) video at 10s interval = 720 frames, budget 60
        // Expected new interval: ceil(7200/60) = 120
        let strategy = KeyframeStrategy::Interval { every_n_secs: 10 };
        let result = apply_keyframe_budget(strategy, 7200.0, 60);
        match result {
            KeyframeStrategy::Interval { every_n_secs } => {
                assert_eq!(every_n_secs, 120);
            }
            _ => panic!("Expected Interval"),
        }
    }

    #[test]
    fn test_apply_keyframe_budget_clamps_hybrid() {
        // 3600s (1h) video, hybrid with min_interval 2s = worst case 1800 frames, budget 30
        // Expected new min_interval: ceil(3600/30) = 120
        let strategy = KeyframeStrategy::Hybrid {
            scene_threshold: 0.3,
            min_interval_secs: 2,
        };
        let result = apply_keyframe_budget(strategy, 3600.0, 30);
        match result {
            KeyframeStrategy::Hybrid {
                scene_threshold,
                min_interval_secs,
            } => {
                assert_eq!(min_interval_secs, 120);
                assert!((scene_threshold - 0.3).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Hybrid"),
        }
    }

    #[test]
    fn test_apply_keyframe_budget_scene_detection_passthrough() {
        // Scene detection can't be pre-budgeted, should pass through unchanged
        let strategy = KeyframeStrategy::SceneDetection { threshold: 0.4 };
        let result = apply_keyframe_budget(strategy, 7200.0, 60);
        match result {
            KeyframeStrategy::SceneDetection { threshold } => {
                assert!((threshold - 0.4).abs() < f64::EPSILON);
            }
            _ => panic!("Expected SceneDetection"),
        }
    }

    #[test]
    fn test_apply_keyframe_budget_zero_duration() {
        let strategy = KeyframeStrategy::Interval { every_n_secs: 10 };
        let result = apply_keyframe_budget(strategy, 0.0, 60);
        match result {
            KeyframeStrategy::Interval { every_n_secs } => {
                assert_eq!(every_n_secs, 10); // unchanged
            }
            _ => panic!("Expected Interval"),
        }
    }

    #[test]
    fn test_truncate_frames_under_budget() {
        let frames: Vec<FrameEntry> = (0..5)
            .map(|i| FrameEntry {
                path: PathBuf::from(format!("frame_{}.jpg", i)),
                timestamp_secs: i as f64 * 10.0,
            })
            .collect();
        let result = truncate_frames(frames, 10);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_truncate_frames_over_budget() {
        let frames: Vec<FrameEntry> = (0..100)
            .map(|i| FrameEntry {
                path: PathBuf::from(format!("frame_{}.jpg", i)),
                timestamp_secs: i as f64 * 1.0,
            })
            .collect();
        let result = truncate_frames(frames, 10);
        assert_eq!(result.len(), 10);
        // First frame should be at index 0
        assert!((result[0].timestamp_secs - 0.0).abs() < f64::EPSILON);
        // Last frame should be near the end
        assert!(result[9].timestamp_secs >= 80.0);
    }

    #[test]
    fn test_read_max_keyframes_from_config() {
        let config = json!({"max_keyframes": 30});
        assert_eq!(read_max_keyframes(&config), 30);
    }

    #[test]
    fn test_read_max_keyframes_default() {
        let config = json!({});
        assert_eq!(read_max_keyframes(&config), VIDEO_MAX_KEYFRAMES);
    }

    #[test]
    fn test_read_max_keyframes_minimum_one() {
        let config = json!({"max_keyframes": 0});
        assert_eq!(read_max_keyframes(&config), 1);
    }

    // ── Checkpoint parsing tests ──────────────────────────────────────

    #[test]
    fn test_parse_checkpoint_empty() {
        let config = json!({});
        let completed = parse_checkpoint(&config);
        assert!(completed.is_empty());
    }

    #[test]
    fn test_parse_checkpoint_with_completed_frames() {
        let config = json!({
            "_checkpoint": {
                "completed_frames": [0, 2, 5, 10]
            }
        });
        let completed = parse_checkpoint(&config);
        assert_eq!(completed.len(), 4);
        assert!(completed.contains(&0));
        assert!(completed.contains(&2));
        assert!(completed.contains(&5));
        assert!(completed.contains(&10));
        assert!(!completed.contains(&1));
    }

    #[test]
    fn test_parse_checkpoint_empty_array() {
        let config = json!({
            "_checkpoint": {
                "completed_frames": []
            }
        });
        let completed = parse_checkpoint(&config);
        assert!(completed.is_empty());
    }

    #[test]
    fn test_parse_checkpoint_malformed() {
        // Non-array completed_frames
        let config = json!({
            "_checkpoint": {
                "completed_frames": "not an array"
            }
        });
        let completed = parse_checkpoint(&config);
        assert!(completed.is_empty());
    }

    // ── FrameDescriptionWriter tests ──────────────────────────────────

    #[test]
    fn test_frame_description_writer_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut writer = FrameDescriptionWriter::new(tmp.path()).unwrap();

        let d1 = json!({"frame_index": 0, "timestamp_secs": 0.0, "description": "Opening shot"});
        let d2 = json!({"frame_index": 1, "timestamp_secs": 10.0, "description": "Second scene"});

        writer.write(&d1).unwrap();
        writer.write(&d2).unwrap();
        assert_eq!(writer.len(), 2);

        let results = writer.read_all();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["frame_index"], 0);
        assert_eq!(results[1]["description"], "Second scene");
    }

    #[test]
    fn test_frame_description_writer_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let writer = FrameDescriptionWriter::new(tmp.path()).unwrap();
        assert_eq!(writer.len(), 0);
        assert!(writer.read_all().is_empty());
    }
}

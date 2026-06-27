//! MediaOptimizeHandler — pre-generates streamable media variants during
//! extraction to avoid active (on-demand) transcoding.
//!
//! Produces derived attachments linked to the parent via `derivation_type`:
//! - `faststart`       – MP4 with moov atom moved to front (copy-only)
//! - `web_compatible`  – MKV/MOV remuxed to MP4 (copy-only)
//! - `audio_only`      – Audio track extracted from video (copy-only)
//! - `preview_720p`    – Downscaled 720p H.264 preview (transcode, large files only)
//! - `web_audio`       – Lossless audio (FLAC/WAV) converted to AAC M4A
//!
//! Queued by `queue_media_optimize_job()` in main.rs for audio/video uploads
//! when the `media_optimize` flag is set.

use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use matric_core::JobType;
use matric_db::{Database, SchemaContext};

use crate::handler::{JobContext, JobHandler, JobResult};

/// Default timeout per ffmpeg/ffprobe command (seconds).
const CMD_TIMEOUT_SECS: u64 = 600;

/// File size threshold (bytes) above which a 720p preview is generated.
const DEFAULT_PREVIEW_THRESHOLD_BYTES: u64 = 100_000_000; // 100 MB

/// Target preview height in pixels.
const DEFAULT_PREVIEW_HEIGHT: u32 = 720;

/// Environment variable overrides.
fn preview_threshold_bytes() -> u64 {
    std::env::var("MEDIA_OPTIMIZE_PREVIEW_THRESHOLD_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PREVIEW_THRESHOLD_BYTES)
}

fn preview_height() -> u32 {
    std::env::var("MEDIA_OPTIMIZE_PREVIEW_HEIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PREVIEW_HEIGHT)
}

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
        .map_err(|e| media_job_failure("Invalid schema", media_error_reason_code(&e.to_string())))
}

/// Metadata returned by ffprobe.
#[derive(Debug, Default)]
struct ProbeResult {
    format_name: String,
    duration_secs: f64,
    size_bytes: u64,
    video_codec: Option<String>,
    video_height: Option<u32>,
    audio_codec: Option<String>,
    has_audio: bool,
    has_video: bool,
    /// Whether the MP4 moov atom is already at the front.
    is_faststart: bool,
}

impl ProbeResult {
    fn is_mp4(&self) -> bool {
        self.format_name.contains("mp4")
            || self.format_name.contains("m4a")
            || self.format_name.contains("mov")
    }

    fn is_mkv(&self) -> bool {
        self.format_name.contains("matroska")
    }

    fn is_mov(&self) -> bool {
        self.format_name.contains("mov") && !self.format_name.contains("mp4")
    }

    fn is_lossless_audio(&self) -> bool {
        matches!(
            self.audio_codec.as_deref(),
            Some("flac") | Some("pcm_s16le") | Some("pcm_s24le") | Some("pcm_s32le") | Some("alac")
        ) || self.format_name.contains("wav")
            || self.format_name.contains("flac")
    }

    fn video_is_web_compatible(&self) -> bool {
        matches!(
            self.video_codec.as_deref(),
            Some("h264") | Some("h265") | Some("hevc") | Some("vp8") | Some("vp9") | Some("av1")
        )
    }
}

/// Run an external command with a timeout.
async fn run_cmd(cmd: &mut Command, timeout_secs: u64) -> Result<std::process::Output, String> {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| format!("Command timed out after {}s", timeout_secs))?
        .map_err(|e| {
            format!(
                "Command failed to start; io_error_kind={}",
                media_io_error_kind(&e)
            )
        })
}

fn media_io_error_kind(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::TimedOut => "timed_out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::WouldBlock => "would_block",
        _ => "io_error",
    }
}

fn media_text_len(text: &str) -> usize {
    text.chars().count()
}

fn media_variant_progress_message(derivation_type: &str) -> String {
    format!(
        "Storing generated variant; derivation_type_len={}",
        media_text_len(derivation_type)
    )
}

fn media_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("not found") || text.contains("no such") || text.contains("missing") {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else if text.contains("database") || text.contains("sql") || text.contains("postgres") {
        "database_error"
    } else if text.contains("invalid") || text.contains("codec") || text.contains("ffprobe") {
        "invalid_media"
    } else {
        "operation_failed"
    }
}

fn media_job_failure(action: &'static str, reason_code: &'static str) -> JobResult {
    JobResult::Failed(format!("{action} ({reason_code})"))
}

fn media_job_failure_from_error(action: &'static str, error: &dyn std::fmt::Display) -> JobResult {
    media_job_failure(action, media_error_reason_code(&error.to_string()))
}

fn media_job_failure_from_io(action: &'static str, error: &std::io::Error) -> JobResult {
    media_job_failure(action, media_io_error_kind(error))
}

fn media_stderr_reason_code(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("invalid data")
        || text.contains("invalid argument")
        || text.contains("moov atom not found")
        || text.contains("could not find codec parameters")
    {
        "invalid_media"
    } else if text.contains("not found") || text.contains("no such") {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else {
        "command_failed"
    }
}

fn media_command_failure_detail(
    command: &'static str,
    status_code: Option<i32>,
    stderr: &[u8],
) -> String {
    format!(
        "{command} failed; status={}; stderr_len={}; stderr_reason={}",
        status_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
        stderr.len(),
        media_stderr_reason_code(stderr)
    )
}

/// Run ffprobe on a file and return structured metadata.
async fn ffprobe(path: &Path) -> Result<ProbeResult, String> {
    let output = run_cmd(
        Command::new("ffprobe")
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(path),
        60,
    )
    .await?;

    if !output.status.success() {
        return Err(media_command_failure_detail(
            "ffprobe",
            output.status.code(),
            &output.stderr,
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe JSON: {}", e))?;

    let mut result = ProbeResult::default();

    // Parse format info
    if let Some(format) = json.get("format") {
        result.format_name = format
            .get("format_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        result.duration_secs = format
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        result.size_bytes = format
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
    }

    // Parse streams
    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let codec_name = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            match codec_type {
                "video" => {
                    result.has_video = true;
                    result.video_codec = Some(codec_name);
                    result.video_height = stream
                        .get("height")
                        .and_then(|v| v.as_u64())
                        .map(|h| h as u32);
                }
                "audio" => {
                    result.has_audio = true;
                    result.audio_codec = Some(codec_name);
                }
                _ => {}
            }
        }
    }

    // Check faststart: if format_name includes "mp4", check if moov is before mdat.
    // ffprobe doesn't directly expose this, but we can check via -show_entries
    // with format_tags. A simpler heuristic: run a quick check.
    if result.is_mp4() {
        result.is_faststart = check_faststart(path).await;
    }

    Ok(result)
}

/// Check if an MP4 file has the moov atom before mdat (faststart).
///
/// Uses ffprobe's -show_entries to look at atom order. If the moov atom
/// comes first, the file is faststart-ready.
async fn check_faststart(path: &Path) -> bool {
    // Use ffprobe with -show_format to get format tags; alternatively use
    // a lightweight heuristic: try to read the first 32 bytes for 'ftyp'
    // followed relatively soon by 'moov'. For simplicity, run ffmpeg with
    // -v trace and check atom order.
    let output = run_cmd(
        Command::new("ffmpeg")
            .arg("-v")
            .arg("trace")
            .arg("-i")
            .arg(path)
            .arg("-f")
            .arg("null")
            .arg("-t")
            .arg("0")
            .arg("-"),
        30,
    )
    .await;

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            // In trace output, look for moov/mdat atom order
            let moov_pos = stderr.find("type:'moov'");
            let mdat_pos = stderr.find("type:'mdat'");
            match (moov_pos, mdat_pos) {
                (Some(m), Some(d)) => m < d,
                _ => false, // Can't determine; assume not faststart
            }
        }
        Err(_) => false,
    }
}

/// Run an ffmpeg command that produces an output file.
async fn run_ffmpeg(args: &[&str], timeout_secs: u64) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y"); // Overwrite output
    for arg in args {
        cmd.arg(arg);
    }

    let output = run_cmd(&mut cmd, timeout_secs).await?;

    if !output.status.success() {
        return Err(media_command_failure_detail(
            "ffmpeg",
            output.status.code(),
            &output.stderr,
        ));
    }

    Ok(())
}

/// A generated media variant ready to be stored.
struct GeneratedVariant {
    path: PathBuf,
    filename: String,
    content_type: String,
    derivation_type: String,
}

pub struct MediaOptimizeHandler {
    db: Database,
}

impl MediaOptimizeHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Generate all applicable variants for a video file.
    async fn optimize_video(
        &self,
        input: &Path,
        probe: &ProbeResult,
        work_dir: &Path,
    ) -> Vec<GeneratedVariant> {
        let mut variants = Vec::new();
        let input_str = input.to_string_lossy();

        // 1. MP4 faststart (move moov atom to front) — copy only
        if probe.is_mp4() && !probe.is_faststart {
            let out = work_dir.join("faststart.mp4");
            let out_str = out.to_string_lossy().to_string();
            match run_ffmpeg(
                &[
                    "-i",
                    &input_str,
                    "-c",
                    "copy",
                    "-movflags",
                    "+faststart",
                    &out_str,
                ],
                CMD_TIMEOUT_SECS,
            )
            .await
            {
                Ok(()) => {
                    variants.push(GeneratedVariant {
                        path: out,
                        filename: "faststart.mp4".into(),
                        content_type: "video/mp4".into(),
                        derivation_type: "faststart".into(),
                    });
                }
                Err(e) => warn!(
                    error_len = media_text_len(&e),
                    error_reason = media_error_reason_code(&e),
                    "Faststart remux failed"
                ),
            }
        }

        // 2. MKV/MOV → MP4 remux (browser-compatible container) — copy only
        if (probe.is_mkv() || probe.is_mov()) && probe.video_is_web_compatible() {
            let out = work_dir.join("web.mp4");
            let out_str = out.to_string_lossy().to_string();
            match run_ffmpeg(
                &[
                    "-i",
                    &input_str,
                    "-c",
                    "copy",
                    "-movflags",
                    "+faststart",
                    &out_str,
                ],
                CMD_TIMEOUT_SECS,
            )
            .await
            {
                Ok(()) => {
                    variants.push(GeneratedVariant {
                        path: out,
                        filename: "web.mp4".into(),
                        content_type: "video/mp4".into(),
                        derivation_type: "web_compatible".into(),
                    });
                }
                Err(e) => warn!(
                    error_len = media_text_len(&e),
                    error_reason = media_error_reason_code(&e),
                    "MKV/MOV remux failed"
                ),
            }
        }

        // 3. Extract audio stream as separate file — copy only
        if probe.has_audio {
            let out = work_dir.join("audio.m4a");
            let out_str = out.to_string_lossy().to_string();
            match run_ffmpeg(
                &["-i", &input_str, "-vn", "-c:a", "copy", &out_str],
                CMD_TIMEOUT_SECS,
            )
            .await
            {
                Ok(()) => {
                    variants.push(GeneratedVariant {
                        path: out,
                        filename: "audio.m4a".into(),
                        content_type: "audio/mp4".into(),
                        derivation_type: "audio_only".into(),
                    });
                }
                Err(e) => {
                    // Fallback: try encoding to AAC if copy fails (incompatible codec)
                    debug!(
                        error_len = media_text_len(&e),
                        error_reason = media_error_reason_code(&e),
                        "Audio copy failed, trying AAC encode"
                    );
                    let out2 = work_dir.join("audio_enc.m4a");
                    let out2_str = out2.to_string_lossy().to_string();
                    match run_ffmpeg(
                        &[
                            "-i", &input_str, "-vn", "-c:a", "aac", "-b:a", "128k", &out2_str,
                        ],
                        CMD_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(()) => {
                            variants.push(GeneratedVariant {
                                path: out2,
                                filename: "audio.m4a".into(),
                                content_type: "audio/mp4".into(),
                                derivation_type: "audio_only".into(),
                            });
                        }
                        Err(e2) => warn!(
                            error_len = media_text_len(&e2),
                            error_reason = media_error_reason_code(&e2),
                            "Audio extraction failed"
                        ),
                    }
                }
            }
        }

        // 4. 720p preview variant (transcode — only for large, high-res files)
        let threshold = preview_threshold_bytes();
        let height = preview_height();
        if probe.size_bytes > threshold {
            if let Some(vh) = probe.video_height {
                if vh > height {
                    let out = work_dir.join("preview.mp4");
                    let out_str = out.to_string_lossy().to_string();
                    let scale = format!("scale=-2:{}", height);
                    match run_ffmpeg(
                        &[
                            "-i",
                            &input_str,
                            "-vf",
                            &scale,
                            "-c:v",
                            "libx264",
                            "-preset",
                            "fast",
                            "-crf",
                            "28",
                            "-c:a",
                            "aac",
                            "-b:a",
                            "128k",
                            "-movflags",
                            "+faststart",
                            &out_str,
                        ],
                        CMD_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(()) => {
                            variants.push(GeneratedVariant {
                                path: out,
                                filename: "preview_720p.mp4".into(),
                                content_type: "video/mp4".into(),
                                derivation_type: "preview_720p".into(),
                            });
                        }
                        Err(e) => warn!(
                            error_len = media_text_len(&e),
                            error_reason = media_error_reason_code(&e),
                            "720p preview generation failed"
                        ),
                    }
                }
            }
        }

        variants
    }

    /// Generate all applicable variants for an audio file.
    async fn optimize_audio(
        &self,
        input: &Path,
        probe: &ProbeResult,
        work_dir: &Path,
    ) -> Vec<GeneratedVariant> {
        let mut variants = Vec::new();
        let input_str = input.to_string_lossy();

        // 1. Lossless → AAC in M4A container
        if probe.is_lossless_audio() {
            let out = work_dir.join("web.m4a");
            let out_str = out.to_string_lossy().to_string();
            match run_ffmpeg(
                &[
                    "-i", &input_str, "-c:a", "aac", "-b:a", "192k", "-vn", &out_str,
                ],
                CMD_TIMEOUT_SECS,
            )
            .await
            {
                Ok(()) => {
                    variants.push(GeneratedVariant {
                        path: out,
                        filename: "web.m4a".into(),
                        content_type: "audio/mp4".into(),
                        derivation_type: "web_audio".into(),
                    });
                }
                Err(e) => warn!(
                    error_len = media_text_len(&e),
                    error_reason = media_error_reason_code(&e),
                    "Lossless AAC conversion failed"
                ),
            }
        }

        // 2. Low-bitrate preview for long audio (>5 min)
        if probe.duration_secs > 300.0 {
            let out = work_dir.join("preview.m4a");
            let out_str = out.to_string_lossy().to_string();
            match run_ffmpeg(
                &[
                    "-i", &input_str, "-c:a", "aac", "-b:a", "64k", "-ac", "1", "-ar", "22050",
                    "-vn", &out_str,
                ],
                CMD_TIMEOUT_SECS,
            )
            .await
            {
                Ok(()) => {
                    variants.push(GeneratedVariant {
                        path: out,
                        filename: "preview.m4a".into(),
                        content_type: "audio/mp4".into(),
                        derivation_type: "audio_preview".into(),
                    });
                }
                Err(e) => warn!(
                    error_len = media_text_len(&e),
                    error_reason = media_error_reason_code(&e),
                    "Audio preview generation failed"
                ),
            }
        }

        variants
    }
}

#[async_trait]
impl JobHandler for MediaOptimizeHandler {
    fn job_type(&self) -> JobType {
        JobType::MediaOptimize
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing media_optimize job payload".into()),
        };

        let attachment_id: Uuid = match payload
            .get("attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid attachment_id".into()),
        };

        let content_type = payload
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("Missing note_id for media optimize".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Loading source attachment"));

        // Check ffmpeg availability
        let ffmpeg_check = Command::new("ffmpeg").arg("-version").output().await;
        if !matches!(ffmpeg_check, Ok(ref o) if o.status.success()) {
            return JobResult::Failed("ffmpeg not found in PATH".into());
        }

        // Download source file to temp directory
        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        let work_dir = match TempDir::new() {
            Ok(d) => d,
            Err(e) => return media_job_failure_from_io("Failed to create temp dir", &e),
        };

        ctx.report_progress(10, Some("Downloading source file"));

        // Download file data
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(tx) => tx,
            Err(e) => return media_job_failure_from_error("Failed to start transaction", &e),
        };
        let (file_data, _ct, original_filename) =
            match file_storage.download_file_tx(&mut tx, attachment_id).await {
                Ok(data) => data,
                Err(e) => return media_job_failure_from_error("Failed to download attachment", &e),
            };
        if let Err(e) = tx.commit().await {
            return media_job_failure_from_error("Transaction commit failed", &e);
        }

        // Determine file extension from content type or filename
        let ext = extension_for_content_type(&content_type, &original_filename);
        let input_path = work_dir.path().join(format!("source.{}", ext));
        if let Err(e) = tokio::fs::write(&input_path, &file_data).await {
            return media_job_failure_from_io("Failed to write temp file", &e);
        }
        drop(file_data); // Free memory

        ctx.report_progress(20, Some("Analyzing media file"));

        // Probe the file
        let probe = match ffprobe(&input_path).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    attachment_id_present = true,
                    error_len = media_text_len(&e),
                    error_reason = media_error_reason_code(&e),
                    "ffprobe failed, skipping optimization"
                );
                return JobResult::Success(Some(json!({
                    "skipped": true,
                    "reason": format!("ffprobe failed: {}", e),
                })));
            }
        };

        info!(
            attachment_id_present = true,
            format_len = media_text_len(&probe.format_name),
            video_codec_len = probe.video_codec.as_deref().map(media_text_len),
            audio_codec_len = probe.audio_codec.as_deref().map(media_text_len),
            duration = probe.duration_secs,
            size = probe.size_bytes,
            "Media probe complete"
        );

        ctx.report_progress(30, Some("Generating optimized variants"));

        // Generate variants based on media type
        let variants = if content_type.starts_with("video/") || probe.has_video {
            self.optimize_video(&input_path, &probe, work_dir.path())
                .await
        } else if content_type.starts_with("audio/") {
            self.optimize_audio(&input_path, &probe, work_dir.path())
                .await
        } else {
            return JobResult::Success(Some(json!({
                "skipped": true,
                "reason": "Not a recognized audio/video content type",
            })));
        };

        if variants.is_empty() {
            info!(
                attachment_id_present = true,
                "No optimization needed for this file"
            );
            return JobResult::Success(Some(json!({
                "variants_created": 0,
                "reason": "No applicable optimizations for this format/size",
            })));
        }

        ctx.report_progress(70, Some("Storing generated variants"));

        // Store each variant as a derived attachment
        let mut stored_count = 0;
        let mut variant_info = Vec::new();

        for (i, variant) in variants.iter().enumerate() {
            let progress = 70 + (25 * (i + 1) / variants.len()) as i32;
            let progress_message = media_variant_progress_message(&variant.derivation_type);
            ctx.report_progress(progress, Some(&progress_message));

            // Read the generated file
            let data = match tokio::fs::read(&variant.path).await {
                Ok(d) => d,
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        derivation_type_len = media_text_len(&variant.derivation_type),
                        error_len = media_text_len(&error_text),
                        error_reason = media_error_reason_code(&error_text),
                        "Failed to read generated variant"
                    );
                    continue;
                }
            };

            let file_size = data.len() as u64;

            // Store as derived attachment
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(tx) => tx,
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        error_len = media_text_len(&error_text),
                        error_reason = media_error_reason_code(&error_text),
                        "Failed to start transaction for variant storage"
                    );
                    continue;
                }
            };

            match file_storage
                .store_derived_attachment_tx(
                    &mut tx,
                    note_id,
                    attachment_id,
                    &variant.filename,
                    &variant.content_type,
                    &data,
                    &variant.derivation_type,
                )
                .await
            {
                Ok(att) => {
                    if let Err(e) = tx.commit().await {
                        let error_text = e.to_string();
                        error!(
                            derivation_type_len = media_text_len(&variant.derivation_type),
                            error_len = media_text_len(&error_text),
                            error_reason = media_error_reason_code(&error_text),
                            "Failed to commit variant"
                        );
                        continue;
                    }
                    stored_count += 1;
                    variant_info.push(json!({
                        "derivation_type": variant.derivation_type,
                        "attachment_id": att.id.to_string(),
                        "content_type": variant.content_type,
                        "size_bytes": file_size,
                    }));
                    info!(
                        parent_attachment_id_present = true,
                        derivation_type_len = media_text_len(&variant.derivation_type),
                        derived_attachment_id_present = true,
                        size = file_size,
                        "Stored media variant"
                    );
                }
                Err(e) => {
                    let error_text = e.to_string();
                    error!(
                        derivation_type_len = media_text_len(&variant.derivation_type),
                        error_len = media_text_len(&error_text),
                        error_reason = media_error_reason_code(&error_text),
                        "Failed to store variant"
                    );
                    let _ = tx.rollback().await;
                }
            }
        }

        ctx.report_progress(100, Some("Media optimization complete"));

        info!(
            attachment_id_present = true,
            variants_created = stored_count,
            "Media optimization complete"
        );

        JobResult::Success(Some(json!({
            "variants_created": stored_count,
            "variants": variant_info,
        })))
    }
}

/// Determine file extension from content type or original filename.
fn extension_for_content_type(content_type: &str, filename: &str) -> String {
    // Try to get extension from original filename
    if let Some(ext) = Path::new(filename).extension().and_then(|e| e.to_str()) {
        return ext.to_lowercase();
    }

    // Fall back to content type mapping
    match content_type {
        "video/mp4" => "mp4",
        "video/x-matroska" => "mkv",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "video/x-msvideo" => "avi",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/webm" => "weba",
        _ => "bin",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_command_failure_detail_redacts_stderr() {
        let stderr = b"ffmpeg: invalid data at /srv/fortemi/uploads/source.mp4 token=mm_key_secret";
        let detail = media_command_failure_detail("ffmpeg", Some(1), stderr);

        assert!(detail.contains("ffmpeg failed"));
        assert!(detail.contains("status=1"));
        assert!(detail.contains("stderr_len="));
        assert!(detail.contains("stderr_reason=invalid_media"));
        assert!(!detail.contains("/srv/fortemi"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("invalid data at"));
    }

    #[test]
    fn media_stderr_reason_code_uses_stable_classes() {
        assert_eq!(
            media_stderr_reason_code(b"Permission denied while opening input"),
            "permission_denied"
        );
        assert_eq!(
            media_stderr_reason_code(b"moov atom not found"),
            "invalid_media"
        );
        assert_eq!(media_stderr_reason_code(b"No such file"), "not_found");
        assert_eq!(
            media_stderr_reason_code(b"operation timed out"),
            "timed_out"
        );
        assert_eq!(
            media_stderr_reason_code(b"opaque backend text"),
            "command_failed"
        );
    }

    #[test]
    fn media_runtime_telemetry_helpers_redact_private_values() {
        let raw_error =
            "postgres://user:pass@db.internal/media failed for /srv/private/mm_key_media";
        let rendered = format!(
            "attachment_id_present=true; error_len={}; error_reason={}",
            media_text_len(raw_error),
            media_error_reason_code(raw_error)
        );

        assert!(rendered.contains("attachment_id_present=true"));
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason=database_error"));
        assert!(!rendered.contains("postgres://user:pass"));
        assert!(!rendered.contains("db.internal"));
        assert!(!rendered.contains("/srv/private"));
        assert!(!rendered.contains("mm_key_media"));
    }

    #[test]
    fn media_variant_progress_message_redacts_derivation_type() {
        let derivation_type = "private-client-preview /srv/media token=sk-secret";
        let rendered = media_variant_progress_message(derivation_type);

        assert!(rendered.contains("Storing generated variant; derivation_type_len="));
        assert!(!rendered.contains("private-client-preview"));
        assert!(!rendered.contains("/srv/media"));
        assert!(!rendered.contains("sk-secret"));
    }

    #[test]
    fn media_job_failures_use_stable_reason_codes() {
        let raw_error =
            "postgres://user:pass@db.internal/media failed for /srv/private/mm_key_media";
        let failure = media_job_failure_from_error("Failed to download attachment", &raw_error);

        match failure {
            JobResult::Failed(message) => {
                assert_eq!(message, "Failed to download attachment (database_error)");
                assert!(!message.contains("postgres://user:pass"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/private"));
                assert!(!message.contains("mm_key_media"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let io_error = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied for /srv/private/mm_key_media",
        );
        let failure = media_job_failure_from_io("Failed to write temp file", &io_error);

        match failure {
            JobResult::Failed(message) => {
                assert_eq!(message, "Failed to write temp file (permission_denied)");
                assert!(!message.contains("/srv/private"));
                assert!(!message.contains("mm_key_media"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    // ── extension_for_content_type ────────────────────────────────────

    #[test]
    fn test_extension_from_filename_takes_priority() {
        // When filename has an extension, it wins over content_type
        assert_eq!(extension_for_content_type("video/mp4", "movie.mp4"), "mp4");
        assert_eq!(
            extension_for_content_type("application/octet-stream", "video.mp4"),
            "mp4"
        );
        assert_eq!(
            extension_for_content_type("video/x-matroska", "file.mkv"),
            "mkv"
        );
    }

    #[test]
    fn test_extension_from_content_type_fallback() {
        // When filename has no extension, fall back to content_type
        assert_eq!(extension_for_content_type("video/mp4", "noext"), "mp4");
        assert_eq!(
            extension_for_content_type("video/x-matroska", "noext"),
            "mkv"
        );
        assert_eq!(
            extension_for_content_type("video/quicktime", "noext"),
            "mov"
        );
        assert_eq!(extension_for_content_type("video/webm", "noext"), "webm");
        assert_eq!(
            extension_for_content_type("video/x-msvideo", "noext"),
            "avi"
        );
        assert_eq!(extension_for_content_type("audio/mpeg", "noext"), "mp3");
        assert_eq!(extension_for_content_type("audio/mp4", "noext"), "m4a");
        assert_eq!(extension_for_content_type("audio/x-m4a", "noext"), "m4a");
        assert_eq!(extension_for_content_type("audio/flac", "noext"), "flac");
        assert_eq!(extension_for_content_type("audio/x-flac", "noext"), "flac");
        assert_eq!(extension_for_content_type("audio/wav", "noext"), "wav");
        assert_eq!(extension_for_content_type("audio/x-wav", "noext"), "wav");
        assert_eq!(extension_for_content_type("audio/ogg", "noext"), "ogg");
        assert_eq!(extension_for_content_type("audio/webm", "noext"), "weba");
    }

    #[test]
    fn test_extension_unknown_content_type() {
        assert_eq!(
            extension_for_content_type("application/octet-stream", "noext"),
            "bin"
        );
        assert_eq!(
            extension_for_content_type("video/x-unknown", "noext"),
            "bin"
        );
    }

    #[test]
    fn test_extension_case_insensitive_filename() {
        // Path::extension handles case; our to_lowercase normalizes
        assert_eq!(extension_for_content_type("video/mp4", "MOVIE.MP4"), "mp4");
        assert_eq!(
            extension_for_content_type("audio/flac", "Song.FLAC"),
            "flac"
        );
    }

    // ── ProbeResult format detection ─────────────────────────────────

    #[test]
    fn test_is_mp4_variants() {
        let cases = [
            ("mp4", true),
            ("mov,mp4,m4a,3gp,3g2,mj2", true),
            ("m4a", true),
            ("matroska,webm", false),
            ("flac", false),
            ("wav", false),
            ("", false),
        ];
        for (format, expected) in cases {
            let probe = ProbeResult {
                format_name: format.into(),
                ..Default::default()
            };
            assert_eq!(probe.is_mp4(), expected, "is_mp4() for '{}'", format);
        }
    }

    #[test]
    fn test_is_mkv_variants() {
        let cases = [
            ("matroska,webm", true),
            ("matroska", true),
            ("mp4", false),
            ("wav", false),
        ];
        for (format, expected) in cases {
            let probe = ProbeResult {
                format_name: format.into(),
                ..Default::default()
            };
            assert_eq!(probe.is_mkv(), expected, "is_mkv() for '{}'", format);
        }
    }

    #[test]
    fn test_is_mov_detection() {
        // MOV format typically reported as "mov,mp4,m4a,3gp" — is_mov returns false
        // because the string also contains "mp4". This is by design: actual MOV
        // files that ffprobe reports as only "mov" are handled.
        let probe = ProbeResult {
            format_name: "mov".into(),
            ..Default::default()
        };
        assert!(probe.is_mov());

        // When format contains both mov and mp4, it's treated as mp4 (not mov)
        let probe2 = ProbeResult {
            format_name: "mov,mp4,m4a".into(),
            ..Default::default()
        };
        assert!(!probe2.is_mov()); // mp4 takes priority
    }

    // ── ProbeResult lossless audio detection ──────────────────────────

    #[test]
    fn test_lossless_audio_codecs() {
        for codec in &["flac", "pcm_s16le", "pcm_s24le", "pcm_s32le", "alac"] {
            let probe = ProbeResult {
                audio_codec: Some(codec.to_string()),
                ..Default::default()
            };
            assert!(
                probe.is_lossless_audio(),
                "codec '{}' should be lossless",
                codec
            );
        }
    }

    #[test]
    fn test_lossy_audio_codecs() {
        for codec in &["aac", "mp3", "opus", "vorbis"] {
            let probe = ProbeResult {
                audio_codec: Some(codec.to_string()),
                ..Default::default()
            };
            assert!(
                !probe.is_lossless_audio(),
                "codec '{}' should not be lossless",
                codec
            );
        }
    }

    #[test]
    fn test_lossless_by_format_name() {
        let probe = ProbeResult {
            format_name: "wav".into(),
            audio_codec: None,
            ..Default::default()
        };
        assert!(probe.is_lossless_audio());

        let probe2 = ProbeResult {
            format_name: "flac".into(),
            audio_codec: None,
            ..Default::default()
        };
        assert!(probe2.is_lossless_audio());
    }

    #[test]
    fn test_not_lossless_no_codec_no_format() {
        let probe = ProbeResult::default();
        assert!(!probe.is_lossless_audio());
    }

    // ── ProbeResult web-compatible video ──────────────────────────────

    #[test]
    fn test_web_compatible_codecs() {
        for codec in &["h264", "h265", "hevc", "vp8", "vp9", "av1"] {
            let probe = ProbeResult {
                video_codec: Some(codec.to_string()),
                ..Default::default()
            };
            assert!(
                probe.video_is_web_compatible(),
                "codec '{}' should be web-compatible",
                codec
            );
        }
    }

    #[test]
    fn test_non_web_compatible_codecs() {
        for codec in &["mpeg2video", "mpeg4", "theora", "wmv3", "rv40"] {
            let probe = ProbeResult {
                video_codec: Some(codec.to_string()),
                ..Default::default()
            };
            assert!(
                !probe.video_is_web_compatible(),
                "codec '{}' should not be web-compatible",
                codec
            );
        }
    }

    #[test]
    fn test_no_video_codec_not_web_compatible() {
        let probe = ProbeResult::default();
        assert!(!probe.video_is_web_compatible());
    }

    // ── ProbeResult combined state ────────────────────────────────────

    #[test]
    fn test_probe_result_default() {
        let probe = ProbeResult::default();
        assert!(probe.format_name.is_empty());
        assert_eq!(probe.duration_secs, 0.0);
        assert_eq!(probe.size_bytes, 0);
        assert!(probe.video_codec.is_none());
        assert!(probe.video_height.is_none());
        assert!(probe.audio_codec.is_none());
        assert!(!probe.has_audio);
        assert!(!probe.has_video);
        assert!(!probe.is_faststart);
    }

    #[test]
    fn test_probe_result_full_video() {
        let probe = ProbeResult {
            format_name: "mp4".into(),
            duration_secs: 120.5,
            size_bytes: 150_000_000,
            video_codec: Some("h264".into()),
            video_height: Some(1080),
            audio_codec: Some("aac".into()),
            has_audio: true,
            has_video: true,
            is_faststart: false,
        };
        assert!(probe.is_mp4());
        assert!(!probe.is_mkv());
        assert!(!probe.is_lossless_audio());
        assert!(probe.video_is_web_compatible());
        assert!(!probe.is_faststart);
    }

    // ── JobType integration ──────────────────────────────────────────

    #[test]
    fn test_media_optimize_job_type_priority() {
        assert_eq!(JobType::MediaOptimize.default_priority(), 3);
    }

    #[test]
    fn test_media_optimize_job_type_string_roundtrip() {
        let s = serde_json::to_string(&JobType::MediaOptimize).unwrap();
        assert_eq!(s, "\"media_optimize\"");
        let back: JobType = serde_json::from_str(&s).unwrap();
        assert_eq!(back, JobType::MediaOptimize);
    }

    // ── ffprobe JSON parsing ─────────────────────────────────────────

    #[test]
    fn test_ffprobe_result_parsing_simulated() {
        // Simulate what ffprobe returns and verify our ProbeResult construction
        // This tests the logic without needing ffprobe installed
        let json: serde_json::Value = serde_json::json!({
            "format": {
                "format_name": "matroska,webm",
                "duration": "185.123",
                "size": "250000000"
            },
            "streams": [
                {
                    "codec_type": "video",
                    "codec_name": "h264",
                    "height": 1080
                },
                {
                    "codec_type": "audio",
                    "codec_name": "aac"
                }
            ]
        });

        // Reconstruct probe parsing logic inline (mirrors ffprobe fn)
        let mut result = ProbeResult::default();

        if let Some(format) = json.get("format") {
            result.format_name = format
                .get("format_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            result.duration_secs = format
                .get("duration")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            result.size_bytes = format
                .get("size")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }

        if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
            for stream in streams {
                let codec_type = stream
                    .get("codec_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let codec_name = stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                match codec_type {
                    "video" => {
                        result.has_video = true;
                        result.video_codec = Some(codec_name);
                        result.video_height = stream
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .map(|h| h as u32);
                    }
                    "audio" => {
                        result.has_audio = true;
                        result.audio_codec = Some(codec_name);
                    }
                    _ => {}
                }
            }
        }

        assert!(result.is_mkv());
        assert!(!result.is_mp4());
        assert!(result.has_video);
        assert!(result.has_audio);
        assert_eq!(result.video_codec.as_deref(), Some("h264"));
        assert_eq!(result.audio_codec.as_deref(), Some("aac"));
        assert_eq!(result.video_height, Some(1080));
        assert!((result.duration_secs - 185.123).abs() < 0.001);
        assert_eq!(result.size_bytes, 250_000_000);
        assert!(result.video_is_web_compatible());
        assert!(!result.is_lossless_audio());
    }

    #[test]
    fn test_ffprobe_result_audio_only() {
        let json: serde_json::Value = serde_json::json!({
            "format": {
                "format_name": "flac",
                "duration": "342.5",
                "size": "45000000"
            },
            "streams": [
                {
                    "codec_type": "audio",
                    "codec_name": "flac"
                }
            ]
        });

        let mut result = ProbeResult::default();
        if let Some(format) = json.get("format") {
            result.format_name = format["format_name"].as_str().unwrap_or("").to_string();
            result.duration_secs = format["duration"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            result.size_bytes = format["size"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
        if let Some(streams) = json["streams"].as_array() {
            for s in streams {
                if s["codec_type"].as_str() == Some("audio") {
                    result.has_audio = true;
                    result.audio_codec = Some(s["codec_name"].as_str().unwrap_or("").to_string());
                }
            }
        }

        assert!(!result.has_video);
        assert!(result.has_audio);
        assert!(result.is_lossless_audio());
        assert!(result.duration_secs > 300.0); // Would trigger audio preview
    }

    #[test]
    fn test_ffprobe_result_missing_fields() {
        // ffprobe JSON with minimal/missing fields should produce safe defaults
        let json: serde_json::Value = serde_json::json!({
            "format": {},
            "streams": []
        });

        let mut result = ProbeResult::default();
        if let Some(format) = json.get("format") {
            result.format_name = format
                .get("format_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }

        assert!(result.format_name.is_empty());
        assert_eq!(result.duration_secs, 0.0);
        assert_eq!(result.size_bytes, 0);
        assert!(!result.has_video);
        assert!(!result.has_audio);
    }

    // ── Variant eligibility logic ────────────────────────────────────

    #[test]
    fn test_faststart_eligibility() {
        // MP4 without faststart → eligible for faststart remux
        let probe = ProbeResult {
            format_name: "mp4".into(),
            is_faststart: false,
            ..Default::default()
        };
        assert!(probe.is_mp4() && !probe.is_faststart);

        // MP4 with faststart → not eligible (already optimized)
        let probe2 = ProbeResult {
            format_name: "mp4".into(),
            is_faststart: true,
            ..Default::default()
        };
        assert!(!probe2.is_mp4() || probe2.is_faststart);

        // MKV → not eligible for faststart (not MP4)
        let probe3 = ProbeResult {
            format_name: "matroska".into(),
            is_faststart: false,
            ..Default::default()
        };
        assert!(!probe3.is_mp4());
    }

    #[test]
    fn test_web_remux_eligibility() {
        // MKV with h264 → eligible for web remux
        let probe = ProbeResult {
            format_name: "matroska".into(),
            video_codec: Some("h264".into()),
            ..Default::default()
        };
        assert!((probe.is_mkv() || probe.is_mov()) && probe.video_is_web_compatible());

        // MKV with mpeg2 → not eligible (codec not web-compatible)
        let probe2 = ProbeResult {
            format_name: "matroska".into(),
            video_codec: Some("mpeg2video".into()),
            ..Default::default()
        };
        assert!(!probe2.video_is_web_compatible());

        // MP4 → not eligible (already in correct container)
        let probe3 = ProbeResult {
            format_name: "mp4".into(),
            video_codec: Some("h264".into()),
            ..Default::default()
        };
        assert!(!probe3.is_mkv() && !probe3.is_mov());
    }

    #[test]
    fn test_preview_eligibility() {
        let threshold = DEFAULT_PREVIEW_THRESHOLD_BYTES;
        let height = DEFAULT_PREVIEW_HEIGHT;

        // Large 1080p file → eligible
        let probe = ProbeResult {
            size_bytes: threshold + 1,
            video_height: Some(1080),
            ..Default::default()
        };
        assert!(probe.size_bytes > threshold && probe.video_height.unwrap_or(0) > height);

        // Small 1080p file → not eligible (below threshold)
        let probe2 = ProbeResult {
            size_bytes: threshold - 1,
            video_height: Some(1080),
            ..Default::default()
        };
        assert!(probe2.size_bytes <= threshold);

        // Large 480p file → not eligible (below target height)
        let probe3 = ProbeResult {
            size_bytes: threshold + 1,
            video_height: Some(480),
            ..Default::default()
        };
        assert!(probe3.video_height.unwrap_or(0) <= height);

        // Large file with no video height → not eligible
        let probe4 = ProbeResult {
            size_bytes: threshold + 1,
            video_height: None,
            ..Default::default()
        };
        assert!(probe4.video_height.is_none());
    }

    #[test]
    fn test_audio_preview_eligibility() {
        // Long audio (>5 min) → eligible for preview
        let probe = ProbeResult {
            duration_secs: 301.0,
            ..Default::default()
        };
        assert!(probe.duration_secs > 300.0);

        // Short audio → not eligible
        let probe2 = ProbeResult {
            duration_secs: 120.0,
            ..Default::default()
        };
        assert!(probe2.duration_secs <= 300.0);
    }

    #[test]
    fn test_lossless_conversion_eligibility() {
        // FLAC → eligible for web_audio
        let probe = ProbeResult {
            audio_codec: Some("flac".into()),
            format_name: "flac".into(),
            ..Default::default()
        };
        assert!(probe.is_lossless_audio());

        // AAC → not eligible (already lossy)
        let probe2 = ProbeResult {
            audio_codec: Some("aac".into()),
            format_name: "mp4".into(),
            ..Default::default()
        };
        assert!(!probe2.is_lossless_audio());
    }

    // ── Config env var parsing ────────────────────────────────────────

    #[test]
    fn test_default_preview_constants() {
        assert_eq!(DEFAULT_PREVIEW_THRESHOLD_BYTES, 100_000_000);
        assert_eq!(DEFAULT_PREVIEW_HEIGHT, 720);
        assert_eq!(CMD_TIMEOUT_SECS, 600);
    }

    // ── extract_schema ───────────────────────────────────────────────

    #[test]
    fn test_extract_schema_default() {
        let job = matric_core::Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::MediaOptimize,
            status: matric_core::JobStatus::Pending,
            priority: 3,
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
            note_id: None,
            job_type: JobType::MediaOptimize,
            status: matric_core::JobStatus::Pending,
            priority: 3,
            payload: Some(serde_json::json!({"schema": "my_archive"})),
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
        assert_eq!(extract_schema(&ctx), "my_archive");
    }

    #[test]
    fn test_extract_schema_empty_string_defaults() {
        let job = matric_core::Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::MediaOptimize,
            status: matric_core::JobStatus::Pending,
            priority: 3,
            payload: Some(serde_json::json!({"schema": ""})),
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
}

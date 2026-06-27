//! Shared audio utilities for transcription and diarization pipelines.
//!
//! Provides format normalization via ffmpeg, duration probing, audio chunking
//! for long files, and orchestrated chunked transcription (Issue #543).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_inference::transcription::{
    TranscriptionBackend, TranscriptionResult, TranscriptionSegment,
};
use tokio::process::Command;
use tracing::{debug, info, warn};

fn audio_io_error_kind(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::TimedOut => "timed_out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::WouldBlock => "would_block",
        _ => "io_error",
    }
}

fn audio_stderr_reason_code(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("invalid data")
        || text.contains("invalid argument")
        || text.contains("could not find codec parameters")
        || text.contains("moov atom not found")
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

fn audio_error_reason_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access denied") {
        "permission_denied"
    } else if lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
    {
        "not_found"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timed_out"
    } else if lower.contains("invalid") || lower.contains("parse") || lower.contains("decode") {
        "invalid_input"
    } else {
        "operation_failed"
    }
}

fn audio_telemetry_text_len(value: &str) -> usize {
    value.chars().count()
}

fn audio_command_failure_detail(
    command: &'static str,
    phase: &'static str,
    status_code: Option<i32>,
    stderr: &[u8],
) -> String {
    format!(
        "{command} {phase} failed; status={}; stderr_len={}; stderr_reason={}",
        status_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
        stderr.len(),
        audio_stderr_reason_code(stderr)
    )
}

fn audio_duration_parse_failure_detail(stdout: &str, error: &dyn std::fmt::Display) -> String {
    let error_text = error.to_string();
    format!(
        "Failed to parse ffprobe duration; stdout_len={}; error_len={}; error_reason={}",
        audio_telemetry_text_len(stdout.trim()),
        audio_telemetry_text_len(&error_text),
        audio_error_reason_code(&error_text)
    )
}

fn audio_transcription_failure_detail(
    phase: &'static str,
    backend_name: &str,
    audio_size_bytes: usize,
    error: &dyn std::fmt::Display,
) -> String {
    let error_text = error.to_string();
    format!(
        "Audio transcription failed; phase={phase}; backend_name_len={}; audio_size_bytes={}; error_len={}; error_reason={}",
        audio_telemetry_text_len(backend_name),
        audio_size_bytes,
        audio_telemetry_text_len(&error_text),
        audio_error_reason_code(&error_text)
    )
}

/// Transcode any audio/video file to 16kHz mono PCM WAV for speech processing.
///
/// This normalizes the input to the standard format accepted by all speech
/// backends (Whisper, pyannote, etc.), eliminating 415 Unsupported Media Type
/// errors from format mismatches.
///
/// # Arguments
/// * `input_path` - Path to the source audio or video file
/// * `output_dir` - Directory to write the transcoded WAV file
///
/// # Returns
/// Path to the output WAV file (`{output_dir}/speech.wav`)
pub async fn transcode_to_speech_wav(
    input_path: &Path,
    output_dir: &Path,
) -> matric_core::Result<PathBuf> {
    let output_path = output_dir.join("speech.wav");

    debug!(
        input_path_len = input_path.display().to_string().len(),
        output_path_len = output_path.display().to_string().len(),
        "Transcoding audio to 16kHz mono PCM WAV"
    );

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS * 2),
        Command::new("ffmpeg")
            .arg("-i")
            .arg(input_path)
            .arg("-vn") // Strip video track (no-op for audio-only files)
            .arg("-acodec")
            .arg("pcm_s16le") // PCM 16-bit little-endian
            .arg("-ar")
            .arg("16000") // 16kHz sample rate (Whisper standard)
            .arg("-ac")
            .arg("1") // Mono
            .arg("-y") // Overwrite if exists
            .arg(&output_path)
            .output(),
    )
    .await
    .map_err(|_| {
        matric_core::Error::Internal(format!(
            "ffmpeg transcode timed out after {}s",
            EXTRACTION_CMD_TIMEOUT_SECS * 2
        ))
    })?
    .map_err(|e| {
        matric_core::Error::Internal(format!(
            "ffmpeg transcode failed to start; io_error_kind={}",
            audio_io_error_kind(&e)
        ))
    })?;

    if !output.status.success() {
        return Err(matric_core::Error::Internal(audio_command_failure_detail(
            "ffmpeg",
            "transcode",
            output.status.code(),
            &output.stderr,
        )));
    }

    // Verify output file was created and is non-empty
    let metadata = std::fs::metadata(&output_path).map_err(|e| {
        matric_core::Error::Internal(format!(
            "Transcoded WAV not found; output_path_len={}; io_error_kind={}",
            output_path.display().to_string().len(),
            audio_io_error_kind(&e)
        ))
    })?;

    if metadata.len() == 0 {
        return Err(matric_core::Error::Internal(
            "ffmpeg produced empty WAV output".into(),
        ));
    }

    debug!(
        output_path_len = output_path.display().to_string().len(),
        size_bytes = metadata.len(),
        "Audio transcode complete"
    );

    Ok(output_path)
}

/// Check whether ffmpeg is available on the system.
pub async fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Probe audio duration in seconds using ffprobe.
///
/// Returns the duration of the audio stream without decoding the full file.
/// Falls back to container-level duration if stream duration is unavailable.
pub async fn probe_duration(input_path: &Path) -> matric_core::Result<f64> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        Command::new("ffprobe")
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("format=duration")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .arg(input_path)
            .output(),
    )
    .await
    .map_err(|_| matric_core::Error::Internal("ffprobe timed out".into()))?
    .map_err(|e| {
        matric_core::Error::Internal(format!(
            "ffprobe failed to start; io_error_kind={}",
            audio_io_error_kind(&e)
        ))
    })?;

    if !output.status.success() {
        return Err(matric_core::Error::Internal(audio_command_failure_detail(
            "ffprobe",
            "duration",
            output.status.code(),
            &output.stderr,
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration: f64 = stdout.trim().parse().map_err(|e| {
        matric_core::Error::Internal(audio_duration_parse_failure_detail(stdout.as_ref(), &e))
    })?;

    debug!(
        input_path_len = input_path.display().to_string().len(),
        duration_secs = duration,
        "Audio duration probed"
    );

    Ok(duration)
}

/// Split an audio file into chunks of `chunk_secs` duration using ffmpeg.
///
/// Uses `-ss` (seek) and `-t` (duration) for each chunk, outputting to
/// `{output_dir}/chunk_NNNN.wav` in 16kHz mono PCM format.
///
/// Returns a sorted list of `(chunk_start_secs, chunk_path)` pairs.
pub async fn split_audio_chunks(
    input_path: &Path,
    output_dir: &Path,
    total_duration_secs: f64,
    chunk_secs: u64,
) -> matric_core::Result<Vec<(f64, PathBuf)>> {
    let mut chunks = Vec::new();
    let mut offset: f64 = 0.0;
    let chunk_dur = chunk_secs as f64;
    let mut index: u32 = 0;

    while offset < total_duration_secs {
        let chunk_path = output_dir.join(format!("chunk_{:04}.wav", index));
        let remaining = total_duration_secs - offset;
        let this_chunk_dur = remaining.min(chunk_dur);

        debug!(
            chunk = index,
            offset_secs = offset,
            duration_secs = this_chunk_dur,
            chunk_path_len = chunk_path.display().to_string().len(),
            "Splitting audio chunk"
        );

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS),
            Command::new("ffmpeg")
                .arg("-ss")
                .arg(format!("{:.3}", offset))
                .arg("-i")
                .arg(input_path)
                .arg("-t")
                .arg(format!("{:.3}", this_chunk_dur))
                .arg("-acodec")
                .arg("pcm_s16le")
                .arg("-ar")
                .arg("16000")
                .arg("-ac")
                .arg("1")
                .arg("-y")
                .arg(&chunk_path)
                .output(),
        )
        .await
        .map_err(|_| {
            matric_core::Error::Internal(format!(
                "ffmpeg chunk split timed out at offset {:.1}s",
                offset
            ))
        })?
        .map_err(|e| {
            matric_core::Error::Internal(format!(
                "ffmpeg chunk split failed to start; chunk={}; io_error_kind={}",
                index,
                audio_io_error_kind(&e)
            ))
        })?;

        if !output.status.success() {
            return Err(matric_core::Error::Internal(format!(
                "ffmpeg chunk split failed at offset {:.1}s; {}",
                offset,
                audio_command_failure_detail(
                    "ffmpeg",
                    "chunk_split",
                    output.status.code(),
                    &output.stderr
                )
            )));
        }

        // Verify chunk file is non-empty
        let meta = std::fs::metadata(&chunk_path).map_err(|e| {
            matric_core::Error::Internal(format!(
                "Chunk file not found; chunk_path_len={}; io_error_kind={}",
                chunk_path.display().to_string().len(),
                audio_io_error_kind(&e)
            ))
        })?;

        if meta.len() > 0 {
            chunks.push((offset, chunk_path));
        } else {
            warn!(chunk = index, offset = offset, "Chunk is empty, skipping");
        }

        offset += chunk_dur;
        index += 1;
    }

    info!(
        chunk_count = chunks.len(),
        total_duration = total_duration_secs,
        chunk_duration = chunk_secs,
        "Audio split into chunks"
    );

    Ok(chunks)
}

/// Merge multiple chunked transcription results into a single result.
///
/// Each entry is `(chunk_start_offset_secs, transcription_result)`.
/// Segment timestamps are adjusted by adding the chunk's start offset.
/// Full text is concatenated with spaces between chunks.
pub fn merge_transcriptions(chunk_results: Vec<(f64, TranscriptionResult)>) -> TranscriptionResult {
    let mut all_segments: Vec<TranscriptionSegment> = Vec::new();
    let mut full_text_parts: Vec<String> = Vec::new();
    let mut total_duration: f64 = 0.0;
    let mut detected_language: Option<String> = None;

    for (offset, result) in chunk_results {
        if !result.full_text.is_empty() {
            full_text_parts.push(result.full_text);
        }

        if let Some(lang) = result.language {
            detected_language = Some(lang);
        }

        if let Some(dur) = result.duration_secs {
            total_duration = (offset + dur).max(total_duration);
        }

        for mut seg in result.segments {
            seg.start_secs += offset;
            seg.end_secs += offset;
            all_segments.push(seg);
        }
    }

    TranscriptionResult {
        full_text: full_text_parts.join(" "),
        segments: all_segments,
        language: detected_language,
        duration_secs: if total_duration > 0.0 {
            Some(total_duration)
        } else {
            None
        },
    }
}

/// Transcribe audio with automatic chunking for long files (Issue #543).
///
/// 1. Probes the audio duration via ffprobe.
/// 2. If duration <= `AUDIO_CHUNK_THRESHOLD_SECS`, reads the file and
///    transcribes in a single pass.
/// 3. If duration > threshold, splits into chunks of `AUDIO_CHUNK_DURATION_SECS`,
///    transcribes each independently, and merges results with correct
///    timestamp offsets.
///
/// This keeps peak memory bounded (~58 MB per 30-min chunk) regardless of
/// total audio length, enabling reliable transcription of 2+ hour files.
///
/// # Arguments
/// * `backend` - The transcription backend (Whisper)
/// * `wav_path` - Path to the transcoded 16kHz mono PCM WAV file
/// * `work_dir` - Temporary directory for chunk files
/// * `language` - Optional language hint (ISO 639-1)
pub async fn transcribe_with_chunking(
    backend: &Arc<dyn TranscriptionBackend>,
    wav_path: &Path,
    work_dir: &Path,
    language: Option<&str>,
) -> matric_core::Result<TranscriptionResult> {
    let chunk_threshold = matric_core::defaults::audio_chunk_threshold_secs();
    let chunk_duration = matric_core::defaults::audio_chunk_duration_secs();

    // Probe duration — if ffprobe fails, fall back to single-pass
    let duration = match probe_duration(wav_path).await {
        Ok(d) => d,
        Err(e) => {
            let error = e.to_string();
            warn!(
                error_len = error.len(),
                error_reason = audio_error_reason_code(&error),
                "Failed to probe audio duration, falling back to single-pass transcription"
            );
            return transcribe_single_pass(backend, wav_path, language).await;
        }
    };

    if duration <= chunk_threshold as f64 {
        debug!(
            duration = duration,
            threshold = chunk_threshold,
            "Audio below chunk threshold, using single-pass transcription"
        );
        return transcribe_single_pass(backend, wav_path, language).await;
    }

    // Long audio — split and transcribe chunks
    let num_chunks = (duration / chunk_duration as f64).ceil() as usize;
    info!(
        duration_secs = duration,
        chunk_duration_secs = chunk_duration,
        num_chunks = num_chunks,
        "Long audio detected, splitting into chunks for transcription"
    );

    let chunks = split_audio_chunks(wav_path, work_dir, duration, chunk_duration).await?;

    if chunks.is_empty() {
        return Err(matric_core::Error::Internal(
            "Audio split produced no chunks".into(),
        ));
    }

    let total_chunks = chunks.len();
    let mut chunk_results: Vec<(f64, TranscriptionResult)> = Vec::with_capacity(total_chunks);

    for (i, (offset, chunk_path)) in chunks.iter().enumerate() {
        info!(
            chunk = i + 1,
            total = total_chunks,
            offset_secs = offset,
            "Transcribing audio chunk"
        );

        let chunk_data = std::fs::read(chunk_path).map_err(|e| {
            matric_core::Error::Internal(format!(
                "Failed to read chunk; chunk_path_len={}; io_error_kind={}",
                chunk_path.display().to_string().len(),
                audio_io_error_kind(&e)
            ))
        })?;

        let result = backend
            .transcribe(&chunk_data, "audio/wav", language)
            .await
            .map_err(|e| {
                matric_core::Error::Internal(audio_transcription_failure_detail(
                    "chunk",
                    backend.model_name(),
                    chunk_data.len(),
                    &e,
                ))
            })?;

        info!(
            chunk = i + 1,
            segments = result.segments.len(),
            "Chunk transcription complete"
        );

        chunk_results.push((*offset, result));
    }

    let merged = merge_transcriptions(chunk_results);

    info!(
        total_segments = merged.segments.len(),
        duration = ?merged.duration_secs,
        language_len = ?merged.language.as_deref().map(audio_telemetry_text_len),
        chunks = total_chunks,
        "Chunked transcription complete — merged {} chunks",
        total_chunks
    );

    Ok(merged)
}

/// Single-pass transcription: read the entire file and transcribe at once.
async fn transcribe_single_pass(
    backend: &Arc<dyn TranscriptionBackend>,
    wav_path: &Path,
    language: Option<&str>,
) -> matric_core::Result<TranscriptionResult> {
    let wav_data = std::fs::read(wav_path).map_err(|e| {
        matric_core::Error::Internal(format!(
            "Failed to read audio; wav_path_len={}; io_error_kind={}",
            wav_path.display().to_string().len(),
            audio_io_error_kind(&e)
        ))
    })?;

    backend
        .transcribe(&wav_data, "audio/wav", language)
        .await
        .map_err(|e| {
            matric_core::Error::Internal(audio_transcription_failure_detail(
                "single_pass",
                backend.model_name(),
                wav_data.len(),
                &e,
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_command_failure_detail_redacts_stderr() {
        let stderr = b"Invalid data found at /srv/fortemi/audio.wav token=mm_key_secret";
        let detail = audio_command_failure_detail("ffmpeg", "transcode", Some(1), stderr);

        assert!(detail.contains("ffmpeg transcode failed"));
        assert!(detail.contains("status=1"));
        assert!(detail.contains("stderr_len="));
        assert!(detail.contains("stderr_reason=invalid_media"));
        assert!(!detail.contains("/srv/fortemi"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("Invalid data found"));
    }

    #[test]
    fn audio_duration_parse_failure_detail_redacts_stdout_and_error() {
        let stdout = "duration=/srv/fortemi/private/audio.wav token=mm_key_secret";
        let error = "invalid float literal at /srv/fortemi/private/audio.wav";
        let detail = audio_duration_parse_failure_detail(stdout, &error);

        assert!(detail.contains("Failed to parse ffprobe duration"));
        assert!(detail.contains("stdout_len="));
        assert!(detail.contains("error_len="));
        assert!(detail.contains("error_reason=invalid_input"));
        assert!(!detail.contains(stdout));
        assert!(!detail.contains(error));
        assert!(!detail.contains("/srv/fortemi"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("invalid float literal"));
    }

    #[test]
    fn audio_transcription_failure_detail_redacts_backend_metadata_and_error() {
        let backend_name = "tenant-whisper-secret-model";
        let error =
            "backend failed at https://speech.internal/v1 token=mm_key_secret /srv/private.wav";
        let detail = audio_transcription_failure_detail("single_pass", backend_name, 12345, &error);

        assert!(detail.contains("Audio transcription failed"));
        assert!(detail.contains("phase=single_pass"));
        assert!(detail.contains("backend_name_len="));
        assert!(detail.contains("audio_size_bytes=12345"));
        assert!(detail.contains("error_len="));
        assert!(detail.contains("error_reason=operation_failed"));
        assert!(!detail.contains(backend_name));
        assert!(!detail.contains("speech.internal"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("/srv/private.wav"));
        assert!(!detail.contains("backend failed"));
    }

    #[test]
    fn audio_stderr_reason_code_uses_stable_classes() {
        assert_eq!(
            audio_stderr_reason_code(b"Permission denied while reading input"),
            "permission_denied"
        );
        assert_eq!(
            audio_stderr_reason_code(b"Could not find codec parameters"),
            "invalid_media"
        );
        assert_eq!(audio_stderr_reason_code(b"No such file"), "not_found");
        assert_eq!(audio_stderr_reason_code(b"request timed out"), "timed_out");
        assert_eq!(
            audio_stderr_reason_code(b"opaque backend text"),
            "command_failed"
        );
    }

    #[test]
    fn audio_error_reason_code_uses_stable_classes() {
        assert_eq!(
            audio_error_reason_code("permission denied reading /srv/private/audio.wav"),
            "permission_denied"
        );
        assert_eq!(
            audio_error_reason_code("No such file token=mm_key_secret"),
            "not_found"
        );
        assert_eq!(
            audio_error_reason_code("decode failed for generated output"),
            "invalid_input"
        );
        assert_eq!(
            audio_error_reason_code("opaque backend text /srv/private/audio.wav"),
            "operation_failed"
        );
    }

    #[test]
    fn audio_language_telemetry_uses_length_only() {
        let language = "tenant-secret-language@example.com";
        let rendered = format!(
            "language_len={:?}",
            Some(language).map(audio_telemetry_text_len)
        );

        assert!(rendered.contains("language_len=Some"));
        assert!(!rendered.contains("tenant-secret-language"));
        assert!(!rendered.contains("example.com"));
    }

    #[test]
    fn audio_io_error_kind_uses_stable_classes() {
        assert_eq!(
            audio_io_error_kind(&std::io::Error::from(std::io::ErrorKind::NotFound)),
            "not_found"
        );
        assert_eq!(
            audio_io_error_kind(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            "permission_denied"
        );
    }

    #[tokio::test]
    async fn test_ffmpeg_available_check() {
        // This test just verifies the function runs without panic.
        // On CI with ffmpeg installed, it should return true.
        let available = ffmpeg_available().await;
        // We don't assert true/false since ffmpeg may or may not be installed
        // in the test environment. Just ensure no crash.
        let _ = available;
    }

    #[tokio::test]
    async fn test_transcode_nonexistent_input() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let result =
            transcode_to_speech_wav(Path::new("/nonexistent/audio.mp3"), tmp_dir.path()).await;
        assert!(result.is_err(), "should fail for nonexistent input");
    }

    #[tokio::test]
    async fn test_probe_duration_nonexistent() {
        let result = probe_duration(Path::new("/nonexistent/audio.wav")).await;
        assert!(result.is_err(), "should fail for nonexistent file");
    }

    #[test]
    fn test_merge_transcriptions_single_chunk() {
        let result = TranscriptionResult {
            full_text: "Hello world".to_string(),
            segments: vec![
                TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 2.0,
                    text: "Hello".to_string(),
                    speaker_id: None,
                    words: None,
                },
                TranscriptionSegment {
                    start_secs: 2.0,
                    end_secs: 4.0,
                    text: "world".to_string(),
                    speaker_id: None,
                    words: None,
                },
            ],
            language: Some("en".to_string()),
            duration_secs: Some(4.0),
        };

        let merged = merge_transcriptions(vec![(0.0, result)]);

        assert_eq!(merged.full_text, "Hello world");
        assert_eq!(merged.segments.len(), 2);
        assert_eq!(merged.segments[0].start_secs, 0.0);
        assert_eq!(merged.segments[1].end_secs, 4.0);
        assert_eq!(merged.language, Some("en".to_string()));
        assert_eq!(merged.duration_secs, Some(4.0));
    }

    #[test]
    fn test_merge_transcriptions_multiple_chunks() {
        let chunk1 = TranscriptionResult {
            full_text: "First chunk".to_string(),
            segments: vec![
                TranscriptionSegment {
                    start_secs: 0.0,
                    end_secs: 5.0,
                    text: "First".to_string(),
                    speaker_id: None,
                    words: None,
                },
                TranscriptionSegment {
                    start_secs: 5.0,
                    end_secs: 10.0,
                    text: "chunk".to_string(),
                    speaker_id: None,
                    words: None,
                },
            ],
            language: Some("en".to_string()),
            duration_secs: Some(10.0),
        };

        let chunk2 = TranscriptionResult {
            full_text: "Second chunk".to_string(),
            segments: vec![TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 8.0,
                text: "Second chunk".to_string(),
                speaker_id: None,
                words: None,
            }],
            language: Some("en".to_string()),
            duration_secs: Some(8.0),
        };

        let chunk3 = TranscriptionResult {
            full_text: "Third chunk".to_string(),
            segments: vec![TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 5.0,
                text: "Third chunk".to_string(),
                speaker_id: None,
                words: None,
            }],
            language: Some("en".to_string()),
            duration_secs: Some(5.0),
        };

        // Chunks at offsets: 0s, 1800s (30 min), 3600s (60 min)
        let merged = merge_transcriptions(vec![(0.0, chunk1), (1800.0, chunk2), (3600.0, chunk3)]);

        assert_eq!(merged.full_text, "First chunk Second chunk Third chunk");
        assert_eq!(merged.segments.len(), 4);

        // Chunk 1 segments: unchanged (offset 0)
        assert_eq!(merged.segments[0].start_secs, 0.0);
        assert_eq!(merged.segments[0].end_secs, 5.0);
        assert_eq!(merged.segments[1].start_secs, 5.0);
        assert_eq!(merged.segments[1].end_secs, 10.0);

        // Chunk 2 segment: offset by 1800s
        assert_eq!(merged.segments[2].start_secs, 1800.0);
        assert_eq!(merged.segments[2].end_secs, 1808.0);

        // Chunk 3 segment: offset by 3600s
        assert_eq!(merged.segments[3].start_secs, 3600.0);
        assert_eq!(merged.segments[3].end_secs, 3605.0);

        // Total duration: max(0+10, 1800+8, 3600+5) = 3605
        assert_eq!(merged.duration_secs, Some(3605.0));
        assert_eq!(merged.language, Some("en".to_string()));
    }

    #[test]
    fn test_merge_transcriptions_empty() {
        let merged = merge_transcriptions(vec![]);
        assert_eq!(merged.full_text, "");
        assert!(merged.segments.is_empty());
        assert!(merged.language.is_none());
        assert!(merged.duration_secs.is_none());
    }

    #[test]
    fn test_merge_transcriptions_preserves_speaker_ids() {
        let chunk = TranscriptionResult {
            full_text: "Speaker test".to_string(),
            segments: vec![TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 3.0,
                text: "Speaker test".to_string(),
                speaker_id: Some("SPEAKER_00".to_string()),
                words: None,
            }],
            language: None,
            duration_secs: Some(3.0),
        };

        let merged = merge_transcriptions(vec![(600.0, chunk)]);
        assert_eq!(merged.segments[0].start_secs, 600.0);
        assert_eq!(merged.segments[0].end_secs, 603.0);
        assert_eq!(
            merged.segments[0].speaker_id,
            Some("SPEAKER_00".to_string())
        );
    }
}

//! Shared audio utilities for transcription and diarization pipelines.
//!
//! Provides format normalization via ffmpeg to produce the standard speech
//! processing format: 16kHz mono PCM WAV (pcm_s16le).

use std::path::{Path, PathBuf};

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use tokio::process::Command;
use tracing::debug;

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
        input = %input_path.display(),
        output = %output_path.display(),
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
    .map_err(|e| matric_core::Error::Internal(format!("Failed to execute ffmpeg: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(matric_core::Error::Internal(format!(
            "ffmpeg transcode failed (exit {}): {}",
            output.status,
            stderr.trim()
        )));
    }

    // Verify output file was created and is non-empty
    let metadata = std::fs::metadata(&output_path).map_err(|e| {
        matric_core::Error::Internal(format!(
            "Transcoded WAV not found at {}: {}",
            output_path.display(),
            e
        ))
    })?;

    if metadata.len() == 0 {
        return Err(matric_core::Error::Internal(
            "ffmpeg produced empty WAV output".into(),
        ));
    }

    debug!(
        output = %output_path.display(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

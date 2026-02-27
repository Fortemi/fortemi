//! Speaker diarization backend traits and types.
//!
//! Provides a pluggable backend trait for speaker diarization services
//! (e.g., pyannote/speaker-diarization-3.1). The diarization pipeline
//! runs after transcription to identify and label speakers, then aligns
//! diarization segments with transcript word timestamps.

use async_trait::async_trait;
use matric_core::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::transcription::TranscriptionSegment;

/// A diarization segment identifying a speaker over a time range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiarizationSegment {
    /// Speaker identifier (e.g., "SPEAKER_00", "SPEAKER_01").
    pub speaker_id: String,
    /// Start time in seconds.
    pub start_secs: f64,
    /// End time in seconds.
    pub end_secs: f64,
}

/// Result of speaker diarization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiarizationResult {
    /// Diarization segments with speaker labels.
    pub segments: Vec<DiarizationSegment>,
    /// Number of distinct speakers detected.
    pub num_speakers: usize,
}

/// Backend for speaker diarization of audio files.
#[async_trait]
pub trait DiarizationBackend: Send + Sync {
    /// Run speaker diarization on an audio file.
    ///
    /// Returns time-aligned speaker segments. The `min_speakers` and `max_speakers`
    /// hints are optional — the backend should auto-detect if not provided.
    async fn diarize(
        &self,
        audio_path: &Path,
        min_speakers: Option<usize>,
        max_speakers: Option<usize>,
    ) -> Result<DiarizationResult>;

    /// Check if the diarization backend is available.
    async fn health_check(&self) -> Result<bool>;

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

/// Align diarization segments with transcript segments.
///
/// For each transcript segment, finds the overlapping diarization segment(s)
/// and assigns the majority speaker. Uses timestamp overlap to determine
/// which speaker is speaking during each transcript segment.
pub fn align_speakers(transcript: &mut [TranscriptionSegment], diarization: &DiarizationResult) {
    for seg in transcript.iter_mut() {
        let mut best_speaker: Option<&str> = None;
        let mut best_overlap = 0.0f64;

        for diar in &diarization.segments {
            let overlap_start = seg.start_secs.max(diar.start_secs);
            let overlap_end = seg.end_secs.min(diar.end_secs);
            let overlap = (overlap_end - overlap_start).max(0.0);

            if overlap > best_overlap {
                best_overlap = overlap;
                best_speaker = Some(&diar.speaker_id);
            }
        }

        if let Some(speaker) = best_speaker {
            seg.speaker_id = Some(speaker.to_string());
        }
    }
}

/// pyannote-compatible REST API client.
///
/// Expects a service exposing a POST endpoint that accepts audio files
/// and returns RTTM-formatted diarization output.
pub struct PyAnnoteBackend {
    base_url: String,
    model: String,
    client: reqwest::Client,
    timeout_secs: u64,
    retry_config: crate::retry::RetryConfig,
    circuit_breaker: crate::circuit_breaker::CircuitBreaker,
}

impl PyAnnoteBackend {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
            timeout_secs: 600, // 10 min for long audio
            retry_config: crate::retry::RetryConfig::default(),
            circuit_breaker: crate::circuit_breaker::CircuitBreaker::new(
                crate::circuit_breaker::CircuitBreakerConfig::new("pyannote"),
            ),
        }
    }

    /// Create from environment variables.
    /// Returns None if DIARIZATION_BASE_URL is not set or empty.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var(matric_core::defaults::ENV_DIARIZATION_BASE_URL).ok()?;
        if base_url.is_empty() {
            return None;
        }
        let model = std::env::var(matric_core::defaults::ENV_DIARIZATION_MODEL)
            .unwrap_or_else(|_| matric_core::defaults::DEFAULT_DIARIZATION_MODEL.to_string());
        Some(Self::new(base_url, model))
    }
}

/// Parse RTTM output into diarization segments.
///
/// RTTM format: `SPEAKER file channel start duration <NA> <NA> speaker_id <NA> <NA>`
fn parse_rttm(rttm: &str) -> Vec<DiarizationSegment> {
    let mut segments = Vec::new();
    for line in rttm.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 8 && parts[0] == "SPEAKER" {
            if let (Ok(start), Ok(duration)) = (parts[3].parse::<f64>(), parts[4].parse::<f64>()) {
                segments.push(DiarizationSegment {
                    speaker_id: parts[7].to_string(),
                    start_secs: start,
                    end_secs: start + duration,
                });
            }
        }
    }
    segments
}

#[async_trait]
impl DiarizationBackend for PyAnnoteBackend {
    async fn diarize(
        &self,
        audio_path: &Path,
        min_speakers: Option<usize>,
        max_speakers: Option<usize>,
    ) -> Result<DiarizationResult> {
        self.circuit_breaker.check_request()?;

        let url = format!("{}/diarize", self.base_url);

        let audio_data = tokio::fs::read(audio_path).await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to read audio file: {}", e))
        })?;

        let file_name = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.wav")
            .to_string();

        // Build multipart form inside retry closure (forms can't be cloned)
        let response = crate::retry::with_retry(&self.retry_config, "pyannote", || {
            let audio = audio_data.clone();
            let url = url.clone();
            let model = self.model.clone();
            let fname = file_name.clone();
            let client = self.client.clone();
            let timeout = self.timeout_secs;
            async move {
                let file_part = reqwest::multipart::Part::bytes(audio)
                    .file_name(fname)
                    .mime_str("audio/wav")
                    .expect("valid MIME type");

                let mut form = reqwest::multipart::Form::new()
                    .part("file", file_part)
                    .text("model", model);

                if let Some(min) = min_speakers {
                    form = form.text("min_speakers", min.to_string());
                }
                if let Some(max) = max_speakers {
                    form = form.text("max_speakers", max.to_string());
                }

                client
                    .post(&url)
                    .multipart(form)
                    .timeout(std::time::Duration::from_secs(timeout))
                    .send()
                    .await
            }
        })
        .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                self.circuit_breaker.record_failure();
                return Err(e);
            }
        };

        if !response.status().is_success() {
            self.circuit_breaker.record_failure();
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(format!(
                "Diarization API returned {}: {}",
                status, body
            )));
        }

        self.circuit_breaker.record_success();

        let rttm_output = response.text().await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to read diarization response: {}", e))
        })?;

        let segments = parse_rttm(&rttm_output);
        let num_speakers = {
            let mut speakers: Vec<&str> = segments.iter().map(|s| s.speaker_id.as_str()).collect();
            speakers.sort_unstable();
            speakers.dedup();
            speakers.len()
        };

        Ok(DiarizationResult {
            segments,
            num_speakers,
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
    fn test_parse_rttm() {
        let rttm = "\
SPEAKER audio_file 1 0.000 2.500 <NA> <NA> SPEAKER_00 <NA> <NA>
SPEAKER audio_file 1 2.500 3.000 <NA> <NA> SPEAKER_01 <NA> <NA>
SPEAKER audio_file 1 5.500 1.500 <NA> <NA> SPEAKER_00 <NA> <NA>
";
        let segments = parse_rttm(rttm);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].speaker_id, "SPEAKER_00");
        assert_eq!(segments[0].start_secs, 0.0);
        assert_eq!(segments[0].end_secs, 2.5);
        assert_eq!(segments[1].speaker_id, "SPEAKER_01");
        assert_eq!(segments[1].start_secs, 2.5);
        assert_eq!(segments[1].end_secs, 5.5);
        assert_eq!(segments[2].speaker_id, "SPEAKER_00");
        assert_eq!(segments[2].start_secs, 5.5);
        assert_eq!(segments[2].end_secs, 7.0);
    }

    #[test]
    fn test_parse_rttm_empty() {
        assert!(parse_rttm("").is_empty());
        assert!(parse_rttm("# comment line\n").is_empty());
    }

    #[test]
    fn test_align_speakers() {
        let diarization = DiarizationResult {
            segments: vec![
                DiarizationSegment {
                    speaker_id: "SPEAKER_00".to_string(),
                    start_secs: 0.0,
                    end_secs: 3.0,
                },
                DiarizationSegment {
                    speaker_id: "SPEAKER_01".to_string(),
                    start_secs: 3.0,
                    end_secs: 6.0,
                },
            ],
            num_speakers: 2,
        };

        let mut transcript = vec![
            TranscriptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "Hello world.".to_string(),
                speaker_id: None,
                words: None,
            },
            TranscriptionSegment {
                start_secs: 2.5,
                end_secs: 5.0,
                text: "How are you?".to_string(),
                speaker_id: None,
                words: None,
            },
        ];

        align_speakers(&mut transcript, &diarization);

        // First segment: 0-2.5 overlaps SPEAKER_00 (0-3.0) by 2.5s
        assert_eq!(transcript[0].speaker_id.as_deref(), Some("SPEAKER_00"));
        // Second segment: 2.5-5.0 overlaps SPEAKER_00 (0-3.0) by 0.5s
        //   and SPEAKER_01 (3.0-6.0) by 2.0s → majority is SPEAKER_01
        assert_eq!(transcript[1].speaker_id.as_deref(), Some("SPEAKER_01"));
    }

    #[test]
    fn test_align_speakers_no_overlap() {
        let diarization = DiarizationResult {
            segments: vec![DiarizationSegment {
                speaker_id: "SPEAKER_00".to_string(),
                start_secs: 10.0,
                end_secs: 15.0,
            }],
            num_speakers: 1,
        };

        let mut transcript = vec![TranscriptionSegment {
            start_secs: 0.0,
            end_secs: 2.0,
            text: "Test".to_string(),
            speaker_id: None,
            words: None,
        }];

        align_speakers(&mut transcript, &diarization);
        // No overlap — speaker_id stays None
        assert_eq!(transcript[0].speaker_id, None);
    }

    #[test]
    fn test_diarization_result_serialization() {
        let result = DiarizationResult {
            segments: vec![
                DiarizationSegment {
                    speaker_id: "SPEAKER_00".to_string(),
                    start_secs: 0.0,
                    end_secs: 2.5,
                },
                DiarizationSegment {
                    speaker_id: "SPEAKER_01".to_string(),
                    start_secs: 2.5,
                    end_secs: 5.0,
                },
            ],
            num_speakers: 2,
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["num_speakers"], 2);
        assert_eq!(json["segments"].as_array().unwrap().len(), 2);

        let deserialized: DiarizationResult = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, result);
    }

    #[test]
    fn test_pyannote_backend_new() {
        let backend = PyAnnoteBackend::new(
            "http://localhost:8001".to_string(),
            "pyannote/speaker-diarization-3.1".to_string(),
        );
        assert_eq!(backend.base_url, "http://localhost:8001");
        assert_eq!(backend.model, "pyannote/speaker-diarization-3.1");
        assert_eq!(backend.timeout_secs, 600);
        assert_eq!(backend.model_name(), "pyannote/speaker-diarization-3.1");
    }
}

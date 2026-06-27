//! Streaming ASR contracts for real-time call ingestion.
//!
//! ADR-RTP-003 separates transport media ingestion from speech recognition.
//! Implementations consume mono signed PCM normalized to 16 kHz and emit partial
//! and final transcript events.

use std::fmt;
use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{stream, Stream};
use matric_core::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub mod deepgram;

pub type TranscriptEventStream = Pin<Box<dyn Stream<Item = TranscriptEvent> + Send>>;

#[derive(Clone, Default)]
pub struct AsrSessionConfig {
    pub sample_rate_hz: u32,
    pub language: Option<String>,
    pub metadata: serde_json::Value,
}

impl fmt::Debug for AsrSessionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsrSessionConfig")
            .field("sample_rate_hz", &self.sample_rate_hz)
            .field(
                "language_len",
                &self.language.as_ref().map(|value| realtime_text_len(value)),
            )
            .field("metadata_class", &asr_json_class(&self.metadata))
            .field(
                "metadata_len",
                &realtime_text_len(&self.metadata.to_string()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub enum TranscriptEvent {
    Partial {
        text: String,
        ts: DateTime<Utc>,
    },
    Final {
        text: String,
        speaker_label: Option<String>,
        start_ts: Option<f64>,
        end_ts: Option<f64>,
        confidence: Option<f32>,
    },
    Error {
        reason: String,
    },
}

impl fmt::Debug for TranscriptEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Partial { text, ts } => f
                .debug_struct("TranscriptEvent::Partial")
                .field("text_len", &realtime_text_len(text))
                .field("timestamp_set", &true)
                .field("timestamp_millis", &ts.timestamp_millis())
                .finish(),
            Self::Final {
                text,
                speaker_label,
                start_ts,
                end_ts,
                confidence,
            } => f
                .debug_struct("TranscriptEvent::Final")
                .field("text_len", &realtime_text_len(text))
                .field(
                    "speaker_label_len",
                    &speaker_label.as_ref().map(|value| realtime_text_len(value)),
                )
                .field("start_ts_set", &start_ts.is_some())
                .field("end_ts_set", &end_ts.is_some())
                .field("confidence", confidence)
                .finish(),
            Self::Error { reason } => f
                .debug_struct("TranscriptEvent::Error")
                .field("reason_len", &realtime_text_len(reason))
                .finish(),
        }
    }
}

fn realtime_text_len(value: &str) -> usize {
    value.chars().count()
}

fn asr_json_class(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Streaming ASR provider factory. See ADR-RTP-003 for the media pipeline.
#[async_trait]
pub trait StreamingASRBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn start_session(&self, config: AsrSessionConfig) -> Result<Box<dyn AsrSession>>;
}

/// Active ASR session that accepts normalized 16 kHz PCM frames.
#[async_trait]
pub trait AsrSession: Send + Sync {
    async fn push_pcm16k(&mut self, samples: &[i16]) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    fn events(&mut self) -> TranscriptEventStream;
}

#[derive(Clone)]
pub struct MockAsrBackend {
    events: Vec<TranscriptEvent>,
}

impl fmt::Debug for MockAsrBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockAsrBackend")
            .field("events_count", &self.events.len())
            .field("events", &self.events)
            .finish()
    }
}

impl Default for MockAsrBackend {
    fn default() -> Self {
        Self {
            events: vec![TranscriptEvent::Final {
                text: "mock transcript".to_string(),
                speaker_label: Some("speaker_0".to_string()),
                start_ts: Some(0.0),
                end_ts: Some(1.0),
                confidence: Some(1.0),
            }],
        }
    }
}

impl MockAsrBackend {
    pub fn new(events: Vec<TranscriptEvent>) -> Self {
        Self { events }
    }
}

#[async_trait]
impl StreamingASRBackend for MockAsrBackend {
    fn name(&self) -> &str {
        "mock-asr"
    }

    async fn start_session(&self, _config: AsrSessionConfig) -> Result<Box<dyn AsrSession>> {
        Ok(Box::new(MockAsrSession::new(self.events.clone())))
    }
}

pub struct MockAsrSession {
    events: Vec<TranscriptEvent>,
    sample_count: usize,
    tx: mpsc::UnboundedSender<TranscriptEvent>,
    rx: Option<mpsc::UnboundedReceiver<TranscriptEvent>>,
}

impl MockAsrSession {
    fn new(events: Vec<TranscriptEvent>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            events,
            sample_count: 0,
            tx,
            rx: Some(rx),
        }
    }
}

#[async_trait]
impl AsrSession for MockAsrSession {
    async fn push_pcm16k(&mut self, samples: &[i16]) -> Result<()> {
        self.sample_count += samples.len();
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        if self.events.is_empty() && self.sample_count > 0 {
            let _ = self.tx.send(TranscriptEvent::Final {
                text: format!("{} samples", self.sample_count),
                speaker_label: None,
                start_ts: Some(0.0),
                end_ts: None,
                confidence: Some(1.0),
            });
        } else {
            for event in self.events.clone() {
                let _ = self.tx.send(event);
            }
        }
        Ok(())
    }

    fn events(&mut self) -> TranscriptEventStream {
        match self.rx.take() {
            Some(rx) => Box::pin(UnboundedReceiverStream::new(rx)),
            None => Box::pin(stream::empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::json;

    #[tokio::test]
    async fn mock_asr_backend_emits_final_events() {
        let backend = MockAsrBackend::default();
        let mut session = backend
            .start_session(AsrSessionConfig::default())
            .await
            .unwrap();
        session.push_pcm16k(&[1, 2, 3]).await.unwrap();
        session.close().await.unwrap();

        let events: Vec<_> = session.events().take(1).collect().await;
        assert!(matches!(
            events.first(),
            Some(TranscriptEvent::Final { .. })
        ));
    }

    #[test]
    fn asr_debug_redacts_transcripts_metadata_and_reasons() {
        let language = "en-US-privaté";
        let partial_text = "patient said 555-1212 and sk-live-secrét-token";
        let final_text = "final transcript contains privaté@example.test";
        let speaker_label = "speaker_privaté_label";
        let error_reason = "Deepgram returned https://api.example.test?token=secrét-token";
        let config = AsrSessionConfig {
            sample_rate_hz: 16_000,
            language: Some(language.to_string()),
            metadata: json!({
                "provider_call_id": "CA1234567890abcdef",
                "recording_url": "https://media.example.test/recording?token=secrét-token",
                "database_url": "postgres://user:pässword@db.example.test/fortemi"
            }),
        };
        let partial = TranscriptEvent::Partial {
            text: partial_text.to_string(),
            ts: Utc::now(),
        };
        let final_event = TranscriptEvent::Final {
            text: final_text.to_string(),
            speaker_label: Some(speaker_label.to_string()),
            start_ts: Some(0.25),
            end_ts: Some(1.5),
            confidence: Some(0.91),
        };
        let error = TranscriptEvent::Error {
            reason: error_reason.to_string(),
        };
        let backend =
            MockAsrBackend::new(vec![partial.clone(), final_event.clone(), error.clone()]);

        let rendered = format!("{config:?}\n{partial:?}\n{final_event:?}\n{error:?}\n{backend:?}");

        for raw in [
            "en-US-privaté",
            "CA1234567890abcdef",
            "recording_url",
            "secrét-token",
            "postgres://user:pässword@db.example.test/fortemi",
            "555-1212",
            "sk-live-secrét-token",
            "privaté@example.test",
            "speaker_privaté_label",
            "Deepgram returned",
            "https://api.example.test",
        ] {
            assert!(
                !rendered.contains(raw),
                "ASR Debug output leaked raw value {raw:?}: {rendered}"
            );
        }

        let metadata_len = config.metadata.to_string().chars().count();
        for expected in [
            format!("language_len: Some({})", language.chars().count()),
            "metadata_class: \"object\"".to_string(),
            format!("metadata_len: {metadata_len}"),
            format!("text_len: {}", partial_text.chars().count()),
            format!("text_len: {}", final_text.chars().count()),
            format!("speaker_label_len: Some({})", speaker_label.chars().count()),
            format!("reason_len: {}", error_reason.chars().count()),
            "events_count: 3".to_string(),
        ] {
            assert!(
                rendered.contains(&expected),
                "ASR Debug output should retain safe metadata field {expected:?}: {rendered}"
            );
        }
    }
}

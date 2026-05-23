//! Streaming ASR contracts for real-time call ingestion.
//!
//! ADR-RTP-003 separates transport media ingestion from speech recognition.
//! Implementations consume mono signed PCM normalized to 16 kHz and emit partial
//! and final transcript events.

use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{stream, Stream};
use matric_core::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub mod deepgram;

pub type TranscriptEventStream = Pin<Box<dyn Stream<Item = TranscriptEvent> + Send>>;

#[derive(Debug, Clone, Default)]
pub struct AsrSessionConfig {
    pub sample_rate_hz: u32,
    pub language: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct MockAsrBackend {
    events: Vec<TranscriptEvent>,
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
}

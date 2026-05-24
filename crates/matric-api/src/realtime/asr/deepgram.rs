//! Deepgram streaming ASR backend.
//!
//! This module contains Deepgram-specific wire protocol details behind the
//! provider-agnostic [`StreamingASRBackend`] contract.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use matric_core::{Error, Result};
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, USER_AGENT};
use tokio_tungstenite::tungstenite::Message;

use super::{
    AsrSession, AsrSessionConfig, StreamingASRBackend, TranscriptEvent, TranscriptEventStream,
};

const DEFAULT_LISTEN_URL: &str = "wss://api.deepgram.com/v1/listen";
const DEFAULT_MODEL: &str = "nova-3";
const DEFAULT_LANGUAGE: &str = "en";
const DEFAULT_ENCODING: &str = "linear16";
const TARGET_SAMPLE_RATE_HZ: u32 = 16_000;
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    pub listen_url: String,
    pub model: String,
    pub language: String,
    pub encoding: String,
    pub sample_rate_hz: u32,
}

impl DeepgramConfig {
    pub fn from_env() -> Result<Self> {
        let api_key = load_api_key()?;
        Ok(Self {
            api_key,
            listen_url: std::env::var("DEEPGRAM_LISTEN_URL")
                .unwrap_or_else(|_| DEFAULT_LISTEN_URL.to_string()),
            model: std::env::var("DEEPGRAM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            language: std::env::var("DEEPGRAM_LANGUAGE")
                .unwrap_or_else(|_| DEFAULT_LANGUAGE.to_string()),
            encoding: std::env::var("DEEPGRAM_ENCODING")
                .unwrap_or_else(|_| DEFAULT_ENCODING.to_string()),
            sample_rate_hz: std::env::var("DEEPGRAM_SAMPLE_RATE_HZ")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(TARGET_SAMPLE_RATE_HZ),
        })
    }

    fn listen_request_url(&self) -> String {
        format!(
            "{}?model={}&language={}&encoding={}&sample_rate={}&interim_results=true&smart_format=true&diarize=true",
            self.listen_url,
            urlencoding::encode(&self.model),
            urlencoding::encode(&self.language),
            urlencoding::encode(&self.encoding),
            self.sample_rate_hz
        )
    }
}

fn load_api_key() -> Result<String> {
    if let Ok(value) = std::env::var("DEEPGRAM_API_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Ok(path) = std::env::var("DEEPGRAM_API_KEY_FILE") {
        let trimmed_path = path.trim();
        if !trimmed_path.is_empty() {
            let contents = fs::read_to_string(trimmed_path)?;
            let key = contents.trim();
            if !key.is_empty() {
                return Ok(key.to_string());
            }
        }
    }

    Err(Error::Config(
        "DEEPGRAM_API_KEY or DEEPGRAM_API_KEY_FILE is required".to_string(),
    ))
}

#[derive(Debug, Default)]
pub struct DeepgramMetrics {
    partial_events: AtomicU64,
    final_events: AtomicU64,
    failover_total: AtomicU64,
    last_partial_latency_ms: AtomicU64,
}

impl DeepgramMetrics {
    pub fn snapshot(&self) -> DeepgramMetricsSnapshot {
        DeepgramMetricsSnapshot {
            partial_events: self.partial_events.load(Ordering::Relaxed),
            final_events: self.final_events.load(Ordering::Relaxed),
            failover_total: self.failover_total.load(Ordering::Relaxed),
            last_partial_latency_ms: self.last_partial_latency_ms.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepgramMetricsSnapshot {
    pub partial_events: u64,
    pub final_events: u64,
    pub failover_total: u64,
    pub last_partial_latency_ms: u64,
}

#[derive(Clone)]
pub struct DeepgramBackend {
    config: DeepgramConfig,
    metrics: Arc<DeepgramMetrics>,
    fallback: Option<Arc<dyn StreamingASRBackend>>,
}

impl DeepgramBackend {
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(DeepgramConfig::from_env()?))
    }

    pub fn new(config: DeepgramConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(DeepgramMetrics::default()),
            fallback: None,
        }
    }

    pub fn with_fallback(mut self, fallback: Arc<dyn StreamingASRBackend>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    pub fn metrics(&self) -> Arc<DeepgramMetrics> {
        self.metrics.clone()
    }

    async fn open_deepgram_session(
        &self,
        session_config: &AsrSessionConfig,
    ) -> Result<Box<dyn AsrSession>> {
        let mut config = self.config.clone();
        if let Some(language) = session_config
            .language
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            config.language = language.trim().to_string();
        }
        if session_config.sample_rate_hz > 0 {
            config.sample_rate_hz = session_config.sample_rate_hz;
        }
        let url = config.listen_request_url();
        let mut request = url
            .into_client_request()
            .map_err(|err| Error::Request(format!("Deepgram request setup failed: {err}")))?;
        let auth = format!("Token {}", config.api_key);
        request.headers_mut().insert(
            AUTHORIZATION,
            auth.parse()
                .map_err(|err| Error::Config(format!("Deepgram auth header invalid: {err}")))?,
        );
        request.headers_mut().insert(
            USER_AGENT,
            "fortemi-realtime-asr/1.0"
                .parse()
                .expect("static user-agent header must parse"),
        );

        let (socket, _) = connect_with_retries(request).await?;
        let (sink, mut source) = socket.split();
        let sink = Arc::new(Mutex::new(sink));
        let (tx, rx) = mpsc::unbounded_channel();
        let metrics = self.metrics.clone();
        let opened_at = Instant::now();

        tokio::spawn(async move {
            while let Some(next) = source.next().await {
                match next {
                    Ok(Message::Text(text)) => {
                        for event in parse_deepgram_message(&text, opened_at, &metrics) {
                            let _ = tx.send(event);
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(err) => {
                        let _ = tx.send(TranscriptEvent::Error {
                            reason: format!("Deepgram WebSocket read failed: {err}"),
                        });
                        break;
                    }
                }
            }
        });

        Ok(Box::new(DeepgramSession { sink, rx: Some(rx) }))
    }
}

#[async_trait]
impl StreamingASRBackend for DeepgramBackend {
    fn name(&self) -> &str {
        "deepgram"
    }

    async fn start_session(&self, config: AsrSessionConfig) -> Result<Box<dyn AsrSession>> {
        match self.open_deepgram_session(&config).await {
            Ok(session) => Ok(session),
            Err(primary_err) => {
                if std::env::var("REALTIME_ASR_BACKEND_FALLBACK").is_ok() {
                    if let Some(fallback) = &self.fallback {
                        self.metrics.failover_total.fetch_add(1, Ordering::Relaxed);
                        return fallback.start_session(config).await;
                    }
                }
                Err(primary_err)
            }
        }
    }
}

async fn connect_with_retries(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
) -> Result<(
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::handshake::client::Response,
)> {
    let mut last_error = None;
    for attempt in 0..3 {
        match tokio_tungstenite::connect_async(request.clone()).await {
            Ok(session) => return Ok(session),
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(reconnect_backoff(attempt)).await;
            }
        }
    }

    Err(Error::Request(format!(
        "Deepgram WebSocket open failed after retries: {}",
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    )))
}

type DeepgramSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

pub struct DeepgramSession {
    sink: Arc<Mutex<DeepgramSink>>,
    rx: Option<mpsc::UnboundedReceiver<TranscriptEvent>>,
}

#[async_trait]
impl AsrSession for DeepgramSession {
    async fn push_pcm16k(&mut self, samples: &[i16]) -> Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.sink
            .lock()
            .await
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(|err| Error::Request(format!("Deepgram audio send failed: {err}")))
    }

    async fn close(&mut self) -> Result<()> {
        self.sink
            .lock()
            .await
            .send(Message::Close(None))
            .await
            .map_err(|err| Error::Request(format!("Deepgram close failed: {err}")))
    }

    fn events(&mut self) -> TranscriptEventStream {
        match self.rx.take() {
            Some(rx) => Box::pin(UnboundedReceiverStream::new(rx)),
            None => Box::pin(futures::stream::empty()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeepgramEnvelope {
    #[serde(rename = "type")]
    event_type: Option<String>,
    channel: Option<DeepgramChannel>,
    is_final: Option<bool>,
    speech_final: Option<bool>,
    description: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: Option<String>,
    confidence: Option<f32>,
    words: Option<Vec<DeepgramWord>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    start: Option<f64>,
    end: Option<f64>,
    speaker: Option<u32>,
}

fn parse_deepgram_message(
    text: &str,
    opened_at: Instant,
    metrics: &DeepgramMetrics,
) -> Vec<TranscriptEvent> {
    match serde_json::from_str::<DeepgramEnvelope>(text) {
        Ok(envelope) => envelope_to_events(envelope, opened_at, metrics),
        Err(err) => vec![TranscriptEvent::Error {
            reason: format!("Deepgram JSON parse failed: {err}"),
        }],
    }
}

fn envelope_to_events(
    envelope: DeepgramEnvelope,
    opened_at: Instant,
    metrics: &DeepgramMetrics,
) -> Vec<TranscriptEvent> {
    if envelope
        .event_type
        .as_deref()
        .is_some_and(|event_type| event_type.eq_ignore_ascii_case("Error"))
    {
        return vec![TranscriptEvent::Error {
            reason: envelope
                .description
                .or(envelope.message)
                .unwrap_or_else(|| "Deepgram error".to_string()),
        }];
    }

    let Some(channel) = envelope.channel else {
        return Vec::new();
    };
    let Some(alternative) = channel.alternatives.into_iter().next() else {
        return Vec::new();
    };
    let Some(transcript) = alternative.transcript.map(|value| value.trim().to_string()) else {
        return Vec::new();
    };
    if transcript.is_empty() {
        return Vec::new();
    }

    let is_final = envelope.is_final.unwrap_or(false) || envelope.speech_final.unwrap_or(false);
    if is_final {
        metrics.final_events.fetch_add(1, Ordering::Relaxed);
        let first_word = alternative.words.as_ref().and_then(|words| words.first());
        let last_word = alternative.words.as_ref().and_then(|words| words.last());
        vec![TranscriptEvent::Final {
            text: transcript,
            speaker_label: first_word
                .and_then(|word| word.speaker)
                .map(|speaker| format!("speaker_{speaker}")),
            start_ts: first_word.and_then(|word| word.start),
            end_ts: last_word.and_then(|word| word.end),
            confidence: alternative.confidence,
        }]
    } else {
        metrics.partial_events.fetch_add(1, Ordering::Relaxed);
        metrics.last_partial_latency_ms.store(
            opened_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        vec![TranscriptEvent::Partial {
            text: transcript,
            ts: chrono::Utc::now(),
        }]
    }
}

pub fn reconnect_backoff(attempt: u32) -> Duration {
    let shift = attempt.min(8);
    let millis = 200_u64.saturating_mul(1_u64 << shift);
    Duration::from_millis(millis).min(MAX_RECONNECT_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> DeepgramMetrics {
        DeepgramMetrics::default()
    }

    #[test]
    fn config_builds_deepgram_listen_url_without_key_in_query() {
        let config = DeepgramConfig {
            api_key: "secret-token".to_string(),
            listen_url: DEFAULT_LISTEN_URL.to_string(),
            model: "nova-3".to_string(),
            language: "en-US".to_string(),
            encoding: "linear16".to_string(),
            sample_rate_hz: 16_000,
        };

        let url = config.listen_request_url();
        assert!(url.starts_with(DEFAULT_LISTEN_URL));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("language=en-US"));
        assert!(!url.contains("secret-token"));
    }

    #[test]
    fn parses_partial_result_event() {
        let metrics = metrics();
        let events = parse_deepgram_message(
            r#"{
              "type": "Results",
              "is_final": false,
              "channel": {"alternatives": [{"transcript": "hello wor"}]}
            }"#,
            Instant::now(),
            &metrics,
        );

        assert!(
            matches!(events.first(), Some(TranscriptEvent::Partial { text, .. }) if text == "hello wor")
        );
        assert_eq!(metrics.snapshot().partial_events, 1);
    }

    #[test]
    fn parses_final_result_event_with_word_metadata() {
        let metrics = metrics();
        let events = parse_deepgram_message(
            r#"{
              "type": "Results",
              "is_final": true,
              "channel": {"alternatives": [{
                "transcript": "hello world",
                "confidence": 0.97,
                "words": [
                  {"start": 0.1, "end": 0.4, "speaker": 2},
                  {"start": 0.4, "end": 0.9, "speaker": 2}
                ]
              }]}
            }"#,
            Instant::now(),
            &metrics,
        );

        assert!(matches!(
            events.first(),
            Some(TranscriptEvent::Final {
                text,
                speaker_label: Some(speaker),
                start_ts: Some(start),
                end_ts: Some(end),
                confidence: Some(confidence),
            }) if text == "hello world"
                && speaker == "speaker_2"
                && (*start - 0.1).abs() < f64::EPSILON
                && (*end - 0.9).abs() < f64::EPSILON
                && (*confidence - 0.97).abs() < f32::EPSILON
        ));
        assert_eq!(metrics.snapshot().final_events, 1);
    }

    #[test]
    fn parses_deepgram_error_without_leaking_auth() {
        let metrics = metrics();
        let events = parse_deepgram_message(
            r#"{"type":"Error","description":"bad request"}"#,
            Instant::now(),
            &metrics,
        );

        assert!(
            matches!(events.first(), Some(TranscriptEvent::Error { reason }) if reason == "bad request")
        );
    }

    #[test]
    fn ignores_empty_transcripts_and_non_result_messages() {
        let metrics = metrics();
        assert!(parse_deepgram_message(
            r#"{"type":"Metadata","request_id":"abc"}"#,
            Instant::now(),
            &metrics,
        )
        .is_empty());
        assert!(parse_deepgram_message(
            r#"{"type":"Results","channel":{"alternatives":[{"transcript":"  "}]}}"#,
            Instant::now(),
            &metrics,
        )
        .is_empty());
    }

    #[tokio::test]
    async fn mocked_websocket_session_sends_audio_and_receives_transcripts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen_url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();

            let audio = socket.next().await.unwrap().unwrap();
            assert!(audio.is_binary());
            assert!(!audio.into_data().is_empty());

            socket
                .send(Message::Text(
                    r#"{
                      "type": "Results",
                      "is_final": false,
                      "channel": {"alternatives": [{"transcript": "hello wor"}]}
                    }"#
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    r#"{
                      "type": "Results",
                      "is_final": true,
                      "channel": {"alternatives": [{
                        "transcript": "hello world",
                        "confidence": 0.98,
                        "words": [{"start": 0.0, "end": 0.5, "speaker": 0}]
                      }]}
                    }"#
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let _ = socket.close(None).await;
        });

        let backend = DeepgramBackend::new(DeepgramConfig {
            api_key: "test-token-not-logged".to_string(),
            listen_url,
            model: "nova-3".to_string(),
            language: "en".to_string(),
            encoding: "linear16".to_string(),
            sample_rate_hz: 16_000,
        });

        let mut session = backend
            .start_session(AsrSessionConfig {
                sample_rate_hz: 16_000,
                language: Some("en-US".to_string()),
                metadata: serde_json::json!({"test": true}),
            })
            .await
            .unwrap();

        session.push_pcm16k(&[1, -2, 3, -4]).await.unwrap();
        let events: Vec<_> = session.events().take(2).collect().await;
        server.await.unwrap();

        assert!(matches!(
            events.first(),
            Some(TranscriptEvent::Partial { text, .. }) if text == "hello wor"
        ));
        assert!(matches!(
            events.get(1),
            Some(TranscriptEvent::Final {
                text,
                speaker_label: Some(speaker),
                confidence: Some(confidence),
                ..
            }) if text == "hello world" && speaker == "speaker_0" && (*confidence - 0.98).abs() < f32::EPSILON
        ));

        let snapshot = backend.metrics().snapshot();
        assert_eq!(snapshot.partial_events, 1);
        assert_eq!(snapshot.final_events, 1);
    }

    #[test]
    fn reconnect_backoff_caps_at_five_seconds() {
        assert_eq!(reconnect_backoff(0), Duration::from_millis(200));
        assert_eq!(reconnect_backoff(1), Duration::from_millis(400));
        assert_eq!(reconnect_backoff(10), Duration::from_secs(5));
    }
}

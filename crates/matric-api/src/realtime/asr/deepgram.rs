//! Deepgram streaming ASR backend.
//!
//! This module contains Deepgram-specific wire protocol details behind the
//! provider-agnostic [`StreamingASRBackend`] contract.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fmt, fs};

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

#[derive(Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    pub listen_url: String,
    pub model: String,
    pub language: String,
    pub encoding: String,
    pub sample_rate_hz: u32,
}

impl fmt::Debug for DeepgramConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramConfig")
            .field("api_key_set", &!self.api_key.is_empty())
            .field("api_key_len", &self.api_key.len())
            .field("listen_url_len", &self.listen_url.len())
            .field("model_len", &self.model.len())
            .field("language_len", &self.language.len())
            .field("encoding_len", &self.encoding.len())
            .field("sample_rate_hz", &self.sample_rate_hz)
            .finish()
    }
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
    partial_latency_sum_ms: AtomicU64,
    partial_latency_le_100: AtomicU64,
    partial_latency_le_250: AtomicU64,
    partial_latency_le_500: AtomicU64,
    partial_latency_le_1000: AtomicU64,
    partial_latency_le_2500: AtomicU64,
    partial_latency_le_inf: AtomicU64,
    last_partial_latency_ms: AtomicU64,
}

impl DeepgramMetrics {
    pub fn snapshot(&self) -> DeepgramMetricsSnapshot {
        DeepgramMetricsSnapshot {
            partial_events: self.partial_events.load(Ordering::Relaxed),
            final_events: self.final_events.load(Ordering::Relaxed),
            failover_total: self.failover_total.load(Ordering::Relaxed),
            partial_latency_sum_ms: self.partial_latency_sum_ms.load(Ordering::Relaxed),
            partial_latency_buckets: DeepgramPartialLatencyBuckets {
                le_100: self.partial_latency_le_100.load(Ordering::Relaxed),
                le_250: self.partial_latency_le_250.load(Ordering::Relaxed),
                le_500: self.partial_latency_le_500.load(Ordering::Relaxed),
                le_1000: self.partial_latency_le_1000.load(Ordering::Relaxed),
                le_2500: self.partial_latency_le_2500.load(Ordering::Relaxed),
                le_inf: self.partial_latency_le_inf.load(Ordering::Relaxed),
            },
            last_partial_latency_ms: self.last_partial_latency_ms.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepgramMetricsSnapshot {
    pub partial_events: u64,
    pub final_events: u64,
    pub failover_total: u64,
    pub partial_latency_sum_ms: u64,
    pub partial_latency_buckets: DeepgramPartialLatencyBuckets,
    pub last_partial_latency_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepgramPartialLatencyBuckets {
    pub le_100: u64,
    pub le_250: u64,
    pub le_500: u64,
    pub le_1000: u64,
    pub le_2500: u64,
    pub le_inf: u64,
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
            .map_err(|err| deepgram_request_error("request setup", err))?;
        let auth = format!("Token {}", config.api_key);
        request.headers_mut().insert(
            AUTHORIZATION,
            auth.parse()
                .map_err(|err| deepgram_config_error("auth header", err))?,
        );
        request.headers_mut().insert(
            USER_AGENT,
            "fortemi-realtime-asr/1.0"
                .parse()
                .expect("static user-agent header must parse"),
        );

        let (socket, _) = connect_with_retries(request.clone()).await?;
        let (sink, source) = socket.split();
        let sink = Arc::new(Mutex::new(Some(sink)));
        let closed = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::unbounded_channel();
        let metrics = self.metrics.clone();
        let opened_at = Instant::now();

        tokio::spawn(read_deepgram_events_with_reconnect(
            source,
            request,
            sink.clone(),
            closed.clone(),
            tx,
            metrics,
            opened_at,
        ));

        Ok(Box::new(DeepgramSession {
            sink,
            closed,
            rx: Some(rx),
        }))
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

    Err(deepgram_request_error(
        "websocket open",
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown error".to_string()),
    ))
}

type DeepgramSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

type DeepgramSink = futures::stream::SplitSink<DeepgramSocket, Message>;
type DeepgramSource = futures::stream::SplitStream<DeepgramSocket>;

async fn read_deepgram_events_with_reconnect(
    mut source: DeepgramSource,
    request: tokio_tungstenite::tungstenite::http::Request<()>,
    sink: Arc<Mutex<Option<DeepgramSink>>>,
    closed: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<TranscriptEvent>,
    metrics: Arc<DeepgramMetrics>,
    opened_at: Instant,
) {
    let mut reconnect_attempt = 0;
    loop {
        match source.next().await {
            Some(Ok(Message::Text(text))) => {
                reconnect_attempt = 0;
                for event in parse_deepgram_message(&text, opened_at, &metrics) {
                    let _ = tx.send(event);
                }
            }
            Some(Ok(Message::Close(_))) | None if closed.load(Ordering::Relaxed) => break,
            Some(Ok(Message::Close(_))) | None => {
                if !reconnect_deepgram_source(
                    &request,
                    &sink,
                    &closed,
                    &mut source,
                    &mut reconnect_attempt,
                )
                .await
                {
                    break;
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(err)) => {
                let _ = tx.send(TranscriptEvent::Error {
                    reason: deepgram_diagnostic_message("WebSocket read", err),
                });
                if !reconnect_deepgram_source(
                    &request,
                    &sink,
                    &closed,
                    &mut source,
                    &mut reconnect_attempt,
                )
                .await
                {
                    let _ = tx.send(TranscriptEvent::Error {
                        reason: "Deepgram WebSocket reconnect failed".to_string(),
                    });
                    break;
                }
            }
        }
    }
}

async fn reconnect_deepgram_source(
    request: &tokio_tungstenite::tungstenite::http::Request<()>,
    sink: &Arc<Mutex<Option<DeepgramSink>>>,
    closed: &Arc<AtomicBool>,
    source: &mut DeepgramSource,
    reconnect_attempt: &mut u32,
) -> bool {
    if closed.load(Ordering::Relaxed) {
        return false;
    }

    tokio::time::sleep(reconnect_backoff(*reconnect_attempt)).await;
    *reconnect_attempt += 1;

    match connect_with_retries(request.clone()).await {
        Ok((socket, _)) => {
            let (new_sink, new_source) = socket.split();
            *sink.lock().await = Some(new_sink);
            *source = new_source;
            true
        }
        Err(_) => false,
    }
}

pub struct DeepgramSession {
    sink: Arc<Mutex<Option<DeepgramSink>>>,
    closed: Arc<AtomicBool>,
    rx: Option<mpsc::UnboundedReceiver<TranscriptEvent>>,
}

#[async_trait]
impl AsrSession for DeepgramSession {
    async fn push_pcm16k(&mut self, samples: &[i16]) -> Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let mut sink = self.sink.lock().await;
        let sink = sink
            .as_mut()
            .ok_or_else(|| Error::Request("Deepgram WebSocket is not connected".to_string()))?;
        sink.send(Message::Binary(bytes.into()))
            .await
            .map_err(|err| deepgram_request_error("audio send", err))
    }

    async fn close(&mut self) -> Result<()> {
        self.closed.store(true, Ordering::Relaxed);
        let mut sink = self.sink.lock().await;
        match sink.as_mut() {
            Some(sink) => {
                let _ = sink.send(Message::Close(None)).await;
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn events(&mut self) -> TranscriptEventStream {
        match self.rx.take() {
            Some(rx) => Box::pin(UnboundedReceiverStream::new(rx)),
            None => Box::pin(futures::stream::empty()),
        }
    }
}

fn deepgram_request_error(kind: &'static str, err: impl fmt::Display) -> Error {
    Error::Request(deepgram_diagnostic_message(kind, err))
}

fn deepgram_config_error(kind: &'static str, err: impl fmt::Display) -> Error {
    Error::Config(deepgram_diagnostic_message(kind, err))
}

fn deepgram_provider_error_message(diagnostic: Option<&str>) -> String {
    let diagnostic = diagnostic.unwrap_or_default();
    format!(
        "Deepgram provider error; diagnostic_class={}; diagnostic_len={}",
        deepgram_diagnostic_class(diagnostic),
        deepgram_text_len(diagnostic)
    )
}

fn deepgram_diagnostic_message(kind: &'static str, err: impl fmt::Display) -> String {
    let diagnostic = err.to_string();
    format!(
        "Deepgram {kind} failed; diagnostic_class={}; diagnostic_len={}",
        deepgram_diagnostic_class(&diagnostic),
        deepgram_text_len(&diagnostic)
    )
}

fn deepgram_diagnostic_class(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if value.is_empty() {
        "empty"
    } else if value.chars().any(char::is_control) {
        "control_chars"
    } else if lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("sk-")
    {
        "secret_candidate"
    } else if lower.contains("://") || lower.starts_with("http") {
        "url_like"
    } else if value.contains('/') || value.contains('\\') {
        "path_like"
    } else {
        "text"
    }
}

fn deepgram_text_len(value: &str) -> usize {
    value.chars().count()
}

#[derive(Deserialize)]
struct DeepgramEnvelope {
    #[serde(rename = "type")]
    event_type: Option<String>,
    channel: Option<DeepgramChannel>,
    is_final: Option<bool>,
    speech_final: Option<bool>,
    description: Option<String>,
    message: Option<String>,
}

impl fmt::Debug for DeepgramEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramEnvelope")
            .field("event_type_len", &self.event_type.as_ref().map(String::len))
            .field("channel_set", &self.channel.is_some())
            .field("is_final", &self.is_final)
            .field("speech_final", &self.speech_final)
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .field("message_len", &self.message.as_ref().map(String::len))
            .finish()
    }
}

#[derive(Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

impl fmt::Debug for DeepgramChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramChannel")
            .field("alternatives_count", &self.alternatives.len())
            .field("alternatives", &self.alternatives)
            .finish()
    }
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    transcript: Option<String>,
    confidence: Option<f32>,
    words: Option<Vec<DeepgramWord>>,
}

impl fmt::Debug for DeepgramAlternative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramAlternative")
            .field("transcript_len", &self.transcript.as_ref().map(String::len))
            .field("confidence", &self.confidence)
            .field("words_count", &self.words.as_ref().map(Vec::len))
            .field("words", &self.words)
            .finish()
    }
}

#[derive(Deserialize)]
struct DeepgramWord {
    start: Option<f64>,
    end: Option<f64>,
    speaker: Option<u32>,
}

impl fmt::Debug for DeepgramWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramWord")
            .field("start_set", &self.start.is_some())
            .field("end_set", &self.end.is_some())
            .field("speaker_set", &self.speaker.is_some())
            .finish()
    }
}

fn parse_deepgram_message(
    text: &str,
    opened_at: Instant,
    metrics: &DeepgramMetrics,
) -> Vec<TranscriptEvent> {
    match serde_json::from_str::<DeepgramEnvelope>(text) {
        Ok(envelope) => envelope_to_events(envelope, opened_at, metrics),
        Err(err) => vec![TranscriptEvent::Error {
            reason: deepgram_diagnostic_message("JSON parse", err),
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
        let diagnostic = envelope.description.or(envelope.message);
        return vec![TranscriptEvent::Error {
            reason: deepgram_provider_error_message(diagnostic.as_deref()),
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
        let latency_ms = opened_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        metrics
            .partial_latency_sum_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        increment_partial_latency_bucket(metrics, latency_ms);
        metrics
            .last_partial_latency_ms
            .store(latency_ms, Ordering::Relaxed);
        vec![TranscriptEvent::Partial {
            text: transcript,
            ts: chrono::Utc::now(),
        }]
    }
}

fn increment_partial_latency_bucket(metrics: &DeepgramMetrics, latency_ms: u64) {
    if latency_ms <= 100 {
        metrics
            .partial_latency_le_100
            .fetch_add(1, Ordering::Relaxed);
    }
    if latency_ms <= 250 {
        metrics
            .partial_latency_le_250
            .fetch_add(1, Ordering::Relaxed);
    }
    if latency_ms <= 500 {
        metrics
            .partial_latency_le_500
            .fetch_add(1, Ordering::Relaxed);
    }
    if latency_ms <= 1000 {
        metrics
            .partial_latency_le_1000
            .fetch_add(1, Ordering::Relaxed);
    }
    if latency_ms <= 2500 {
        metrics
            .partial_latency_le_2500
            .fetch_add(1, Ordering::Relaxed);
    }
    metrics
        .partial_latency_le_inf
        .fetch_add(1, Ordering::Relaxed);
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
    fn config_debug_redacts_provider_credentials_and_topology() {
        let config = DeepgramConfig {
            api_key: "dg_sk_live_customer@example.com".to_string(),
            listen_url: "wss://api.deepgram.com/v1/listen?token=sk-live-url".to_string(),
            model: "nova-private-model".to_string(),
            language: "en-private".to_string(),
            encoding: "linear16-private".to_string(),
            sample_rate_hz: 16_000,
        };

        let rendered = format!("{config:?}");

        assert!(rendered.contains("DeepgramConfig"));
        assert!(rendered.contains("api_key_set"));
        assert!(rendered.contains("api_key_len"));
        assert!(rendered.contains("listen_url_len"));
        assert!(rendered.contains("sample_rate_hz"));

        for raw in [
            "dg_sk_live",
            "customer@example.com",
            "api.deepgram.com",
            "sk-live-url",
            "nova-private-model",
            "en-private",
            "linear16-private",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }

    #[test]
    fn deepgram_wire_debug_redacts_transcripts_and_provider_messages() {
        let envelope: DeepgramEnvelope = serde_json::from_str(
            r#"{
              "type": "Results",
              "is_final": true,
              "speech_final": true,
              "channel": {"alternatives": [{
                "transcript": "customer@example.com said sk-live-deepgram near /srv/private",
                "confidence": 0.97,
                "words": [
                  {"start": 0.1, "end": 0.4, "speaker": 2},
                  {"start": 0.4, "end": 0.9, "speaker": 2}
                ]
              }]},
              "description": "postgres://user:pass@db.internal/app",
              "message": "provider message with mm_key_deepgram"
            }"#,
        )
        .unwrap();

        let rendered_envelope = format!("{envelope:?}");
        let rendered_channel = format!("{:?}", envelope.channel.as_ref().unwrap());
        let rendered_alternative = format!(
            "{:?}",
            envelope
                .channel
                .as_ref()
                .unwrap()
                .alternatives
                .first()
                .unwrap()
        );
        let rendered_word = format!(
            "{:?}",
            envelope
                .channel
                .as_ref()
                .unwrap()
                .alternatives
                .first()
                .unwrap()
                .words
                .as_ref()
                .unwrap()
                .first()
                .unwrap()
        );
        let combined = format!(
            "{rendered_envelope}\n{rendered_channel}\n{rendered_alternative}\n{rendered_word}"
        );

        assert!(rendered_envelope.contains("DeepgramEnvelope"));
        assert!(rendered_envelope.contains("description_len"));
        assert!(rendered_envelope.contains("message_len"));
        assert!(rendered_channel.contains("alternatives_count"));
        assert!(rendered_alternative.contains("transcript_len"));
        assert!(rendered_alternative.contains("words_count"));
        assert!(rendered_word.contains("speaker_set"));

        for raw in [
            "customer@example.com",
            "sk-live-deepgram",
            "/srv/private",
            "postgres://user:pass",
            "db.internal",
            "provider message",
            "mm_key_deepgram",
            "speaker_2",
        ] {
            assert!(!combined.contains(raw), "raw value leaked: {raw}");
        }
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
        let diagnostic = "bad request for token sk-live-deepgram at wss://api.deepgram.com/listen";
        let events = parse_deepgram_message(
            &format!(r#"{{"type":"Error","description":"{diagnostic}"}}"#),
            Instant::now(),
            &metrics,
        );

        let reason = match events.first() {
            Some(TranscriptEvent::Error { reason }) => reason,
            other => panic!("unexpected event: {other:?}"),
        };

        assert!(reason.contains("Deepgram provider error"));
        assert!(reason.contains("diagnostic_class=secret_candidate"));
        assert!(reason.contains(&format!("diagnostic_len={}", diagnostic.chars().count())));
        for raw in ["sk-live-deepgram", "api.deepgram.com", "bad request"] {
            assert!(
                !reason.contains(raw),
                "raw provider diagnostic leaked: {raw}"
            );
        }
    }

    #[test]
    fn deepgram_diagnostics_report_classes_without_raw_values() {
        let diagnostic =
            "request failed for token sk-live-secret at postgres://user:pass@db.internal/app";
        let message = deepgram_diagnostic_message("request setup", diagnostic);

        assert!(message.contains("Deepgram request setup failed"));
        assert!(message.contains("diagnostic_class=secret_candidate"));
        assert!(message.contains(&format!("diagnostic_len={}", diagnostic.chars().count())));
        for raw in [
            "sk-live-secret",
            "postgres://user:pass",
            "db.internal",
            "token ",
        ] {
            assert!(!message.contains(raw), "raw diagnostic leaked: {raw}");
        }
    }

    #[test]
    fn malformed_deepgram_json_errors_do_not_echo_payload() {
        let metrics = metrics();
        let raw_payload = r#"{"type":"Results","channel":"sk-live-secret"#;
        let events = parse_deepgram_message(raw_payload, Instant::now(), &metrics);
        let reason = match events.first() {
            Some(TranscriptEvent::Error { reason }) => reason,
            other => panic!("unexpected event: {other:?}"),
        };

        assert!(reason.contains("Deepgram JSON parse failed"));
        assert!(reason.contains("diagnostic_class="));
        assert!(reason.contains("diagnostic_len="));
        for raw in ["sk-live-secret", "Results", "channel"] {
            assert!(!reason.contains(raw), "raw parse payload leaked: {raw}");
        }
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
    async fn session_open_failure_can_fail_over_to_mock_backend() {
        std::env::set_var("REALTIME_ASR_BACKEND_FALLBACK", "mock");

        let backend = DeepgramBackend::new(DeepgramConfig {
            api_key: "test-token-not-logged".to_string(),
            listen_url: "ws://127.0.0.1:9/v1/listen".to_string(),
            model: "nova-3".to_string(),
            language: "en".to_string(),
            encoding: "linear16".to_string(),
            sample_rate_hz: 16_000,
        })
        .with_fallback(Arc::new(crate::realtime::asr::MockAsrBackend::default()));

        let mut session = backend
            .start_session(AsrSessionConfig::default())
            .await
            .unwrap();
        session.push_pcm16k(&[1, 2, 3]).await.unwrap();
        session.close().await.unwrap();
        let events: Vec<_> = session.events().take(1).collect().await;

        std::env::remove_var("REALTIME_ASR_BACKEND_FALLBACK");

        assert!(matches!(
            events.first(),
            Some(TranscriptEvent::Final { text, .. }) if text == "mock transcript"
        ));
        assert_eq!(backend.metrics().snapshot().failover_total, 1);
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

    #[tokio::test]
    async fn websocket_disconnect_reconnects_and_continues_transcripts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen_url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());

        let server = tokio::spawn(async move {
            let (first_stream, _) = listener.accept().await.unwrap();
            let mut first_socket = tokio_tungstenite::accept_async(first_stream).await.unwrap();
            first_socket
                .send(Message::Text(
                    r#"{
                      "type": "Results",
                      "is_final": false,
                      "channel": {"alternatives": [{"transcript": "reconnect"}]}
                    }"#
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let _ = first_socket.close(None).await;

            let (second_stream, _) = listener.accept().await.unwrap();
            let mut second_socket = tokio_tungstenite::accept_async(second_stream)
                .await
                .unwrap();
            second_socket
                .send(Message::Text(
                    r#"{
                      "type": "Results",
                      "is_final": true,
                      "channel": {"alternatives": [{
                        "transcript": "reconnect complete",
                        "confidence": 0.91,
                        "words": [{"start": 0.0, "end": 0.5, "speaker": 1}]
                      }]}
                    }"#
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let _ = second_socket.close(None).await;
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

        let events = tokio::time::timeout(
            Duration::from_secs(5),
            session.events().take(2).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
        session.close().await.unwrap();
        server.await.unwrap();

        assert!(matches!(
            events.first(),
            Some(TranscriptEvent::Partial { text, .. }) if text == "reconnect"
        ));
        assert!(matches!(
            events.get(1),
            Some(TranscriptEvent::Final {
                text,
                speaker_label: Some(speaker),
                confidence: Some(confidence),
                ..
            }) if text == "reconnect complete" && speaker == "speaker_1" && (*confidence - 0.91).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn reconnect_backoff_caps_at_five_seconds() {
        assert_eq!(reconnect_backoff(0), Duration::from_millis(200));
        assert_eq!(reconnect_backoff(1), Duration::from_millis(400));
        assert_eq!(reconnect_backoff(10), Duration::from_secs(5));
    }
}

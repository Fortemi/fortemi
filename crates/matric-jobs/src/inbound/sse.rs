//! Server-Sent Events (SSE) inbound connector (#835).
//!
//! Holds a long-lived HTTP connection to an upstream SSE endpoint, parses the
//! `text/event-stream` wire format, and normalizes each event into the shared
//! outbox. Resumption uses the `Last-Event-ID` request header seeded from the
//! last *committed* event id, so a reconnect resumes exactly where the last
//! durable write left off (mirror of our outbound SSE).
//!
//! Reconnect + exponential backoff are provided by the supervisor: on
//! disconnect/stream-end the connector drops its connection and returns
//! [`InboundError::Transient`], the supervisor backs off (2^n, capped) and
//! calls `next_event` again, which reconnects with the resume header.
//! Malformed events are dead-lettered by the supervisor's `process` path.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex as AsyncMutex;
use tracing::info;

use super::source::{
    telemetry_destination_class, telemetry_text_len, InboundError, InboundEvent,
    InboundEventSource, InboundResult, Offset,
};

/// Connector config, deserialized from the `inbound_source.config` JSONB.
#[derive(Debug, Clone, Deserialize)]
pub struct SseConfig {
    pub url: String,
    /// Arbitrary request headers, e.g. `{"Authorization": "Bearer ..."}`.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// JSON field (within a parsed object payload) used as the outbox
    /// `event_type`; falls back to the SSE `event:` name, then `default`.
    #[serde(default = "default_event_type_field")]
    pub event_type_field: String,
    #[serde(default = "default_event_type")]
    pub default_event_type: String,
    /// Optional allow-list of event types to emit; others are skipped.
    #[serde(default)]
    pub event_type_filter: Option<Vec<String>>,
    /// HTTP connect timeout (seconds) for the streaming request.
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

fn default_event_type_field() -> String {
    "event_type".to_string()
}
fn default_event_type() -> String {
    "external.sse.v1".to_string()
}
fn default_connect_timeout_secs() -> u64 {
    10
}

struct SseState {
    resp: reqwest::Response,
    buf: Vec<u8>,
}

/// A long-lived SSE consumer connector.
pub struct SseSource {
    name: String,
    config: SseConfig,
    client: reqwest::Client,
    /// Lazily (re)established connection + unparsed byte buffer.
    conn: AsyncMutex<Option<SseState>>,
    /// Last *committed* event id; sent as `Last-Event-ID` on reconnect.
    last_committed_id: StdMutex<Option<String>>,
}

impl SseSource {
    /// Build from JSON config (sync; used by the connector registry).
    pub fn from_config(name: &str, config: &Value) -> InboundResult<Self> {
        let cfg: SseConfig = serde_json::from_value(config.clone())
            .map_err(|e| InboundError::Transient(format!("invalid sse config: {e}")))?;
        let url = cfg.url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(InboundError::Transient(
                "sse config requires an http(s) url".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(cfg.connect_timeout_secs))
            .build()
            .map_err(|e| InboundError::Transient(format!("sse client build: {e}")))?;
        Ok(Self {
            name: name.to_string(),
            config: cfg,
            client,
            conn: AsyncMutex::new(None),
            last_committed_id: StdMutex::new(None),
        })
    }

    async fn connect(&self) -> InboundResult<SseState> {
        let mut req = self
            .client
            .get(self.config.url.trim())
            .header("Accept", "text/event-stream");
        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(id) = self.last_committed_id.lock().unwrap().clone() {
            if !id.is_empty() {
                req = req.header("Last-Event-ID", id);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| InboundError::Transient(format!("sse connect: {e}")))?;
        if !resp.status().is_success() {
            return Err(InboundError::Transient(format!(
                "sse upstream returned HTTP {}",
                resp.status()
            )));
        }
        info!(
            source_name_len = telemetry_text_len(&self.name),
            destination_class = telemetry_destination_class(self.config.url.trim()),
            destination_len = telemetry_text_len(self.config.url.trim()),
            "sse connector connected"
        );
        Ok(SseState {
            resp,
            buf: Vec::new(),
        })
    }
}

#[async_trait]
impl InboundEventSource for SseSource {
    async fn next_event(&self) -> InboundResult<InboundEvent> {
        let mut guard = self.conn.lock().await;
        loop {
            if guard.is_none() {
                *guard = Some(self.connect().await?);
            }
            let state = guard.as_mut().expect("connected above");
            if let Some(ev) = take_event(&mut state.buf, &self.config) {
                return Ok(ev);
            }
            match state.resp.chunk().await {
                Ok(Some(chunk)) => state.buf.extend_from_slice(&chunk),
                Ok(None) => {
                    *guard = None;
                    return Err(InboundError::Transient("sse stream ended".to_string()));
                }
                Err(e) => {
                    *guard = None;
                    return Err(InboundError::Transient(format!("sse read: {e}")));
                }
            }
        }
    }

    async fn commit(&self, offset: Offset) -> InboundResult<()> {
        if !offset.is_empty() {
            *self.last_committed_id.lock().unwrap() = Some(offset);
        }
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Take the next emittable event from `buf`, draining consumed bytes. Returns
/// `None` when no complete, non-filtered event is buffered yet. Keepalive
/// comments, data-less blocks, and filtered-out event types are skipped.
fn take_event(buf: &mut Vec<u8>, cfg: &SseConfig) -> Option<InboundEvent> {
    loop {
        let (block_len, sep_len) = find_boundary(buf)?;
        let block = String::from_utf8_lossy(&buf[..block_len]).into_owned();
        buf.drain(..block_len + sep_len);
        if let Some(ev) = parse_block(&block, cfg) {
            if cfg
                .event_type_filter
                .as_ref()
                .map(|allow| allow.iter().any(|t| t == &ev.event_type))
                .unwrap_or(true)
            {
                return Some(ev);
            }
            // Filtered out — keep scanning for the next event.
        }
        // No data / comment-only block — keep scanning.
    }
}

/// Locate the end of the first event block. Returns `(block_len, sep_len)`
/// where `sep_len` is the length of the terminating blank line (`\n\n` or
/// `\r\n\r\n`). `None` when no complete block is buffered.
fn find_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    let mut lf = None;
    let mut crlf = None;
    for i in 0..buf.len() {
        if buf[i] == b'\n' {
            if i >= 1 && buf[i - 1] == b'\n' {
                lf = Some(i - 1);
                break;
            }
            if i >= 3 && &buf[i - 3..=i] == b"\r\n\r\n" {
                crlf = Some(i - 3);
                break;
            }
        }
    }
    match (lf, crlf) {
        (Some(p), _) => Some((p, 2)),
        (_, Some(p)) => Some((p, 4)),
        _ => None,
    }
}

/// Parse one SSE event block into an `InboundEvent`, or `None` if it carries no
/// `data` (comment/keepalive). Concatenated `data:` lines form the payload;
/// `id:` becomes the offset; `event:` is the fallback event type.
fn parse_block(block: &str, cfg: &SseConfig) -> Option<InboundEvent> {
    let mut data = String::new();
    let mut id: Option<String> = None;
    let mut event_name: Option<String> = None;
    for raw in block.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "data" => {
                data.push_str(value);
                data.push('\n');
            }
            "id" => id = Some(value.to_string()),
            "event" => event_name = Some(value.to_string()),
            _ => {} // "retry" and unknown fields ignored
        }
    }
    if data.is_empty() {
        return None;
    }
    let data = data.strip_suffix('\n').unwrap_or(&data).to_string();

    // Payload: parse JSON objects through; wrap anything else as { "data": ... }.
    let payload = match serde_json::from_str::<Value>(&data) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            let mut m = Map::new();
            m.insert("data".to_string(), Value::String(data));
            Value::Object(m)
        }
    };

    let event_type = payload
        .get(&cfg.event_type_field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| event_name.filter(|s| !s.is_empty()))
        .unwrap_or_else(|| cfg.default_event_type.clone());

    Some(InboundEvent::new(
        event_type,
        payload,
        id.unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(extra: Value) -> SseConfig {
        let mut base = json!({"url": "https://example.test/stream"});
        if let (Value::Object(b), Value::Object(e)) = (&mut base, extra) {
            for (k, v) in e {
                b.insert(k, v);
            }
        }
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn from_config_requires_http_url() {
        assert!(SseSource::from_config("s", &json!({"url":"https://x/stream"})).is_ok());
        assert!(SseSource::from_config("s", &json!({"url":"redis://x"})).is_err());
        assert!(SseSource::from_config("s", &json!({})).is_err());
    }

    #[test]
    fn parses_json_event_with_id_and_event_type_field() {
        let mut buf =
            b"id: 7\nevent: ping\ndata: {\"event_type\":\"metric.v1\",\"v\":3}\n\n".to_vec();
        let ev = take_event(&mut buf, &cfg(json!({}))).expect("event");
        assert_eq!(ev.offset, "7");
        assert_eq!(ev.event_type, "metric.v1");
        assert_eq!(ev.payload["v"], 3);
        assert!(buf.is_empty());
    }

    #[test]
    fn multiline_data_and_event_name_fallback() {
        let mut buf = b"event: alert\ndata: line1\ndata: line2\n\n".to_vec();
        let ev = take_event(&mut buf, &cfg(json!({}))).expect("event");
        assert_eq!(ev.event_type, "alert"); // no event_type field -> SSE event name
        assert_eq!(ev.payload["data"], "line1\nline2");
    }

    #[test]
    fn skips_comments_and_dataless_blocks() {
        let mut buf = b": keepalive\n\nevent: x\n\ndata: hi\n\n".to_vec();
        let ev = take_event(&mut buf, &cfg(json!({}))).expect("event after skips");
        assert_eq!(ev.payload["data"], "hi");
        assert_eq!(ev.event_type, "external.sse.v1"); // default
    }

    #[test]
    fn event_type_filter_drops_unmatched() {
        let c = cfg(json!({"event_type_filter": ["keep.v1"]}));
        let mut buf =
            b"data: {\"event_type\":\"drop.v1\"}\n\ndata: {\"event_type\":\"keep.v1\"}\n\n"
                .to_vec();
        let ev = take_event(&mut buf, &c).expect("kept event");
        assert_eq!(ev.event_type, "keep.v1");
    }

    #[test]
    fn partial_block_buffers_until_complete() {
        let mut buf = b"data: {\"event_type\":\"a.v1\"}".to_vec(); // no terminating blank line
        assert!(take_event(&mut buf, &cfg(json!({}))).is_none());
        buf.extend_from_slice(b"\n\n");
        let ev = take_event(&mut buf, &cfg(json!({}))).expect("now complete");
        assert_eq!(ev.event_type, "a.v1");
    }

    #[test]
    fn handles_crlf_line_endings() {
        let mut buf = b"id: 9\r\ndata: {\"event_type\":\"c.v1\"}\r\n\r\n".to_vec();
        let ev = take_event(&mut buf, &cfg(json!({}))).expect("crlf event");
        assert_eq!(ev.offset, "9");
        assert_eq!(ev.event_type, "c.v1");
    }
}

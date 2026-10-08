//! Redacting span processor: second line of defence for exported spans (#1156).

use std::borrow::Cow;
use std::time::Duration;

use opentelemetry::trace::Status;
use opentelemetry::{Context, KeyValue, Value};
use opentelemetry_sdk::trace::{Span, SpanData, SpanEvents, SpanProcessor};
use opentelemetry_sdk::Resource;

pub(crate) const REDACTED: &str = "[redacted]";
const MAX_ATTRIBUTE_VALUE_CHARS: usize = 256;

/// Keys (exact) that may carry raw URLs, queries, peers or statements.
const DENIED_KEYS: &[&str] = &[
    "uri",
    "url",
    "url.full",
    "url.query",
    "url.path",
    "http.url",
    "http.target",
    "db.statement",
    "db.query.text",
    "user_agent.original",
    "client.address",
    "network.peer.address",
];

/// Key fragments that indicate secrets, content or identity.
const DENIED_KEY_FRAGMENTS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "authorization",
    "cookie",
    "api_key",
    "apikey",
    "credential",
    "content",
    "body",
    "prompt",
    "query",
    "email",
    "tenant",
    "user_id",
    "header",
];

/// Derived-metric suffixes that are safe regardless of the key stem.
const SAFE_KEY_SUFFIXES: &[&str] = &["_len", "_count", "_class", "_code", "_set", "_secs", "_ms"];

fn attribute_key_allowed(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if DENIED_KEYS.contains(&key.as_str()) {
        return false;
    }
    if SAFE_KEY_SUFFIXES.iter().any(|suffix| key.ends_with(suffix)) {
        return true;
    }
    !DENIED_KEY_FRAGMENTS
        .iter()
        .any(|fragment| key.contains(fragment))
}

fn string_value_is_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.chars().count() > MAX_ATTRIBUTE_VALUE_CHARS
        || lower.contains("bearer ")
        || lower.contains("basic ")
        || lower.contains("://")
        || (value.contains('@') && value.contains('.'))
}

/// Sanitize one attribute; `None` drops it.
pub(crate) fn redact_attribute(kv: KeyValue) -> Option<KeyValue> {
    if !attribute_key_allowed(kv.key.as_str()) {
        return None;
    }
    let value = match &kv.value {
        Value::String(s) if string_value_is_sensitive(s.as_str()) => Value::from(REDACTED),
        Value::Array(_) => Value::from(REDACTED),
        other => other.clone(),
    };
    Some(KeyValue::new(kv.key, value))
}

/// Wraps the exporting processor and sanitizes every finished span.
#[derive(Debug)]
pub(crate) struct RedactingSpanProcessor<P> {
    inner: P,
}

impl<P> RedactingSpanProcessor<P> {
    pub(crate) fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P: SpanProcessor> SpanProcessor for RedactingSpanProcessor<P> {
    fn on_start(&self, span: &mut Span, cx: &Context) {
        self.inner.on_start(span, cx);
    }

    fn on_end(&self, mut span: SpanData) {
        let before = span.attributes.len();
        span.attributes = std::mem::take(&mut span.attributes)
            .into_iter()
            .filter_map(redact_attribute)
            .collect();
        span.dropped_attributes_count = span
            .dropped_attributes_count
            .saturating_add((before - span.attributes.len()) as u32);
        // Log events can quote arbitrary diagnostic fields; stdout JSON logs
        // carry them with trace ids instead.
        span.events = SpanEvents::default();
        if let Status::Error { .. } = span.status {
            span.status = Status::Error {
                description: Cow::Borrowed(""),
            };
        }
        for link in span.links.links.iter_mut() {
            link.attributes.clear();
        }
        self.inner.on_end(span);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

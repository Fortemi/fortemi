//! Per-connector metrics for inbound event sources (#833):
//! `inbound_source_events_total`, `inbound_source_errors_total`,
//! `inbound_source_lag`.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default, Clone, Copy)]
struct Counters {
    events: u64,
    errors: u64,
    lag: u64,
}

/// Thread-safe per-source counters keyed by connector name.
#[derive(Default)]
pub struct InboundMetrics {
    inner: Mutex<HashMap<String, Counters>>,
}

impl InboundMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// `inbound_source_events_total` += 1 for `name`.
    pub fn record_event(&self, name: &str) {
        self.inner
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .events += 1;
    }

    /// `inbound_source_errors_total` += 1 for `name`.
    pub fn record_error(&self, name: &str) {
        self.inner
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .errors += 1;
    }

    /// Set `inbound_source_lag` for `name` (connector-reported backlog).
    pub fn set_lag(&self, name: &str, lag: u64) {
        self.inner
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .lag = lag;
    }

    /// JSON snapshot: `{ "<name>": { events_total, errors_total, lag }, ... }`.
    pub fn snapshot(&self) -> Value {
        let guard = self.inner.lock().unwrap();
        let mut out = serde_json::Map::new();
        for (name, c) in guard.iter() {
            out.insert(
                name.clone(),
                json!({
                    "events_total": c.events,
                    "errors_total": c.errors,
                    "lag": c.lag,
                }),
            );
        }
        Value::Object(out)
    }

    /// Public health snapshot for unauthenticated probes.
    ///
    /// This intentionally omits connector names and per-source counters because
    /// source names and activity patterns can disclose tenants, partners, or
    /// internal broker topology.
    pub fn public_snapshot(&self) -> Value {
        let guard = self.inner.lock().unwrap();
        let mut events_total = 0u64;
        let mut errors_total = 0u64;
        let mut lag_total = 0u64;
        let mut lag_max = 0u64;
        for c in guard.values() {
            events_total = events_total.saturating_add(c.events);
            errors_total = errors_total.saturating_add(c.errors);
            lag_total = lag_total.saturating_add(c.lag);
            lag_max = lag_max.max(c.lag);
        }
        json!({
            "sources_total": guard.len(),
            "events_total": events_total,
            "errors_total": errors_total,
            "lag_total": lag_total,
            "lag_max": lag_max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_snapshots() {
        let m = InboundMetrics::new();
        m.record_event("redis-a");
        m.record_event("redis-a");
        m.record_error("redis-a");
        m.set_lag("redis-a", 7);
        let snap = m.snapshot();
        assert_eq!(snap["redis-a"]["events_total"], 2);
        assert_eq!(snap["redis-a"]["errors_total"], 1);
        assert_eq!(snap["redis-a"]["lag"], 7);
    }

    #[test]
    fn public_snapshot_aggregates_without_connector_names() {
        let m = InboundMetrics::new();
        m.record_event("tenant-alpha redis://user:secret@internal");
        m.record_error("tenant-beta\nbroker");
        m.set_lag("tenant-alpha redis://user:secret@internal", 7);
        m.set_lag("tenant-beta\nbroker", 11);

        let snap = m.public_snapshot();
        assert_eq!(snap["sources_total"], 2);
        assert_eq!(snap["events_total"], 1);
        assert_eq!(snap["errors_total"], 1);
        assert_eq!(snap["lag_total"], 18);
        assert_eq!(snap["lag_max"], 11);

        let rendered = serde_json::to_string(&snap).unwrap();
        assert!(!rendered.contains("tenant-alpha"));
        assert!(!rendered.contains("tenant-beta"));
        assert!(!rendered.contains("redis://"));
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("internal"));
    }
}

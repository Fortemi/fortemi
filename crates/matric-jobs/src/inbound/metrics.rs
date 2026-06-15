//! Per-connector metrics for inbound event sources (#833):
//! `inbound_source_events_total`, `inbound_source_errors_total`,
//! `inbound_source_lag`. Exposed as a JSON snapshot for `/health/streaming`.

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
}

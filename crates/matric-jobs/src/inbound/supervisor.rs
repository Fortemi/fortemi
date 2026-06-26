//! Inbound connector supervisor (#833).
//!
//! Loads enabled `inbound_source` registrations, builds a connector per source
//! via the [`SourceRegistry`], and runs each in its own task: pull an event,
//! validate it, write it to the shared `event_outbox`, then commit the upstream
//! offset (at-least-once). Transient fetch errors back off and retry; events
//! that fail processing `max_attempts` times are dead-lettered to `inbound_dlq`
//! and skipped. Connector tasks are aborted on shutdown (the uncommitted event,
//! if any, is re-delivered on restart).

use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use matric_db::{CreateOutboxEvent, Database};

use super::metrics::InboundMetrics;
use super::registry::SourceRegistry;
use super::source::{
    inbound_error_reason_code, telemetry_text_len, InboundError, InboundEvent, InboundEventSource,
};

const DEFAULT_MAX_ATTEMPTS: u32 = 3;
const MAX_BACKOFF_SECS: u64 = 30;

/// Drives registered inbound connectors into the shared outbox.
pub struct InboundSupervisor {
    db: Database,
    metrics: Arc<InboundMetrics>,
    max_attempts: u32,
}

impl InboundSupervisor {
    pub fn new(db: Database, metrics: Arc<InboundMetrics>) -> Self {
        Self {
            db,
            metrics,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }

    /// Override the per-event retry budget before dead-lettering (min 1).
    pub fn with_max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = n.max(1);
        self
    }

    pub fn metrics(&self) -> Arc<InboundMetrics> {
        self.metrics.clone()
    }

    /// Load enabled sources from the DB, build them via `registry`, and spawn a
    /// task per connector. Unknown kinds (no registered builder) are skipped
    /// with a warning. Returns the spawned handles (abort to stop).
    pub async fn start_from_registry(
        self: Arc<Self>,
        registry: &SourceRegistry,
    ) -> Vec<JoinHandle<()>> {
        let enabled = match self.db.inbound_sources.list_enabled().await {
            Ok(v) => v,
            Err(e) => {
                let error = e.to_string();
                warn!(
                    reason_code = inbound_error_reason_code(&error),
                    error_len = telemetry_text_len(&error),
                    "inbound supervisor failed to load enabled sources"
                );
                return Vec::new();
            }
        };
        let mut sources: Vec<Box<dyn InboundEventSource>> = Vec::new();
        for cfg in enabled {
            match registry.build(&cfg.kind, &cfg.name, &cfg.config) {
                Ok(src) => {
                    info!(
                        source_name_len = telemetry_text_len(&cfg.name),
                        kind_len = telemetry_text_len(&cfg.kind),
                        "inbound supervisor starting connector"
                    );
                    sources.push(src);
                }
                Err(e) => {
                    let error = e.to_string();
                    warn!(
                        source_name_len = telemetry_text_len(&cfg.name),
                        kind_len = telemetry_text_len(&cfg.kind),
                        reason_code = inbound_error_reason_code(&error),
                        error_len = telemetry_text_len(&error),
                        "inbound supervisor skipped connector"
                    );
                }
            }
        }
        self.spawn_all(sources)
    }

    /// Spawn a per-connector task for each source.
    pub fn spawn_all(
        self: Arc<Self>,
        sources: Vec<Box<dyn InboundEventSource>>,
    ) -> Vec<JoinHandle<()>> {
        sources
            .into_iter()
            .map(|s| {
                let me = self.clone();
                tokio::spawn(async move { me.run_source(s).await })
            })
            .collect()
    }

    /// Drive one connector until it closes. Public so it can be awaited directly
    /// (e.g. in tests with an [`super::source::InMemorySource`]).
    pub async fn run_source(&self, source: Box<dyn InboundEventSource>) {
        let name = source.name().to_string();
        let mut fetch_fail = 0u32;
        loop {
            match source.next_event().await {
                Ok(event) => {
                    fetch_fail = 0;
                    self.handle_event(source.as_ref(), event).await;
                }
                Err(InboundError::Closed) => {
                    info!(
                        source_name_len = telemetry_text_len(&name),
                        "inbound connector closed"
                    );
                    break;
                }
                Err(InboundError::Transient(e)) => {
                    self.metrics.record_error(&name);
                    fetch_fail = fetch_fail.saturating_add(1);
                    let delay = backoff(fetch_fail);
                    warn!(
                        source_name_len = telemetry_text_len(&name),
                        reason_code = inbound_error_reason_code(&e),
                        error_len = telemetry_text_len(&e),
                        retry_ms = delay.as_millis() as u64,
                        "inbound connector fetch error"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn handle_event(&self, source: &dyn InboundEventSource, event: InboundEvent) {
        let name = source.name();
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self.process(name, &event).await {
                Ok(()) => {
                    if let Err(e) = source.commit(event.offset.clone()).await {
                        let error = e.to_string();
                        warn!(
                            source_name_len = telemetry_text_len(name),
                            reason_code = inbound_error_reason_code(&error),
                            error_len = telemetry_text_len(&error),
                            "inbound connector offset commit failed"
                        );
                    }
                    self.metrics.record_event(name);
                    return;
                }
                Err(reason) if attempt >= self.max_attempts => {
                    warn!(
                        source_name_len = telemetry_text_len(name),
                        attempts = attempt,
                        reason_code = inbound_error_reason_code(&reason),
                        reason_len = telemetry_text_len(&reason),
                        "inbound connector dead-lettering event"
                    );
                    if let Err(e) = self
                        .db
                        .inbound_sources
                        .record_dlq(
                            name,
                            Some(&event.offset),
                            Some(&event.payload),
                            &reason,
                            attempt as i32,
                        )
                        .await
                    {
                        let error = e.to_string();
                        warn!(
                            source_name_len = telemetry_text_len(name),
                            reason_code = inbound_error_reason_code(&error),
                            error_len = telemetry_text_len(&error),
                            "inbound connector DLQ write failed"
                        );
                    }
                    // Commit to skip the poison event; healthy events stay at-least-once.
                    let _ = source.commit(event.offset.clone()).await;
                    self.metrics.record_error(name);
                    return;
                }
                Err(_) => tokio::time::sleep(backoff(attempt)).await,
            }
        }
    }

    /// Validate and durably write one event to the shared outbox. Returns
    /// `Err(reason)` on structural malformation or an outbox failure.
    async fn process(
        &self,
        source_name: &str,
        event: &InboundEvent,
    ) -> std::result::Result<(), String> {
        if event.event_type.trim().is_empty() {
            return Err("event_type is empty".to_string());
        }
        if event.payload.is_null() {
            return Err("payload is null".to_string());
        }
        let outbox = CreateOutboxEvent::new(
            event.event_type.as_str(),
            "inbound_event",
            matric_core::new_v7(),
            json!({
                "source": source_name,
                "offset": event.offset,
                "event_type": event.event_type,
                "payload": event.payload,
            }),
            None,
        );
        self.db
            .outbox
            .emit_event(outbox)
            .await
            .map(|_| ())
            .map_err(|_| "outbox_emit_failed".to_string())
    }
}

/// Exponential backoff capped at `MAX_BACKOFF_SECS` (2^attempt seconds).
fn backoff(attempt: u32) -> Duration {
    let secs = (1u64 << attempt.min(5)).min(MAX_BACKOFF_SECS);
    Duration::from_secs(secs)
}

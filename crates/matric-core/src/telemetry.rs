//! Bounded-cardinality OpenTelemetry hooks shared by every crate (#1156).
//!
//! Every function here is safe to call unconditionally. Without the `otel`
//! feature, or when the API process has not installed an OpenTelemetry
//! provider and propagator (export disabled at runtime), they are cheap no-ops.
//!
//! Attribute values are always closed vocabularies passed as `&'static str`.
//! Callers must never pass tenant ids, user ids, note content, raw paths, URLs,
//! provider error text, or credentials. The metric and attribute names below
//! are the documented contract in `docs/content/observability.md`.

use serde_json::Value as JsonValue;
use std::time::Duration;

/// Instrumentation scope name used for every Fortemi meter and tracer.
pub const INSTRUMENTATION_SCOPE: &str = "fortemi";

/// W3C trace-context header name.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Reserved job-payload key carrying the enqueuing request's `traceparent`.
///
/// Written only when trace export is active and the payload is a JSON object;
/// the worker removes it before handlers see the payload.
pub const JOB_TRACE_PAYLOAD_KEY: &str = "_fortemi_traceparent";

/// Documented metric names.
pub mod metric_names {
    pub const HTTP_SERVER_REQUEST_DURATION: &str = "http.server.request.duration";
    pub const HTTP_SERVER_ACTIVE_REQUESTS: &str = "http.server.active_requests";
    pub const JOB_EXECUTION_DURATION: &str = "fortemi.job.execution.duration";
    pub const JOB_QUEUE_DEPTH: &str = "fortemi.job.queue.depth";
    pub const JOB_QUEUE_OLDEST_PENDING_AGE: &str = "fortemi.job.queue.oldest_pending_age";
    pub const INFERENCE_DURATION: &str = "fortemi.inference.duration";
    pub const DB_POOL_CONNECTIONS: &str = "fortemi.db.pool.connections";
    pub const DB_POOL_MAX_CONNECTIONS: &str = "fortemi.db.pool.max_connections";
    pub const QUOTA_ADMISSION_DECISIONS: &str = "fortemi.quota.admission.decisions";
    pub const KMS_OPERATIONS: &str = "fortemi.kms.operations";
}

/// Documented attribute keys.
pub mod attribute_names {
    pub const HTTP_REQUEST_METHOD: &str = "http.request.method";
    pub const HTTP_ROUTE: &str = "http.route";
    pub const HTTP_RESPONSE_STATUS_CODE: &str = "http.response.status_code";
    pub const ROUTE_CLASS: &str = "fortemi.route.class";
    pub const JOB_TYPE: &str = "fortemi.job.type";
    pub const JOB_OUTCOME: &str = "fortemi.job.outcome";
    pub const JOB_STATE: &str = "fortemi.job.state";
    pub const INFERENCE_OPERATION: &str = "fortemi.inference.operation";
    pub const INFERENCE_PROVIDER: &str = "fortemi.inference.provider";
    pub const OUTCOME: &str = "fortemi.outcome";
    pub const DB_POOL_STATE: &str = "fortemi.db.pool.state";
    pub const QUOTA_DECISION: &str = "fortemi.quota.decision";
    pub const KMS_OPERATION: &str = "fortemi.kms.operation";
    pub const KMS_FAILURE_CLASS: &str = "fortemi.kms.failure_class";
}

/// Histogram bucket boundaries (seconds) shared by every duration histogram.
pub const DURATION_BUCKETS_SECONDS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0, 60.0,
    120.0, 300.0, 600.0,
];

/// Validate a W3C `traceparent` value (version `00`, lowercase hex, non-zero ids).
///
/// Values arriving from job payloads are attacker-influenced database rows, so
/// anything outside the exact grammar is dropped rather than forwarded.
pub fn is_valid_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    let [version, trace_id, span_id, flags] = parts.as_slice() else {
        return false;
    };
    let lower_hex = |s: &str, len: usize| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    *version == "00"
        && lower_hex(trace_id, 32)
        && lower_hex(span_id, 16)
        && lower_hex(flags, 2)
        && trace_id.bytes().any(|b| b != b'0')
        && span_id.bytes().any(|b| b != b'0')
}

/// Copy the active trace context into a job payload (JSON objects only).
///
/// `None` and non-object payloads are returned unchanged so handlers that
/// distinguish "no payload" keep their semantics.
pub fn attach_trace_to_job_payload(payload: Option<JsonValue>) -> Option<JsonValue> {
    match payload {
        Some(JsonValue::Object(mut map)) => {
            if let Some(traceparent) = current_traceparent() {
                map.insert(
                    JOB_TRACE_PAYLOAD_KEY.to_string(),
                    JsonValue::String(traceparent),
                );
            }
            Some(JsonValue::Object(map))
        }
        other => other,
    }
}

/// Remove the reserved trace key from a job payload, returning it when valid.
pub fn take_trace_from_job_payload(payload: &mut Option<JsonValue>) -> Option<String> {
    let map = payload.as_mut()?.as_object_mut()?;
    match map.remove(JOB_TRACE_PAYLOAD_KEY)? {
        JsonValue::String(value) if is_valid_traceparent(&value) => Some(value),
        _ => None,
    }
}

/// Header pair to inject into outbound provider requests, when tracing is active.
pub fn trace_header() -> Option<(&'static str, String)> {
    current_traceparent().map(|value| (TRACEPARENT_HEADER, value))
}

/// Outbound header map carrying `traceparent` (empty when tracing is inactive).
pub fn trace_header_map() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some((name, value)) = trace_header() {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    }
    headers
}

/// Coarse outcome label for a fallible operation.
pub fn outcome_label<T, E>(result: &Result<T, E>) -> &'static str {
    if result.is_ok() {
        "ok"
    } else {
        "error"
    }
}

#[cfg(feature = "otel")]
mod active {
    use super::{attribute_names as attr, metric_names, DURATION_BUCKETS_SECONDS};
    use opentelemetry::metrics::{Counter, Histogram};
    use opentelemetry::trace::TraceContextExt;
    use opentelemetry::KeyValue;
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use std::time::Duration;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    struct Instruments {
        job_duration: Histogram<f64>,
        inference_duration: Histogram<f64>,
        quota_decisions: Counter<u64>,
        kms_operations: Counter<u64>,
    }

    // Instruments are created on first use, which is always after the API has
    // installed (or deliberately skipped) the global meter provider at startup.
    fn instruments() -> &'static Instruments {
        static INSTRUMENTS: OnceLock<Instruments> = OnceLock::new();
        INSTRUMENTS.get_or_init(|| {
            let meter = opentelemetry::global::meter(super::INSTRUMENTATION_SCOPE);
            Instruments {
                job_duration: meter
                    .f64_histogram(metric_names::JOB_EXECUTION_DURATION)
                    .with_unit("s")
                    .with_description("Job handler execution time by job type and outcome")
                    .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
                    .build(),
                inference_duration: meter
                    .f64_histogram(metric_names::INFERENCE_DURATION)
                    .with_unit("s")
                    .with_description("Embedding and generation provider call latency")
                    .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
                    .build(),
                quota_decisions: meter
                    .u64_counter(metric_names::QUOTA_ADMISSION_DECISIONS)
                    .with_unit("{decision}")
                    .with_description("Redis request-quota admission decisions")
                    .build(),
                kms_operations: meter
                    .u64_counter(metric_names::KMS_OPERATIONS)
                    .with_unit("{operation}")
                    .with_description("KMS startup canary and decrypt outcomes")
                    .build(),
            }
        })
    }

    pub(super) fn current_traceparent() -> Option<String> {
        let context = tracing::Span::current().context();
        if !context.span().span_context().is_valid() {
            return None;
        }
        let mut carrier = HashMap::new();
        opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&context, &mut carrier)
        });
        carrier
            .remove(super::TRACEPARENT_HEADER)
            .filter(|value| super::is_valid_traceparent(value))
    }

    pub(super) fn set_span_parent(span: &tracing::Span, traceparent: &str) -> bool {
        let mut carrier = HashMap::new();
        carrier.insert(
            super::TRACEPARENT_HEADER.to_string(),
            traceparent.to_string(),
        );
        let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&carrier)
        });
        if !parent.span().span_context().is_valid() {
            return false;
        }
        span.set_parent(parent).is_ok()
    }

    pub(super) fn span_trace_ids(span: &tracing::Span) -> Option<(String, String)> {
        let context = span.context();
        let span_ref = context.span();
        let span_context = span_ref.span_context();
        span_context.is_valid().then(|| {
            (
                span_context.trace_id().to_string(),
                span_context.span_id().to_string(),
            )
        })
    }

    pub(super) fn record_job(job_type: &'static str, outcome: &'static str, elapsed: Duration) {
        instruments().job_duration.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new(attr::JOB_TYPE, job_type),
                KeyValue::new(attr::JOB_OUTCOME, outcome),
            ],
        );
    }

    pub(super) fn record_inference(
        operation: &'static str,
        provider: &'static str,
        outcome: &'static str,
        elapsed: Duration,
    ) {
        instruments().inference_duration.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new(attr::INFERENCE_OPERATION, operation),
                KeyValue::new(attr::INFERENCE_PROVIDER, provider),
                KeyValue::new(attr::OUTCOME, outcome),
            ],
        );
    }

    pub(super) fn record_quota(decision: &'static str) {
        instruments()
            .quota_decisions
            .add(1, &[KeyValue::new(attr::QUOTA_DECISION, decision)]);
    }

    pub(super) fn record_kms(
        operation: &'static str,
        outcome: &'static str,
        failure_class: &'static str,
    ) {
        instruments().kms_operations.add(
            1,
            &[
                KeyValue::new(attr::KMS_OPERATION, operation),
                KeyValue::new(attr::OUTCOME, outcome),
                KeyValue::new(attr::KMS_FAILURE_CLASS, failure_class),
            ],
        );
    }
}

/// `traceparent` for the current tracing span, when trace export is active.
pub fn current_traceparent() -> Option<String> {
    #[cfg(feature = "otel")]
    {
        active::current_traceparent()
    }
    #[cfg(not(feature = "otel"))]
    {
        None
    }
}

/// Make `span` a child of the remote context in `traceparent`. Returns whether
/// the parent was applied (false when tracing is inactive or the value is bad).
pub fn set_span_parent_from_traceparent(span: &tracing::Span, traceparent: &str) -> bool {
    if !is_valid_traceparent(traceparent) {
        return false;
    }
    #[cfg(feature = "otel")]
    {
        active::set_span_parent(span, traceparent)
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = span;
        false
    }
}

/// Hex `(trace_id, span_id)` of `span`, used to stamp log-correlation fields.
pub fn span_trace_ids(span: &tracing::Span) -> Option<(String, String)> {
    #[cfg(feature = "otel")]
    {
        active::span_trace_ids(span)
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = span;
        None
    }
}

/// Record one job execution (`outcome`: success, failed, retry).
pub fn record_job_execution(job_type: &'static str, outcome: &'static str, elapsed: Duration) {
    #[cfg(feature = "otel")]
    active::record_job(job_type, outcome, elapsed);
    #[cfg(not(feature = "otel"))]
    let _ = (job_type, outcome, elapsed);
}

/// Record one inference provider call (`operation`: embed, generate, ...).
pub fn record_inference(
    operation: &'static str,
    provider: &'static str,
    outcome: &'static str,
    elapsed: Duration,
) {
    #[cfg(feature = "otel")]
    active::record_inference(operation, provider, outcome, elapsed);
    #[cfg(not(feature = "otel"))]
    let _ = (operation, provider, outcome, elapsed);
}

/// Record a Redis quota admission decision (allowed, rejected, unavailable, invalid).
pub fn record_quota_admission(decision: &'static str) {
    #[cfg(feature = "otel")]
    active::record_quota(decision);
    #[cfg(not(feature = "otel"))]
    let _ = decision;
}

/// Record a KMS operation outcome. `failure_class` is `none` on success.
pub fn record_kms_operation(
    operation: &'static str,
    outcome: &'static str,
    failure_class: &'static str,
) {
    #[cfg(feature = "otel")]
    active::record_kms(operation, outcome, failure_class);
    #[cfg(not(feature = "otel"))]
    let _ = (operation, outcome, failure_class);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VALID: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn traceparent_grammar_is_strict() {
        assert!(is_valid_traceparent(VALID));
        assert!(!is_valid_traceparent(
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01"
        ));
        assert!(!is_valid_traceparent(
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01"
        ));
        assert!(!is_valid_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"
        ));
        assert!(!is_valid_traceparent(
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        ));
        assert!(!is_valid_traceparent("note body; Bearer secret"));
        assert!(!is_valid_traceparent(&format!("{VALID}-extra")));
    }

    #[test]
    fn take_trace_strips_key_and_rejects_garbage() {
        let mut payload = Some(json!({"a": 1, JOB_TRACE_PAYLOAD_KEY: VALID}));
        assert_eq!(
            take_trace_from_job_payload(&mut payload).as_deref(),
            Some(VALID)
        );
        assert_eq!(payload, Some(json!({"a": 1})));

        let mut tampered = Some(json!({JOB_TRACE_PAYLOAD_KEY: "Bearer secret"}));
        assert_eq!(take_trace_from_job_payload(&mut tampered), None);
        assert_eq!(tampered, Some(json!({})));

        let mut none: Option<JsonValue> = None;
        assert_eq!(take_trace_from_job_payload(&mut none), None);
    }

    #[test]
    fn attach_without_active_trace_leaves_payload_unchanged() {
        assert_eq!(attach_trace_to_job_payload(None), None);
        assert_eq!(
            attach_trace_to_job_payload(Some(json!([1, 2]))),
            Some(json!([1, 2]))
        );
        assert_eq!(
            attach_trace_to_job_payload(Some(json!({"a": 1}))),
            Some(json!({"a": 1}))
        );
        assert_eq!(trace_header(), None);
    }
}

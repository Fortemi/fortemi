//! Redaction tests: no content, credential or URL sentinel may reach exported spans.

use super::super::redaction::{redact_attribute, RedactingSpanProcessor, REDACTED};
use super::*;
use axum::{body::Body, routing::post, Router};
use opentelemetry::Value;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SimpleSpanProcessor};
use tower::ServiceExt;
use tracing_subscriber::layer::SubscriberExt;

const TOKEN_SENTINEL: &str = "mm_at_TOKENSENTINEL0123456789";
const NOTE_SENTINEL: &str = "NOTEBODYSENTINEL private diary entry";
const QUERY_SENTINEL: &str = "QUERYSENTINEL";
const TENANT_SENTINEL: &str = "TENANTSENTINEL-7f0c";
const INBOUND_TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

#[test]
fn attribute_redaction_rules() {
    let dropped = [
        KeyValue::new("uri", "/api/v1/notes?token=x"),
        KeyValue::new("url.full", "https://h/p"),
        KeyValue::new("authorization", "Bearer x"),
        KeyValue::new("note_content", "hello"),
        KeyValue::new("tenant_id", "t"),
        KeyValue::new("http.request.header.cookie", "c"),
        KeyValue::new("search_query", "q"),
    ];
    for kv in dropped {
        assert!(
            redact_attribute(kv.clone()).is_none(),
            "{kv:?} must be dropped"
        );
    }
    let kept = redact_attribute(KeyValue::new("prompt_len", 42_i64)).expect("length kept");
    assert_eq!(kept.value, Value::I64(42));
    let kept = redact_attribute(KeyValue::new("http.route", "/api/v1/notes/{id}")).unwrap();
    assert_eq!(kept.value.as_str(), "/api/v1/notes/{id}");
    for value in [
        "Bearer abc",
        "https://user:pass@host/x",
        "someone@example.com",
        &"x".repeat(300),
    ] {
        let masked = redact_attribute(KeyValue::new("detail", value.to_string())).unwrap();
        assert_eq!(masked.value.as_str(), REDACTED);
    }
}

#[test]
fn unknown_paths_never_leak_into_route_labels() {
    let (route, class) = route_labels("/definitely/not/a/route/SECRET");
    assert_eq!((route, class), ("unmatched", "unmatched"));
    let (route, _) = route_labels("/api/v1/notes");
    assert_eq!(route, "/api/v1/notes");
}

async fn sensitive_handler(headers: axum::http::HeaderMap, body: String) -> &'static str {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let span = tracing::info_span!(
        "note.create",
        note_content = %body,
        authorization = %auth,
        tenant_id = TENANT_SENTINEL,
        detail = %format!("https://user:pw@provider.example/x?token={QUERY_SENTINEL}"),
        echoed = %auth,
        body_len = body.len(),
    );
    let _entered = span.enter();
    tracing::info!(note = %body, token = %auth, "handled sensitive request");
    "ok"
}

#[tokio::test(flavor = "current_thread")]
async fn exported_spans_contain_no_sensitive_values() {
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(RedactingSpanProcessor::new(SimpleSpanProcessor::new(
            exporter.clone(),
        )))
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("redaction-test")));
    let _default = tracing::subscriber::set_default(subscriber);

    let meter = opentelemetry::global::meter("redaction-test");
    let telemetry: HttpTelemetryState = Some(Arc::new(HttpTelemetry::new(&meter)));
    let app = Router::new()
        .route("/api/v1/notes", post(sensitive_handler))
        .layer(axum::middleware::from_fn_with_state(
            telemetry,
            super::super::http_middleware,
        ));

    let request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/v1/notes?token={QUERY_SENTINEL}"))
        .header("authorization", format!("Bearer {TOKEN_SENTINEL}"))
        .header(
            "traceparent",
            format!("00-{INBOUND_TRACE_ID}-00f067aa0ba902b7-01"),
        )
        .header("baggage", format!("tenant={TENANT_SENTINEL}"))
        .body(Body::from(NOTE_SENTINEL))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    assert!(spans.len() >= 2, "server and handler spans exported");
    let rendered = format!("{spans:?}");
    for sentinel in [
        TOKEN_SENTINEL,
        NOTE_SENTINEL,
        QUERY_SENTINEL,
        TENANT_SENTINEL,
        "diary",
    ] {
        assert!(
            !rendered.contains(sentinel),
            "sentinel {sentinel} leaked into exported spans"
        );
    }

    let server = spans
        .iter()
        .find(|span| span.name == "POST /api/v1/notes")
        .expect("server span named by route template");
    assert_eq!(server.span_context.trace_id().to_string(), INBOUND_TRACE_ID);
    let attribute = |key: &str| {
        server
            .attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.clone())
    };
    assert_eq!(
        attribute("http.route").map(|v| v.as_str().into_owned()),
        Some("/api/v1/notes".to_string())
    );
    assert_eq!(
        attribute("http.response.status_code"),
        Some(Value::I64(200))
    );
    assert!(spans.iter().all(|span| span.events.is_empty()));
    let handler = spans
        .iter()
        .find(|span| span.name == "note.create")
        .expect("handler span exported");
    assert_eq!(
        handler.span_context.trace_id().to_string(),
        INBOUND_TRACE_ID
    );
    assert!(handler
        .attributes
        .iter()
        .any(|kv| kv.key.as_str() == "body_len"));
}

#[tokio::test(flavor = "current_thread")]
async fn job_payload_carries_trace_context_to_worker_span() {
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(RedactingSpanProcessor::new(SimpleSpanProcessor::new(
            exporter.clone(),
        )))
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("job-propagation-test")));
    let _default = tracing::subscriber::set_default(subscriber);

    let enqueue = tracing::info_span!("enqueue");
    let mut payload = enqueue.in_scope(|| {
        matric_core::telemetry::attach_trace_to_job_payload(Some(
            serde_json::json!({"schema": "public"}),
        ))
    });
    let enqueue_trace = matric_core::telemetry::span_trace_ids(&enqueue)
        .expect("enqueue span is sampled")
        .0;
    drop(enqueue);

    let traceparent = matric_core::telemetry::take_trace_from_job_payload(&mut payload)
        .expect("payload carried traceparent");
    assert_eq!(payload, Some(serde_json::json!({"schema": "public"})));
    let worker = tracing::info_span!(parent: None, "job.execute");
    assert!(matric_core::telemetry::set_span_parent_from_traceparent(
        &worker,
        &traceparent
    ));
    worker.in_scope(|| {});
    drop(worker);
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    let job = spans
        .iter()
        .find(|span| span.name == "job.execute")
        .expect("worker span exported");
    assert_eq!(job.span_context.trace_id().to_string(), enqueue_trace);
    assert!(job.parent_span_is_remote);
}

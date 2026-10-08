//! OTLP exporter wiring, redacting span processor and HTTP telemetry (#1156).

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{extract::Request, http::Method, middleware::Next, response::Response};
use matric_core::telemetry::{attribute_names as attr, metric_names, DURATION_BUCKETS_SECONDS};
use opentelemetry::metrics::{Histogram, Meter, UpDownCounter};
use opentelemetry::propagation::Extractor;
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::Layer;

use super::redaction::RedactingSpanProcessor;
use super::{BoxedLayer, HttpTelemetryState, OtelSettings, OtlpProtocol, DEFAULT_SERVICE_NAME};
use crate::route_policy::{route_policy_for_path, PolicyClass};

const RUNTIME_SAMPLE_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        _ => "_OTHER",
    }
}

pub(crate) fn route_class_label(class: PolicyClass) -> &'static str {
    match class {
        PolicyClass::Public => "public",
        PolicyClass::PublicWithInlineProof => "public_inline_proof",
        PolicyClass::AuthenticatedRead => "authenticated_read",
        PolicyClass::AuthenticatedWrite => "authenticated_write",
        PolicyClass::AdminOperator => "admin_operator",
        PolicyClass::TenantObject => "tenant_object",
        PolicyClass::SystemHealth => "system_health",
        PolicyClass::OAuth => "oauth",
        PolicyClass::RealtimeTransport => "realtime_transport",
    }
}

/// Route template and class for a concrete path; never the raw path.
pub(crate) fn route_labels(path: &str) -> (&'static str, &'static str) {
    match route_policy_for_path(path) {
        Some(policy) => (policy.path, route_class_label(policy.class)),
        None => ("unmatched", "unmatched"),
    }
}

struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        // Only W3C trace context is honoured; baggage is never read.
        if key != matric_core::telemetry::TRACEPARENT_HEADER && key != "tracestate" {
            return None;
        }
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        vec![matric_core::telemetry::TRACEPARENT_HEADER, "tracestate"]
    }
}

/// HTTP server instruments plus the server-span factory.
pub struct HttpTelemetry {
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
}

struct ActiveRequestGuard<'a> {
    counter: &'a UpDownCounter<i64>,
    attributes: [KeyValue; 2],
}

impl Drop for ActiveRequestGuard<'_> {
    fn drop(&mut self) {
        self.counter.add(-1, &self.attributes);
    }
}

impl HttpTelemetry {
    pub(crate) fn new(meter: &Meter) -> Self {
        Self {
            duration: meter
                .f64_histogram(metric_names::HTTP_SERVER_REQUEST_DURATION)
                .with_unit("s")
                .with_description("HTTP server request duration by route template and class")
                .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
                .build(),
            active: meter
                .i64_up_down_counter(metric_names::HTTP_SERVER_ACTIVE_REQUESTS)
                .with_unit("{request}")
                .with_description("In-flight HTTP server requests")
                .build(),
        }
    }

    pub(crate) async fn handle(&self, request: Request, next: Next) -> Response {
        let method = method_label(request.method());
        let (route, route_class) = route_labels(request.uri().path());
        let span = tracing::info_span!(
            target: "fortemi::http",
            "http.server.request",
            otel.name = %format_args!("{method} {route}"),
            otel.kind = "server",
            otel.status_code = tracing::field::Empty,
            http.request.method = method,
            http.route = route,
            fortemi.route.class = route_class,
            http.response.status_code = tracing::field::Empty,
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
        );
        let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(request.headers()))
        });
        if parent.span().span_context().is_valid() {
            let _ = span.set_parent(parent);
        }
        if let Some((trace_id, span_id)) = matric_core::telemetry::span_trace_ids(&span) {
            span.record("trace_id", trace_id.as_str());
            span.record("span_id", span_id.as_str());
        }

        let active_attributes = [
            KeyValue::new(attr::HTTP_REQUEST_METHOD, method),
            KeyValue::new(attr::ROUTE_CLASS, route_class),
        ];
        self.active.add(1, &active_attributes);
        let _guard = ActiveRequestGuard {
            counter: &self.active,
            attributes: active_attributes,
        };
        let start = Instant::now();
        let response = next.run(request).instrument(span.clone()).await;
        let status = response.status().as_u16();
        span.record("http.response.status_code", i64::from(status));
        if status >= 500 {
            span.record("otel.status_code", "error");
        }
        self.duration.record(
            start.elapsed().as_secs_f64(),
            &[
                KeyValue::new(attr::HTTP_REQUEST_METHOD, method),
                KeyValue::new(attr::HTTP_ROUTE, route),
                KeyValue::new(attr::ROUTE_CLASS, route_class),
                KeyValue::new(attr::HTTP_RESPONSE_STATUS_CODE, i64::from(status)),
            ],
        );
        response
    }
}

#[derive(Default)]
struct QueueSample {
    pending: AtomicI64,
    delayed: AtomicI64,
    processing: AtomicI64,
    dead: AtomicI64,
    incompatible: AtomicI64,
    oldest_pending_age_bits: AtomicU64,
}

/// Live OpenTelemetry state; flushes and shuts the providers down on drop.
pub struct OtelRuntime {
    tracer: Option<opentelemetry_sdk::trace::Tracer>,
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    traces_filter: String,
    http: Option<Arc<HttpTelemetry>>,
    gauges: std::sync::Mutex<Vec<Box<dyn std::any::Any + Send>>>,
}

fn resource(settings: &OtelSettings) -> Resource {
    let mut builder = Resource::builder()
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
    if !settings.service_name_from_env {
        builder = builder.with_service_name(DEFAULT_SERVICE_NAME);
    }
    builder.build()
}

fn otlp_protocol(protocol: OtlpProtocol) -> Protocol {
    match protocol {
        OtlpProtocol::Grpc => Protocol::Grpc,
        OtlpProtocol::HttpProtobuf => Protocol::HttpBinary,
    }
}

impl OtelRuntime {
    /// Build exporters for the selected signals. Endpoint, headers, timeout,
    /// sampler, export interval and resource attributes are read by the SDK
    /// from the standard `OTEL_*` variables. Must run inside the Tokio runtime.
    pub fn init(settings: &OtelSettings) -> anyhow::Result<Self> {
        let resource = resource(settings);
        let mut runtime = Self {
            tracer: None,
            tracer_provider: None,
            meter_provider: None,
            traces_filter: settings.traces_filter.clone(),
            http: None,
            gauges: std::sync::Mutex::new(Vec::new()),
        };
        tracing_subscriber::EnvFilter::try_new(&settings.traces_filter).map_err(|_| {
            anyhow::anyhow!("FORTEMI_OTEL_TRACES_FILTER contains invalid filter directives.")
        })?;

        if let Some(protocol) = settings.traces {
            let exporter = match protocol {
                OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_protocol(otlp_protocol(protocol))
                    .build(),
                OtlpProtocol::HttpProtobuf => opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_protocol(otlp_protocol(protocol))
                    .build(),
            }
            .map_err(|_| anyhow::anyhow!("OTLP span exporter could not be configured"))?;
            let processor =
                RedactingSpanProcessor::new(BatchSpanProcessor::builder(exporter).build());
            let provider = SdkTracerProvider::builder()
                .with_span_processor(processor)
                .with_resource(resource.clone())
                .build();
            runtime.tracer = Some(provider.tracer(matric_core::telemetry::INSTRUMENTATION_SCOPE));
            opentelemetry::global::set_text_map_propagator(
                opentelemetry_sdk::propagation::TraceContextPropagator::new(),
            );
            opentelemetry::global::set_tracer_provider(provider.clone());
            runtime.tracer_provider = Some(provider);
        }

        if let Some(protocol) = settings.metrics {
            let exporter = match protocol {
                OtlpProtocol::Grpc => opentelemetry_otlp::MetricExporter::builder()
                    .with_tonic()
                    .with_protocol(otlp_protocol(protocol))
                    .build(),
                OtlpProtocol::HttpProtobuf => opentelemetry_otlp::MetricExporter::builder()
                    .with_http()
                    .with_protocol(otlp_protocol(protocol))
                    .build(),
            }
            .map_err(|_| anyhow::anyhow!("OTLP metric exporter could not be configured"))?;
            let provider = SdkMeterProvider::builder()
                .with_reader(PeriodicReader::builder(exporter).build())
                .with_resource(resource)
                .build();
            opentelemetry::global::set_meter_provider(provider.clone());
            runtime.meter_provider = Some(provider);
        }

        if runtime.tracer.is_some() || runtime.meter_provider.is_some() {
            let meter = opentelemetry::global::meter(matric_core::telemetry::INSTRUMENTATION_SCOPE);
            runtime.http = Some(Arc::new(HttpTelemetry::new(&meter)));
        }
        Ok(runtime)
    }

    /// Tracing layer bridging `tracing` spans to OpenTelemetry, with its own
    /// filter so `RUST_LOG` verbosity does not change what is exported.
    pub fn tracing_layer<S>(&mut self) -> Option<BoxedLayer<S>>
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
    {
        let tracer = self.tracer.take()?;
        let filter = tracing_subscriber::EnvFilter::try_new(&self.traces_filter).ok()?;
        Some(Box::new(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(filter),
        ))
    }

    pub fn http_state(&self) -> HttpTelemetryState {
        self.http.clone()
    }

    /// Register DB-pool and job-queue gauges. Queue counts are sampled on a
    /// background task because gauge callbacks must not block on the database.
    pub fn register_runtime_gauges(&self, db: matric_db::Database) {
        if self.meter_provider.is_none() {
            return;
        }
        let meter = opentelemetry::global::meter(matric_core::telemetry::INSTRUMENTATION_SCOPE);
        let sample = Arc::new(QueueSample::default());
        let pool = db.pool.clone();
        let pool_for_max = db.pool.clone();
        let queue_sample = sample.clone();
        let age_sample = sample.clone();
        let mut gauges = self.gauges.lock().unwrap_or_else(|e| e.into_inner());
        gauges.push(Box::new(
            meter
                .i64_observable_gauge(metric_names::DB_POOL_CONNECTIONS)
                .with_unit("{connection}")
                .with_description("Primary PostgreSQL pool connections by state")
                .with_callback(move |observer| {
                    let size = i64::from(pool.size());
                    let idle = pool.num_idle() as i64;
                    observer.observe(idle, &[KeyValue::new(attr::DB_POOL_STATE, "idle")]);
                    observer.observe(
                        (size - idle).max(0),
                        &[KeyValue::new(attr::DB_POOL_STATE, "used")],
                    );
                })
                .build(),
        ));
        gauges.push(Box::new(
            meter
                .i64_observable_gauge(metric_names::DB_POOL_MAX_CONNECTIONS)
                .with_unit("{connection}")
                .with_description("Configured primary PostgreSQL pool size limit")
                .with_callback(move |observer| {
                    observer.observe(i64::from(pool_for_max.options().get_max_connections()), &[]);
                })
                .build(),
        ));
        gauges.push(Box::new(
            meter
                .i64_observable_gauge(metric_names::JOB_QUEUE_DEPTH)
                .with_unit("{job}")
                .with_description("Job queue depth by state (sampled every 15s)")
                .with_callback(move |observer| {
                    for (state, value) in [
                        ("pending", &queue_sample.pending),
                        ("delayed", &queue_sample.delayed),
                        ("processing", &queue_sample.processing),
                        ("dead", &queue_sample.dead),
                        ("incompatible", &queue_sample.incompatible),
                    ] {
                        observer.observe(
                            value.load(Ordering::Relaxed),
                            &[KeyValue::new(attr::JOB_STATE, state)],
                        );
                    }
                })
                .build(),
        ));
        gauges.push(Box::new(
            meter
                .f64_observable_gauge(metric_names::JOB_QUEUE_OLDEST_PENDING_AGE)
                .with_unit("s")
                .with_description("Age of the oldest due pending job (sampled every 15s)")
                .with_callback(move |observer| {
                    observer.observe(
                        f64::from_bits(age_sample.oldest_pending_age_bits.load(Ordering::Relaxed)),
                        &[],
                    );
                })
                .build(),
        ));
        drop(gauges);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RUNTIME_SAMPLE_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                use matric_core::JobRepository;
                if let Ok(stats) = db.jobs.queue_stats().await {
                    sample.pending.store(stats.pending, Ordering::Relaxed);
                    sample.delayed.store(stats.delayed, Ordering::Relaxed);
                    sample.processing.store(stats.processing, Ordering::Relaxed);
                    sample.dead.store(stats.dead, Ordering::Relaxed);
                    sample
                        .incompatible
                        .store(stats.incompatible, Ordering::Relaxed);
                } else {
                    tracing::debug!(sampler = "job_queue_depth", "Queue depth sample failed");
                }
                if let Ok(age) = db.jobs.oldest_pending_age_seconds().await {
                    sample
                        .oldest_pending_age_bits
                        .store(age.to_bits(), Ordering::Relaxed);
                }
            }
        });
    }

    pub fn log_startup(&self, settings: &OtelSettings) {
        tracing::info!(
            otel_traces = settings.traces.map(OtlpProtocol::as_str).unwrap_or("off"),
            otel_metrics = settings.metrics.map(OtlpProtocol::as_str).unwrap_or("off"),
            otel_sdk_disabled = settings.sdk_disabled,
            otel_endpoint_class = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .map(|value| crate::telemetry_url_class(&value))
                .unwrap_or("unset"),
            "OpenTelemetry export configured"
        );
        if settings.ignored_logs_exporter {
            tracing::warn!(
                "OTEL_LOGS_EXPORTER is ignored: logs are written as JSON to stdout (LOG_FORMAT=json) with trace_id/span_id span fields"
            );
        }
    }
}

impl Drop for OtelRuntime {
    fn drop(&mut self) {
        if let Some(provider) = self.tracer_provider.take() {
            let _ = provider.shutdown();
        }
        if let Some(provider) = self.meter_provider.take() {
            let _ = provider.shutdown();
        }
    }
}

#[cfg(test)]
#[path = "redaction_tests.rs"]
mod redaction_tests;

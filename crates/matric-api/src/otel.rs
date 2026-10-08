//! OpenTelemetry traces and metrics export over OTLP (#1156).
//!
//! Compiled only with the `otel` Cargo feature and, even then, inactive until
//! the standard `OTEL_*` variables opt in (see `OtelSettings::from_env`). The
//! public surface is identical with and without the feature so `main.rs` hooks
//! stay unconditional.
//!
//! Redaction contract: span attributes come from closed vocabularies (route
//! templates, route classes, job types). `RedactingSpanProcessor` is a second
//! line of defence that drops sensitive keys, masks credential- or URL-shaped
//! values, and strips span events (logs stay on stdout with trace ids).

use std::sync::Arc;

use axum::{extract::Request, extract::State, middleware::Next, response::Response};

/// Middleware state: `None` when HTTP telemetry is inactive.
pub type HttpTelemetryState = Option<Arc<HttpTelemetry>>;

/// Boxed tracing layer contributed by OpenTelemetry (absent when inactive).
pub type BoxedLayer<S> = Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>;

/// Parsed `OTEL_*` configuration; only closed values, never endpoint strings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtelSettings {
    pub traces: Option<OtlpProtocol>,
    pub metrics: Option<OtlpProtocol>,
    pub traces_filter: String,
    pub service_name_from_env: bool,
    pub ignored_logs_exporter: bool,
    pub sdk_disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtlpProtocol {
    Grpc,
    HttpProtobuf,
}

impl OtlpProtocol {
    #[cfg_attr(not(feature = "otel"), allow(dead_code))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grpc => "grpc",
            Self::HttpProtobuf => "http/protobuf",
        }
    }
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub const DEFAULT_SERVICE_NAME: &str = "fortemi-api";
pub const DEFAULT_TRACES_FILTER: &str = "info";
const TRACES_FILTER_ENV: &str = "FORTEMI_OTEL_TRACES_FILTER";

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl OtelSettings {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_env_with(|name| std::env::var(name).ok())
    }

    /// Export is OFF unless a signal is selected:
    /// * `OTEL_SDK_DISABLED=true` disables everything;
    /// * `OTEL_{TRACES,METRICS}_EXPORTER=otlp` enables that signal;
    /// * with the exporter variable unset, a generic or signal-specific
    ///   `OTEL_EXPORTER_OTLP_*ENDPOINT` enables the signal;
    /// * `none` disables the signal; any other exporter value is a startup error.
    pub fn from_env_with<F>(env: F) -> anyhow::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let sdk_disabled = match non_empty(env("OTEL_SDK_DISABLED"))
            .map(|v| v.to_ascii_lowercase())
            .as_deref()
        {
            None | Some("false") => false,
            Some("true") => true,
            Some(_) => {
                anyhow::bail!("OTEL_SDK_DISABLED has an invalid value. Expected true or false.")
            }
        };
        let traces_filter =
            non_empty(env(TRACES_FILTER_ENV)).unwrap_or_else(|| DEFAULT_TRACES_FILTER.to_string());
        let ignored_logs_exporter = non_empty(env("OTEL_LOGS_EXPORTER"))
            .is_some_and(|value| !value.eq_ignore_ascii_case("none"));
        let service_name_from_env = non_empty(env("OTEL_SERVICE_NAME")).is_some();
        if sdk_disabled {
            return Ok(Self {
                traces_filter,
                service_name_from_env,
                ignored_logs_exporter,
                sdk_disabled,
                ..Self::default()
            });
        }
        let generic_endpoint = non_empty(env("OTEL_EXPORTER_OTLP_ENDPOINT")).is_some();
        let signal = |exporter_var: &str, endpoint_var: &str, protocol_var: &str| {
            let selected = match non_empty(env(exporter_var))
                .map(|v| v.to_ascii_lowercase())
                .as_deref()
            {
                None => generic_endpoint || non_empty(env(endpoint_var)).is_some(),
                Some("otlp") => true,
                Some("none") => false,
                Some(_) => {
                    anyhow::bail!("{exporter_var} has an unsupported value. Expected otlp or none.")
                }
            };
            if !selected {
                return Ok(None);
            }
            let protocol = non_empty(env(protocol_var))
                .or_else(|| non_empty(env("OTEL_EXPORTER_OTLP_PROTOCOL")));
            match protocol.map(|v| v.to_ascii_lowercase()).as_deref() {
                None | Some("http/protobuf") => Ok(Some(OtlpProtocol::HttpProtobuf)),
                Some("grpc") => Ok(Some(OtlpProtocol::Grpc)),
                Some(_) => anyhow::bail!(
                    "{protocol_var} / OTEL_EXPORTER_OTLP_PROTOCOL has an unsupported value. Expected grpc or http/protobuf."
                ),
            }
        };
        Ok(Self {
            traces: signal(
                "OTEL_TRACES_EXPORTER",
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
            )?,
            metrics: signal(
                "OTEL_METRICS_EXPORTER",
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
                "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
            )?,
            traces_filter,
            service_name_from_env,
            ignored_logs_exporter,
            sdk_disabled,
        })
    }
}

#[cfg(feature = "otel")]
mod active;
#[cfg(feature = "otel")]
mod redaction;
#[cfg(feature = "otel")]
pub use active::{HttpTelemetry, OtelRuntime};

#[cfg(not(feature = "otel"))]
mod inactive {
    use super::{BoxedLayer, OtelSettings};

    /// Placeholder; HTTP telemetry needs the `otel` feature.
    pub struct HttpTelemetry;

    /// Runtime handle for builds without the `otel` feature.
    pub struct OtelRuntime {
        requested: bool,
    }

    impl OtelRuntime {
        pub fn init(settings: &OtelSettings) -> anyhow::Result<Self> {
            Ok(Self {
                requested: settings.traces.is_some() || settings.metrics.is_some(),
            })
        }

        pub fn tracing_layer<S>(&mut self) -> Option<BoxedLayer<S>>
        where
            S: tracing::Subscriber
                + for<'a> tracing_subscriber::registry::LookupSpan<'a>
                + Send
                + Sync,
        {
            None
        }

        pub fn http_state(&self) -> super::HttpTelemetryState {
            None
        }

        pub fn register_runtime_gauges(&self, _db: matric_db::Database) {}

        pub fn log_startup(&self, settings: &OtelSettings) {
            if self.requested {
                tracing::warn!(
                    "OTEL_* variables request OTLP export but this binary was built without the `otel` feature; telemetry export is disabled"
                );
            }
            let _ = settings;
        }
    }
}
#[cfg(not(feature = "otel"))]
pub use inactive::{HttpTelemetry, OtelRuntime};

/// Server-span + HTTP metrics middleware. A no-op pass-through when inactive.
pub async fn http_middleware(
    State(state): State<HttpTelemetryState>,
    request: Request,
    next: Next,
) -> Response {
    match state {
        #[cfg(feature = "otel")]
        Some(telemetry) => telemetry.handle(request, next).await,
        #[cfg(not(feature = "otel"))]
        Some(_) => next.run(request).await,
        None => next.run(request).await,
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(vars: &[(&str, &str)]) -> anyhow::Result<OtelSettings> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        OtelSettings::from_env_with(|name| map.get(name).cloned())
    }

    #[test]
    fn export_is_off_by_default() {
        let settings = parse(&[]).unwrap();
        assert_eq!(settings.traces, None);
        assert_eq!(settings.metrics, None);
        assert_eq!(settings.traces_filter, DEFAULT_TRACES_FILTER);
        // A service name alone never enables export.
        let settings = parse(&[("OTEL_SERVICE_NAME", "x")]).unwrap();
        assert_eq!((settings.traces, settings.metrics), (None, None));
    }

    #[test]
    fn endpoint_enables_both_signals_with_http_default() {
        let settings = parse(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4318")]).unwrap();
        assert_eq!(settings.traces, Some(OtlpProtocol::HttpProtobuf));
        assert_eq!(settings.metrics, Some(OtlpProtocol::HttpProtobuf));
    }

    #[test]
    fn per_signal_selection_and_protocols() {
        let settings = parse(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317"),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc"),
            ("OTEL_METRICS_EXPORTER", "none"),
        ])
        .unwrap();
        assert_eq!(settings.traces, Some(OtlpProtocol::Grpc));
        assert_eq!(settings.metrics, None);

        let settings = parse(&[
            ("OTEL_METRICS_EXPORTER", "otlp"),
            ("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "grpc"),
        ])
        .unwrap();
        assert_eq!(settings.traces, None);
        assert_eq!(settings.metrics, Some(OtlpProtocol::Grpc));

        let settings = parse(&[(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            "http://c:4318/v1/traces",
        )])
        .unwrap();
        assert_eq!(settings.traces, Some(OtlpProtocol::HttpProtobuf));
        assert_eq!(settings.metrics, None);
    }

    #[test]
    fn sdk_disabled_wins_and_bad_values_fail_closed() {
        let settings = parse(&[
            ("OTEL_SDK_DISABLED", "true"),
            ("OTEL_TRACES_EXPORTER", "otlp"),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://c:4318"),
        ])
        .unwrap();
        assert!(settings.sdk_disabled);
        assert_eq!((settings.traces, settings.metrics), (None, None));
        assert!(parse(&[("OTEL_TRACES_EXPORTER", "zipkin")]).is_err());
        assert!(parse(&[
            ("OTEL_TRACES_EXPORTER", "otlp"),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json")
        ])
        .is_err());
        assert!(parse(&[("OTEL_SDK_DISABLED", "maybe")]).is_err());
        assert!(
            parse(&[("OTEL_LOGS_EXPORTER", "otlp")])
                .unwrap()
                .ignored_logs_exporter
        );
    }
}

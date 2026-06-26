//! Runtime inference configuration management handlers.
//!
//! Provides GET/POST/DELETE endpoints for reading, updating, and resetting
//! inference backend configuration at runtime without a server restart.
//!
//! Configuration precedence (highest → lowest):
//!   db_override → env → default

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::IpAddr;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::middleware::archive_routing::ArchiveContext;
use crate::{ApiError, AppState, Auth};
use matric_core::defaults::EMBED_DIMENSION;
use matric_core::InferenceBackend as InferenceBackendTrait;
use matric_core::ServerEvent;
use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass, AuthPrincipal, TracingSink,
};
use matric_inference::OllamaBackend;

// =============================================================================
// LOCAL CONFIG TYPES
// =============================================================================

/// Source attribution for a single config value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    DbOverride,
    Env,
    Default,
}

/// A config value with its source attribution.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourcedValue {
    pub value: String,
    pub source: ConfigSource,
}

impl std::fmt::Debug for SourcedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedValue")
            .field("value_len", &telemetry_text_len(&self.value))
            .field("source", &self.source)
            .finish()
    }
}

/// Ollama config fields with source attribution.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourcedOllamaConfig {
    pub base_url: SourcedValue,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

impl std::fmt::Debug for SourcedOllamaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedOllamaConfig")
            .field("base_url_class", &telemetry_url_class(&self.base_url.value))
            .field("base_url_len", &telemetry_text_len(&self.base_url.value))
            .field(
                "generation_model_len",
                &telemetry_text_len(&self.generation_model.value),
            )
            .field(
                "embedding_model_len",
                &telemetry_text_len(&self.embedding_model.value),
            )
            .finish()
    }
}

/// OpenAI config fields with source attribution.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourcedOpenAIConfig {
    pub base_url: SourcedValue,
    /// API key metadata only. Null if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SourcedValue>,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

impl std::fmt::Debug for SourcedOpenAIConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedOpenAIConfig")
            .field("base_url_class", &telemetry_url_class(&self.base_url.value))
            .field("base_url_len", &telemetry_text_len(&self.base_url.value))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "api_key_len",
                &self
                    .api_key
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.value)),
            )
            .field(
                "generation_model_len",
                &telemetry_text_len(&self.generation_model.value),
            )
            .field(
                "embedding_model_len",
                &telemetry_text_len(&self.embedding_model.value),
            )
            .finish()
    }
}

/// llama.cpp config fields with source attribution.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourcedLlamaCppConfig {
    pub base_url: SourcedValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SourcedValue>,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

impl std::fmt::Debug for SourcedLlamaCppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedLlamaCppConfig")
            .field("base_url_class", &telemetry_url_class(&self.base_url.value))
            .field("base_url_len", &telemetry_text_len(&self.base_url.value))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "api_key_len",
                &self
                    .api_key
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.value)),
            )
            .field(
                "generation_model_len",
                &telemetry_text_len(&self.generation_model.value),
            )
            .field(
                "embedding_model_len",
                &telemetry_text_len(&self.embedding_model.value),
            )
            .finish()
    }
}

/// OpenRouter config fields with source attribution.
///
/// OpenRouter speaks the OpenAI-compatible protocol but layers two extra
/// headers on top: `HTTP-Referer` for routing rules and `X-Title` for app
/// attribution. Fortemi defaults these to `https://fortemi.io` / `Fortemi`;
/// operators shipping Fortemi as a sidecar can override per app via
/// `OPENROUTER_HTTP_REFERER` and `OPENROUTER_APP_NAME` env vars or the
/// runtime `http_referer` / `app_name` fields. Embeddings are not supported
/// by OpenRouter; the field is omitted from this struct intentionally.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourcedOpenRouterConfig {
    pub base_url: SourcedValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SourcedValue>,
    pub generation_model: SourcedValue,
    pub http_referer: SourcedValue,
    pub app_name: SourcedValue,
}

impl std::fmt::Debug for SourcedOpenRouterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedOpenRouterConfig")
            .field("base_url_class", &telemetry_url_class(&self.base_url.value))
            .field("base_url_len", &telemetry_text_len(&self.base_url.value))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "api_key_len",
                &self
                    .api_key
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.value)),
            )
            .field(
                "generation_model_len",
                &telemetry_text_len(&self.generation_model.value),
            )
            .field(
                "http_referer_class",
                &telemetry_url_class(&self.http_referer.value),
            )
            .field(
                "http_referer_len",
                &telemetry_text_len(&self.http_referer.value),
            )
            .field("app_name_len", &telemetry_text_len(&self.app_name.value))
            .finish()
    }
}

/// Full effective inference config response with source attribution.
#[derive(Serialize)]
pub struct InferenceConfigResponse {
    pub default_backend: String,
    /// Embedding-route override. `None` (omitted from JSON) means embeddings
    /// route through `default_backend`. When set, embedding calls go to this
    /// provider id instead — typical for "OpenRouter for chat, local for
    /// embeddings" deployments where the chat provider doesn't expose
    /// embeddings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_backend: Option<SourcedValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama: Option<SourcedOllamaConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<SourcedOpenAIConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llamacpp: Option<SourcedLlamaCppConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openrouter: Option<SourcedOpenRouterConfig>,
    pub providers: Vec<String>,
}

impl std::fmt::Debug for InferenceConfigResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceConfigResponse")
            .field(
                "default_backend_len",
                &telemetry_text_len(&self.default_backend),
            )
            .field(
                "embedding_backend_len",
                &self
                    .embedding_backend
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.value)),
            )
            .field("ollama_present", &self.ollama.is_some())
            .field("openai_present", &self.openai.is_some())
            .field("llamacpp_present", &self.llamacpp.is_some())
            .field("openrouter_present", &self.openrouter.is_some())
            .field("provider_count", &self.providers.len())
            .finish()
    }
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

/// Partial update request body (all fields optional).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateInferenceConfigRequest {
    pub ollama: Option<PartialOllamaConfig>,
    pub openai: Option<PartialOpenAIConfig>,
    pub llamacpp: Option<PartialLlamaCppConfig>,
    pub openrouter: Option<PartialOpenRouterConfig>,
    /// Independent embedding-route override. Set to a provider id (e.g.
    /// `"ollama"`, `"openai"`, `"llamacpp"`) to route embedding calls
    /// through that provider regardless of the active default. Pass `null`
    /// (the JSON literal) to clear the override; omit the field entirely
    /// to leave it unchanged. Validated against the live registry — the
    /// chosen provider must be registered and support embeddings, else
    /// the call is rejected with 400.
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub embedding_backend: Option<Option<String>>,
}

impl std::fmt::Debug for UpdateInferenceConfigRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateInferenceConfigRequest")
            .field("ollama_present", &self.ollama.is_some())
            .field("openai_present", &self.openai.is_some())
            .field("llamacpp_present", &self.llamacpp.is_some())
            .field("openrouter_present", &self.openrouter.is_some())
            .field(
                "embedding_backend_state",
                &debug_nested_option_state(&self.embedding_backend),
            )
            .finish()
    }
}

/// Custom deserializer that distinguishes "field absent" (`None`) from
/// "field present and null" (`Some(None)`). Lets clients explicitly clear
/// the embedding override without touching other fields.
fn deserialize_optional_field<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(de)?))
}

/// Partial Ollama config (all fields optional).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PartialOllamaConfig {
    pub base_url: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

impl std::fmt::Debug for PartialOllamaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartialOllamaConfig")
            .field("base_url_class", &debug_optional_url_class(&self.base_url))
            .field("base_url_len", &debug_optional_text_len(&self.base_url))
            .field(
                "generation_model_len",
                &debug_optional_text_len(&self.generation_model),
            )
            .field(
                "embedding_model_len",
                &debug_optional_text_len(&self.embedding_model),
            )
            .finish()
    }
}

/// Partial OpenAI config (all fields optional).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PartialOpenAIConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

impl std::fmt::Debug for PartialOpenAIConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartialOpenAIConfig")
            .field("base_url_class", &debug_optional_url_class(&self.base_url))
            .field("base_url_len", &debug_optional_text_len(&self.base_url))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "generation_model_len",
                &debug_optional_text_len(&self.generation_model),
            )
            .field(
                "embedding_model_len",
                &debug_optional_text_len(&self.embedding_model),
            )
            .finish()
    }
}

/// Partial llama.cpp config (all fields optional).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PartialLlamaCppConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

impl std::fmt::Debug for PartialLlamaCppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartialLlamaCppConfig")
            .field("base_url_class", &debug_optional_url_class(&self.base_url))
            .field("base_url_len", &debug_optional_text_len(&self.base_url))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "generation_model_len",
                &debug_optional_text_len(&self.generation_model),
            )
            .field(
                "embedding_model_len",
                &debug_optional_text_len(&self.embedding_model),
            )
            .finish()
    }
}

/// Partial OpenRouter config (all fields optional).
///
/// `http_referer` and `app_name` override the Fortemi defaults
/// (`https://fortemi.io` / `Fortemi`) used in OpenRouter's `HTTP-Referer`
/// and `X-Title` headers. Embeddings are unsupported by OpenRouter so no
/// embedding-model field is offered.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PartialOpenRouterConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub generation_model: Option<String>,
    pub http_referer: Option<String>,
    pub app_name: Option<String>,
}

impl std::fmt::Debug for PartialOpenRouterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartialOpenRouterConfig")
            .field("base_url_class", &debug_optional_url_class(&self.base_url))
            .field("base_url_len", &debug_optional_text_len(&self.base_url))
            .field("api_key_present", &self.api_key.is_some())
            .field(
                "generation_model_len",
                &debug_optional_text_len(&self.generation_model),
            )
            .field(
                "http_referer_class",
                &debug_optional_url_class(&self.http_referer),
            )
            .field(
                "http_referer_len",
                &debug_optional_text_len(&self.http_referer),
            )
            .field("app_name_len", &debug_optional_text_len(&self.app_name))
            .finish()
    }
}

fn debug_optional_text_len(value: &Option<String>) -> Option<usize> {
    value.as_deref().map(telemetry_text_len)
}

fn debug_optional_url_class(value: &Option<String>) -> &'static str {
    value
        .as_deref()
        .map(telemetry_url_class)
        .unwrap_or("absent")
}

fn debug_nested_option_state<T>(value: &Option<Option<T>>) -> &'static str {
    match value {
        None => "absent",
        Some(None) => "clear",
        Some(Some(_)) => "set",
    }
}

/// Query parameters for POST /api/v1/inference/config.
#[derive(Debug, Deserialize)]
pub struct UpdateConfigQuery {
    /// If true, probe the Ollama endpoint for reachability before persisting.
    /// Narrower than `atomic` — kept for backwards compatibility.
    #[serde(default)]
    pub validate: bool,
    /// If true, validate the merged config and return the would-be effective
    /// state without persisting or hot-swapping the live registry. Useful for
    /// pre-flight checks from operator UIs. Mutually composable with
    /// `atomic`: `dry_run=true&atomic=true` probes every changed backend,
    /// returns the resolution, and discards.
    #[serde(default)]
    pub dry_run: bool,
    /// If true, probe every backend that this request touches (Ollama,
    /// OpenAI, OpenRouter, llama.cpp) before committing. On any probe
    /// failure, abort with 503 and do not persist or hot-swap. Avoids the
    /// brief error window where a half-applied config serves bad creds while
    /// the operator notices.
    #[serde(default)]
    pub atomic: bool,
}

/// Response from POST /api/v1/inference/config.
#[derive(Debug, Serialize)]
pub struct UpdateInferenceConfigResponse {
    pub status: String,
    pub previous: Value,
    pub current: Value,
    pub warnings: Vec<String>,
}

/// Response from DELETE /api/v1/inference/config.
#[derive(Debug, Serialize)]
pub struct ResetInferenceConfigResponse {
    pub status: String,
    pub effective: Value,
}

// =============================================================================
// HELPERS
// =============================================================================

fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    let msg = msg.into();
    match status {
        StatusCode::BAD_REQUEST => ApiError::BadRequest(msg).into_response(),
        StatusCode::SERVICE_UNAVAILABLE => ApiError::ServiceUnavailable(msg).into_response(),
        StatusCode::INTERNAL_SERVER_ERROR => ApiError::Internal(msg).into_response(),
        StatusCode::NOT_FOUND => ApiError::NotFound(msg).into_response(),
        StatusCode::CONFLICT => ApiError::Conflict(msg).into_response(),
        _ => ApiError::Internal(msg).into_response(),
    }
}

fn inference_config_database_failed() -> axum::response::Response {
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Inference configuration database operation failed.",
    )
}

/// Redact an API key without carrying any raw prefix/suffix material.
fn redact_api_key(key: &str) -> String {
    redacted_secret_metadata(telemetry_text_len(key))
}

fn telemetry_text_len(value: &str) -> usize {
    value.chars().count()
}

fn inference_json_class(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn inference_source_ip_class(value: &str) -> &'static str {
    match value.parse::<IpAddr>() {
        Ok(IpAddr::V4(addr)) if addr.is_loopback() => "loopback_v4",
        Ok(IpAddr::V4(addr)) if addr.is_private() => "private_v4",
        Ok(IpAddr::V4(_)) => "public_v4",
        Ok(IpAddr::V6(addr)) if addr.is_loopback() => "loopback_v6",
        Ok(IpAddr::V6(_)) => "public_v6",
        Err(_) => "invalid",
    }
}

fn redacted_secret_metadata(len: usize) -> String {
    format!("<secret_present_len:{len}>")
}

fn redact_inference_config_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, nested) in map {
                let value = if key == "api_key" {
                    redact_inference_secret_value(nested)
                } else {
                    redact_inference_config_json(nested)
                };
                redacted.insert(key.clone(), value);
            }
            Value::Object(redacted)
        }
        Value::Array(values) => {
            Value::Array(values.iter().map(redact_inference_config_json).collect())
        }
        _ => value.clone(),
    }
}

fn redact_inference_secret_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            if let Some(source) = map.get("source") {
                redacted.insert("source".to_string(), source.clone());
            }
            let len = map
                .get("value")
                .and_then(|v| v.as_str())
                .map(telemetry_text_len)
                .unwrap_or(0);
            redacted.insert(
                "value".to_string(),
                Value::String(redacted_secret_metadata(len)),
            );
            Value::Object(redacted)
        }
        Value::String(secret) => {
            Value::String(redacted_secret_metadata(telemetry_text_len(secret)))
        }
        Value::Null => Value::Null,
        _ => Value::String("<secret_present>".to_string()),
    }
}

const INFERENCE_CONFIG_AUDIT_WRITE_FAILURE: &str = "inference_config_audit_write_failed";
const INFERENCE_CONFIG_AUDIT_EMIT_FAILURE: &str = "inference_config_audit_emit_failed";
const INFERENCE_CONFIG_OVERRIDE_READ_FAILURE: &str = "inference_config_override_read_failed";
const INFERENCE_CONFIG_OVERRIDE_PERSIST_FAILURE: &str = "inference_config_override_persist_failed";
const INFERENCE_CONFIG_OVERRIDE_DELETE_FAILURE: &str = "inference_config_override_delete_failed";
const INFERENCE_CONFIG_AUDIT_FETCH_FAILURE: &str = "inference_config_audit_fetch_failed";
const INFERENCE_CONFIG_EMBEDDING_VALIDATION_FAILURE: &str =
    "inference_config_embedding_validation_failed";

fn telemetry_url_class(raw: &str) -> &'static str {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return "invalid_url";
    };
    let Some(host) = url.host_str() else {
        return "unknown_host";
    };
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "api.openai.com"
        || host.ends_with(".openai.com")
        || host == "openrouter.ai"
        || host.ends_with(".openrouter.ai")
    {
        return "managed_provider";
    }
    if is_local_or_private_telemetry_host(&host) {
        return "local_or_private";
    }
    "external"
}

fn is_local_or_private_telemetry_host(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".local") {
        return true;
    }
    match lower.parse::<IpAddr>() {
        Ok(IpAddr::V4(addr)) => {
            addr.is_loopback() || addr.is_private() || addr.is_link_local() || addr.is_unspecified()
        }
        Ok(IpAddr::V6(addr)) => addr.is_loopback() || addr.is_unspecified(),
        Err(_) => false,
    }
}

fn probe_failure_reason(error: &str) -> &'static str {
    match error {
        PROBE_REASON_TIMEOUT => return "timeout",
        PROBE_REASON_CONNECT_FAILED => return "connect_failed",
        PROBE_REASON_HTTP_STATUS => return "http_status",
        PROBE_REASON_INVALID_RESPONSE => return "invalid_response",
        PROBE_REASON_UNKNOWN_PROVIDER => return "unknown_provider",
        _ => {}
    }

    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("connection refused") || lower.contains("connect") {
        "connect_failed"
    } else if lower.contains("http ") || lower.contains("status") {
        "http_status"
    } else if lower.contains("json") || lower.contains("missing") {
        "invalid_response"
    } else if lower.contains("unknown provider") {
        "unknown_provider"
    } else {
        "probe_failed"
    }
}

const PROBE_REASON_TIMEOUT: &str = "probe timed out";
const PROBE_REASON_CONNECT_FAILED: &str = "probe connection failed";
const PROBE_REASON_HTTP_STATUS: &str = "probe returned unsuccessful status";
const PROBE_REASON_INVALID_RESPONSE: &str = "probe returned invalid response";
const PROBE_REASON_UNKNOWN_PROVIDER: &str = "probe provider is not supported";
const HTTP_CLIENT_INIT_ERROR: &str = "Failed to initialize HTTP client";

/// Write a row to `inference_config_audit` (#656). Best-effort — DB
/// failure is logged at warn level but never propagates to the caller.
/// Surface errors must not block the live config change.
///
/// `before_json` and `after_json` are passed straight through; the
/// effective-config builder already redacts API keys via `redact_api_key`,
/// so anything coming from `build_effective_config` is safe to persist.
async fn write_audit_row(
    pool: &sqlx::PgPool,
    action: &str,
    before_json: Option<&Value>,
    after_json: Option<&Value>,
    source_ip: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO inference_config_audit
            (changed_by, action, before_json, after_json, source_ip)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind("anonymous")
    .bind(action)
    .bind(before_json)
    .bind(after_json)
    .bind(source_ip)
    .execute(pool)
    .await;

    if let Err(e) = result {
        let diagnostic = e.to_string();
        warn!(
            error_len = telemetry_text_len(&diagnostic),
            action = %action,
            detail = INFERENCE_CONFIG_AUDIT_WRITE_FAILURE,
            "Failed to write inference_config_audit row"
        );
    }
}

async fn emit_inference_config_audit_event(event: AuditEvent) {
    if let Err(err) = TracingSink.emit(event).await {
        let diagnostic = err.to_string();
        warn!(
            error_len = telemetry_text_len(&diagnostic),
            detail = INFERENCE_CONFIG_AUDIT_EMIT_FAILURE,
            "failed to emit inference config audit event"
        );
    }
}

fn inference_config_audit_event(
    auth: &Auth,
    action: &str,
    archive_schema: Option<&str>,
    changed_fields: &[String],
    current: &Value,
) -> AuditEvent {
    let scope = if archive_schema.is_some() {
        "archive"
    } else {
        "global"
    };
    let resource_id = if archive_schema.is_some() {
        "archive_override"
    } else {
        "global_override"
    };
    let safe_changed_fields: Vec<String> = changed_fields
        .iter()
        .map(|field| sanitize_inference_config_field_name(field))
        .collect();

    let mut event = AuditEvent::new("inference_config", action, AuditOutcome::Success)
        .with_principal(inference_config_principal_audit_id(&auth.principal))
        .with_resource("inference_config", resource_id)
        .with_attr("config_scope", scope)
        .with_attr("changed_field_count", safe_changed_fields.len() as i64)
        .with_attr("changed_fields", serde_json::json!(safe_changed_fields))
        .with_attr("ollama_present", current.get("ollama").is_some())
        .with_attr("openai_present", current.get("openai").is_some())
        .with_attr("llamacpp_present", current.get("llamacpp").is_some())
        .with_attr("openrouter_present", current.get("openrouter").is_some())
        .with_attr(
            "embedding_backend_present",
            current.get("embedding_backend").is_some(),
        );

    if let Some(schema) = archive_schema {
        event = event.with_attr("archive_schema_len", schema.chars().count() as i64);
    }

    event.source = AuditSource::Api;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.failure_policy = AuditFailurePolicy::BestEffort;
    event.severity = AuditSeverity::Info;
    event.sanitized()
}

fn sanitize_inference_config_field_name(field: &str) -> String {
    let valid_shape = !field.is_empty()
        && field.len() <= 96
        && field
            .chars()
            .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '_' | '.'))
        && (matches!(
            field,
            "__reset__" | "__reset_archive__" | "default_backend" | "embedding_backend"
        ) || field.starts_with("ollama.")
            || field.starts_with("openai.")
            || field.starts_with("llamacpp.")
            || field.starts_with("openrouter."));

    if valid_shape {
        field.to_string()
    } else {
        "unknown".to_string()
    }
}

fn inference_config_principal_audit_id(principal: &AuthPrincipal) -> String {
    match principal {
        AuthPrincipal::OAuthClient {
            client_id, user_id, ..
        } => user_id
            .as_ref()
            .map(|user_id| format!("oauth_user:{user_id}"))
            .unwrap_or_else(|| format!("oauth_client:{client_id}")),
        AuthPrincipal::ApiKey { key_id, .. } => format!("api_key:{key_id}"),
        AuthPrincipal::Anonymous => "anonymous".to_string(),
    }
}

/// Diff two effective-config JSON blobs and return the dotted field names
/// that changed. Used to populate `InferenceConfigChanged.changed_fields`
/// (#657) so reactive UIs can render targeted updates without diffing the
/// full config themselves.
///
/// Walks one level into per-provider blocks (`ollama.base_url`,
/// `openrouter.generation_model`, etc.) and emits top-level field names
/// (`default_backend`, `embedding_backend`) directly. Identical sub-trees
/// are not enumerated — we want a flat list of leaves that differ.
///
/// API keys are intentionally compared, but the returned list contains
/// only field _names_ — never values.
fn diff_changed_fields(prev: &Value, curr: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let prev_obj = prev.as_object();
    let curr_obj = curr.as_object();
    let (prev_obj, curr_obj) = match (prev_obj, curr_obj) {
        (Some(p), Some(c)) => (p, c),
        _ => return out,
    };

    let mut keys: std::collections::BTreeSet<&String> = prev_obj.keys().collect();
    keys.extend(curr_obj.keys());

    for k in keys {
        let p = prev_obj.get(k);
        let c = curr_obj.get(k);
        match (p, c) {
            (Some(pv), Some(cv)) if pv == cv => {}
            (Some(pv), Some(cv)) if pv.is_object() && cv.is_object() => {
                let po = pv.as_object().unwrap();
                let co = cv.as_object().unwrap();
                let mut sub: std::collections::BTreeSet<&String> = po.keys().collect();
                sub.extend(co.keys());
                for sk in sub {
                    if po.get(sk) != co.get(sk) {
                        out.push(format!("{}.{}", k, sk));
                    }
                }
            }
            _ => out.push(k.clone()),
        }
    }
    out
}

/// Validate an Ollama base_url and model names. Returns an error message on failure.
fn validate_ollama(base_url: &str, gen_model: &str, embed_model: &str) -> Result<(), String> {
    if base_url.is_empty() {
        return Err("Ollama base_url cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("Ollama base_url must start with http:// or https://".to_string());
    }
    if gen_model.is_empty() {
        return Err("Ollama generation_model cannot be empty".to_string());
    }
    if embed_model.is_empty() {
        return Err("Ollama embedding_model cannot be empty".to_string());
    }
    Ok(())
}

/// Validate an OpenAI base_url and model names. Returns an error message on failure.
fn validate_openai(base_url: &str, gen_model: &str, embed_model: &str) -> Result<(), String> {
    if base_url.is_empty() {
        return Err("OpenAI base_url cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("OpenAI base_url must start with http:// or https://".to_string());
    }
    if gen_model.is_empty() {
        return Err("OpenAI generation_model cannot be empty".to_string());
    }
    if embed_model.is_empty() {
        return Err("OpenAI embedding_model cannot be empty".to_string());
    }
    Ok(())
}

/// Env-sourced Ollama defaults (mirrors InferenceConfig::from_env logic).
struct EnvOllama {
    base_url: String,
    generation_model: String,
    embedding_model: String,
}

impl EnvOllama {
    fn read() -> Self {
        let base_url = std::env::var("MATRIC_OLLAMA_URL")
            .or_else(|_| std::env::var("OLLAMA_BASE"))
            .or_else(|_| std::env::var("OLLAMA_URL"))
            .or_else(|_| std::env::var("OLLAMA_HOST"))
            .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let generation_model = std::env::var("MATRIC_OLLAMA_GENERATION_MODEL")
            .or_else(|_| std::env::var("OLLAMA_GEN_MODEL"))
            .unwrap_or_else(|_| matric_core::defaults::GEN_MODEL.to_string());
        let embedding_model = std::env::var("MATRIC_OLLAMA_EMBEDDING_MODEL")
            .or_else(|_| std::env::var("OLLAMA_EMBED_MODEL"))
            .unwrap_or_else(|_| matric_core::defaults::EMBED_MODEL.to_string());
        Self {
            base_url,
            generation_model,
            embedding_model,
        }
    }
}

/// Default Ollama values (compile-time constants from matric_core::defaults).
struct DefaultOllama;

impl DefaultOllama {
    fn base_url() -> &'static str {
        matric_core::defaults::OLLAMA_URL
    }
    fn generation_model() -> &'static str {
        matric_core::defaults::GEN_MODEL
    }
    fn embedding_model() -> &'static str {
        matric_core::defaults::EMBED_MODEL
    }
}

/// Pick the first non-empty value across db > env > default, returning with source.
fn pick(db_val: Option<&str>, env_val: &str, default_val: &str) -> SourcedValue {
    if let Some(v) = db_val.filter(|s| !s.is_empty()) {
        return SourcedValue {
            value: v.to_string(),
            source: ConfigSource::DbOverride,
        };
    }
    if !env_val.is_empty() && env_val != default_val {
        return SourcedValue {
            value: env_val.to_string(),
            source: ConfigSource::Env,
        };
    }
    SourcedValue {
        value: default_val.to_string(),
        source: ConfigSource::Default,
    }
}

/// Read the raw DB override blob from user_config.
async fn read_db_override(pool: &sqlx::PgPool) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> =
        sqlx::query_as("SELECT value FROM user_config WHERE key = 'inference_override'")
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// Read the per-archive override blob from `archive_inference_override` (#655).
/// Returns `None` when no row exists for this schema (archive falls back to
/// the global config).
async fn read_archive_override(
    pool: &sqlx::PgPool,
    schema: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> =
        sqlx::query_as("SELECT override FROM archive_inference_override WHERE schema_name = $1")
            .bind(schema)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// Determine whether the request is archive-scoped. Returns the schema name
/// when an explicit non-default archive is selected via `X-Fortemi-Memory`,
/// `None` otherwise (request applies to the global config).
///
/// We treat `public` and the configured default-archive schema as "global"
/// — operators using the default archive interact with the global config,
/// matching pre-#655 behavior. Per-archive overrides are opt-in by
/// explicitly addressing a non-default archive via the header.
fn archive_override_schema(ctx: &ArchiveContext) -> Option<&str> {
    if ctx.is_default || ctx.schema == "public" {
        None
    } else {
        Some(ctx.schema.as_str())
    }
}

/// Build the effective sourced config by layering db > env > default.
fn build_effective_config(db: Option<&Value>) -> InferenceConfigResponse {
    let env_o = EnvOllama::read();
    let db_ollama = db.and_then(|v| v.get("ollama"));
    let db_openai = db.and_then(|v| v.get("openai"));

    // Ollama is always present (it is the default backend).
    let ollama = Some(SourcedOllamaConfig {
        base_url: pick(
            db_ollama
                .and_then(|o| o.get("base_url"))
                .and_then(|v| v.as_str()),
            &env_o.base_url,
            DefaultOllama::base_url(),
        ),
        generation_model: pick(
            db_ollama
                .and_then(|o| o.get("generation_model"))
                .and_then(|v| v.as_str()),
            &env_o.generation_model,
            DefaultOllama::generation_model(),
        ),
        embedding_model: pick(
            db_ollama
                .and_then(|o| o.get("embedding_model"))
                .and_then(|v| v.as_str()),
            &env_o.embedding_model,
            DefaultOllama::embedding_model(),
        ),
    });

    // OpenAI only shown if the DB override or env has it configured.
    let openai = if db_openai.is_some() {
        let db_base = db_openai
            .and_then(|o| o.get("base_url"))
            .and_then(|v| v.as_str());
        let db_gen = db_openai
            .and_then(|o| o.get("generation_model"))
            .and_then(|v| v.as_str());
        let db_embed = db_openai
            .and_then(|o| o.get("embedding_model"))
            .and_then(|v| v.as_str());
        let db_key = db_openai
            .and_then(|o| o.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let default_openai_url = "https://api.openai.com/v1";
        let default_gen = "gpt-4o-mini";
        let default_embed = "text-embedding-3-small";

        let api_key = db_key.map(|k| SourcedValue {
            value: redact_api_key(k),
            source: ConfigSource::DbOverride,
        });

        Some(SourcedOpenAIConfig {
            base_url: pick(db_base, "", default_openai_url),
            api_key,
            generation_model: pick(db_gen, "", default_gen),
            embedding_model: pick(db_embed, "", default_embed),
        })
    } else {
        None
    };

    // llama.cpp only shown if the DB override or env has it configured.
    let db_llamacpp = db.and_then(|v| v.get("llamacpp"));
    let env_llamacpp_url = std::env::var("LLAMACPP_BASE_URL").unwrap_or_default();
    let llamacpp = if db_llamacpp.is_some() || !env_llamacpp_url.is_empty() {
        let db_base = db_llamacpp
            .and_then(|o| o.get("base_url"))
            .and_then(|v| v.as_str());
        let db_gen = db_llamacpp
            .and_then(|o| o.get("generation_model"))
            .and_then(|v| v.as_str());
        let db_embed = db_llamacpp
            .and_then(|o| o.get("embedding_model"))
            .and_then(|v| v.as_str());
        let db_key = db_llamacpp
            .and_then(|o| o.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let default_url = matric_core::defaults::LLAMACPP_URL;
        let default_gen = "default";
        let default_embed = "default";

        let api_key = db_key.map(|k| SourcedValue {
            value: redact_api_key(k),
            source: ConfigSource::DbOverride,
        });

        Some(SourcedLlamaCppConfig {
            base_url: pick(db_base, &env_llamacpp_url, default_url),
            api_key,
            generation_model: pick(db_gen, "", default_gen),
            embedding_model: pick(db_embed, "", default_embed),
        })
    } else {
        None
    };

    // OpenRouter only shown if the DB override or env has it configured.
    let db_openrouter = db.and_then(|v| v.get("openrouter"));
    let env_openrouter_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    let openrouter = if db_openrouter.is_some() || !env_openrouter_key.is_empty() {
        let db_base = db_openrouter
            .and_then(|o| o.get("base_url"))
            .and_then(|v| v.as_str());
        let db_gen = db_openrouter
            .and_then(|o| o.get("generation_model"))
            .and_then(|v| v.as_str());
        let db_key = db_openrouter
            .and_then(|o| o.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let db_referer = db_openrouter
            .and_then(|o| o.get("http_referer"))
            .and_then(|v| v.as_str());
        let db_app = db_openrouter
            .and_then(|o| o.get("app_name"))
            .and_then(|v| v.as_str());

        // Pull the catalog defaults so values stay in lockstep with the
        // static profile registry (see crates/matric-inference/src/provider_profiles.rs).
        let profile = matric_inference::lookup_provider_profile("openrouter");
        let default_url = profile
            .and_then(|p| p.default_base_url)
            .unwrap_or("https://openrouter.ai/api/v1");
        let default_gen = profile
            .and_then(|p| p.default_generation_model)
            .unwrap_or("anthropic/claude-sonnet-4");
        // Defaults sourced from the catalog's ProfileHeaderSource::Default
        // values so changing them in one place updates all reads.
        let env_referer = std::env::var("OPENROUTER_HTTP_REFERER").unwrap_or_default();
        let env_app = std::env::var("OPENROUTER_APP_NAME").unwrap_or_default();
        let default_referer = "https://fortemi.io";
        let default_app = "Fortemi";

        let api_key = db_key
            .map(|k| SourcedValue {
                value: redact_api_key(k),
                source: ConfigSource::DbOverride,
            })
            .or_else(|| {
                if !env_openrouter_key.is_empty() {
                    Some(SourcedValue {
                        value: redact_api_key(&env_openrouter_key),
                        source: ConfigSource::Env,
                    })
                } else {
                    None
                }
            });

        Some(SourcedOpenRouterConfig {
            base_url: pick(db_base, "", default_url),
            api_key,
            generation_model: pick(db_gen, "", default_gen),
            http_referer: pick(db_referer, &env_referer, default_referer),
            app_name: pick(db_app, &env_app, default_app),
        })
    } else {
        None
    };

    let mut providers = vec!["ollama".to_string()];
    if openai.is_some() {
        providers.push("openai".to_string());
    }
    if llamacpp.is_some() {
        providers.push("llamacpp".to_string());
    }
    if openrouter.is_some() {
        providers.push("openrouter".to_string());
    }

    // embedding_backend override: db_override > env > absent.
    let db_embedding = db
        .and_then(|v| v.get("embedding_backend"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let env_embedding = std::env::var("MATRIC_EMBEDDING_PROVIDER")
        .ok()
        .filter(|s| !s.is_empty());
    let embedding_backend = if let Some(v) = db_embedding {
        Some(SourcedValue {
            value: v.to_string(),
            source: ConfigSource::DbOverride,
        })
    } else {
        env_embedding.map(|v| SourcedValue {
            value: v,
            source: ConfigSource::Env,
        })
    };

    InferenceConfigResponse {
        default_backend: "ollama".to_string(),
        embedding_backend,
        ollama,
        openai,
        llamacpp,
        openrouter,
        providers,
    }
}

// =============================================================================
// HANDLERS
// =============================================================================

/// GET /api/v1/inference/config — return effective config with source attribution.
///
/// Layers DB overrides on top of environment variables on top of compiled defaults.
/// Each field indicates whether its value came from the database, an env var, or the default.
#[utoipa::path(
    get,
    path = "/api/v1/inference/config",
    tag = "Inference",
    responses(
        (status = 200, description = "Effective inference configuration"),
        (status = 500, description = "Database error"),
    )
)]
pub async fn get_inference_config(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> impl IntoResponse {
    let db_override = match read_db_override(&state.db.pool).await {
        Ok(v) => v,
        Err(e) => {
            let diagnostic = e.to_string();
            warn!(
                error_len = telemetry_text_len(&diagnostic),
                detail = INFERENCE_CONFIG_OVERRIDE_READ_FAILURE,
                "Failed to read inference_override from user_config"
            );
            return inference_config_database_failed();
        }
    };

    // #655: layer per-archive override on top of global when X-Fortemi-Memory
    // selects a non-default archive. Merge precedence: archive > global > env >
    // default. The merge is shallow — a per-archive `openrouter` block fully
    // replaces the global `openrouter` block rather than field-merging.
    // Operators get full per-archive isolation; the trade-off is that you
    // can't inherit the global api_key while overriding the model. (Field-
    // level merge can be added later if demand surfaces.)
    let merged = if let Some(schema) = archive_override_schema(&archive_ctx) {
        match read_archive_override(&state.db.pool, schema).await {
            Ok(Some(arch)) => Some(merge_archive_over_global(db_override.as_ref(), &arch)),
            Ok(None) => db_override,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    error_len = telemetry_text_len(&diagnostic),
                    schema_len = telemetry_text_len(schema),
                    detail = INFERENCE_CONFIG_OVERRIDE_READ_FAILURE,
                    "Failed to read archive_inference_override; falling back to global"
                );
                db_override
            }
        }
    } else {
        db_override
    };

    let effective = build_effective_config(merged.as_ref());
    (
        StatusCode::OK,
        Json(serde_json::to_value(effective).unwrap()),
    )
        .into_response()
}

/// Shallow-merge an archive override on top of the global override.
///
/// For each top-level key (`default_backend`, `embedding_backend`, per-
/// provider blocks), the archive value wins if present. Fields the archive
/// override doesn't touch fall through to the global. Provider blocks are
/// replaced wholesale, not field-merged — see `get_inference_config` for the
/// rationale.
fn merge_archive_over_global(global: Option<&Value>, archive: &Value) -> Value {
    let mut out = global.cloned().unwrap_or_else(|| serde_json::json!({}));
    if let (Some(out_obj), Some(arch_obj)) = (out.as_object_mut(), archive.as_object()) {
        for (k, v) in arch_obj {
            out_obj.insert(k.clone(), v.clone());
        }
    }
    out
}

/// POST /api/v1/inference/config — apply partial override and rebuild backend.
///
/// Accepts a partial config (any fields may be absent). Merges with any existing
/// DB override, validates, persists to `user_config`, and rebuilds the Ollama backend
/// in the `inference_runtime` slot. Pass `?validate=true` to probe endpoint reachability
/// before persisting.
#[utoipa::path(
    post,
    path = "/api/v1/inference/config",
    tag = "Inference",
    request_body = UpdateInferenceConfigRequest,
    params(
        ("validate" = Option<bool>, Query, description = "Pre-check endpoint reachability before persisting"),
    ),
    responses(
        (status = 200, description = "Config applied"),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Database or backend error"),
    )
)]
pub async fn update_inference_config(
    auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(params): Query<UpdateConfigQuery>,
    Json(req): Json<UpdateInferenceConfigRequest>,
) -> impl IntoResponse {
    // 0. #655: when X-Fortemi-Memory selects a non-default archive, we
    // operate on the archive's override row instead of the global one.
    // Storage path diverges (archive_inference_override vs user_config),
    // and the live ProviderRegistry hot-swap is skipped — runtime
    // per-archive routing is a follow-up that needs a per-schema backend
    // cache.
    let archive_schema = archive_override_schema(&archive_ctx).map(String::from);

    // 1. Read existing override blob as baseline. For archive-scoped
    // requests this is the per-schema row; for global it's the
    // user_config row. Either way, the merge logic below operates on
    // whatever blob we read.
    let existing_db = if let Some(ref schema) = archive_schema {
        match read_archive_override(&state.db.pool, schema).await {
            Ok(v) => v.unwrap_or(serde_json::json!({})),
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    error_len = telemetry_text_len(&diagnostic),
                    schema_len = telemetry_text_len(schema),
                    detail = INFERENCE_CONFIG_OVERRIDE_READ_FAILURE,
                    "Failed to read existing archive_inference_override"
                );
                return inference_config_database_failed();
            }
        }
    } else {
        match read_db_override(&state.db.pool).await {
            Ok(v) => v.unwrap_or(serde_json::json!({})),
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    error_len = telemetry_text_len(&diagnostic),
                    detail = INFERENCE_CONFIG_OVERRIDE_READ_FAILURE,
                    "Failed to read existing inference_override"
                );
                return inference_config_database_failed();
            }
        }
    };

    // 2. Capture previous effective config for the response. For archive-
    // scoped reads, layer the archive blob on top of the global so the
    // returned `previous` reflects the operator's view.
    let previous = if archive_schema.is_some() {
        let global = read_db_override(&state.db.pool).await.unwrap_or(None);
        let layered = merge_archive_over_global(global.as_ref(), &existing_db);
        serde_json::to_value(build_effective_config(Some(&layered))).unwrap()
    } else {
        serde_json::to_value(build_effective_config(Some(&existing_db))).unwrap()
    };

    // 3. Merge new values into existing DB override blob.
    let mut merged = existing_db;

    if let Some(partial_ollama) = &req.ollama {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("ollama")
            .or_insert(serde_json::json!({}));

        // Resolve current values for fields not being overridden.
        let env_o = EnvOllama::read();
        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let merged_base = partial_ollama.base_url.clone().unwrap_or_else(|| {
            if cur_base.is_empty() {
                env_o.base_url.clone()
            } else {
                cur_base.to_string()
            }
        });
        let merged_gen = partial_ollama.generation_model.clone().unwrap_or_else(|| {
            if cur_gen.is_empty() {
                env_o.generation_model.clone()
            } else {
                cur_gen.to_string()
            }
        });
        let merged_embed = partial_ollama.embedding_model.clone().unwrap_or_else(|| {
            if cur_embed.is_empty() {
                env_o.embedding_model.clone()
            } else {
                cur_embed.to_string()
            }
        });

        if let Err(e) = validate_ollama(&merged_base, &merged_gen, &merged_embed) {
            return err(
                StatusCode::BAD_REQUEST,
                format!("Ollama config invalid: {e}"),
            )
            .into_response();
        }

        // Optional pre-flight reachability probe.
        if params.validate {
            let probe = OllamaBackend::with_config(
                merged_base.clone(),
                merged_embed.clone(),
                merged_gen.clone(),
                EMBED_DIMENSION,
            );
            match probe.health_check().await {
                Ok(true) => {}
                Ok(false) => {
                    return ApiError::ProviderFailure {
                        capability: "Ollama inference configuration",
                        detail: "Ollama health check returned unhealthy".to_string(),
                    }
                    .into_response();
                }
                Err(e) => {
                    let error_text = e.to_string();
                    warn!(
                        provider = "ollama",
                        base_url_class = telemetry_url_class(&merged_base),
                        base_url_len = telemetry_text_len(&merged_base),
                        reason_code = probe_failure_reason(&error_text),
                        error_len = telemetry_text_len(&error_text),
                        "Ollama inference configuration probe failed"
                    );
                    return ApiError::ProviderFailure {
                        capability: "Ollama inference configuration",
                        detail: "Ollama health check request failed".to_string(),
                    }
                    .into_response();
                }
            }
        }

        // Write changed fields into the blob.
        let obj = entry.as_object_mut().expect("json object");
        if partial_ollama.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base.clone()));
        }
        if partial_ollama.generation_model.is_some() {
            obj.insert(
                "generation_model".to_string(),
                Value::String(merged_gen.clone()),
            );
        }
        if partial_ollama.embedding_model.is_some() {
            obj.insert(
                "embedding_model".to_string(),
                Value::String(merged_embed.clone()),
            );
        }

        // Hot-swap Ollama backend.
        let new_backend = std::sync::Arc::new(OllamaBackend::with_config(
            merged_base,
            merged_embed,
            merged_gen,
            EMBED_DIMENSION,
        ));
        let mut rt = state.inference_runtime.write().unwrap();
        rt.generation_backend = Some(new_backend);
        info!("Ollama backend hot-swapped from POST /api/v1/inference/config");
    }

    if let Some(partial_openai) = &req.openai {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("openai")
            .or_insert(serde_json::json!({}));

        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_key = entry
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        let default_openai_url = "https://api.openai.com/v1";
        let merged_base = partial_openai.base_url.clone().unwrap_or_else(|| {
            if cur_base.is_empty() {
                default_openai_url.to_string()
            } else {
                cur_base.to_string()
            }
        });
        let merged_gen = partial_openai.generation_model.clone().unwrap_or_else(|| {
            if cur_gen.is_empty() {
                "gpt-4o-mini".to_string()
            } else {
                cur_gen.to_string()
            }
        });
        let merged_embed = partial_openai.embedding_model.clone().unwrap_or_else(|| {
            if cur_embed.is_empty() {
                "text-embedding-3-small".to_string()
            } else {
                cur_embed.to_string()
            }
        });

        if let Err(e) = validate_openai(&merged_base, &merged_gen, &merged_embed) {
            return err(
                StatusCode::BAD_REQUEST,
                format!("OpenAI config invalid: {e}"),
            )
            .into_response();
        }

        let obj = entry.as_object_mut().expect("json object");
        if partial_openai.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base));
        }
        if let Some(ref key) = partial_openai.api_key.as_deref().or(cur_key.as_deref()) {
            obj.insert("api_key".to_string(), Value::String(key.to_string()));
        }
        if partial_openai.generation_model.is_some() {
            obj.insert("generation_model".to_string(), Value::String(merged_gen));
        }
        if partial_openai.embedding_model.is_some() {
            obj.insert("embedding_model".to_string(), Value::String(merged_embed));
        }
    }

    if let Some(partial_llamacpp) = &req.llamacpp {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("llamacpp")
            .or_insert(serde_json::json!({}));

        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_key = entry
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        let merged_base = partial_llamacpp.base_url.clone().unwrap_or_else(|| {
            if cur_base.is_empty() {
                matric_core::defaults::LLAMACPP_URL.to_string()
            } else {
                cur_base.to_string()
            }
        });
        let merged_gen = partial_llamacpp
            .generation_model
            .clone()
            .unwrap_or_else(|| {
                if cur_gen.is_empty() {
                    "default".to_string()
                } else {
                    cur_gen.to_string()
                }
            });
        let merged_embed = partial_llamacpp.embedding_model.clone().unwrap_or_else(|| {
            if cur_embed.is_empty() {
                "default".to_string()
            } else {
                cur_embed.to_string()
            }
        });

        let obj = entry.as_object_mut().expect("json object");
        if partial_llamacpp.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base));
        }
        if let Some(ref key) = partial_llamacpp.api_key.as_deref().or(cur_key.as_deref()) {
            obj.insert("api_key".to_string(), Value::String(key.to_string()));
        }
        if partial_llamacpp.generation_model.is_some() {
            obj.insert("generation_model".to_string(), Value::String(merged_gen));
        }
        if partial_llamacpp.embedding_model.is_some() {
            obj.insert("embedding_model".to_string(), Value::String(merged_embed));
        }
    }

    if let Some(partial_or) = &req.openrouter {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("openrouter")
            .or_insert(serde_json::json!({}));

        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_key = entry
            .get("api_key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_referer = entry
            .get("http_referer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_app = entry.get("app_name").and_then(|v| v.as_str()).unwrap_or("");

        // Defaults pulled from the catalog so they stay in lockstep with
        // crates/matric-inference/src/provider_profiles.rs::OPENROUTER_PROFILE.
        let profile = matric_inference::lookup_provider_profile("openrouter");
        let default_url = profile
            .and_then(|p| p.default_base_url)
            .unwrap_or("https://openrouter.ai/api/v1");
        let default_gen = profile
            .and_then(|p| p.default_generation_model)
            .unwrap_or("anthropic/claude-sonnet-4");
        let env_url = std::env::var("OPENROUTER_BASE_URL").unwrap_or_default();
        let env_gen = std::env::var("OPENROUTER_GEN_MODEL").unwrap_or_default();
        let env_referer = std::env::var("OPENROUTER_HTTP_REFERER").unwrap_or_default();
        let env_app = std::env::var("OPENROUTER_APP_NAME").unwrap_or_default();

        let merged_base = partial_or.base_url.clone().unwrap_or_else(|| {
            if !cur_base.is_empty() {
                cur_base.to_string()
            } else if !env_url.is_empty() {
                env_url.clone()
            } else {
                default_url.to_string()
            }
        });
        let merged_gen = partial_or.generation_model.clone().unwrap_or_else(|| {
            if !cur_gen.is_empty() {
                cur_gen.to_string()
            } else if !env_gen.is_empty() {
                env_gen.clone()
            } else {
                default_gen.to_string()
            }
        });
        let merged_referer = partial_or.http_referer.clone().unwrap_or_else(|| {
            if !cur_referer.is_empty() {
                cur_referer.to_string()
            } else if !env_referer.is_empty() {
                env_referer.clone()
            } else {
                "https://fortemi.io".to_string()
            }
        });
        let merged_app = partial_or.app_name.clone().unwrap_or_else(|| {
            if !cur_app.is_empty() {
                cur_app.to_string()
            } else if !env_app.is_empty() {
                env_app.clone()
            } else {
                "Fortemi".to_string()
            }
        });

        if merged_base.is_empty()
            || (!merged_base.starts_with("http://") && !merged_base.starts_with("https://"))
        {
            return err(
                StatusCode::BAD_REQUEST,
                "OpenRouter base_url must start with http:// or https://".to_string(),
            )
            .into_response();
        }
        if merged_gen.is_empty() {
            return err(
                StatusCode::BAD_REQUEST,
                "OpenRouter generation_model cannot be empty".to_string(),
            )
            .into_response();
        }

        let obj = entry.as_object_mut().expect("json object");
        if partial_or.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base));
        }
        if let Some(ref key) = partial_or.api_key.as_deref().or(cur_key.as_deref()) {
            obj.insert("api_key".to_string(), Value::String(key.to_string()));
        }
        if partial_or.generation_model.is_some() {
            obj.insert("generation_model".to_string(), Value::String(merged_gen));
        }
        if partial_or.http_referer.is_some() {
            obj.insert("http_referer".to_string(), Value::String(merged_referer));
        }
        if partial_or.app_name.is_some() {
            obj.insert("app_name".to_string(), Value::String(merged_app));
        }
    }

    // embedding_backend handling: distinguish "field absent" (leave as-is)
    // from "field present and null" (clear override) from "field present and
    // a string" (set override). Validates against the catalog so we reject
    // non-existent ids and providers that don't support embeddings before
    // committing.
    if let Some(opt_provider) = &req.embedding_backend {
        match opt_provider {
            None => {
                // Explicit clear.
                merged
                    .as_object_mut()
                    .expect("json object")
                    .remove("embedding_backend");
            }
            Some(id) => {
                let trimmed = id.trim();
                if trimmed.is_empty() {
                    return err(
                        StatusCode::BAD_REQUEST,
                        "embedding_backend cannot be empty; use null to clear it instead",
                    )
                    .into_response();
                }
                let profile = matric_inference::lookup_provider_profile(trimmed);
                match profile {
                    None => {
                        return err(
                            StatusCode::BAD_REQUEST,
                            format!(
                                "embedding_backend '{}' is not a known provider; \
                                 valid ids: ollama, openai, llamacpp, openrouter",
                                trimmed
                            ),
                        )
                        .into_response();
                    }
                    Some(p) if !p.supports_embeddings() => {
                        return err(
                            StatusCode::BAD_REQUEST,
                            format!(
                                "embedding_backend '{}' does not support embeddings; \
                                 pick a provider with the Embedding capability",
                                trimmed
                            ),
                        )
                        .into_response();
                    }
                    Some(_) => {
                        merged.as_object_mut().expect("json object").insert(
                            "embedding_backend".to_string(),
                            Value::String(trimmed.to_string()),
                        );
                    }
                }
            }
        }
    }

    // Atomic-mode pre-flight: probe every backend touched by this request
    // before committing. On any failure, abort with 503 so the live registry
    // and DB stay on the previous good config.
    if params.atomic {
        let probe_client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to build probe HTTP client: {e}"),
                )
                .into_response();
            }
        };

        let mut probe_failures: Vec<String> = Vec::new();

        if req.ollama.is_some() {
            if let Some(o) = merged.get("ollama") {
                let url = o
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_end_matches('/');
                if !url.is_empty() {
                    if let Err(e) = probe_ollama(&probe_client, url).await {
                        warn!(
                            provider = "ollama",
                            base_url_class = telemetry_url_class(url),
                            base_url_len = telemetry_text_len(url),
                            reason_code = probe_failure_reason(&e),
                            error_len = telemetry_text_len(&e),
                            "Atomic inference probe failed"
                        );
                        probe_failures.push("ollama".to_string());
                    }
                }
            }
        }

        if req.openai.is_some() {
            if let Some(o) = merged.get("openai") {
                let url = o
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_end_matches('/');
                let api_key = o.get("api_key").and_then(|v| v.as_str());
                if !url.is_empty() {
                    if let Err(e) = probe_openai(&probe_client, url, api_key).await {
                        warn!(
                            provider = "openai",
                            base_url_class = telemetry_url_class(url),
                            base_url_len = telemetry_text_len(url),
                            reason_code = probe_failure_reason(&e),
                            error_len = telemetry_text_len(&e),
                            "Atomic inference probe failed"
                        );
                        probe_failures.push("openai".to_string());
                    }
                }
            }
        }

        if req.llamacpp.is_some() {
            if let Some(o) = merged.get("llamacpp") {
                let url = o
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_end_matches('/');
                let api_key = o.get("api_key").and_then(|v| v.as_str());
                if !url.is_empty() {
                    // llama-server speaks OpenAI-compatible, so probe via
                    // /v1/models like any other OpenAI-compat endpoint.
                    if let Err(e) = probe_openai(&probe_client, url, api_key).await {
                        warn!(
                            provider = "llamacpp",
                            base_url_class = telemetry_url_class(url),
                            base_url_len = telemetry_text_len(url),
                            reason_code = probe_failure_reason(&e),
                            error_len = telemetry_text_len(&e),
                            "Atomic inference probe failed"
                        );
                        probe_failures.push("llamacpp".to_string());
                    }
                }
            }
        }

        if req.openrouter.is_some() {
            if let Some(o) = merged.get("openrouter") {
                let url = o
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_end_matches('/');
                let api_key = o.get("api_key").and_then(|v| v.as_str());
                if !url.is_empty() {
                    // OpenRouter speaks OpenAI-compatible — same probe path.
                    if let Err(e) = probe_openai(&probe_client, url, api_key).await {
                        warn!(
                            provider = "openrouter",
                            base_url_class = telemetry_url_class(url),
                            base_url_len = telemetry_text_len(url),
                            reason_code = probe_failure_reason(&e),
                            error_len = telemetry_text_len(&e),
                            "Atomic inference probe failed"
                        );
                        probe_failures.push("openrouter".to_string());
                    }
                }
            }
        }

        if !probe_failures.is_empty() {
            warn!(
                failure_count = probe_failures.len(),
                failed_providers = %probe_failures.join(","),
                "Atomic probe failed; aborting config swap"
            );
            return ApiError::ProviderFailure {
                capability: "Inference configuration probe",
                detail: format!("failed providers: {}", probe_failures.join(", ")),
            }
            .into_response();
        }
    }

    // Dry-run short-circuit: surface the would-be effective config without
    // touching the live registry or DB.
    if params.dry_run {
        let current = serde_json::to_value(build_effective_config(Some(&merged))).unwrap();
        let response = UpdateInferenceConfigResponse {
            status: "dry_run".to_string(),
            previous,
            current,
            warnings: vec![],
        };
        info!("POST /api/v1/inference/config dry-run completed (no changes persisted)");
        return (
            StatusCode::OK,
            Json(serde_json::to_value(response).unwrap()),
        )
            .into_response();
    }

    // For archive-scoped requests, skip the global hot-swap path entirely.
    // The override is persisted to archive_inference_override so subsequent
    // GETs see it; runtime per-archive routing (resolving the registry for
    // the archive at request time) is a follow-up — it requires a
    // per-schema backend cache that's a meaningful addition.
    if let Some(ref schema) = archive_schema {
        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO archive_inference_override (schema_name, override, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (schema_name) DO UPDATE
                SET override = EXCLUDED.override, updated_at = NOW()
            "#,
        )
        .bind(schema)
        .bind(&merged)
        .execute(&state.db.pool)
        .await
        {
            let diagnostic = e.to_string();
            warn!(
                error_len = telemetry_text_len(&diagnostic),
                schema_len = telemetry_text_len(schema),
                detail = INFERENCE_CONFIG_OVERRIDE_PERSIST_FAILURE,
                "Failed to persist archive_inference_override"
            );
            return inference_config_database_failed();
        }
        info!(
            schema = %schema,
            "archive_inference_override persisted via POST /api/v1/inference/config"
        );

        // Build "current effective config as the operator sees it" by
        // layering the new archive blob on top of the global.
        let global = read_db_override(&state.db.pool).await.unwrap_or(None);
        let layered = merge_archive_over_global(global.as_ref(), &merged);
        let current_typed = build_effective_config(Some(&layered));
        let current = serde_json::to_value(&current_typed).unwrap();

        let changed_fields = diff_changed_fields(&previous, &current);
        if !changed_fields.is_empty() {
            let embedding_backend = current_typed
                .embedding_backend
                .as_ref()
                .map(|sv| sv.value.clone());
            // Note: the event payload doesn't carry the schema today; per-
            // archive event scoping is filed as a future enhancement on #655.
            state.event_bus.emit(ServerEvent::InferenceConfigChanged {
                default_backend: current_typed.default_backend.clone(),
                embedding_backend,
                changed_fields: changed_fields.clone(),
            });
        }

        write_audit_row(
            &state.db.pool,
            "set_archive",
            Some(&previous),
            Some(&current),
            None,
        )
        .await;
        emit_inference_config_audit_event(inference_config_audit_event(
            &auth,
            "config_set_archive",
            Some(schema),
            &changed_fields,
            &current,
        ))
        .await;

        let response = UpdateInferenceConfigResponse {
            status: "applied_archive".to_string(),
            previous,
            current,
            warnings: vec![format!(
                "Per-archive override persisted for schema '{}'. Live runtime \
                 routing per archive is a follow-up; for now subsequent GETs \
                 with the same X-Fortemi-Memory header will reflect the override, \
                 and the global registry continues to serve traffic until the \
                 per-archive resolver lands.",
                schema
            )],
        };
        return (
            StatusCode::OK,
            Json(serde_json::to_value(response).unwrap()),
        )
            .into_response();
    }

    // Rebuild provider registry from merged config so all providers are hot-swapped.
    {
        let mut new_registry = matric_inference::ProviderRegistry::from_env();

        // Layer DB overrides for llama.cpp (env registration may have been empty at startup).
        if let Some(db_lc) = merged.get("llamacpp") {
            if let Some(base_url) = db_lc.get("base_url").and_then(|v| v.as_str()) {
                if !base_url.is_empty() && !new_registry.has_provider("llamacpp") {
                    new_registry.register(matric_inference::ProviderConfig {
                        id: "llamacpp".to_string(),
                        base_url: base_url.to_string(),
                        api_key: db_lc
                            .get("api_key")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                        capabilities: vec![
                            matric_inference::ProviderCapability::Generation,
                            matric_inference::ProviderCapability::Embedding,
                        ],
                        timeout: std::time::Duration::from_secs(300),
                        is_default: false,
                        health: matric_inference::ProviderHealth::Unknown,
                        http_referer: None,
                        x_title: None,
                    });
                }
            }
        }

        // Layer DB overrides for OpenRouter. Honors http_referer / app_name
        // so per-deployment routing rules and X-Title attribution take
        // effect immediately on hot-swap (defaults are applied by
        // build_effective_config; here we just propagate whatever the
        // merged blob holds).
        if let Some(db_or) = merged.get("openrouter") {
            let api_key = db_or
                .get("api_key")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .or_else(|| std::env::var("OPENROUTER_API_KEY").ok());
            // Without a key we can't make calls — skip registration entirely
            // rather than stamping a half-built provider into the registry.
            if api_key.is_some() {
                let base_url = db_or
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
                    .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string());

                let http_referer = db_or
                    .get("http_referer")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| std::env::var("OPENROUTER_HTTP_REFERER").ok())
                    .or_else(|| Some("https://fortemi.io".to_string()));
                let x_title = db_or
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| std::env::var("OPENROUTER_APP_NAME").ok())
                    .or_else(|| Some("Fortemi".to_string()));

                // Replace whatever from_env registered so later overrides win.
                new_registry.register(matric_inference::ProviderConfig {
                    id: "openrouter".to_string(),
                    base_url,
                    api_key,
                    capabilities: vec![matric_inference::ProviderCapability::Generation],
                    timeout: std::time::Duration::from_secs(300),
                    is_default: false,
                    health: matric_inference::ProviderHealth::Unknown,
                    http_referer,
                    x_title,
                });
            }
        }

        // Apply embedding_backend override (DB takes precedence over env;
        // env was already honored by ProviderRegistry::from_env above).
        if let Some(embed_id) = merged
            .get("embedding_backend")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            // Validation already happened earlier in the merge step; if the
            // provider isn't registered yet (e.g. just-added OpenRouter via
            // this same POST) we still set it — the registration above ran
            // first. validate_embedding_routing returns informative warnings.
            new_registry.set_embedding_provider(Some(embed_id.to_string()));
            if let Err(e) = new_registry.validate_embedding_routing() {
                let diagnostic = e.to_string();
                warn!(
                    error_len = telemetry_text_len(&diagnostic),
                    detail = INFERENCE_CONFIG_EMBEDDING_VALIDATION_FAILURE,
                    "embedding_backend override fails validation"
                );
            } else {
                info!(
                    embedding_provider = %embed_id,
                    "Independent embedding routing applied via runtime config"
                );
            }
        } else if merged.get("embedding_backend").is_some() {
            // Field present but empty — clear override.
            new_registry.set_embedding_provider(None);
        }

        let mut rt = state.inference_runtime.write().unwrap();
        rt.provider_registry = std::sync::Arc::new(new_registry);
        info!("Provider registry rebuilt from POST /api/v1/inference/config");
    }

    // 4. Persist merged config to DB.
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO user_config (key, value)
        VALUES ('inference_override', $1)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(&merged)
    .execute(&state.db.pool)
    .await
    {
        let diagnostic = e.to_string();
        warn!(
            error_len = telemetry_text_len(&diagnostic),
            detail = INFERENCE_CONFIG_OVERRIDE_PERSIST_FAILURE,
            "Failed to persist inference_override"
        );
        return inference_config_database_failed();
    }

    info!("inference_override persisted via POST /api/v1/inference/config");

    // 5. Build current effective config + emit hot-swap event (#657).
    let current_typed = build_effective_config(Some(&merged));
    let current = serde_json::to_value(&current_typed).unwrap();

    let changed_fields = diff_changed_fields(&previous, &current);
    if !changed_fields.is_empty() {
        let embedding_backend = current_typed
            .embedding_backend
            .as_ref()
            .map(|sv| sv.value.clone());
        state.event_bus.emit(ServerEvent::InferenceConfigChanged {
            default_backend: current_typed.default_backend.clone(),
            embedding_backend,
            changed_fields: changed_fields.clone(),
        });
    }

    // Audit log entry (#656). Best-effort — a failed insert is logged but
    // doesn't fail the request.
    write_audit_row(&state.db.pool, "set", Some(&previous), Some(&current), None).await;
    emit_inference_config_audit_event(inference_config_audit_event(
        &auth,
        "config_set",
        None,
        &changed_fields,
        &current,
    ))
    .await;

    let response = UpdateInferenceConfigResponse {
        status: "applied".to_string(),
        previous,
        current,
        warnings: vec![],
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
        .into_response()
}

/// DELETE /api/v1/inference/config — remove DB overrides and revert to env/defaults.
///
/// Deletes the `inference_override` row from `user_config`, then rebuilds the Ollama
/// backend from environment variables and swaps it into `inference_runtime`.
#[utoipa::path(
    delete,
    path = "/api/v1/inference/config",
    tag = "Inference",
    responses(
        (status = 200, description = "Config reset to env/defaults"),
        (status = 500, description = "Database or backend error"),
    )
)]
pub async fn delete_inference_config(
    auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> impl IntoResponse {
    // #655: archive-scoped DELETE clears just the per-archive override row;
    // the global config is untouched and continues to serve traffic.
    if let Some(schema) = archive_override_schema(&archive_ctx) {
        // Snapshot previous (layered) effective for audit/event.
        let global = read_db_override(&state.db.pool).await.unwrap_or(None);
        let prev_archive = read_archive_override(&state.db.pool, schema)
            .await
            .unwrap_or(None);
        let prev_layered = match (&global, &prev_archive) {
            (g, Some(a)) => merge_archive_over_global(g.as_ref(), a),
            (Some(g), None) => g.clone(),
            (None, None) => serde_json::json!({}),
        };
        let prev_effective =
            serde_json::to_value(build_effective_config(Some(&prev_layered))).unwrap();

        if let Err(e) = sqlx::query("DELETE FROM archive_inference_override WHERE schema_name = $1")
            .bind(schema)
            .execute(&state.db.pool)
            .await
        {
            let diagnostic = e.to_string();
            warn!(
                error_len = telemetry_text_len(&diagnostic),
                schema_len = telemetry_text_len(schema),
                detail = INFERENCE_CONFIG_OVERRIDE_DELETE_FAILURE,
                "Failed to delete archive_inference_override"
            );
            return inference_config_database_failed();
        }
        info!(
            schema = %schema,
            "archive_inference_override deleted via DELETE /api/v1/inference/config"
        );

        // Effective is now just the global config (the archive override is gone).
        let effective_typed = build_effective_config(global.as_ref());
        let effective = serde_json::to_value(&effective_typed).unwrap();
        let embedding_backend = effective_typed
            .embedding_backend
            .as_ref()
            .map(|sv| sv.value.clone());
        state.event_bus.emit(ServerEvent::InferenceConfigChanged {
            default_backend: effective_typed.default_backend.clone(),
            embedding_backend,
            changed_fields: vec!["__reset_archive__".to_string()],
        });

        write_audit_row(
            &state.db.pool,
            "reset_archive",
            Some(&prev_effective),
            Some(&effective),
            None,
        )
        .await;
        let changed_fields = vec!["__reset_archive__".to_string()];
        emit_inference_config_audit_event(inference_config_audit_event(
            &auth,
            "config_reset_archive",
            Some(schema),
            &changed_fields,
            &effective,
        ))
        .await;

        let response = ResetInferenceConfigResponse {
            status: "reset_archive".to_string(),
            effective,
        };
        return (
            StatusCode::OK,
            Json(serde_json::to_value(response).unwrap()),
        )
            .into_response();
    }

    // 0. Snapshot previous effective config for audit + event payload.
    let prev_effective = match read_db_override(&state.db.pool).await {
        Ok(v) => serde_json::to_value(build_effective_config(v.as_ref())).unwrap(),
        Err(_) => serde_json::Value::Null,
    };

    // 1. Delete DB override row.
    if let Err(e) = sqlx::query("DELETE FROM user_config WHERE key = 'inference_override'")
        .execute(&state.db.pool)
        .await
    {
        let diagnostic = e.to_string();
        warn!(
            error_len = telemetry_text_len(&diagnostic),
            detail = INFERENCE_CONFIG_OVERRIDE_DELETE_FAILURE,
            "Failed to delete inference_override"
        );
        return inference_config_database_failed();
    }

    info!("inference_override deleted via DELETE /api/v1/inference/config");

    // 2. Rebuild backend from env and hot-swap.
    let new_backend = std::sync::Arc::new(OllamaBackend::from_env());
    {
        let mut rt = state.inference_runtime.write().unwrap();
        rt.generation_backend = Some(new_backend);
    }

    // 3. Build effective config + emit reset event (#657).
    //
    // `__reset__` is a sentinel in `changed_fields` — clients that opt into
    // reactive updates can render a "config reset" notice without parsing
    // the empty diff (DELETE removes the entire DB override row, so most
    // fields revert to env/default in one operation).
    let effective_typed = build_effective_config(None);
    let effective = serde_json::to_value(&effective_typed).unwrap();
    let embedding_backend = effective_typed
        .embedding_backend
        .as_ref()
        .map(|sv| sv.value.clone());
    state.event_bus.emit(ServerEvent::InferenceConfigChanged {
        default_backend: effective_typed.default_backend.clone(),
        embedding_backend,
        changed_fields: vec!["__reset__".to_string()],
    });

    // Audit log entry (#656). Best-effort.
    write_audit_row(
        &state.db.pool,
        "reset",
        Some(&prev_effective),
        Some(&effective),
        None,
    )
    .await;
    let changed_fields = vec!["__reset__".to_string()];
    emit_inference_config_audit_event(inference_config_audit_event(
        &auth,
        "config_reset",
        None,
        &changed_fields,
        &effective,
    ))
    .await;

    let response = ResetInferenceConfigResponse {
        status: "reset".to_string(),
        effective,
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
        .into_response()
}

// =============================================================================
// AUDIT LOG ENDPOINT (#656)
// =============================================================================

/// One row from `inference_config_audit`. JSON blobs are sanitized again at
/// read time so the diagnostic endpoint never depends solely on writer hygiene.
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub changed_at: chrono::DateTime<chrono::Utc>,
    pub changed_by: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
}

impl std::fmt::Debug for AuditRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditRow")
            .field("id", &self.id)
            .field("changed_at", &self.changed_at)
            .field("changed_by_len", &telemetry_text_len(&self.changed_by))
            .field("action_len", &telemetry_text_len(&self.action))
            .field(
                "before_json_class",
                &self.before_json.as_ref().map(inference_json_class),
            )
            .field(
                "before_json_len",
                &self
                    .before_json
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.to_string())),
            )
            .field(
                "after_json_class",
                &self.after_json.as_ref().map(inference_json_class),
            )
            .field(
                "after_json_len",
                &self
                    .after_json
                    .as_ref()
                    .map(|value| telemetry_text_len(&value.to_string())),
            )
            .field(
                "source_ip_class",
                &self.source_ip.as_deref().map(inference_source_ip_class),
            )
            .finish()
    }
}

impl AuditRow {
    fn redacted(mut self) -> Self {
        self.before_json = self.before_json.as_ref().map(redact_inference_config_json);
        self.after_json = self.after_json.as_ref().map(redact_inference_config_json);
        self
    }
}

/// Query parameters for the audit log endpoint.
#[derive(Deserialize)]
pub struct AuditQuery {
    /// Maximum entries to return. Capped at 200 to bound payload size.
    #[serde(default = "default_audit_limit")]
    pub limit: i64,
    /// Filter to a specific actor ("anonymous" or an OAuth subject).
    pub changed_by: Option<String>,
    /// Filter to a specific action ("set", "reset", etc.).
    pub action: Option<String>,
}

impl std::fmt::Debug for AuditQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditQuery")
            .field("limit", &self.limit)
            .field(
                "changed_by_len",
                &self
                    .changed_by
                    .as_ref()
                    .map(|value| telemetry_text_len(value)),
            )
            .field(
                "action_len",
                &self.action.as_ref().map(|value| telemetry_text_len(value)),
            )
            .finish()
    }
}

fn default_audit_limit() -> i64 {
    50
}

/// `GET /api/v1/inference/config/audit` — return recent audit entries.
///
/// Newest first. Default limit 50; max 200. Optional `changed_by` and
/// `action` filters compose. Returns `{ entries: [...] }`.
#[utoipa::path(
    get,
    path = "/api/v1/inference/config/audit",
    tag = "Inference",
    params(
        ("limit" = Option<i64>, Query, description = "Max entries (default 50, max 200)"),
        ("changed_by" = Option<String>, Query, description = "Filter by actor"),
        ("action" = Option<String>, Query, description = "Filter by action type"),
    ),
    responses(
        (status = 200, description = "Audit entries"),
        (status = 500, description = "Database error"),
    )
)]
pub async fn get_inference_config_audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> impl IntoResponse {
    let limit = query.limit.clamp(1, 200);

    // Three optional filters yield four valid SQL shapes; build dynamically
    // with bound params to keep things simple and injection-safe.
    let mut sql = String::from(
        "SELECT id, changed_at, changed_by, action, before_json, after_json, source_ip \
         FROM inference_config_audit WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();
    if query.changed_by.is_some() {
        sql.push_str(&format!(" AND changed_by = ${}", binds.len() + 1));
        binds.push(query.changed_by.clone().unwrap());
    }
    if query.action.is_some() {
        sql.push_str(&format!(" AND action = ${}", binds.len() + 1));
        binds.push(query.action.clone().unwrap());
    }
    sql.push_str(&format!(
        " ORDER BY changed_at DESC LIMIT ${}",
        binds.len() + 1
    ));

    let mut q = sqlx::query_as::<_, AuditRow>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    q = q.bind(limit);

    match q.fetch_all(&state.db.pool).await {
        Ok(entries) => {
            let entries: Vec<_> = entries.into_iter().map(AuditRow::redacted).collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "entries": entries })),
            )
                .into_response()
        }
        Err(e) => {
            let diagnostic = e.to_string();
            warn!(
                error_len = telemetry_text_len(&diagnostic),
                detail = INFERENCE_CONFIG_AUDIT_FETCH_FAILURE,
                "Failed to fetch inference_config_audit"
            );
            inference_config_database_failed()
        }
    }
}

// =============================================================================
// TEST-CONNECTION ENDPOINT
// =============================================================================

/// Request body for the connection test endpoint.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct TestConnectionRequest {
    /// Base URL to probe (e.g. "http://gpu-server:11434").
    pub base_url: String,
    /// Protocol hint: "ollama", "openai", or "auto" (try both).
    #[serde(default = "default_test_provider")]
    pub provider: String,
    /// Bearer token for OpenAI-compatible endpoints (not required for Ollama).
    #[serde(default)]
    pub api_key: Option<String>,
    /// Request timeout in seconds (default: 10, clamped to 1–120).
    #[serde(default = "default_test_timeout")]
    pub timeout_secs: u64,
}

impl std::fmt::Debug for TestConnectionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestConnectionRequest")
            .field("base_url_class", &telemetry_url_class(&self.base_url))
            .field("base_url_len", &telemetry_text_len(&self.base_url))
            .field("provider_len", &telemetry_text_len(&self.provider))
            .field("api_key_present", &self.api_key.is_some())
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

fn default_test_provider() -> String {
    "auto".to_string()
}

fn default_test_timeout() -> u64 {
    10
}

/// Detected capability flags for the remote provider.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DetectedCapabilities {
    pub generation: bool,
    pub embedding: bool,
    pub vision: bool,
}

/// Connection test response — successful or not.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TestConnectionResponse {
    pub reachable: bool,
    /// Protocol detected: "ollama" or "openai". Null when unreachable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_provider: Option<String>,
    /// Ollama version string (e.g. "0.6.1"), when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_version: Option<String>,
    /// Model names returned by the remote endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,
    /// Round-trip latency in milliseconds for the detection probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Inferred capability flags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<DetectedCapabilities>,
    /// Human-readable error message when `reachable` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Actionable recovery suggestions when `reachable` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

impl std::fmt::Debug for TestConnectionResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestConnectionResponse")
            .field("reachable", &self.reachable)
            .field(
                "detected_provider_len",
                &self
                    .detected_provider
                    .as_ref()
                    .map(|value| telemetry_text_len(value)),
            )
            .field(
                "ollama_version_len",
                &self
                    .ollama_version
                    .as_ref()
                    .map(|value| telemetry_text_len(value)),
            )
            .field(
                "available_model_count",
                &self.available_models.as_ref().map(Vec::len),
            )
            .field("latency_ms", &self.latency_ms)
            .field("capabilities_present", &self.capabilities.is_some())
            .field(
                "error_len",
                &self.error.as_ref().map(|value| telemetry_text_len(value)),
            )
            .field("suggestion_count", &self.suggestions.as_ref().map(Vec::len))
            .finish()
    }
}

// Internal detection result — not serialized directly.
struct DetectionResult {
    provider: String,
    ollama_version: Option<String>,
    models: Vec<String>,
    latency_ms: u64,
}

/// Test connectivity to an inference endpoint and auto-detect its protocol.
///
/// Probes the given URL using either the Ollama or OpenAI-compatible protocol
/// (or both when `provider` is "auto"). Returns available models and detected
/// capabilities on success, or a structured error with actionable suggestions
/// on failure.
#[utoipa::path(
    post,
    path = "/api/v1/inference/test-connection",
    tag = "Inference",
    request_body = TestConnectionRequest,
    responses(
        (status = 200, description = "Connection test result (check reachable field)", body = TestConnectionResponse),
        (status = 400, description = "Invalid request body"),
    )
)]
pub async fn test_connection(
    State(_state): State<AppState>,
    Json(req): Json<TestConnectionRequest>,
) -> (StatusCode, Json<TestConnectionResponse>) {
    let base_url = req.base_url.trim_end_matches('/').to_string();
    let timeout = std::time::Duration::from_secs(req.timeout_secs.clamp(1, 120));

    if base_url.is_empty()
        || (!base_url.starts_with("http://") && !base_url.starts_with("https://"))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(TestConnectionResponse {
                reachable: false,
                detected_provider: None,
                ollama_version: None,
                available_models: None,
                latency_ms: None,
                capabilities: None,
                error: Some("base_url must start with http:// or https://".to_string()),
                suggestions: Some(vec![
                    "Provide a full URL, e.g. http://localhost:11434".to_string()
                ]),
            }),
        );
    }

    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TestConnectionResponse {
                    reachable: false,
                    detected_provider: None,
                    ollama_version: None,
                    available_models: None,
                    latency_ms: None,
                    capabilities: None,
                    error: Some(HTTP_CLIENT_INIT_ERROR.to_string()),
                    suggestions: None,
                }),
            );
        }
    };

    info!(
        base_url_class = telemetry_url_class(&base_url),
        base_url_len = telemetry_text_len(&base_url),
        provider = %req.provider,
        "Testing inference endpoint connection"
    );

    // Choose probe order based on hint or URL pattern.
    //
    // The two probe families (`ollama`, `openai`) correspond to the two
    // wire protocols Fortemi understands. For non-builtin hints we look up
    // the static profile catalog: anything declared as `BackendKind::Ollama`
    // probes through the Ollama path; anything OpenAI-compatible (OpenRouter,
    // llama.cpp, future vLLM/LiteLLM/etc.) probes through the OpenAI path.
    let providers_to_try: Vec<&str> = match req.provider.as_str() {
        "ollama" => vec!["ollama"],
        "openai" => vec!["openai"],
        other => {
            if let Some(profile) = matric_inference::lookup_provider_profile(other) {
                match profile.backend {
                    matric_inference::BackendKind::Ollama => vec!["ollama"],
                    matric_inference::BackendKind::OpenAICompatible => vec!["openai"],
                }
            } else {
                match auto_detect_from_url(&base_url) {
                    Some("ollama") => vec!["ollama", "openai"],
                    Some("openai") => vec!["openai", "ollama"],
                    _ => vec!["ollama", "openai"],
                }
            }
        }
    };

    for provider in providers_to_try {
        match detect_provider(&client, &base_url, provider, req.api_key.as_deref()).await {
            Ok(result) => {
                let caps = infer_capabilities(provider, &result.models);
                debug!(
                    provider = %result.provider,
                    models = result.models.len(),
                    latency_ms = result.latency_ms,
                    "Inference provider detected"
                );
                return (
                    StatusCode::OK,
                    Json(TestConnectionResponse {
                        reachable: true,
                        detected_provider: Some(result.provider),
                        ollama_version: result.ollama_version,
                        available_models: Some(result.models),
                        latency_ms: Some(result.latency_ms),
                        capabilities: Some(caps),
                        error: None,
                        suggestions: None,
                    }),
                );
            }
            Err(e) => {
                debug!(
                    provider = %provider,
                    reason_code = probe_failure_reason(&e),
                    error_len = telemetry_text_len(&e),
                    "Provider probe failed"
                );
            }
        }
    }

    let (error_msg, suggestions) = classify_connection_error(&base_url);
    warn!(
        base_url_class = telemetry_url_class(&base_url),
        base_url_len = telemetry_text_len(&base_url),
        reason_code = "endpoint_unreachable",
        "Inference endpoint unreachable"
    );

    (
        StatusCode::OK,
        Json(TestConnectionResponse {
            reachable: false,
            detected_provider: None,
            ollama_version: None,
            available_models: None,
            latency_ms: None,
            capabilities: None,
            error: Some(error_msg),
            suggestions: Some(suggestions),
        }),
    )
}

// =============================================================================
// PROVIDER DETECTION
// =============================================================================

/// Probe a URL for a specific provider protocol.
async fn detect_provider(
    client: &reqwest::Client,
    base_url: &str,
    provider: &str,
    api_key: Option<&str>,
) -> Result<DetectionResult, String> {
    match provider {
        "ollama" => probe_ollama(client, base_url).await,
        "openai" => probe_openai(client, base_url, api_key).await,
        _ => Err(PROBE_REASON_UNKNOWN_PROVIDER.to_string()),
    }
}

async fn probe_ollama(client: &reqwest::Client, base_url: &str) -> Result<DetectionResult, String> {
    let t0 = Instant::now();

    let resp = client
        .get(format!("{base_url}/api/tags"))
        .send()
        .await
        .map_err(|e| classify_reqwest_error(&e))?;

    let latency_ms = t0.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Err(PROBE_REASON_HTTP_STATUS.to_string());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| PROBE_REASON_INVALID_RESPONSE.to_string())?;

    let models_arr = body
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| PROBE_REASON_INVALID_RESPONSE.to_string())?;

    let models: Vec<String> = models_arr
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    // Best-effort version fetch (non-fatal).
    let ollama_version = fetch_ollama_version(client, base_url).await;

    Ok(DetectionResult {
        provider: "ollama".to_string(),
        ollama_version,
        models,
        latency_ms,
    })
}

async fn fetch_ollama_version(client: &reqwest::Client, base_url: &str) -> Option<String> {
    let resp = client
        .get(format!("{base_url}/api/version"))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("version")
        .and_then(|s| s.as_str())
        .map(String::from)
}

async fn probe_openai(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<DetectionResult, String> {
    let t0 = Instant::now();

    let mut req = client.get(format!("{base_url}/v1/models"));
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.map_err(|e| classify_reqwest_error(&e))?;

    let latency_ms = t0.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Err(PROBE_REASON_HTTP_STATUS.to_string());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| PROBE_REASON_INVALID_RESPONSE.to_string())?;

    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| PROBE_REASON_INVALID_RESPONSE.to_string())?;

    let models: Vec<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    Ok(DetectionResult {
        provider: "openai".to_string(),
        ollama_version: None,
        models,
        latency_ms,
    })
}

// =============================================================================
// CAPABILITY INFERENCE
// =============================================================================

fn infer_capabilities(provider: &str, models: &[String]) -> DetectedCapabilities {
    match provider {
        "ollama" => DetectedCapabilities {
            generation: models.iter().any(|m| !is_embedding_model(m)),
            embedding: models.iter().any(|m| is_embedding_model(m)),
            vision: models.iter().any(|m| is_vision_model(m)),
        },
        // OpenAI-compatible: assume generation and embedding are always available.
        _ => DetectedCapabilities {
            generation: !models.is_empty(),
            embedding: true,
            vision: false,
        },
    }
}

fn is_embedding_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("embed")
        || lower.contains("nomic")
        || lower.contains("mxbai")
        || lower.contains("bge")
        || lower.contains("minilm")
        || lower.contains("e5-")
        || lower.contains("gte-")
}

fn is_vision_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    // qwen3.5:* is natively multimodal (see CLAUDE.md OLLAMA_VISION_MODEL)
    lower.contains("llava")
        || lower.contains("bakllava")
        || lower.contains("moondream")
        || lower.contains("minicpm-v")
        || lower.contains("-vl")
        || lower.contains("vision")
        || lower.starts_with("qwen3.5:")
}

// =============================================================================
// URL-BASED INITIAL GUESS
// =============================================================================

/// Return a provider hint from URL patterns alone, without any network call.
///
/// Used to order probe attempts in "auto" mode so the most-likely protocol is
/// tried first.
pub fn auto_detect_from_url(url: &str) -> Option<&'static str> {
    if url.contains(":11434") {
        return Some("ollama");
    }
    if url.contains("api.openai.com") || url.contains("openrouter.ai") {
        return Some("openai");
    }
    if url.contains("/v1") {
        return Some("openai");
    }
    None
}

// =============================================================================
// ERROR HELPERS
// =============================================================================

fn classify_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        PROBE_REASON_TIMEOUT.to_string()
    } else if e.is_connect() {
        PROBE_REASON_CONNECT_FAILED.to_string()
    } else {
        "request failed".to_string()
    }
}

fn classify_connection_error(base_url: &str) -> (String, Vec<String>) {
    let is_local = base_url.contains("localhost") || base_url.contains("127.0.0.1");

    let error_msg = "Could not connect to the inference endpoint".to_string();

    let mut suggestions = vec![
        "Verify the endpoint is reachable from the Fortemi host".to_string(),
        "Check that the inference server is running".to_string(),
    ];

    if is_local && base_url.contains(":11434") {
        suggestions.insert(
            0,
            "Check that Ollama is running: systemctl status ollama".to_string(),
        );
    } else if is_local {
        suggestions.insert(
            0,
            "For Ollama, the default port is 11434 — try http://localhost:11434".to_string(),
        );
    }

    // Warn if no port is present after the host.
    let after_scheme = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_part = after_scheme.split('/').next().unwrap_or("");
    if !host_part.contains(':') {
        suggestions
            .push("Ensure the URL includes the port number (e.g. http://host:11434)".to_string());
    }

    if base_url.contains("docker") || base_url.contains("container") {
        suggestions
            .push("If using Docker, ensure the container is on the same network".to_string());
    }

    (error_msg, suggestions)
}

// =============================================================================
// UNIT TESTS
// =============================================================================

#[cfg(test)]
mod tests_connection {
    use super::*;

    #[tokio::test]
    async fn inference_config_database_failure_returns_fixed_problem() {
        let raw_database_detail = "postgres://user:secret@db.internal/app";
        let response = inference_config_database_failed();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/internal-error"
        );
        assert_eq!(problem["detail"], "An internal error occurred.");
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
        let serialized = problem.to_string();
        assert!(!serialized.contains(raw_database_detail));
        assert!(!serialized.contains("postgres://"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("db.internal"));
    }

    #[tokio::test]
    async fn inference_validation_errors_return_problem_without_submitted_url_echo() {
        let submitted_url = "ftp://token:secret@provider.internal/v1";
        let validation = validate_ollama(submitted_url, "gen", "embed").unwrap_err();
        assert!(!validation.contains(submitted_url));
        assert!(!validation.contains("secret"));

        let response = err(
            StatusCode::BAD_REQUEST,
            format!("Ollama config invalid: {validation}"),
        )
        .into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/validation-error"
        );
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
        let serialized = problem.to_string();
        assert!(!serialized.contains(submitted_url));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn inference_config_url_validation_does_not_echo_submitted_url() {
        let submitted_url = "ftp://token:secret@provider.internal/v1";

        let ollama = validate_ollama(submitted_url, "gen", "embed").unwrap_err();
        assert!(!ollama.contains(submitted_url));
        assert!(!ollama.contains("secret"));

        let openai = validate_openai(submitted_url, "gen", "embed").unwrap_err();
        assert!(!openai.contains(submitted_url));
        assert!(!openai.contains("secret"));
    }

    #[test]
    fn auto_detect_ollama_by_port() {
        assert_eq!(
            auto_detect_from_url("http://localhost:11434"),
            Some("ollama")
        );
        assert_eq!(
            auto_detect_from_url("http://gpu-server:11434"),
            Some("ollama")
        );
    }

    #[test]
    fn auto_detect_openai_by_domain() {
        assert_eq!(
            auto_detect_from_url("https://api.openai.com/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_openrouter() {
        assert_eq!(
            auto_detect_from_url("https://openrouter.ai/api/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_v1_path() {
        assert_eq!(
            auto_detect_from_url("http://myserver:8080/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_unknown_returns_none() {
        assert_eq!(auto_detect_from_url("http://myserver:8080"), None);
    }

    #[test]
    fn embedding_model_patterns() {
        assert!(is_embedding_model("nomic-embed-text"));
        assert!(is_embedding_model("mxbai-embed-large"));
        assert!(is_embedding_model("bge-small-en"));
        assert!(is_embedding_model("text-embedding-3-small"));
        assert!(!is_embedding_model("qwen3:8b"));
        assert!(!is_embedding_model("llama3.2:3b"));
    }

    #[test]
    fn vision_model_patterns() {
        assert!(is_vision_model("llava:34b"));
        assert!(is_vision_model("moondream:latest"));
        assert!(is_vision_model("qwen3.5:9b"));
        assert!(is_vision_model("minicpm-v:8b"));
        assert!(!is_vision_model("qwen3:8b")); // qwen3 (no .5) is not vision
        assert!(!is_vision_model("nomic-embed-text"));
    }

    #[test]
    fn ollama_mixed_models_all_caps() {
        let models = vec![
            "qwen3:8b".to_string(),
            "nomic-embed-text".to_string(),
            "llava:13b".to_string(),
        ];
        let caps = infer_capabilities("ollama", &models);
        assert!(caps.generation);
        assert!(caps.embedding);
        assert!(caps.vision);
    }

    #[test]
    fn ollama_embed_only_no_generation() {
        let models = vec!["nomic-embed-text".to_string()];
        let caps = infer_capabilities("ollama", &models);
        assert!(!caps.generation);
        assert!(caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn ollama_empty_models_no_caps() {
        let caps = infer_capabilities("ollama", &[]);
        assert!(!caps.generation);
        assert!(!caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn openai_always_has_generation_and_embedding() {
        let models = vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()];
        let caps = infer_capabilities("openai", &models);
        assert!(caps.generation);
        assert!(caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn error_for_ollama_local_suggests_systemctl() {
        let (msg, suggestions) = classify_connection_error("http://localhost:11434");
        assert!(!msg.is_empty());
        assert!(suggestions
            .iter()
            .any(|s| s.contains("systemctl status ollama")));
    }

    #[test]
    fn error_for_url_without_port_suggests_port() {
        let (_, suggestions) = classify_connection_error("http://myserver");
        assert!(suggestions.iter().any(|s| s.contains("port")));
    }

    #[test]
    fn error_for_url_with_port_no_redundant_hint() {
        let (_, suggestions) = classify_connection_error("http://myserver:11434");
        assert!(!suggestions.iter().any(|s| s.contains("port number")));
    }

    #[test]
    fn inference_probe_telemetry_url_class_avoids_raw_url_parts() {
        assert_eq!(
            telemetry_url_class("https://sk-secret@example.com/v1?api_key=token"),
            "external"
        );
        assert_eq!(
            telemetry_url_class("https://user:pass@10.0.0.5/internal?token=secret"),
            "local_or_private"
        );
        assert_eq!(
            telemetry_url_class("https://api.openai.com/v1/models?key=secret"),
            "managed_provider"
        );
        assert_eq!(telemetry_url_class("not a url with secret"), "invalid_url");
    }

    #[test]
    fn inference_config_diagnostic_classes_are_fixed_and_redacted() {
        let raw_diagnostics = [
            "postgres://fortemi:secret@db.internal/fortemi",
            "https://provider.example.com/v1?api_key=secret",
            "Bearer inference-config-token",
            "/srv/fortemi/config/inference.json",
            "tenant_alpha_archive_schema",
        ];
        let classes = [
            INFERENCE_CONFIG_AUDIT_WRITE_FAILURE,
            INFERENCE_CONFIG_AUDIT_EMIT_FAILURE,
            INFERENCE_CONFIG_OVERRIDE_READ_FAILURE,
            INFERENCE_CONFIG_OVERRIDE_PERSIST_FAILURE,
            INFERENCE_CONFIG_OVERRIDE_DELETE_FAILURE,
            INFERENCE_CONFIG_AUDIT_FETCH_FAILURE,
            INFERENCE_CONFIG_EMBEDDING_VALIDATION_FAILURE,
        ];

        for class in classes {
            assert!(class.starts_with("inference_config_"));
            for raw in raw_diagnostics {
                assert!(!class.contains(raw));
            }
            assert!(!class.contains("postgres://"));
            assert!(!class.contains("https://"));
            assert!(!class.contains("Bearer "));
            assert!(!class.contains("/srv/"));
            assert!(!class.contains("api_key="));
            assert!(!class.contains("tenant_alpha"));
        }
    }

    #[test]
    fn inference_config_api_key_metadata_never_echoes_secret_material() {
        for secret in [
            "sk",
            "sk-secret-prefix-that-should-not-appear",
            "openrouter-secret-token",
        ] {
            let redacted = redact_api_key(secret);
            assert!(redacted.starts_with("<secret_present_len:"));
            assert!(!redacted.contains(secret));
            assert!(!redacted.contains("sk-"));
            assert!(!redacted.contains("openrouter"));
            assert!(!redacted.contains("secret-token"));
        }
    }

    #[test]
    fn inference_config_audit_json_redacts_api_key_values_recursively() {
        let raw = serde_json::json!({
            "openai": {
                "api_key": {
                    "value": "sk-audit-secret-prefix",
                    "source": "db_override"
                },
                "generation_model": {
                    "value": "gpt-4o-mini",
                    "source": "default"
                }
            },
            "nested": [{
                "api_key": "short"
            }]
        });

        let redacted = redact_inference_config_json(&raw);
        let rendered = serde_json::to_string(&redacted).unwrap();

        assert!(rendered.contains("<secret_present_len:22>"));
        assert!(rendered.contains("<secret_present_len:5>"));
        assert!(rendered.contains("db_override"));
        assert!(rendered.contains("gpt-4o-mini"));
        assert!(!rendered.contains("sk-audit-secret-prefix"));
        assert!(!rendered.contains("\"short\""));
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("secret-prefix"));
    }

    #[test]
    fn inference_config_audit_debug_redacts_rows_and_queries() {
        let row = AuditRow {
            id: 42,
            changed_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            changed_by: "api_key:secret-operator-key-id".to_string(),
            action: "set-secret-provider".to_string(),
            before_json: Some(serde_json::json!({
                "openai": {
                    "base_url": "https://user:pass@api.openai.com/v1?api_key=secret",
                    "api_key": "sk-before-secret",
                    "generation_model": "gpt-before-secret"
                }
            })),
            after_json: Some(serde_json::json!({
                "openrouter": {
                    "base_url": "https://openrouter.ai/api/v1?token=secret",
                    "api_key": {
                        "value": "sk-or-after-secret",
                        "source": "db_override"
                    },
                    "app_name": "secret tenant app"
                }
            })),
            source_ip: Some("203.0.113.77".to_string()),
        };
        let query = AuditQuery {
            limit: 10,
            changed_by: Some("api_key:secret-operator-key-id".to_string()),
            action: Some("set-secret-provider".to_string()),
        };

        let rendered = format!("{row:?}{query:?}");

        assert!(rendered.contains("before_json_class"));
        assert!(rendered.contains("after_json_class"));
        assert!(rendered.contains("source_ip_class"));
        assert!(rendered.contains("changed_by_len"));
        assert!(rendered.contains("action_len"));
        assert!(!rendered.contains("secret-operator-key-id"));
        assert!(!rendered.contains("set-secret-provider"));
        assert!(!rendered.contains("203.0.113.77"));
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("api_key=secret"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains("api.openai.com"));
        assert!(!rendered.contains("openrouter.ai"));
        assert!(!rendered.contains("sk-before-secret"));
        assert!(!rendered.contains("sk-or-after-secret"));
        assert!(!rendered.contains("gpt-before-secret"));
        assert!(!rendered.contains("secret tenant app"));
    }

    #[test]
    fn inference_connection_suggestions_do_not_echo_raw_base_url() {
        let secret_url = "https://user:pass@provider.example.com:8443/v1?token=secret";
        let (_, suggestions) = classify_connection_error(secret_url);
        let rendered = suggestions.join("\n");
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("provider.example.com"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains(secret_url));
    }

    #[test]
    fn inference_probe_failure_reason_uses_stable_codes() {
        assert_eq!(probe_failure_reason(PROBE_REASON_TIMEOUT), "timeout");
        assert_eq!(
            probe_failure_reason(PROBE_REASON_CONNECT_FAILED),
            "connect_failed"
        );
        assert_eq!(
            probe_failure_reason(PROBE_REASON_HTTP_STATUS),
            "http_status"
        );
        assert_eq!(
            probe_failure_reason(PROBE_REASON_INVALID_RESPONSE),
            "invalid_response"
        );
        assert_eq!(
            probe_failure_reason(PROBE_REASON_UNKNOWN_PROVIDER),
            "unknown_provider"
        );
        assert_eq!(
            probe_failure_reason("secret backend message"),
            "probe_failed"
        );
    }

    #[test]
    fn inference_probe_reason_strings_do_not_preserve_backend_detail() {
        let rendered = [
            PROBE_REASON_UNKNOWN_PROVIDER,
            PROBE_REASON_HTTP_STATUS,
            PROBE_REASON_INVALID_RESPONSE,
            HTTP_CLIENT_INIT_ERROR,
        ]
        .join("\n");

        assert!(!rendered.contains("tenant-secret-provider"));
        assert!(!rendered.contains("GET /api/tags"));
        assert!(!rendered.contains("GET /v1/models"));
        assert!(!rendered.contains("HTTP 401"));
        assert!(!rendered.contains("Invalid JSON"));
        assert!(!rendered.contains("line"));
        assert!(!rendered.contains("column"));
    }

    #[test]
    fn inference_config_request_debug_redacts_secret_fields() {
        fn sourced(value: &str) -> SourcedValue {
            SourcedValue {
                value: value.to_string(),
                source: ConfigSource::DbOverride,
            }
        }

        let req = UpdateInferenceConfigRequest {
            ollama: Some(PartialOllamaConfig {
                base_url: Some("http://localhost:11434?token=secret".to_string()),
                generation_model: Some("qwen-secret-model".to_string()),
                embedding_model: None,
            }),
            openai: Some(PartialOpenAIConfig {
                base_url: Some("https://user:pass@api.openai.com/v1?api_key=secret".to_string()),
                api_key: Some("sk-secret-openai-key".to_string()),
                generation_model: Some("gpt-secret-model".to_string()),
                embedding_model: Some("text-secret-embedding".to_string()),
            }),
            llamacpp: Some(PartialLlamaCppConfig {
                base_url: Some("http://10.0.0.4:8080/v1?token=secret".to_string()),
                api_key: Some("llamacpp-secret-key".to_string()),
                generation_model: Some("local-secret-model".to_string()),
                embedding_model: None,
            }),
            openrouter: Some(PartialOpenRouterConfig {
                base_url: Some("https://openrouter.ai/api/v1?token=secret".to_string()),
                api_key: Some("sk-or-secret-openrouter-key".to_string()),
                generation_model: Some("anthropic/secret-model".to_string()),
                http_referer: Some("https://tenant-secret.example/app".to_string()),
                app_name: Some("secret tenant app".to_string()),
            }),
            embedding_backend: Some(Some("secret-embedding-backend".to_string())),
        };
        let response = InferenceConfigResponse {
            default_backend: "secret-default-backend".to_string(),
            embedding_backend: Some(sourced("secret-embedding-backend")),
            ollama: Some(SourcedOllamaConfig {
                base_url: sourced("http://localhost:11434?token=secret"),
                generation_model: sourced("qwen-secret-model"),
                embedding_model: sourced("nomic-secret-embedding"),
            }),
            openai: Some(SourcedOpenAIConfig {
                base_url: sourced("https://user:pass@api.openai.com/v1?api_key=secret"),
                api_key: Some(sourced("<secret_present_len:22>")),
                generation_model: sourced("gpt-secret-model"),
                embedding_model: sourced("text-secret-embedding"),
            }),
            llamacpp: Some(SourcedLlamaCppConfig {
                base_url: sourced("http://10.0.0.4:8080/v1?token=secret"),
                api_key: Some(sourced("<secret_present_len:19>")),
                generation_model: sourced("local-secret-model"),
                embedding_model: sourced("local-secret-embedding"),
            }),
            openrouter: Some(SourcedOpenRouterConfig {
                base_url: sourced("https://openrouter.ai/api/v1?token=secret"),
                api_key: Some(sourced("<secret_present_len:27>")),
                generation_model: sourced("anthropic/secret-model"),
                http_referer: sourced("https://tenant-secret.example/app"),
                app_name: sourced("secret tenant app"),
            }),
            providers: vec![
                "openai-secret-provider".to_string(),
                "openrouter-secret-provider".to_string(),
            ],
        };

        let rendered = format!(
            "{req:?}{response:?}{:?}{:?}{:?}{:?}",
            response.ollama.as_ref(),
            response.openai.as_ref(),
            response.llamacpp.as_ref(),
            response.openrouter.as_ref()
        );
        assert!(rendered.contains("openai_present: true"));
        assert!(rendered.contains("openrouter_present: true"));
        assert!(rendered.contains("embedding_backend_state: \"set\""));
        assert!(rendered.contains("provider_count: 2"));
        assert!(rendered.contains("api_key_present: true"));
        assert!(rendered.contains("base_url_class"));
        assert!(!rendered.contains("sk-secret-openai-key"));
        assert!(!rendered.contains("llamacpp-secret-key"));
        assert!(!rendered.contains("sk-or-secret-openrouter-key"));
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("api_key=secret"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains("api.openai.com"));
        assert!(!rendered.contains("openrouter.ai"));
        assert!(!rendered.contains("tenant-secret.example"));
        assert!(!rendered.contains("secret-default-backend"));
        assert!(!rendered.contains("qwen-secret-model"));
        assert!(!rendered.contains("nomic-secret-embedding"));
        assert!(!rendered.contains("gpt-secret-model"));
        assert!(!rendered.contains("text-secret-embedding"));
        assert!(!rendered.contains("local-secret-model"));
        assert!(!rendered.contains("local-secret-embedding"));
        assert!(!rendered.contains("anthropic/secret-model"));
        assert!(!rendered.contains("secret tenant app"));
        assert!(!rendered.contains("secret-embedding-backend"));
        assert!(!rendered.contains("openai-secret-provider"));
        assert!(!rendered.contains("openrouter-secret-provider"));
    }

    #[test]
    fn test_connection_request_debug_redacts_url_and_api_key() {
        let req = TestConnectionRequest {
            base_url: "https://user:pass@provider.example.com:8443/v1?token=secret".to_string(),
            provider: "openai".to_string(),
            api_key: Some("sk-secret-test-connection".to_string()),
            timeout_secs: 15,
        };
        let response = TestConnectionResponse {
            reachable: false,
            detected_provider: Some("secret-provider-protocol".to_string()),
            ollama_version: Some("secret-version-0.0.1".to_string()),
            available_models: Some(vec![
                "gpt-secret-model".to_string(),
                "embedding-secret-model".to_string(),
            ]),
            latency_ms: Some(123),
            capabilities: Some(DetectedCapabilities {
                generation: true,
                embedding: false,
                vision: false,
            }),
            error: Some(
                "provider https://user:pass@provider.example.com/v1?token=secret failed"
                    .to_string(),
            ),
            suggestions: Some(vec![
                "Check https://provider.example.com/secret-console".to_string(),
                "Rotate sk-secret-test-connection".to_string(),
            ]),
        };

        let rendered = format!("{req:?}{response:?}");
        assert!(rendered.contains("base_url_class: \"external\""));
        assert!(rendered.contains("api_key_present: true"));
        assert!(rendered.contains("timeout_secs: 15"));
        assert!(rendered.contains("available_model_count"));
        assert!(rendered.contains("capabilities_present: true"));
        assert!(rendered.contains("suggestion_count"));
        assert!(!rendered.contains("sk-secret-test-connection"));
        assert!(!rendered.contains("secret-provider-protocol"));
        assert!(!rendered.contains("secret-version"));
        assert!(!rendered.contains("gpt-secret-model"));
        assert!(!rendered.contains("embedding-secret-model"));
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("provider.example.com"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains("secret-console"));
        assert!(!rendered.contains(&req.base_url));
    }

    #[test]
    fn inference_config_audit_event_uses_metadata_only() {
        let auth = Auth {
            principal: AuthPrincipal::ApiKey {
                key_id: uuid::Uuid::parse_str("018fd1a0-0000-7000-8000-000000000201").unwrap(),
                scope: "admin sk-secret-scope".to_string(),
            },
        };
        let changed_fields = vec![
            "openai.api_key".to_string(),
            "openrouter.http_referer".to_string(),
            "bad\nfield=sk-secret".to_string(),
        ];
        let current = serde_json::json!({
            "openai": {
                "api_key": {"value": "sk-secret-openai-key", "source": "db_override"},
                "base_url": {"value": "https://user:pass@api.openai.com/v1?token=secret", "source": "db_override"},
                "generation_model": {"value": "gpt-secret-model", "source": "db_override"}
            },
            "openrouter": {
                "http_referer": {"value": "https://tenant-secret.example/app", "source": "db_override"},
                "app_name": {"value": "secret tenant app", "source": "db_override"}
            },
            "embedding_backend": {"value": "secret-embedding-backend", "source": "db_override"}
        });

        let event = inference_config_audit_event(
            &auth,
            "config_set_archive",
            Some("tenant_secret_schema"),
            &changed_fields,
            &current,
        );
        let rendered = serde_json::to_string(&event).unwrap();

        assert_eq!(event.category, "inference_config");
        assert_eq!(event.action, "config_set_archive");
        assert_eq!(event.outcome, AuditOutcome::Success);
        assert_eq!(event.source, AuditSource::Api);
        assert_eq!(event.visibility, AuditVisibilityClass::SecurityRestricted);
        assert_eq!(event.resource_kind.as_deref(), Some("inference_config"));
        assert_eq!(event.resource_id.as_deref(), Some("archive_override"));
        assert_eq!(
            event
                .attrs
                .get("changed_field_count")
                .and_then(|v| v.as_i64()),
            Some(3)
        );
        assert_eq!(
            event.attrs.get("config_scope").and_then(|v| v.as_str()),
            Some("archive")
        );
        assert_eq!(
            event
                .attrs
                .get("archive_schema_len")
                .and_then(|v| v.as_i64()),
            Some("tenant_secret_schema".chars().count() as i64)
        );
        assert!(rendered.contains("openai.api_key"));
        assert!(rendered.contains("unknown"));
        assert!(!rendered.contains("sk-secret-openai-key"));
        assert!(!rendered.contains("sk-secret-scope"));
        assert!(!rendered.contains("sk_secret"));
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("api.openai.com"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains("gpt-secret-model"));
        assert!(!rendered.contains("tenant-secret.example"));
        assert!(!rendered.contains("secret tenant app"));
        assert!(!rendered.contains("secret-embedding-backend"));
        assert!(!rendered.contains("tenant_secret_schema"));
        assert!(!rendered.contains("bad\nfield=sk-secret"));
    }

    // -----------------------------------------------------------------------
    // diff_changed_fields tests (#657)
    // -----------------------------------------------------------------------

    #[test]
    fn diff_returns_empty_when_identical() {
        let v = serde_json::json!({"default_backend": "ollama", "ollama": {"base_url": "x"}});
        assert!(diff_changed_fields(&v, &v).is_empty());
    }

    #[test]
    fn diff_detects_top_level_change() {
        let prev = serde_json::json!({"default_backend": "ollama"});
        let curr = serde_json::json!({"default_backend": "openrouter"});
        let diff = diff_changed_fields(&prev, &curr);
        assert_eq!(diff, vec!["default_backend".to_string()]);
    }

    #[test]
    fn diff_walks_into_provider_blocks() {
        let prev = serde_json::json!({
            "ollama": {"base_url": "http://a", "generation_model": "qwen3.5:9b"},
        });
        let curr = serde_json::json!({
            "ollama": {"base_url": "http://b", "generation_model": "qwen3.5:9b"},
        });
        let diff = diff_changed_fields(&prev, &curr);
        assert_eq!(diff, vec!["ollama.base_url".to_string()]);
    }

    #[test]
    fn diff_reports_added_provider_block() {
        let prev = serde_json::json!({});
        let curr = serde_json::json!({"openrouter": {"api_key": "redacted"}});
        let diff = diff_changed_fields(&prev, &curr);
        assert!(diff.iter().any(|f| f.starts_with("openrouter")));
    }

    #[test]
    fn diff_reports_embedding_backend_added() {
        let prev = serde_json::json!({"default_backend": "openrouter"});
        let curr = serde_json::json!({
            "default_backend": "openrouter",
            "embedding_backend": {"value": "ollama", "source": "db_override"}
        });
        let diff = diff_changed_fields(&prev, &curr);
        assert_eq!(diff, vec!["embedding_backend".to_string()]);
    }

    #[test]
    fn diff_does_not_carry_values_in_field_names() {
        // Field names must never include the secret values themselves.
        let prev = serde_json::json!({"openai": {"api_key": "sk-old"}});
        let curr = serde_json::json!({"openai": {"api_key": "sk-NEW"}});
        let diff = diff_changed_fields(&prev, &curr);
        assert_eq!(diff, vec!["openai.api_key".to_string()]);
        for f in &diff {
            assert!(!f.contains("sk-"), "field name must not leak key value");
        }
    }
}

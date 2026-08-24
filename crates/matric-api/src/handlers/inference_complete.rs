//! Provider-agnostic chat completion endpoints (Issue #628).
//!
//! `POST /api/v1/inference/complete` and `/stream` route to any registered
//! provider. Community Edition permits transient BYOK credentials; hosted
//! deployments require an opaque id for a tenant/user-scoped stored secret.
//!
//! Plus `GET /api/v1/inference/providers` reporting what's available based
//! on env config + a live Ollama probe.

use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use matric_api::services::unseal_user_secret;
use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass, AuthPrincipal, GenerationBackend, MeteringError, UsageAttributeKey,
    UsageAttributeValue, UsageAttributes, UsageClass, UsageCorrelation, UsageDimension, UsageEvent,
    UsageMeasurement, UsageMeter, UsageOutcome, UsageProducer, UsageSource, UsageSubject,
    UsageUnit,
};
#[cfg(feature = "hosted-auth")]
use matric_core::{EmbeddingBackend, UsageQuantity};
use matric_db::{PgUserSecretRepository, TenantScopedConn};
use matric_inference::destination_policy::DestinationSource;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::middleware::tenant_scope::TenantRequestScope;
use crate::{
    canonical_usage_request_id, usage_subject_from_auth, ApiError, AppState, Auth,
    TenantScopeReleasedBeforeStreaming,
};

const INFERENCE_COMPLETION_PROVIDER_DETAIL: &str =
    "Inference completion backend failed. Check server logs for diagnostics.";

// =============================================================================
// REQUEST + RESPONSE TYPES
// =============================================================================

/// A single chat message — `{role, content}`.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl std::fmt::Debug for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatMessage")
            .field("role_len", &complete_text_len(&self.role))
            .field("content_len", &complete_text_len(&self.content))
            .finish()
    }
}

/// Request body for `/complete` and `/stream`.
///
/// All fields except `model` and `messages` are optional. BYOK clients
/// may inject `provider_id` and `api_key` per-request from client-side
/// state without server-side persistence.
///
/// `temperature`, `max_tokens`, `think` are accepted but not currently
/// forwarded — the underlying `GenerationBackend` trait doesn't take them.
/// When the trait grows a richer API these become effective; kept in the
/// wire format now to avoid a breaking change later.
#[derive(Clone, Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct CompleteRequest {
    /// Provider id — `ollama`, `openai`, `openrouter`, `llamacpp`. If absent
    /// or `null`, default provider (Ollama) is used.
    #[serde(default)]
    pub provider_id: Option<String>,

    /// Per-request API key override. Takes precedence over registered config
    /// and env vars. Use `null`/omit for keyless providers (Ollama).
    #[serde(default)]
    pub api_key: Option<String>,

    /// Per-request base URL override. Mostly useful for local LLM endpoints
    /// (Ollama on a non-default host, llama.cpp on a custom port).
    #[serde(default)]
    pub base_url: Option<String>,

    /// Opaque ID of a hosted stored provider credential. Hosted mode requires
    /// this field and rejects inline `api_key` and `base_url` values.
    #[serde(default)]
    #[schema(ignore)]
    pub secret_id: Option<Uuid>,

    /// Required: model identifier (provider-specific format, e.g. `qwen3:8b`,
    /// `gpt-4o`, `anthropic/claude-sonnet-4.5`).
    pub model: String,

    /// Required: chat messages. First system role (if any) becomes the
    /// system prompt; remaining messages are formatted as a transcript.
    pub messages: Vec<ChatMessage>,

    #[serde(default)]
    pub temperature: Option<f32>,

    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Reserved for reasoning models — currently a hint, not enforced.
    #[serde(default)]
    pub think: Option<bool>,
}

impl std::fmt::Debug for CompleteRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base_url_class = self
            .base_url
            .as_deref()
            .map(complete_request_url_class)
            .unwrap_or("absent");
        let base_url_len = self.base_url.as_deref().map(complete_text_len).unwrap_or(0);
        let message_content_chars: usize = self
            .messages
            .iter()
            .map(|message| complete_text_len(&message.content))
            .sum();

        f.debug_struct("CompleteRequest")
            .field("provider_id_present", &self.provider_id.is_some())
            .field("api_key_present", &self.api_key.is_some())
            .field("base_url_class", &base_url_class)
            .field("base_url_len", &base_url_len)
            .field("secret_id_present", &self.secret_id.is_some())
            .field("model_len", &complete_text_len(&self.model))
            .field("message_count", &self.messages.len())
            .field("message_content_chars", &message_content_chars)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("think", &self.think)
            .finish()
    }
}

impl Drop for CompleteRequest {
    fn drop(&mut self) {
        if let Some(api_key) = &mut self.api_key {
            api_key.zeroize();
        }
    }
}

fn complete_request_url_class(raw: &str) -> &'static str {
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
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|addr| match addr {
                std::net::IpAddr::V4(addr) => {
                    addr.is_loopback()
                        || addr.is_private()
                        || addr.is_link_local()
                        || addr.is_unspecified()
                }
                std::net::IpAddr::V6(addr) => addr.is_loopback() || addr.is_unspecified(),
            })
    {
        return "local_or_private";
    }
    "external"
}

fn complete_text_len(value: &str) -> usize {
    value.chars().count()
}

/// Response body for `/complete`.
#[derive(Clone, Serialize, ToSchema)]
pub struct CompleteResponse {
    pub content: String,
    pub finish_reason: String,
    pub model: String,
    pub provider_id: String,
}

impl std::fmt::Debug for CompleteResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompleteResponse")
            .field("content_len", &complete_text_len(&self.content))
            .field("finish_reason_len", &complete_text_len(&self.finish_reason))
            .field("model_len", &complete_text_len(&self.model))
            .field("provider_id_len", &complete_text_len(&self.provider_id))
            .finish()
    }
}

/// Hosted stored-secret embedding request.
#[cfg(feature = "hosted-auth")]
#[derive(Clone, Deserialize, ToSchema)]
pub struct EmbedRequest {
    pub provider_id: String,
    pub secret_id: Uuid,
    pub model: String,
    pub dimension: usize,
    pub input: Vec<String>,
}

#[cfg(feature = "hosted-auth")]
impl std::fmt::Debug for EmbedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedRequest")
            .field("provider_id_len", &complete_text_len(&self.provider_id))
            .field("secret_id_present", &true)
            .field("model_len", &complete_text_len(&self.model))
            .field("dimension", &self.dimension)
            .field("input_count", &self.input.len())
            .field(
                "input_chars",
                &self
                    .input
                    .iter()
                    .map(|value| complete_text_len(value))
                    .sum::<usize>(),
            )
            .finish()
    }
}

#[cfg(feature = "hosted-auth")]
#[derive(Clone, Serialize, ToSchema)]
pub struct EmbedResponse {
    pub provider_id: String,
    pub model: String,
    pub dimension: usize,
    pub embeddings: Vec<Vec<f32>>,
}

#[cfg(feature = "hosted-auth")]
impl std::fmt::Debug for EmbedResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedResponse")
            .field("provider_id_len", &complete_text_len(&self.provider_id))
            .field("model_len", &complete_text_len(&self.model))
            .field("dimension", &self.dimension)
            .field("embedding_count", &self.embeddings.len())
            .finish()
    }
}

/// One entry in the `/providers` response.
#[derive(Clone, Serialize, ToSchema)]
pub struct ProviderInfo {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub base_url: String,
    pub capabilities: Vec<String>,
    /// True when the provider is reachable using server-side config alone
    /// (env vars present + remote responsive). UI uses this to hide the
    /// "add your key" form for already-configured providers.
    pub server_configured: bool,
    /// True when the provider needs an API key the server doesn't have.
    /// UI shows the BYOK form when this is `true`.
    pub requires_user_key: bool,
    /// True when the profile claims embedding support. UI uses this to gate
    /// embedding-related actions; the runtime gates embedding requests early
    /// against this flag to avoid confusing 404s from providers that only
    /// expose chat completions (e.g. OpenRouter).
    pub supports_embeddings: bool,
}

impl std::fmt::Debug for ProviderInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderInfo")
            .field("id_len", &complete_text_len(&self.id))
            .field("type_len", &complete_text_len(&self.r#type))
            .field("name_len", &complete_text_len(&self.name))
            .field(
                "base_url_class",
                &complete_request_url_class(&self.base_url),
            )
            .field("base_url_len", &complete_text_len(&self.base_url))
            .field("capability_count", &self.capabilities.len())
            .field("server_configured", &self.server_configured)
            .field("requires_user_key", &self.requires_user_key)
            .field("supports_embeddings", &self.supports_embeddings)
            .finish()
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

#[cfg(feature = "hosted-auth")]
#[derive(Clone, Serialize)]
pub struct HostedProviderCatalogEntry {
    pub provider_id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub models: Vec<String>,
}

#[cfg(feature = "hosted-auth")]
impl std::fmt::Debug for HostedProviderCatalogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedProviderCatalogEntry")
            .field("provider_id_len", &complete_text_len(&self.provider_id))
            .field("name_len", &complete_text_len(&self.name))
            .field("capability_count", &self.capabilities.len())
            .field("model_count", &self.models.len())
            .finish()
    }
}

#[cfg(feature = "hosted-auth")]
#[derive(Clone, Debug, Serialize)]
pub struct HostedProviderCatalogResponse {
    pub providers: Vec<HostedProviderCatalogEntry>,
}

impl std::fmt::Debug for ProvidersResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvidersResponse")
            .field("provider_count", &self.providers.len())
            .finish()
    }
}

const INFERENCE_FAILURE_MESSAGE: &str =
    "Inference provider failed. Check server logs for diagnostics.";

// =============================================================================
// HANDLERS
// =============================================================================

/// `GET /api/v1/inference/providers` — list providers Fortemi can route for.
///
/// Driven by the static catalog in `matric_inference::provider_profiles` (the
/// 4 v1 profiles, plus any future additions). For each known profile we
/// consult the live `ProviderRegistry` to determine `server_configured` —
/// providers in the registry have env-var config wired and can be used
/// without BYOK; profiles in the catalog but not in the registry render as
/// "available, bring your own key".
#[utoipa::path(
    get,
    path = "/api/v1/inference/providers",
    tag = "Inference",
    responses(
        (status = 200, description = "Available inference providers", body = ProvidersResponse),
    )
)]
pub async fn list_providers(
    State(state): State<AppState>,
    auth: Auth,
    scope: Option<Extension<TenantRequestScope>>,
) -> Result<Json<ProvidersResponse>, axum::response::Response> {
    use matric_inference::provider_profiles;

    let registry = state.provider_registry();
    let mut providers = Vec::new();
    let hosted_allowed = if state.inference_destination_policy.is_hosted() {
        let scope = scope
            .as_ref()
            .map(|Extension(scope)| scope)
            .ok_or_else(|| {
                ApiError::Unauthorized("Hosted inference context is unavailable".to_string())
                    .into_response()
            })?;
        Some(
            hosted_active_providers(&auth, scope)
                .await
                .map_err(IntoResponse::into_response)?,
        )
    } else {
        None
    };

    for profile in provider_profiles::iter() {
        if hosted_allowed
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(profile.id))
        {
            continue;
        }
        // server_configured: the registry was built from env at startup, so
        // a profile registered there had its credentials/base URL detected.
        // For keyless providers (Ollama, llama.cpp) just being registered is
        // sufficient; for keyed providers the api_key must be present.
        let registered = registry.get_provider(profile.id);
        let server_configured = !state.inference_destination_policy.is_hosted()
            && match registered {
                Some(cfg) => !profile.requires_api_key || cfg.api_key.is_some(),
                None => false,
            };

        // Use the registered base URL when available so operators see the
        // effective configured value; fall back to the profile's documented
        // default for the BYOK render path.
        let base_url = if state.inference_destination_policy.is_hosted() {
            String::new()
        } else {
            registered
                .map(|c| c.base_url.clone())
                .or_else(|| profile.default_base_url.map(String::from))
                .unwrap_or_default()
        };

        // Capability list comes from the catalog — it's the source of truth
        // for what a profile can do, regardless of whether it's currently
        // configured. The registered ProviderConfig may have a narrower list
        // (e.g. Ollama loses Vision when no vision model is loaded), but for
        // the picker UI we want to advertise the profile's full capability
        // footprint.
        let capabilities: Vec<String> =
            profile.capabilities.iter().map(|c| c.to_string()).collect();
        providers.push(ProviderInfo {
            id: profile.id.to_string(),
            r#type: profile.id.to_string(),
            name: profile.display_name.to_string(),
            base_url,
            capabilities,
            server_configured,
            requires_user_key: !state.inference_destination_policy.is_hosted()
                && profile.requires_api_key
                && !server_configured,
            supports_embeddings: profile.supports_embeddings(),
        });
    }

    Ok(Json(ProvidersResponse { providers }))
}

/// Hidden hosted catalog of caller-available provider profiles and
/// operator-approved generation/embedding defaults.
#[cfg(feature = "hosted-auth")]
pub async fn list_hosted_catalog(
    State(state): State<AppState>,
    auth: Auth,
    Extension(scope): Extension<TenantRequestScope>,
) -> Result<Json<HostedProviderCatalogResponse>, ApiError> {
    if !state.inference_destination_policy.is_hosted() {
        return Err(ApiError::NotFound("Endpoint is unavailable".to_string()));
    }
    let allowed = hosted_active_providers(&auth, &scope).await?;
    let providers = matric_inference::provider_profiles::iter()
        .filter(|profile| allowed.contains(profile.id))
        .map(|profile| HostedProviderCatalogEntry {
            provider_id: profile.id.to_string(),
            name: profile.display_name.to_string(),
            capabilities: profile
                .capabilities
                .iter()
                .map(ToString::to_string)
                .collect(),
            models: approved_profile_models(profile),
        })
        .collect();
    Ok(Json(HostedProviderCatalogResponse { providers }))
}

async fn hosted_active_providers(
    auth: &Auth,
    scope: &TenantRequestScope,
) -> Result<HashSet<String>, ApiError> {
    let user_id = match &auth.principal {
        AuthPrincipal::OAuthClient {
            user_id: Some(user_id),
            ..
        } => user_id.clone(),
        _ => {
            return Err(ApiError::Unauthorized(
                "Hosted inference requires a user identity".to_string(),
            ))
        }
    };
    let tenant_id = scope.tenant().tenant_id();
    scope
        .with_connection(move |connection| {
            Box::pin(async move {
                PgUserSecretRepository::list_tx(connection, tenant_id, &user_id)
                    .await
                    .map(|secrets| {
                        secrets
                            .into_iter()
                            .filter(|secret| secret.revoked_at.is_none())
                            .map(|secret| secret.provider)
                            .collect()
                    })
            })
        })
        .await
        .map_err(ApiError::from)
}

#[cfg(feature = "hosted-auth")]
fn approved_profile_models(
    profile: &matric_inference::provider_profiles::ProviderProfile,
) -> Vec<String> {
    let mut models = Vec::new();
    for model in [
        profile.default_generation_model.map(str::to_string),
        profile.default_embedding_model.map(str::to_string),
        profile
            .env
            .generation_model
            .and_then(|name| std::env::var(name).ok()),
        profile
            .env
            .embedding_model
            .and_then(|name| std::env::var(name).ok()),
    ]
    .into_iter()
    .flatten()
    {
        if !models.contains(&model) {
            models.push(model);
        }
    }
    models
}

/// `POST /api/v1/inference/complete` — provider-agnostic chat completion.
///
/// Stateless: builds a fresh backend from request-time creds (or registered
/// config or env), runs one generate call, returns the result.
#[utoipa::path(
    post,
    path = "/api/v1/inference/complete",
    tag = "Inference",
    request_body = CompleteRequest,
    responses(
        (status = 200, description = "Completion result", body = CompleteResponse),
        (status = 400, description = "Invalid request or provider configuration"),
        (status = 502, description = "Inference provider failure"),
    )
)]
pub async fn complete(
    State(state): State<AppState>,
    auth: Auth,
    scope: Option<Extension<TenantRequestScope>>,
    headers: HeaderMap,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, axum::response::Response> {
    let provider_id = req
        .provider_id
        .clone()
        .unwrap_or_else(|| "ollama".to_string());

    // Validate input.
    if req.messages.is_empty() {
        return Err(ApiError::BadRequest("messages array is empty".to_string()).into_response());
    }
    if req.model.is_empty() {
        return Err(ApiError::BadRequest("model is required".to_string()).into_response());
    }

    let backend = match resolve_request_backend(
        &state,
        RequestBackendInput {
            provider_id: &provider_id,
            api_key: req.api_key.as_deref(),
            base_url: req.base_url.as_deref(),
            secret_id: req.secret_id,
            model: &req.model,
        },
        &auth,
        scope.as_ref().map(|Extension(scope)| scope),
    )
    .await
    {
        Ok(b) => b,
        Err(e) => {
            warn!(
                provider_id_len = complete_text_len(&provider_id),
                reason_code = e,
                "Failed to resolve inline backend"
            );
            return Err(ApiError::BadRequest(
                "Provider resolution failed. Check provider id and credentials.".to_string(),
            )
            .into_response());
        }
    };

    let (system, prompt) = flatten_messages(&req.messages);
    let metering = inference_usage_context(&state, &auth, &headers, &provider_id, Utc::now())
        .inspect_err(|error| {
            warn!(
                error_len = complete_text_len(&error.to_string()),
                "Inference usage context construction failed"
            );
        });

    debug!(
        provider_id_len = complete_text_len(&provider_id),
        model_len = complete_text_len(&req.model),
        prompt_len = complete_text_len(&prompt),
        has_system = !system.is_empty(),
        "Running completion via inline backend"
    );

    let result = if system.is_empty() {
        backend.generate(&prompt).await
    } else {
        backend.generate_with_system(&system, &prompt).await
    };

    match result {
        Ok(content) => {
            if let Ok(context) = &metering {
                context.record(UsageOutcome::Completed).await;
            }
            info!(
                provider_id_len = complete_text_len(&provider_id),
                model_len = complete_text_len(&req.model),
                content_len = complete_text_len(&content),
                "Completion succeeded"
            );
            Ok(Json(CompleteResponse {
                content,
                finish_reason: "stop".to_string(),
                model: req.model.clone(),
                provider_id,
            }))
        }
        Err(e) => {
            if let Ok(context) = &metering {
                context.record(UsageOutcome::FailedAfterPartialUsage).await;
            }
            error!(
                provider_id_len = complete_text_len(&provider_id),
                model_len = complete_text_len(&req.model),
                error_len = complete_text_len(&e.to_string()),
                "Completion failed"
            );
            Err(ApiError::ProviderFailure {
                capability: "Inference completion",
                detail: INFERENCE_COMPLETION_PROVIDER_DETAIL.to_string(),
            }
            .into_response())
        }
    }
}

/// Hosted-only embedding through a caller-owned stored provider credential.
#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    post,
    path = "/api/v1/inference/embed",
    tag = "Inference",
    request_body = EmbedRequest,
    responses(
        (status = 200, description = "Embedding result", body = EmbedResponse),
        (status = 400, description = "Invalid or unavailable provider configuration"),
        (status = 502, description = "Inference provider failure"),
    )
)]
pub async fn embed_stored(
    State(state): State<AppState>,
    auth: Auth,
    Extension(scope): Extension<TenantRequestScope>,
    headers: HeaderMap,
    Json(req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, axum::response::Response> {
    if !state.inference_destination_policy.is_hosted() {
        return Err(ApiError::NotFound("Endpoint is unavailable".to_string()).into_response());
    }
    if req.provider_id.trim().is_empty()
        || req.model.trim().is_empty()
        || req.input.is_empty()
        || req.input.len() > 2048
        || req.dimension == 0
        || req.dimension > 65_536
        || req
            .input
            .len()
            .checked_mul(req.dimension)
            .is_none_or(|values| values > 1_048_576)
    {
        return Err(ApiError::BadRequest("Invalid embedding request".to_string()).into_response());
    }

    let backend = resolve_stored_embedding_backend(
        &state,
        &auth,
        &scope,
        req.secret_id,
        &req.provider_id,
        &req.model,
        req.dimension,
    )
    .await
    .map_err(|reason| {
        warn!(
            provider_id_len = complete_text_len(&req.provider_id),
            reason_code = reason,
            "Failed to resolve stored embedding backend"
        );
        ApiError::BadRequest(
            "Provider resolution failed. Check provider id and stored credential.".to_string(),
        )
        .into_response()
    })?;

    let result = backend.embed_texts(&req.input).await;
    match result {
        Ok(vectors)
            if vectors.len() == req.input.len()
                && vectors
                    .iter()
                    .all(|vector| vector.as_slice().len() == req.dimension) =>
        {
            record_embedding_usage(
                &state,
                &auth,
                &headers,
                &req.provider_id,
                Some(vectors.len()),
                UsageOutcome::Completed,
            )
            .await;
            Ok(Json(EmbedResponse {
                provider_id: req.provider_id,
                model: req.model,
                dimension: req.dimension,
                embeddings: vectors
                    .into_iter()
                    .map(|vector| vector.as_slice().to_vec())
                    .collect(),
            }))
        }
        Ok(_) | Err(_) => {
            record_embedding_usage(
                &state,
                &auth,
                &headers,
                &req.provider_id,
                None,
                UsageOutcome::FailedAfterPartialUsage,
            )
            .await;
            error!(
                provider_id_len = complete_text_len(&req.provider_id),
                model_len = complete_text_len(&req.model),
                "Stored-secret embedding failed"
            );
            Err(ApiError::ProviderFailure {
                capability: "Inference embedding",
                detail: INFERENCE_COMPLETION_PROVIDER_DETAIL.to_string(),
            }
            .into_response())
        }
    }
}

#[cfg(feature = "hosted-auth")]
async fn resolve_stored_embedding_backend(
    state: &AppState,
    auth: &Auth,
    scope: &TenantRequestScope,
    secret_id: Uuid,
    provider_id: &str,
    model: &str,
    dimension: usize,
) -> Result<Box<dyn EmbeddingBackend>, &'static str> {
    let (stored_key, context) =
        load_stored_inference_key(state, auth, scope, secret_id, provider_id).await?;
    if !hosted_model_allowed(provider_id, model, true) {
        emit_stored_inference_audit(
            state.audit_sink.as_ref(),
            &context,
            Some("model_not_allowed"),
        )
        .await?;
        return Err("model_not_allowed");
    }
    let (base_url, client) = approved_provider_client(
        state,
        provider_id,
        None,
        DestinationSource::OperatorConfiguration,
        Some(&context),
    )
    .await?;
    let registry = state.provider_registry();
    let backend = match registry.resolve_embedding_inline_approved(
        provider_id,
        Some(stored_key.as_str()),
        &base_url,
        model,
        dimension,
        client,
    ) {
        Ok(backend) => backend,
        Err(_) => {
            emit_stored_inference_audit(
                state.audit_sink.as_ref(),
                &context,
                Some("provider_resolution_failed"),
            )
            .await?;
            return Err("provider_resolution_failed");
        }
    };
    emit_stored_inference_audit(state.audit_sink.as_ref(), &context, None).await?;
    mark_stored_inference_key_used(state, context);
    Ok(backend)
}

fn hosted_model_allowed(provider_id: &str, model: &str, embedding: bool) -> bool {
    let Some(profile) = matric_inference::provider_profiles::lookup(provider_id) else {
        return false;
    };
    let capability = if embedding {
        matric_inference::ProviderCapability::Embedding
    } else {
        matric_inference::ProviderCapability::Generation
    };
    if !profile.supports(capability) {
        return false;
    }
    let (default_model, env_name) = if embedding {
        (profile.default_embedding_model, profile.env.embedding_model)
    } else {
        (
            profile.default_generation_model,
            profile.env.generation_model,
        )
    };
    default_model == Some(model)
        || env_name
            .and_then(|name| std::env::var(name).ok())
            .is_some_and(|configured| configured == model)
}

#[cfg(feature = "hosted-auth")]
async fn record_embedding_usage(
    state: &AppState,
    auth: &Auth,
    headers: &HeaderMap,
    provider_id: &str,
    vector_count: Option<usize>,
    outcome: UsageOutcome,
) {
    let Ok(subject) = usage_subject_from_auth(Some(auth)) else {
        return;
    };
    let request_id = canonical_usage_request_id(headers);
    for (dimension, suffix, count) in [
        (UsageDimension::EmbeddingVectors, "vectors", vector_count),
        (UsageDimension::EmbeddingTokens, "tokens", None),
    ] {
        let measurement = match count.and_then(|count| u64::try_from(count).ok()) {
            Some(count) => {
                UsageQuantity::whole(count, dimension.unit()).map(UsageMeasurement::Measured)
            }
            None => Ok(UsageMeasurement::Unavailable {
                unit: dimension.unit(),
            }),
        };
        let Ok(measurement) = measurement else {
            continue;
        };
        let mut attrs = UsageAttributes::default();
        if let Some(provider) = matric_inference::provider_profiles::lookup(provider_id) {
            let _ = UsageAttributeValue::label(provider.id)
                .and_then(|value| attrs.insert(&dimension, UsageAttributeKey::Provider, value));
        }
        let correlation = match request_id.as_deref() {
            Some(request_id) => UsageCorrelation::default().with_request_id(request_id),
            None => Ok(UsageCorrelation::default()),
        };
        let Ok(correlation) = correlation else {
            continue;
        };
        let event_id = Uuid::now_v7();
        let event = UsageEvent::new(
            format!("inference:{event_id}:embedding:{suffix}:actual"),
            Utc::now(),
            subject.clone(),
            dimension,
            measurement,
            UsageClass::BillableActual,
            UsageProducer::Inference,
            if count.is_some() {
                UsageSource::LocalMeasured
            } else {
                UsageSource::Unavailable
            },
            outcome,
        )
        .map(|event| event.with_identity(event_id, Utc::now()))
        .and_then(|event| event.with_correlation(correlation))
        .and_then(|event| event.with_attrs(attrs));
        if let Ok(event) = event {
            if let Err(error) = state.usage_meter.record(&event).await {
                warn!(
                    error_len = complete_text_len(&error.to_string()),
                    "Best-effort embedding usage recording failed"
                );
            }
        }
    }
}

#[derive(Clone)]
struct InferenceUsageContext {
    meter: Arc<dyn UsageMeter>,
    subject: UsageSubject,
    request_id: Option<String>,
    provider: Option<&'static str>,
    event_time: DateTime<Utc>,
    input_event_id: Uuid,
    output_event_id: Uuid,
}

impl InferenceUsageContext {
    async fn record(&self, outcome: UsageOutcome) {
        for (dimension, event_id, suffix) in [
            (
                UsageDimension::InferenceInputTokens,
                self.input_event_id,
                "input",
            ),
            (
                UsageDimension::InferenceOutputTokens,
                self.output_event_id,
                "output",
            ),
        ] {
            let event = inference_usage_event(self, dimension, event_id, suffix, outcome);
            match event {
                Ok(event) => {
                    if let Err(error) = self.meter.record(&event).await {
                        warn!(
                            error_len = complete_text_len(&error.to_string()),
                            "Best-effort inference usage recording failed"
                        );
                    }
                }
                Err(error) => {
                    warn!(
                        error_len = complete_text_len(&error.to_string()),
                        "Inference usage event construction failed"
                    );
                }
            }
        }
    }
}

fn inference_usage_context(
    state: &AppState,
    auth: &Auth,
    headers: &HeaderMap,
    provider_id: &str,
    event_time: DateTime<Utc>,
) -> Result<InferenceUsageContext, MeteringError> {
    Ok(InferenceUsageContext {
        meter: state.usage_meter.clone(),
        subject: usage_subject_from_auth(Some(auth))?,
        request_id: canonical_usage_request_id(headers),
        provider: matric_inference::provider_profiles::lookup(provider_id)
            .map(|profile| profile.id),
        event_time,
        input_event_id: Uuid::now_v7(),
        output_event_id: Uuid::now_v7(),
    })
}

fn inference_usage_event(
    context: &InferenceUsageContext,
    dimension: UsageDimension,
    event_id: Uuid,
    suffix: &str,
    outcome: UsageOutcome,
) -> Result<UsageEvent, MeteringError> {
    let mut attrs = UsageAttributes::default();
    if let Some(provider) = context.provider {
        attrs.insert(
            &dimension,
            UsageAttributeKey::Provider,
            UsageAttributeValue::label(provider)?,
        )?;
    }
    let correlation = match context.request_id.as_deref() {
        Some(request_id) => UsageCorrelation::default().with_request_id(request_id)?,
        None => UsageCorrelation::default(),
    };

    UsageEvent::new(
        format!("inference:{event_id}:{suffix}:actual"),
        context.event_time,
        context.subject.clone(),
        dimension,
        UsageMeasurement::Unavailable {
            unit: UsageUnit::Token,
        },
        UsageClass::BillableActual,
        UsageProducer::Inference,
        UsageSource::Unavailable,
        outcome,
    )?
    .with_identity(event_id, Utc::now())
    .with_correlation(correlation)?
    .with_attrs(attrs)
}

/// `POST /api/v1/inference/stream` — same shape as `/complete`, returns SSE.
///
/// Uses `GenerationBackend::stream_generate[_with_system]` (#629). For
/// backends that override with real token streaming (Ollama), this emits
/// one `delta` event per NDJSON chunk from upstream. Backends that still
/// use the trait default fall back to a single large `delta` (wire
/// compatible, just not progressive).
#[utoipa::path(
    post,
    path = "/api/v1/inference/stream",
    tag = "Inference",
    request_body = CompleteRequest,
    responses(
        (status = 200, description = "Server-sent completion stream", content_type = "text/event-stream"),
        (status = 400, description = "Invalid request or provider configuration"),
        (status = 502, description = "Inference provider failure"),
    )
)]
pub async fn stream(
    State(state): State<AppState>,
    auth: Auth,
    scope: Option<Extension<TenantRequestScope>>,
    headers: HeaderMap,
    Json(req): Json<CompleteRequest>,
) -> Result<axum::response::Response, axum::response::Response> {
    use futures::StreamExt;

    let provider_id = req
        .provider_id
        .clone()
        .unwrap_or_else(|| "ollama".to_string());

    if req.messages.is_empty() {
        return Err(ApiError::BadRequest("messages array is empty".to_string()).into_response());
    }
    if req.model.is_empty() {
        return Err(ApiError::BadRequest("model is required".to_string()).into_response());
    }

    let backend = match resolve_request_backend(
        &state,
        RequestBackendInput {
            provider_id: &provider_id,
            api_key: req.api_key.as_deref(),
            base_url: req.base_url.as_deref(),
            secret_id: req.secret_id,
            model: &req.model,
        },
        &auth,
        scope.as_ref().map(|Extension(scope)| scope),
    )
    .await
    {
        Ok(b) => b,
        Err(e) => {
            warn!(
                provider_id_len = complete_text_len(&provider_id),
                reason_code = e,
                "Failed to resolve inline stream backend"
            );
            return Err(ApiError::BadRequest(
                "Provider resolution failed. Check provider id and credentials.".to_string(),
            )
            .into_response());
        }
    };

    let (system, prompt) = flatten_messages(&req.messages);
    let metering = inference_usage_context(&state, &auth, &headers, &provider_id, Utc::now())
        .inspect_err(|error| {
            warn!(
                error_len = complete_text_len(&error.to_string()),
                "Inference stream usage context construction failed"
            );
        });

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let model_name = req.model.clone();
    let pid_clone = provider_id.clone();

    tokio::spawn(async move {
        // Ask the backend for a chunk stream. The trait default wraps
        // generate() in a one-item stream; Ollama and OpenAI overrides
        // yield many items.
        let stream_result = if system.is_empty() {
            backend.stream_generate(&prompt).await
        } else {
            backend.stream_generate_with_system(&system, &prompt).await
        };

        match stream_result {
            Ok(mut chunks) => {
                while let Some(chunk) = chunks.next().await {
                    match chunk {
                        Ok(content) => {
                            let payload = serde_json::json!({"content": content}).to_string();
                            if tx
                                .send(Ok(Event::default().event("delta").data(payload)))
                                .await
                                .is_err()
                            {
                                if let Ok(context) = &metering {
                                    context.record(UsageOutcome::ClientInterrupted).await;
                                }
                                return;
                            }
                        }
                        Err(e) => {
                            if let Ok(context) = &metering {
                                context.record(UsageOutcome::ProviderInterrupted).await;
                            }
                            error!(
                                provider_id_len = complete_text_len(&pid_clone),
                                model_len = complete_text_len(&model_name),
                                error_len = complete_text_len(&e.to_string()),
                                "Inference stream chunk failed"
                            );
                            let err_payload = inference_failed_sse_payload();
                            let _ = tx
                                .send(Ok(Event::default().event("error").data(err_payload)))
                                .await;
                            return;
                        }
                    }
                }
                if let Ok(context) = &metering {
                    context.record(UsageOutcome::Completed).await;
                }
                let done_payload = serde_json::json!({
                    "finish_reason": "stop",
                    "model": model_name,
                    "provider_id": pid_clone,
                })
                .to_string();
                let _ = tx
                    .send(Ok(Event::default().event("done").data(done_payload)))
                    .await;
            }
            Err(e) => {
                if let Ok(context) = &metering {
                    context.record(UsageOutcome::FailedAfterPartialUsage).await;
                }
                error!(
                    provider_id_len = complete_text_len(&pid_clone),
                    model_len = complete_text_len(&model_name),
                    error_len = complete_text_len(&e.to_string()),
                    "Inference stream failed"
                );
                let err_payload = inference_failed_sse_payload();
                let _ = tx
                    .send(Ok(Event::default().event("error").data(err_payload)))
                    .await;
            }
        }
    });

    let event_stream = ReceiverStream::new(rx);
    let mut response = Sse::new(event_stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response();
    if state.inference_destination_policy.is_hosted() {
        response
            .extensions_mut()
            .insert(TenantScopeReleasedBeforeStreaming);
    }
    Ok(response)
}

struct RequestBackendInput<'a> {
    provider_id: &'a str,
    api_key: Option<&'a str>,
    base_url: Option<&'a str>,
    secret_id: Option<Uuid>,
    model: &'a str,
}

async fn resolve_request_backend(
    state: &AppState,
    input: RequestBackendInput<'_>,
    auth: &Auth,
    scope: Option<&TenantRequestScope>,
) -> Result<Box<dyn GenerationBackend>, &'static str> {
    let RequestBackendInput {
        provider_id,
        api_key,
        base_url,
        secret_id,
        model,
    } = input;
    let registry = state.provider_registry();
    let (stored_key, stored_use) = if state.inference_destination_policy.is_hosted() {
        if api_key.is_some() || base_url.is_some() {
            emit_hosted_override_audit(state, auth, scope, provider_id, secret_id).await?;
            return Err("caller_credential_or_destination_denied");
        }
        let secret_id = secret_id.ok_or("stored_credential_required")?;
        let scope = scope.ok_or("tenant_scope_required")?;
        let loaded = load_stored_inference_key(state, auth, scope, secret_id, provider_id).await?;
        if !hosted_model_allowed(provider_id, model, false) {
            emit_stored_inference_audit(
                state.audit_sink.as_ref(),
                &loaded.1,
                Some("model_not_allowed"),
            )
            .await?;
            return Err("model_not_allowed");
        }
        (Some(loaded.0), Some(loaded.1))
    } else {
        if secret_id.is_some() {
            return Err("stored_credential_unavailable");
        }
        (None, None)
    };

    let source = if state.inference_destination_policy.is_hosted() {
        DestinationSource::OperatorConfiguration
    } else if base_url.is_some() {
        DestinationSource::CallerRequest
    } else if registry.get_provider(provider_id).is_some() {
        DestinationSource::OperatorConfiguration
    } else {
        DestinationSource::BuiltInDefault
    };
    let (approved_base_url, client) =
        approved_provider_client(state, provider_id, base_url, source, stored_use.as_ref()).await?;
    let resolved = registry
        .resolve_generation_inline_approved(
            provider_id,
            stored_key.as_ref().map(|key| key.as_str()).or(api_key),
            &approved_base_url,
            model,
            client,
        )
        .map_err(|_| "provider_resolution_failed");

    match resolved {
        Ok(backend) => {
            if let Some(context) = stored_use {
                emit_stored_inference_audit(state.audit_sink.as_ref(), &context, None).await?;
                mark_stored_inference_key_used(state, context);
            }
            Ok(backend)
        }
        Err(reason) => {
            if let Some(context) = stored_use {
                emit_stored_inference_audit(state.audit_sink.as_ref(), &context, Some(reason))
                    .await?;
            }
            Err(reason)
        }
    }
}

struct StoredInferenceUse {
    tenant_id: Uuid,
    user_id: String,
    secret_id: Uuid,
    provider: String,
}

async fn emit_hosted_override_audit(
    state: &AppState,
    auth: &Auth,
    scope: Option<&TenantRequestScope>,
    provider_id: &str,
    secret_id: Option<Uuid>,
) -> Result<(), &'static str> {
    let (
        Some(scope),
        AuthPrincipal::OAuthClient {
            user_id: Some(user_id),
            ..
        },
    ) = (scope, &auth.principal)
    else {
        return Ok(());
    };
    let mut event = AuditEvent::new(
        "stored_credential",
        "inference_override_attempt",
        AuditOutcome::Failure,
    )
    .with_tenant(scope.tenant().tenant_id().to_string())
    .with_principal(format!("oauth_user:{user_id}"))
    .with_resource(
        "stored_credential",
        secret_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "not_supplied".to_string()),
    )
    .with_attr("reason_code", "caller_credential_or_destination_denied")
    .with_failure_policy(AuditFailurePolicy::FailClosed);
    if let Some(profile) = matric_inference::provider_profiles::lookup(provider_id) {
        event = event.with_attr("provider", profile.id);
    }
    event.reason = Some("caller_credential_or_destination_denied".to_string());
    event.source = AuditSource::Api;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.severity = AuditSeverity::Warn;
    state
        .audit_sink
        .emit(event.sanitized())
        .await
        .map_err(|_| "audit_unavailable")
}

async fn approved_provider_client(
    state: &AppState,
    provider_id: &str,
    base_url: Option<&str>,
    source: DestinationSource,
    context: Option<&StoredInferenceUse>,
) -> Result<(String, reqwest::Client), &'static str> {
    let registry = state.provider_registry();
    let raw_destination = match registry.resolve_generation_destination(provider_id, base_url) {
        Ok(destination) => destination,
        Err(_) => {
            audit_stored_resolution_failure(state, context, "provider_resolution_failed").await?;
            return Err("provider_resolution_failed");
        }
    };
    let approved = match state
        .inference_destination_policy
        .authorize(provider_id, &raw_destination, source)
        .await
    {
        Ok(approved) => approved,
        Err(error) => {
            let reason = error.reason_code();
            audit_stored_resolution_failure(state, context, reason).await?;
            return Err(reason);
        }
    };
    let timeout = registry
        .get_provider(provider_id)
        .map(|config| config.timeout)
        .unwrap_or_else(|| Duration::from_secs(300));
    let client = match approved.build_client(timeout, false) {
        Ok(client) => client,
        Err(error) => {
            let reason = error.reason_code();
            audit_stored_resolution_failure(state, context, reason).await?;
            return Err(reason);
        }
    };
    Ok((approved.base_url().to_string(), client))
}

async fn audit_stored_resolution_failure(
    state: &AppState,
    context: Option<&StoredInferenceUse>,
    reason: &'static str,
) -> Result<(), &'static str> {
    if let Some(context) = context {
        emit_stored_inference_audit(state.audit_sink.as_ref(), context, Some(reason)).await?;
    }
    Ok(())
}

async fn load_stored_inference_key(
    state: &AppState,
    auth: &Auth,
    scope: &TenantRequestScope,
    secret_id: Uuid,
    requested_provider: &str,
) -> Result<(Zeroizing<String>, StoredInferenceUse), &'static str> {
    let requested_provider = matric_inference::provider_profiles::lookup(requested_provider)
        .map(|profile| profile.id)
        .ok_or("stored_credential_unavailable")?;
    let user_id = match &auth.principal {
        AuthPrincipal::OAuthClient {
            user_id: Some(user_id),
            ..
        } => user_id.clone(),
        _ => return Err("user_identity_required"),
    };
    let tenant_id = scope.tenant().tenant_id();
    let mut context = StoredInferenceUse {
        tenant_id,
        user_id: user_id.clone(),
        secret_id,
        provider: requested_provider.to_string(),
    };
    let lookup_user_id = user_id.clone();
    let stored = match scope
        .with_connection(move |connection| {
            Box::pin(async move {
                PgUserSecretRepository::get_active_tx(
                    connection,
                    tenant_id,
                    &lookup_user_id,
                    secret_id,
                )
                .await
            })
        })
        .await
    {
        Ok(Some(stored)) => stored,
        Ok(None) | Err(_) => {
            emit_stored_inference_audit(
                state.audit_sink.as_ref(),
                &context,
                Some("stored_credential_unavailable"),
            )
            .await?;
            return Err("stored_credential_unavailable");
        }
    };
    context.provider = stored.metadata.provider.clone();
    if context.provider != requested_provider {
        emit_stored_inference_audit(
            state.audit_sink.as_ref(),
            &context,
            Some("stored_credential_unavailable"),
        )
        .await?;
        return Err("stored_credential_unavailable");
    }
    let Some(key_provider) = state.key_provider.as_deref() else {
        emit_stored_inference_audit(
            state.audit_sink.as_ref(),
            &context,
            Some("key_service_unavailable"),
        )
        .await?;
        return Err("key_service_unavailable");
    };
    let key = match unseal_user_secret(
        key_provider,
        tenant_id,
        &context.user_id,
        secret_id,
        &context.provider,
        stored.encrypted_blob,
    )
    .await
    {
        Ok(key) => key,
        Err(_) => {
            emit_stored_inference_audit(
                state.audit_sink.as_ref(),
                &context,
                Some("key_operation_denied"),
            )
            .await?;
            return Err("key_operation_denied");
        }
    };
    Ok((key, context))
}

async fn emit_stored_inference_audit(
    sink: &dyn AuditSink,
    context: &StoredInferenceUse,
    reason: Option<&'static str>,
) -> Result<(), &'static str> {
    let outcome = if reason.is_some() {
        AuditOutcome::Failure
    } else {
        AuditOutcome::Success
    };
    let mut event = AuditEvent::new("stored_credential", "inference_use", outcome)
        .with_tenant(context.tenant_id.to_string())
        .with_principal(format!("oauth_user:{}", context.user_id))
        .with_resource("stored_credential", context.secret_id.to_string())
        .with_attr("provider", context.provider.clone())
        .with_failure_policy(AuditFailurePolicy::FailClosed);
    if let Some(reason) = reason {
        event.reason = Some(reason.to_string());
        event = event.with_attr("reason_code", reason);
    }
    event.source = AuditSource::Api;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.severity = if reason.is_some() {
        AuditSeverity::Warn
    } else {
        AuditSeverity::Info
    };
    sink.emit(event.sanitized())
        .await
        .map_err(|_| "audit_unavailable")
}

fn mark_stored_inference_key_used(state: &AppState, context: StoredInferenceUse) {
    let pool = state.db.pool.clone();
    tokio::spawn(async move {
        let Ok(mut connection) = TenantScopedConn::begin(&pool, context.tenant_id).await else {
            warn!(
                provider_id_len = complete_text_len(&context.provider),
                "Best-effort stored credential usage update could not start"
            );
            return;
        };
        let updated = PgUserSecretRepository::mark_used_tx(
            connection.executor(),
            context.tenant_id,
            &context.user_id,
            context.secret_id,
        )
        .await;
        if updated.is_ok() && connection.commit().await.is_ok() {
            return;
        }
        warn!(
            provider_id_len = complete_text_len(&context.provider),
            "Best-effort stored credential usage update failed"
        );
    });
}

// =============================================================================
// HELPERS
// =============================================================================

/// Flatten OpenAI-style chat messages into a (system, prompt) pair so we
/// can call the trait's `generate_with_system`. The system text is the
/// concatenation of all `system` role messages; the prompt is a transcript
/// of the remaining messages with role labels.
fn flatten_messages(messages: &[ChatMessage]) -> (String, String) {
    let mut system_parts = Vec::new();
    let mut transcript = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "system" => system_parts.push(msg.content.as_str()),
            role => {
                if !transcript.is_empty() {
                    transcript.push('\n');
                }
                transcript.push_str(&format!("{}: {}", role, msg.content));
            }
        }
    }
    let system = system_parts.join("\n\n");
    // If the conversation is just one user turn, drop the "user: " prefix
    // so the model gets a clean prompt.
    let prompt = if messages.len() == 1 && messages[0].role == "user" {
        messages[0].content.clone()
    } else if messages.iter().filter(|m| m.role != "system").count() == 1 {
        messages
            .iter()
            .find(|m| m.role != "system")
            .map(|m| m.content.clone())
            .unwrap_or_default()
    } else {
        // Multi-turn — append "assistant:" so the next-token continues the
        // assistant's reply.
        format!("{}\nassistant:", transcript)
    };
    (system, prompt)
}

fn inference_failed_sse_payload() -> String {
    serde_json::json!({
        "error": INFERENCE_FAILURE_MESSAGE,
        "code": "INFERENCE_FAILED",
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_completion_provider_detail_is_fixed_and_redacted() {
        assert_eq!(
            INFERENCE_COMPLETION_PROVIDER_DETAIL,
            "Inference completion backend failed. Check server logs for diagnostics."
        );
        assert!(!INFERENCE_COMPLETION_PROVIDER_DETAIL.contains("https://"));
        assert!(!INFERENCE_COMPLETION_PROVIDER_DETAIL.contains("token"));
        assert!(!INFERENCE_COMPLETION_PROVIDER_DETAIL.contains("/srv/fortemi"));
    }

    #[test]
    fn inference_failed_sse_payload_uses_generic_message() {
        let raw_error = "provider https://user:pass@example.com/v1 failed with sk-secret at /tmp/x";
        let payload = inference_failed_sse_payload();
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(value["error"], INFERENCE_FAILURE_MESSAGE);
        assert_eq!(value["code"], "INFERENCE_FAILED");
        assert!(!payload.contains(raw_error));
        assert!(!payload.contains("user:pass"));
        assert!(!payload.contains("sk-secret"));
        assert!(!payload.contains("/tmp/x"));
    }

    #[test]
    fn complete_telemetry_lengths_redact_private_values() {
        let value = "provider/private-model user@example.com token=sk-secret";

        assert_eq!(complete_text_len(value), value.chars().count());
        assert_eq!(complete_text_len(value), 55);

        // Flattened prompts and generated content telemetry must report Unicode
        // character counts, not byte counts. Multibyte transcripts would otherwise
        // leak encoding-dependent byte sizes through the prompt_len/content_len fields.
        let multibyte = "café — 日本語 transcript";
        assert_eq!(complete_text_len(multibyte), multibyte.chars().count());
        assert!(complete_text_len(multibyte) < multibyte.len());
    }

    #[test]
    fn complete_request_debug_redacts_byok_secret_and_prompt_fields() {
        let secret_id = Uuid::parse_str("0198dc7f-3ed1-7000-8000-000000000731").unwrap();
        let req = CompleteRequest {
            provider_id: Some("openai".to_string()),
            api_key: Some("sk-secret-provider-key".to_string()),
            base_url: Some("https://user:pass@api.openai.com/v1?token=secret".to_string()),
            secret_id: Some(secret_id),
            model: "gpt-secret-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "patient prompt with secret transcript".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(128),
            think: Some(false),
        };
        let message = ChatMessage {
            role: "system-secret-role".to_string(),
            content: "system prompt with credential sk-message-secret".to_string(),
        };
        let response = CompleteResponse {
            content: "model output with private transcript secret".to_string(),
            finish_reason: "stop-secret-reason".to_string(),
            model: "gpt-secret-response-model".to_string(),
            provider_id: "openai-secret-provider".to_string(),
        };
        let provider = ProviderInfo {
            id: "provider-secret-id".to_string(),
            r#type: "provider-secret-type".to_string(),
            name: "Provider Secret Name".to_string(),
            base_url: "https://user:pass@llm.example/v1?token=provider-secret".to_string(),
            capabilities: vec!["secret-capability".to_string()],
            server_configured: true,
            requires_user_key: true,
            supports_embeddings: false,
        };
        let providers = ProvidersResponse {
            providers: vec![provider.clone()],
        };

        let rendered = format!("{req:?}{message:?}{response:?}{provider:?}{providers:?}");
        assert!(rendered.contains("api_key_present: true"));
        assert!(rendered.contains("base_url_class: \"managed_provider\""));
        assert!(rendered.contains("message_count: 1"));
        assert!(rendered.contains("role_len"));
        assert!(rendered.contains("content_len"));
        assert!(rendered.contains("content_len"));
        assert!(rendered.contains("provider_count"));
        assert!(rendered.contains("capability_count"));
        assert!(!rendered.contains("sk-secret-provider-key"));
        assert!(!rendered.contains("sk-message-secret"));
        assert!(!rendered.contains("user:pass"));
        assert!(!rendered.contains("token=secret"));
        assert!(!rendered.contains("provider-secret"));
        assert!(!rendered.contains("api.openai.com"));
        assert!(!rendered.contains("llm.example"));
        assert!(!rendered.contains("gpt-secret-model"));
        assert!(!rendered.contains("gpt-secret-response-model"));
        assert!(!rendered.contains("patient prompt"));
        assert!(!rendered.contains("secret transcript"));
        assert!(!rendered.contains("system-secret-role"));
        assert!(!rendered.contains("system prompt"));
        assert!(!rendered.contains("private transcript"));
        assert!(!rendered.contains("stop-secret-reason"));
        assert!(!rendered.contains("Provider Secret Name"));
        assert!(!rendered.contains("secret-capability"));
        assert!(!rendered.contains(&secret_id.to_string()));
    }

    #[test]
    #[cfg(feature = "hosted-auth")]
    fn embedding_debug_omits_secret_id_inputs_models_and_vectors() {
        let secret_id = Uuid::parse_str("0198dc7f-3ed1-7000-8000-000000000732").unwrap();
        let request = EmbedRequest {
            provider_id: "openai-private-provider".to_string(),
            secret_id,
            model: "private-embedding-model".to_string(),
            dimension: 3,
            input: vec!["private embedding input".to_string()],
        };
        let response = EmbedResponse {
            provider_id: "openai-private-provider".to_string(),
            model: "private-embedding-model".to_string(),
            dimension: 3,
            embeddings: vec![vec![0.123_456, 0.654_321, 0.777_777]],
        };

        let rendered = format!("{request:?}{response:?}");
        assert!(rendered.contains("secret_id_present: true"));
        assert!(rendered.contains("input_count: 1"));
        assert!(rendered.contains("embedding_count: 1"));
        for private in [
            secret_id.to_string(),
            "openai-private-provider".to_string(),
            "private-embedding-model".to_string(),
            "private embedding input".to_string(),
            "0.123456".to_string(),
        ] {
            assert!(!rendered.contains(&private));
        }
    }

    #[test]
    #[cfg(feature = "hosted-auth")]
    fn hosted_model_policy_is_profile_and_capability_specific() {
        assert!(hosted_model_allowed("openai", "gpt-4o-mini", false));
        assert!(hosted_model_allowed(
            "openai",
            "text-embedding-3-small",
            true
        ));
        assert!(!hosted_model_allowed(
            "openai",
            "caller-controlled-model",
            false
        ));
        assert!(!hosted_model_allowed(
            "openrouter",
            "text-embedding-3-small",
            true
        ));

        let profile = matric_inference::provider_profiles::lookup("openai").unwrap();
        let models = approved_profile_models(profile);
        assert!(models.contains(&"gpt-4o-mini".to_string()));
        assert!(models.contains(&"text-embedding-3-small".to_string()));
        assert_eq!(models.len(), models.iter().collect::<HashSet<_>>().len());
    }

    #[tokio::test]
    async fn unavailable_inference_usage_is_replay_safe_and_privacy_bounded() {
        let meter = matric_core::InMemoryMeter::default();
        let request_id = Uuid::now_v7();
        let context = InferenceUsageContext {
            meter: Arc::new(meter.clone()),
            subject: UsageSubject::unknown()
                .with_client("resolved-client")
                .unwrap(),
            request_id: Some(request_id.to_string()),
            provider: Some("openai"),
            event_time: Utc::now(),
            input_event_id: Uuid::now_v7(),
            output_event_id: Uuid::now_v7(),
        };

        context.record(UsageOutcome::Completed).await;
        let events = meter.events().await;

        assert_eq!(events.len(), 2);
        for event in &events {
            assert!(matches!(
                event.dimension,
                UsageDimension::InferenceInputTokens | UsageDimension::InferenceOutputTokens
            ));
            assert_eq!(
                event.measurement,
                UsageMeasurement::Unavailable {
                    unit: UsageUnit::Token
                }
            );
            assert_eq!(event.class, UsageClass::BillableActual);
            assert_eq!(event.producer, UsageProducer::Inference);
            assert_eq!(event.source, UsageSource::Unavailable);
            assert_eq!(event.outcome, UsageOutcome::Completed);
            assert_eq!(
                event.correlation.request_id(),
                Some(request_id.to_string().as_str())
            );
            assert_eq!(
                event.attrs.get(UsageAttributeKey::Provider),
                Some(&UsageAttributeValue::Label("openai".to_string()))
            );
            assert!(event.idempotency_key.contains(&event.event_id.to_string()));

            meter
                .record(event)
                .await
                .expect("exact inference usage replay must be idempotent");
        }
        assert_eq!(meter.events().await.len(), 2);

        let encoded = serde_json::to_string(&events).unwrap();
        for forbidden in [
            "patient prompt",
            "private completion",
            "sk-provider-secret",
            "https://provider.example/v1",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn arbitrary_provider_input_is_not_exported_as_usage_metadata() {
        let raw_provider =
            "openai?api_key=sk-secret&callback=https://user:pass@provider.example/v1";
        let meter = matric_core::InMemoryMeter::default();
        let context = InferenceUsageContext {
            meter: Arc::new(meter.clone()),
            subject: UsageSubject::unknown(),
            request_id: None,
            provider: matric_inference::provider_profiles::lookup(raw_provider)
                .map(|profile| profile.id),
            event_time: Utc::now(),
            input_event_id: Uuid::now_v7(),
            output_event_id: Uuid::now_v7(),
        };

        context.record(UsageOutcome::Completed).await;
        let events = meter.events().await;

        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|event| event.attrs.get(UsageAttributeKey::Provider).is_none()));
        let encoded = serde_json::to_string(&events).unwrap();
        assert!(!encoded.contains(raw_provider));
        assert!(!encoded.contains("sk-secret"));
        assert!(!encoded.contains("provider.example"));
        assert!(!encoded.contains("user:pass"));
    }

    #[tokio::test]
    async fn streaming_terminal_outcomes_remain_distinct() {
        for expected in [
            UsageOutcome::Completed,
            UsageOutcome::ClientInterrupted,
            UsageOutcome::ProviderInterrupted,
            UsageOutcome::FailedAfterPartialUsage,
        ] {
            let meter = matric_core::InMemoryMeter::default();
            let context = InferenceUsageContext {
                meter: Arc::new(meter.clone()),
                subject: UsageSubject::anonymous("stream-test").unwrap(),
                request_id: Some(Uuid::now_v7().to_string()),
                provider: Some("ollama"),
                event_time: Utc::now(),
                input_event_id: Uuid::now_v7(),
                output_event_id: Uuid::now_v7(),
            };

            context.record(expected).await;
            let events = meter.events().await;

            assert_eq!(events.len(), 2);
            assert!(events.iter().all(|event| event.outcome == expected));
        }
    }
}

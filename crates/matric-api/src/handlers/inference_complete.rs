//! Provider-agnostic chat completion endpoints (Issue #628).
//!
//! Stateless `POST /api/v1/inference/complete` and `/stream` that route to
//! any registered or BYOK provider. Per-request `api_key` + `base_url` in
//! the body allow downstream clients to pass user-supplied keys
//! without server-side persistence.
//!
//! Plus `GET /api/v1/inference/providers` reporting what's available based
//! on env config + a live Ollama probe.

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tracing::{debug, error, info, warn};

use crate::{ApiError, AppState};

const INFERENCE_COMPLETION_PROVIDER_DETAIL: &str =
    "Inference completion backend failed. Check server logs for diagnostics.";

// =============================================================================
// REQUEST + RESPONSE TYPES
// =============================================================================

/// A single chat message — `{role, content}`.
#[derive(Clone, Deserialize, Serialize)]
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
#[derive(Clone, Deserialize)]
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
            .field("model_len", &complete_text_len(&self.model))
            .field("message_count", &self.messages.len())
            .field("message_content_chars", &message_content_chars)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("think", &self.think)
            .finish()
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
#[derive(Clone, Serialize)]
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

/// One entry in the `/providers` response.
#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderInfo>,
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
pub async fn list_providers(State(state): State<AppState>) -> impl IntoResponse {
    use matric_inference::provider_profiles;

    let registry = state.provider_registry();
    let mut providers = Vec::new();

    for profile in provider_profiles::iter() {
        // server_configured: the registry was built from env at startup, so
        // a profile registered there had its credentials/base URL detected.
        // For keyless providers (Ollama, llama.cpp) just being registered is
        // sufficient; for keyed providers the api_key must be present.
        let registered = registry.get_provider(profile.id);
        let server_configured = match registered {
            Some(cfg) => !profile.requires_api_key || cfg.api_key.is_some(),
            None => false,
        };

        // Use the registered base URL when available so operators see the
        // effective configured value; fall back to the profile's documented
        // default for the BYOK render path.
        let base_url = registered
            .map(|c| c.base_url.clone())
            .or_else(|| profile.default_base_url.map(String::from))
            .unwrap_or_default();

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
            requires_user_key: profile.requires_api_key && !server_configured,
            supports_embeddings: profile.supports_embeddings(),
        });
    }

    Json(ProvidersResponse { providers })
}

/// `POST /api/v1/inference/complete` — provider-agnostic chat completion.
///
/// Stateless: builds a fresh backend from request-time creds (or registered
/// config or env), runs one generate call, returns the result.
pub async fn complete(
    State(state): State<AppState>,
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

    let registry = state.provider_registry();
    let backend = match registry.resolve_generation_inline(
        &provider_id,
        req.api_key.as_deref(),
        req.base_url.as_deref(),
        &req.model,
    ) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                provider_id_len = complete_text_len(&provider_id),
                error_len = complete_text_len(&e.to_string()),
                "Failed to resolve inline backend"
            );
            return Err(ApiError::BadRequest(
                "Provider resolution failed. Check provider id and credentials.".to_string(),
            )
            .into_response());
        }
    };

    let (system, prompt) = flatten_messages(&req.messages);

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
            info!(
                provider_id_len = complete_text_len(&provider_id),
                model_len = complete_text_len(&req.model),
                content_len = complete_text_len(&content),
                "Completion succeeded"
            );
            Ok(Json(CompleteResponse {
                content,
                finish_reason: "stop".to_string(),
                model: req.model,
                provider_id,
            }))
        }
        Err(e) => {
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

/// `POST /api/v1/inference/stream` — same shape as `/complete`, returns SSE.
///
/// Uses `GenerationBackend::stream_generate[_with_system]` (#629). For
/// backends that override with real token streaming (Ollama), this emits
/// one `delta` event per NDJSON chunk from upstream. Backends that still
/// use the trait default fall back to a single large `delta` (wire
/// compatible, just not progressive).
pub async fn stream(
    State(state): State<AppState>,
    Json(req): Json<CompleteRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, axum::response::Response> {
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

    let registry = state.provider_registry();
    let backend = match registry.resolve_generation_inline(
        &provider_id,
        req.api_key.as_deref(),
        req.base_url.as_deref(),
        &req.model,
    ) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                provider_id_len = complete_text_len(&provider_id),
                error_len = complete_text_len(&e.to_string()),
                "Failed to resolve inline stream backend"
            );
            return Err(ApiError::BadRequest(
                "Provider resolution failed. Check provider id and credentials.".to_string(),
            )
            .into_response());
        }
    };

    let (system, prompt) = flatten_messages(&req.messages);

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
                                // Receiver closed — client disconnected.
                                return;
                            }
                        }
                        Err(e) => {
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
    Ok(Sse::new(event_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
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
        let req = CompleteRequest {
            provider_id: Some("openai".to_string()),
            api_key: Some("sk-secret-provider-key".to_string()),
            base_url: Some("https://user:pass@api.openai.com/v1?token=secret".to_string()),
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
    }
}

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
use axum::http::StatusCode;
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

use crate::AppState;

// =============================================================================
// REQUEST + RESPONSE TYPES
// =============================================================================

/// A single chat message — `{role, content}`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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
#[derive(Debug, Clone, Deserialize)]
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

/// Response body for `/complete`.
#[derive(Debug, Clone, Serialize)]
pub struct CompleteResponse {
    pub content: String,
    pub finish_reason: String,
    pub model: String,
    pub provider_id: String,
}

/// One entry in the `/providers` response.
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

/// Standard error envelope for all three endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceError {
    pub error: String,
    pub code: String,
}

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
) -> Result<Json<CompleteResponse>, (StatusCode, Json<InferenceError>)> {
    let provider_id = req
        .provider_id
        .clone()
        .unwrap_or_else(|| "ollama".to_string());

    // Validate input.
    if req.messages.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "messages array is empty",
            "EMPTY_MESSAGES",
        ));
    }
    if req.model.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "model is required",
            "MODEL_REQUIRED",
        ));
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
                provider_id = %provider_id,
                error = %e,
                "Failed to resolve inline backend"
            );
            return Err(error(
                StatusCode::BAD_REQUEST,
                &format!("provider resolution failed: {}", e),
                "PROVIDER_RESOLUTION_FAILED",
            ));
        }
    };

    let (system, prompt) = flatten_messages(&req.messages);

    debug!(
        provider_id = %provider_id,
        model = %req.model,
        prompt_len = prompt.len(),
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
                provider_id = %provider_id,
                model = %req.model,
                content_len = content.len(),
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
                provider_id = %provider_id,
                model = %req.model,
                error = %e,
                "Completion failed"
            );
            Err(error(
                StatusCode::BAD_GATEWAY,
                &format!("inference error: {}", e),
                "INFERENCE_FAILED",
            ))
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
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<InferenceError>)>
{
    use futures::StreamExt;

    let provider_id = req
        .provider_id
        .clone()
        .unwrap_or_else(|| "ollama".to_string());

    if req.messages.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "messages array is empty",
            "EMPTY_MESSAGES",
        ));
    }
    if req.model.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "model is required",
            "MODEL_REQUIRED",
        ));
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
            return Err(error(
                StatusCode::BAD_REQUEST,
                &format!("provider resolution failed: {}", e),
                "PROVIDER_RESOLUTION_FAILED",
            ));
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
                            let err_payload = serde_json::json!({
                                "error": e.to_string(),
                                "code": "INFERENCE_FAILED",
                            })
                            .to_string();
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
                let err_payload = serde_json::json!({
                    "error": e.to_string(),
                    "code": "INFERENCE_FAILED",
                })
                .to_string();
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

fn error(status: StatusCode, msg: &str, code: &str) -> (StatusCode, Json<InferenceError>) {
    (
        status,
        Json(InferenceError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

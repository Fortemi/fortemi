//! Synchronous chat handler — calls Ollama directly, bypassing the job queue.
//!
//! GPU availability is gated by a `tokio::Semaphore` in AppState. When all
//! permits are taken (by other chat requests or concurrent GPU usage), the
//! endpoint returns 503 immediately rather than queuing.

use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

use crate::AppState;
use matric_inference::discovery::ModelDiscovery;
use matric_inference::profiles::ModelRegistry;
use matric_inference::OllamaBackend;

/// Bounded capacity of the SSE event channel for a streaming chat response.
/// Large enough that a reasonably-paced client never sees drops, small enough
/// to bound memory if a client stalls.
const CHAT_STREAM_CHANNEL_CAPACITY: usize = 256;

// =============================================================================
// REQUEST / RESPONSE TYPES
// =============================================================================

/// Chat request matching the HotM UI contract.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChatRequest {
    /// The user's message text.
    pub input: String,
    /// Optional model slug override. If omitted, uses the server's default generation model.
    /// Must be an installed Ollama model with "language" capability.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional context for RAG-style grounding.
    #[serde(default)]
    pub context: Option<ChatContext>,
}

/// Contextual information for grounding the chat response.
///
/// Fields like `note_id`, `collection_id`, `search_query` are part of the HotM
/// contract and deserialized for future RAG integration.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ChatContext {
    /// Note ID to ground the response in.
    #[serde(default)]
    pub note_id: Option<String>,
    /// Collection ID to scope context.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Search query to fetch relevant notes.
    #[serde(default)]
    pub search_query: Option<String>,
    /// Full conversation history for multi-turn.
    #[serde(default)]
    pub conversation_history: Option<Vec<ChatMessage>>,
}

/// A single message in the conversation.
#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema)]
pub struct ChatMessage {
    /// Role: "system", "user", or "assistant".
    pub role: String,
    /// Message content.
    pub content: String,
    /// Optional timestamp (ISO 8601) — passed through but not used server-side.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timestamp: Option<String>,
}

/// Chat response matching the HotM UI contract.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// Response messages (typically one assistant message).
    pub messages: Vec<ChatMessage>,
    /// Actions the UI should perform (empty for now — future: search_notes, create_note, etc.).
    pub actions: Vec<ChatAction>,
    /// Information about the model that produced this response.
    pub model_info: ChatModelInfo,
}

/// Placeholder for future UI-driven actions.
#[derive(Debug, Serialize)]
pub struct ChatAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub payload: serde_json::Value,
}

/// Model metadata included in every chat response so clients can display
/// context budget, thinking capability, and speed expectations.
#[derive(Debug, Serialize)]
pub struct ChatModelInfo {
    /// Model slug used for this request (e.g., "qwen3:8b").
    pub model: String,
    /// Native context window in tokens (0 if unknown).
    pub context_window: usize,
    /// Estimated available context after system prompt overhead (tokens).
    pub estimated_available_context: usize,
    /// Maximum output tokens the model can generate (0 if unknown).
    pub max_output_tokens: usize,
    /// Whether this model has thinking/reasoning capability.
    pub supports_thinking: bool,
    /// Thinking type label (e.g., "explicit_tags", "verbose_reasoning", "none").
    pub thinking_type: String,
    /// Output speed in tokens/sec (0.0 if unknown).
    pub speed_tok_s: f32,
    /// Model parameter size (e.g., "8.2B") if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_size: Option<String>,
    /// Model family (e.g., "qwen3", "llama") if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

// =============================================================================
// SYSTEM PROMPT
// =============================================================================

const SYSTEM_PROMPT: &str = "\
You are a knowledgeable assistant integrated into a personal knowledge base. \
Your role is to help the user understand, explore, and build on their stored knowledge.

Guidelines:
- Be concise and direct. Avoid filler phrases.
- When context from notes is provided, ground your answers in that context.
- If you don't know something, say so rather than guessing.
- Use markdown formatting when helpful (lists, code blocks, headers).
- Respect the conversational tone — this is an interactive chat, not a document.";

/// Overhead tokens consumed by the system prompt (rough estimate: ~4 chars/token).
const SYSTEM_PROMPT_OVERHEAD_TOKENS: usize = 200;

// =============================================================================
// HANDLER
// =============================================================================

/// POST /api/v1/chat — synchronous LLM chat.
///
/// Acquires a GPU semaphore permit (non-blocking), calls Ollama directly,
/// and returns the response. Returns 503 if no permits available.
/// Supports optional model selection via the `model` field — must be an
/// installed Ollama model with language capability. Response includes
/// model metadata (context window, thinking support, speed).
#[utoipa::path(
    post,
    path = "/api/v1/chat",
    tag = "Chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "Chat response"),
        (status = 400, description = "Invalid request"),
        (status = 503, description = "Chat unavailable or busy"),
    )
)]
pub async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    // 1. Validate input
    let input = req.input.trim();
    if input.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "input must not be empty"})),
        )
            .into_response();
    }

    // 2. Check backend is configured
    let backend = match state.generation_backend() {
        Some(b) => b,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Chat not configured — Ollama generation backend is not available",
                    "retry_after": 30
                })),
            )
                .into_response();
        }
    };

    // 2b. Check the provider is currently reachable (#630).
    // The periodic reachability probe sets this flag; if the provider (e.g.
    // Ollama) is down or not yet started, fail fast with a retry hint rather
    // than blocking on a generation request that will time out.
    if !state.inference_available.load(Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Chat provider not reachable — retry after inference provider starts",
                "retry_after": 30
            })),
        )
            .into_response();
    }

    // 3. Try to acquire a semaphore permit (non-blocking)
    let semaphore = match &state.chat_semaphore {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Chat not configured",
                    "retry_after": 30
                })),
            )
                .into_response();
        }
    };

    let _permit = match semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Chat service is currently at capacity. All GPU inference threads are busy processing requests. Please retry shortly.",
                    "retry_after": 5
                })),
            )
                .into_response();
        }
    };

    // 4. Resolve model — use requested model or fall back to server default
    let requested_model = req.model.as_deref();
    let chat_backend = if let Some(model_slug) = requested_model {
        // Validate the model is installed and capable of chat
        match validate_chat_model(model_slug, &state).await {
            Ok(()) => {
                let mut b = OllamaBackend::from_env();
                b.set_gen_model(model_slug.to_string());
                std::sync::Arc::new(b)
            }
            Err(err_response) => return err_response,
        }
    } else {
        backend.clone()
    };

    // 5. Look up model profile for metadata
    let registry = ModelRegistry::new();
    let model_name = chat_backend.gen_model_name();
    let model_info = build_model_info(model_name, &registry);

    // 6. Build conversation messages
    let mut messages: Vec<(String, String)> = Vec::new();

    // System prompt
    messages.push(("system".to_string(), SYSTEM_PROMPT.to_string()));

    // Conversation history (if provided)
    if let Some(ref ctx) = req.context {
        if let Some(ref history) = ctx.conversation_history {
            for msg in history {
                messages.push((msg.role.clone(), msg.content.clone()));
            }
        }
    }

    // Current user message (always appended last)
    messages.push(("user".to_string(), input.to_string()));

    debug!(
        model = model_name,
        message_count = messages.len(),
        "Starting chat request"
    );

    // 7. Call Ollama
    match chat_backend.chat_multi_turn(messages).await {
        Ok(content) => {
            info!(
                model = model_name,
                response_len = content.len(),
                "Chat response generated"
            );

            let response = ChatResponse {
                messages: vec![ChatMessage {
                    role: "assistant".to_string(),
                    content,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                }],
                actions: vec![],
                model_info,
            };

            (
                StatusCode::OK,
                Json(serde_json::to_value(response).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            warn!(error = %e, model = model_name, "Chat generation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Generation failed: {}", e),
                })),
            )
                .into_response()
        }
    }
    // _permit dropped here — semaphore slot released
}

// =============================================================================
// MODEL VALIDATION
// =============================================================================

/// Validate that the requested model slug is installed on Ollama and is a
/// language-capable model (not embedding-only or vision-only).
async fn validate_chat_model(
    model_slug: &str,
    _state: &AppState,
) -> Result<(), axum::response::Response> {
    let ollama_base_url = std::env::var("OLLAMA_BASE")
        .or_else(|_| std::env::var("OLLAMA_URL"))
        .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());

    let discovery = ModelDiscovery::new(&ollama_base_url);
    let models = match discovery.discover_models().await {
        Ok(result) => result,
        Err(e) => {
            warn!(error = %e, "Failed to discover models for chat validation");
            // If discovery fails, allow the request through — Ollama will reject if invalid
            return Ok(());
        }
    };

    // Check model is installed
    let found = models.models.iter().any(|m| m.name == model_slug);
    if !found {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Model '{}' is not installed on Ollama", model_slug),
                "available_models": models.generation_models,
            })),
        )
            .into_response());
    }

    // Check it's not an embedding-only model
    let is_embed_only = models
        .models
        .iter()
        .find(|m| m.name == model_slug)
        .map(|m| m.is_likely_embedding())
        .unwrap_or(false);

    if is_embed_only {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Model '{}' is an embedding model and cannot be used for chat", model_slug),
                "available_models": models.generation_models,
            })),
        )
            .into_response());
    }

    Ok(())
}

/// Build ChatModelInfo from the profile registry (if known) or with sensible defaults.
fn build_model_info(model_name: &str, registry: &ModelRegistry) -> ChatModelInfo {
    match registry.get(model_name) {
        Some(profile) => ChatModelInfo {
            model: model_name.to_string(),
            context_window: profile.native_context,
            estimated_available_context: profile
                .native_context
                .saturating_sub(SYSTEM_PROMPT_OVERHEAD_TOKENS),
            max_output_tokens: profile.max_output,
            supports_thinking: profile.is_thinking_model(),
            thinking_type: serde_json::to_value(profile.thinking_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "unknown".to_string()),
            speed_tok_s: profile.speed_tok_s,
            parameter_size: Some(profile.size.clone()),
            family: Some(profile.family.clone()),
        },
        None => ChatModelInfo {
            model: model_name.to_string(),
            context_window: 0,
            estimated_available_context: 0,
            max_output_tokens: 0,
            supports_thinking: false,
            thinking_type: "unknown".to_string(),
            speed_tok_s: 0.0,
            parameter_size: None,
            family: None,
        },
    }
}

// =============================================================================
// STREAMING CHAT (Issue #812, #814)
// =============================================================================

/// Counters for the streaming chat endpoint, exposed on `/health/streaming`
/// (#814). All counters are monotonic and process-lifetime.
#[derive(Debug, Default)]
pub struct ChatStreamMetrics {
    /// Total streaming chat requests that began streaming (permit acquired).
    pub streams_started: AtomicU64,
    /// Streams that ran to natural completion (`done` event emitted).
    pub streams_completed: AtomicU64,
    /// Streams that ended with a generation/transport error.
    pub streams_errored: AtomicU64,
    /// Streams cut short because the client disconnected or stalled.
    pub client_disconnects: AtomicU64,
    /// Total content chunks ("tokens") successfully delivered to clients.
    pub tokens_streamed_total: AtomicU64,
    /// Total content chunks generated but NOT delivered — dropped because the
    /// client disconnected or could not drain the buffer within the send
    /// window. This is the `chat_stream_dropped_tokens_total` metric (#814).
    pub dropped_tokens_total: AtomicU64,
}

impl ChatStreamMetrics {
    /// Snapshot all counters as a JSON object for `/health/streaming`.
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "chat_stream_started_total": {
                "type": "counter",
                "value": self.streams_started.load(Ordering::Relaxed)
            },
            "chat_stream_completed_total": {
                "type": "counter",
                "value": self.streams_completed.load(Ordering::Relaxed)
            },
            "chat_stream_errored_total": {
                "type": "counter",
                "value": self.streams_errored.load(Ordering::Relaxed)
            },
            "chat_stream_client_disconnect_total": {
                "type": "counter",
                "value": self.client_disconnects.load(Ordering::Relaxed)
            },
            "chat_stream_tokens_total": {
                "type": "counter",
                "value": self.tokens_streamed_total.load(Ordering::Relaxed)
            },
            "chat_stream_dropped_tokens_total": {
                "type": "counter",
                "value": self.dropped_tokens_total.load(Ordering::Relaxed)
            },
        })
    }
}

/// Per-chunk send window. If a content chunk cannot be handed to the SSE
/// channel within this window (client not draining), remaining tokens are
/// shed rather than holding the GPU permit indefinitely. Override with
/// `CHAT_STREAM_SEND_TIMEOUT_SECS`.
fn chat_stream_send_timeout() -> Duration {
    let secs = std::env::var("CHAT_STREAM_SEND_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(30);
    Duration::from_secs(secs)
}

fn service_unavailable(msg: &str, retry_after: u64) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "error": msg, "retry_after": retry_after })),
    )
        .into_response()
}

/// One Server-Sent Event frame produced by the streaming chat pump.
///
/// Kept as a plain struct rather than axum's opaque `Event` so the pump's
/// framing (`delta`/`done`/`error`) and JSON payloads are assertable in unit
/// tests (#816). The handler maps each frame to an SSE [`Event`] at the
/// channel boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatStreamFrame {
    /// SSE event name: `"delta"`, `"done"`, or `"error"`.
    pub event: &'static str,
    /// JSON-encoded event payload.
    pub data: String,
}

impl ChatStreamFrame {
    fn delta(content: &str) -> Self {
        Self {
            event: "delta",
            data: serde_json::json!({ "content": content }).to_string(),
        }
    }

    fn done(model_name: &str) -> Self {
        Self {
            event: "done",
            data: serde_json::json!({ "finish_reason": "stop", "model": model_name }).to_string(),
        }
    }

    fn error(message: String) -> Self {
        Self {
            event: "error",
            data: serde_json::json!({ "error": message, "code": "GENERATION_FAILED" }).to_string(),
        }
    }

    fn into_event(self) -> Event {
        Event::default().event(self.event).data(self.data)
    }
}

/// Pump a backend content stream into the SSE frame channel, emitting one
/// `delta` frame per content chunk, a terminal `done` frame on completion, or
/// an `error` frame on failure — while accounting delivered vs dropped tokens
/// in `metrics`.
///
/// Extracted from the handler so the SSE framing, terminator, error path, and
/// backpressure/dropped-token accounting are unit-testable without a live
/// model (#816).
async fn pump_chat_stream<S>(
    mut chunks: S,
    tx: mpsc::Sender<ChatStreamFrame>,
    metrics: Arc<ChatStreamMetrics>,
    model_name: String,
    send_timeout: Duration,
) where
    S: futures::Stream<Item = matric_core::Result<String>> + Unpin,
{
    let mut delivered: u64 = 0;
    let mut dropped: u64 = 0;

    while let Some(item) = chunks.next().await {
        match item {
            Ok(content) => {
                if content.is_empty() {
                    continue;
                }
                match tokio::time::timeout(send_timeout, tx.send(ChatStreamFrame::delta(&content)))
                    .await
                {
                    Ok(Ok(())) => {
                        delivered += 1;
                    }
                    Ok(Err(_closed)) => {
                        // Receiver dropped — client disconnected mid-stream.
                        dropped += 1;
                        record_disconnect(&metrics, delivered, dropped);
                        return;
                    }
                    Err(_elapsed) => {
                        // Client is not draining the SSE buffer within the send
                        // window — shed the remaining token rather than hold the
                        // GPU permit indefinitely (#814 backpressure).
                        dropped += 1;
                        record_disconnect(&metrics, delivered, dropped);
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(ChatStreamFrame::error(e.to_string())).await;
                metrics.streams_errored.fetch_add(1, Ordering::Relaxed);
                metrics
                    .tokens_streamed_total
                    .fetch_add(delivered, Ordering::Relaxed);
                return;
            }
        }
    }

    // Stream exhausted normally — emit the terminal `done` frame.
    let _ = tx.send(ChatStreamFrame::done(&model_name)).await;
    metrics.streams_completed.fetch_add(1, Ordering::Relaxed);
    metrics
        .tokens_streamed_total
        .fetch_add(delivered, Ordering::Relaxed);
}

/// Record the metrics for a stream that ended early because the client
/// disconnected or stalled.
fn record_disconnect(metrics: &ChatStreamMetrics, delivered: u64, dropped: u64) {
    metrics.client_disconnects.fetch_add(1, Ordering::Relaxed);
    metrics
        .tokens_streamed_total
        .fetch_add(delivered, Ordering::Relaxed);
    metrics
        .dropped_tokens_total
        .fetch_add(dropped, Ordering::Relaxed);
}

/// POST /api/v1/chat/stream — streaming LLM chat over Server-Sent Events.
///
/// Identical request contract to [`chat_handler`] (input, optional model,
/// optional context with conversation history), but streams the assistant
/// response progressively. Acquires an owned GPU semaphore permit held for the
/// full stream lifetime; returns 503 immediately if no permits are available.
///
/// SSE event shape (consistent with `POST /api/v1/inference/stream`):
/// - `delta` — `{"content": "<chunk>"}` per content chunk
/// - `done`  — `{"finish_reason": "stop", "model": "<slug>"}` on completion
/// - `error` — `{"error": "<msg>", "code": "GENERATION_FAILED"}` on failure
#[utoipa::path(
    post,
    path = "/api/v1/chat/stream",
    tag = "Chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "SSE stream of assistant tokens (delta/done/error events)"),
        (status = 400, description = "Invalid request"),
        (status = 503, description = "Chat unavailable or busy"),
    )
)]
pub async fn chat_stream_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Response {
    // 1. Validate input
    let input = req.input.trim();
    if input.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "input must not be empty"})),
        )
            .into_response();
    }

    // 2. Backend configured?
    let backend = match state.generation_backend() {
        Some(b) => b,
        None => {
            return service_unavailable(
                "Chat not configured — Ollama generation backend is not available",
                30,
            )
        }
    };

    // 2b. Provider reachable? (#630)
    if !state.inference_available.load(Ordering::Relaxed) {
        return service_unavailable(
            "Chat provider not reachable — retry after inference provider starts",
            30,
        );
    }

    // 3. Acquire an OWNED permit — held for the full stream lifetime.
    let semaphore = match &state.chat_semaphore {
        Some(s) => s.clone(),
        None => return service_unavailable("Chat not configured", 30),
    };
    let permit = match semaphore.try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return service_unavailable(
                "Chat service is currently at capacity. All GPU inference threads are busy processing requests. Please retry shortly.",
                5,
            )
        }
    };

    // 4. Resolve model — requested or server default.
    let requested_model = req.model.as_deref();
    let chat_backend = if let Some(model_slug) = requested_model {
        match validate_chat_model(model_slug, &state).await {
            Ok(()) => {
                let mut b = OllamaBackend::from_env();
                b.set_gen_model(model_slug.to_string());
                Arc::new(b)
            }
            Err(err_response) => return err_response,
        }
    } else {
        backend.clone()
    };
    let model_name = chat_backend.gen_model_name().to_string();

    // 5. Build conversation messages (same shape as chat_handler).
    let mut messages: Vec<(String, String)> = Vec::new();
    messages.push(("system".to_string(), SYSTEM_PROMPT.to_string()));
    if let Some(ref ctx) = req.context {
        if let Some(ref history) = ctx.conversation_history {
            for msg in history {
                messages.push((msg.role.clone(), msg.content.clone()));
            }
        }
    }
    messages.push(("user".to_string(), input.to_string()));

    debug!(
        model = %model_name,
        message_count = messages.len(),
        "Starting streaming chat request"
    );

    // 6. Begin streaming.
    let metrics = state.chat_stream_metrics.clone();
    metrics.streams_started.fetch_add(1, Ordering::Relaxed);

    let (tx, rx) = mpsc::channel::<ChatStreamFrame>(CHAT_STREAM_CHANNEL_CAPACITY);
    let send_timeout = chat_stream_send_timeout();

    tokio::spawn(async move {
        // The owned permit lives until this task ends, releasing the GPU slot
        // only when the stream completes, errors, or the client disconnects.
        let _permit = permit;

        match chat_backend.chat_multi_turn_stream(messages).await {
            Ok(chunks) => {
                pump_chat_stream(chunks, tx, metrics, model_name, send_timeout).await;
            }
            Err(e) => {
                warn!(error = %e, model = %model_name, "Streaming chat failed to start");
                let _ = tx
                    .send(ChatStreamFrame::error(format!("Generation failed: {}", e)))
                    .await;
                metrics.streams_errored.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let event_stream =
        ReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(event_stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

// =============================================================================
// LIST CHAT MODELS
// =============================================================================

/// Response from GET /api/v1/chat/models.
#[derive(Debug, Serialize)]
pub struct ListChatModelsResponse {
    /// All installed Ollama models capable of chat (excludes embedding-only models).
    pub models: Vec<ChatModelInfo>,
    /// The server's default chat model slug.
    pub default_model: String,
}

/// GET /api/v1/chat/models — list installed models available for chat.
///
/// Returns every installed Ollama model that supports text generation (i.e., not
/// embedding-only), enriched with context window, thinking capability, speed,
/// and parameter size from the model profile registry.
#[utoipa::path(
    get,
    path = "/api/v1/chat/models",
    tag = "Chat",
    responses(
        (status = 200, description = "Available chat models"),
        (status = 503, description = "Ollama not reachable"),
    )
)]
pub async fn list_chat_models(State(state): State<AppState>) -> impl IntoResponse {
    let ollama_base_url = std::env::var("OLLAMA_BASE")
        .or_else(|_| std::env::var("OLLAMA_URL"))
        .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());

    let discovery = ModelDiscovery::new(&ollama_base_url);
    let discovered = match discovery.discover_models().await {
        Ok(result) => result,
        Err(e) => {
            warn!(error = %e, "Failed to discover Ollama models for chat");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": format!("Cannot reach Ollama: {}", e),
                })),
            )
                .into_response();
        }
    };

    let registry = ModelRegistry::new();

    // Default model: from generation_backend if configured, else from env
    let default_model = state
        .generation_backend()
        .as_ref()
        .map(|b| b.gen_model_name().to_string())
        .unwrap_or_else(|| {
            std::env::var("OLLAMA_GEN_MODEL")
                .unwrap_or_else(|_| matric_core::defaults::GEN_MODEL.to_string())
        });

    let models: Vec<ChatModelInfo> = discovered
        .models
        .iter()
        .filter(|m| !m.is_likely_embedding())
        .map(|m| {
            let mut info = build_model_info(&m.name, &registry);
            // For models not in the profile registry, fill in what we can from Ollama discovery
            if info.parameter_size.is_none() {
                info.parameter_size = m.parameter_size.clone();
            }
            if info.family.is_none() {
                info.family = m.family.clone();
            }
            info
        })
        .collect();

    let response = ListChatModelsResponse {
        models,
        default_model,
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
        .into_response()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
    }

    #[test]
    fn test_build_model_info_known_model() {
        let registry = ModelRegistry::new();
        let info = build_model_info("qwen3:8b", &registry);
        assert_eq!(info.model, "qwen3:8b");
        assert!(info.context_window > 0);
        assert!(info.estimated_available_context > 0);
        assert!(info.estimated_available_context < info.context_window);
        assert!(info.max_output_tokens > 0);
        assert!(info.speed_tok_s > 0.0);
        assert!(info.family.is_some());
        assert!(info.parameter_size.is_some());
    }

    #[test]
    fn test_build_model_info_unknown_model() {
        let registry = ModelRegistry::new();
        let info = build_model_info("totally-unknown:latest", &registry);
        assert_eq!(info.model, "totally-unknown:latest");
        assert_eq!(info.context_window, 0);
        assert_eq!(info.estimated_available_context, 0);
        assert!(!info.supports_thinking);
        assert_eq!(info.thinking_type, "unknown");
    }

    #[test]
    fn test_build_model_info_thinking_model() {
        let registry = ModelRegistry::new();
        let info = build_model_info("deepseek-r1:14b", &registry);
        assert!(info.supports_thinking);
        assert_ne!(info.thinking_type, "none");
        assert_ne!(info.thinking_type, "unknown");
    }

    #[test]
    fn test_chat_request_deserialize_minimal() {
        let json = r#"{"input": "Hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, "Hello");
        assert!(req.model.is_none());
        assert!(req.context.is_none());
    }

    #[test]
    fn test_chat_request_deserialize_with_model() {
        let json = r#"{"input": "Hello", "model": "qwen3:8b"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model.as_deref(), Some("qwen3:8b"));
    }

    #[test]
    fn test_chat_request_deserialize_full() {
        let json = r#"{
            "input": "What is this about?",
            "model": "gpt-oss:20b",
            "context": {
                "note_id": "abc-123",
                "conversation_history": [
                    {"role": "user", "content": "Hi"},
                    {"role": "assistant", "content": "Hello!"}
                ]
            }
        }"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, "What is this about?");
        assert_eq!(req.model.as_deref(), Some("gpt-oss:20b"));
        let ctx = req.context.unwrap();
        assert_eq!(ctx.note_id.as_deref(), Some("abc-123"));
        let history = ctx.conversation_history.unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_chat_response_serialize() {
        let response = ChatResponse {
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "Hello!".to_string(),
                timestamp: Some("2026-02-27T00:00:00Z".to_string()),
            }],
            actions: vec![],
            model_info: ChatModelInfo {
                model: "qwen3:8b".to_string(),
                context_window: 40960,
                estimated_available_context: 40760,
                max_output_tokens: 4096,
                supports_thinking: false,
                thinking_type: "not_tested".to_string(),
                speed_tok_s: 144.3,
                parameter_size: Some("8.2B".to_string()),
                family: Some("qwen3".to_string()),
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Hello!"));
        assert!(json.contains("context_window"));
        assert!(json.contains("40960"));
        assert!(json.contains("qwen3:8b"));
    }

    #[test]
    fn test_chat_message_serialize_without_timestamp() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "test".to_string(),
            timestamp: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("timestamp"));
    }

    // =========================================================================
    // HotM Consumer Contract Tests (Issue #549)
    // =========================================================================

    // --- Request Deserialization Edge Cases ---

    /// Issue #549 test case #6: Empty context object — all fields undefined.
    #[test]
    fn test_chat_request_empty_context_object() {
        let json = r#"{"input": "hello", "context": {}}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, "hello");
        let ctx = req.context.unwrap();
        assert!(ctx.note_id.is_none());
        assert!(ctx.collection_id.is_none());
        assert!(ctx.search_query.is_none());
        assert!(ctx.conversation_history.is_none());
    }

    /// Issue #549 test case #7: Partial context — only note_id set.
    #[test]
    fn test_chat_request_partial_context_only_note_id() {
        let json = r#"{"input": "hello", "context": {"note_id": "abc-123"}}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        let ctx = req.context.unwrap();
        assert_eq!(ctx.note_id.as_deref(), Some("abc-123"));
        assert!(ctx.collection_id.is_none());
        assert!(ctx.search_query.is_none());
        assert!(ctx.conversation_history.is_none());
    }

    /// Issue #549 test case #8: Long conversation history (20+ messages).
    #[test]
    fn test_chat_request_long_conversation_history() {
        let mut messages = Vec::new();
        for i in 0..25 {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            messages.push(serde_json::json!({
                "role": role,
                "content": format!("Message {}", i)
            }));
        }
        let json = serde_json::json!({
            "input": "latest message",
            "context": {
                "conversation_history": messages
            }
        });
        let req: ChatRequest = serde_json::from_str(&json.to_string()).unwrap();
        let history = req.context.unwrap().conversation_history.unwrap();
        assert_eq!(history.len(), 25);
    }

    /// Issue #549 test case #9: Empty input string deserializes (handler rejects).
    #[test]
    fn test_chat_request_empty_input_deserializes() {
        let json = r#"{"input": ""}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, "");
        // Handler should reject this with 400 — tested at handler level.
    }

    /// Issue #549 test case #9 variant: Whitespace-only input deserializes.
    #[test]
    fn test_chat_request_whitespace_only_input_deserializes() {
        let json = r#"{"input": "   \t\n  "}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input.trim(), "");
        // Handler trims and rejects — tested at handler level.
    }

    /// Issue #549 test case #10: Large input (5000+ characters).
    #[test]
    fn test_chat_request_large_input() {
        let large_input = "x".repeat(6000);
        let json = serde_json::json!({"input": large_input});
        let req: ChatRequest = serde_json::from_str(&json.to_string()).unwrap();
        assert_eq!(req.input.len(), 6000);
    }

    /// Issue #549 test case #17: Missing `input` field — deserialization fails.
    #[test]
    fn test_chat_request_missing_input_field() {
        let json = r#"{"context": {"note_id": "abc"}}"#;
        let result = serde_json::from_str::<ChatRequest>(json);
        assert!(
            result.is_err(),
            "Missing `input` should fail deserialization"
        );
    }

    /// Issue #549 test case #16: Invalid JSON body — deserialization fails.
    #[test]
    fn test_chat_request_invalid_json() {
        let json = r#"{not valid json"#;
        let result = serde_json::from_str::<ChatRequest>(json);
        assert!(result.is_err());
    }

    /// Context fields explicitly set to null should deserialize as None.
    #[test]
    fn test_chat_request_context_with_explicit_nulls() {
        let json = r#"{
            "input": "hi",
            "context": {
                "note_id": null,
                "collection_id": null,
                "search_query": null,
                "conversation_history": null
            }
        }"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        let ctx = req.context.unwrap();
        assert!(ctx.note_id.is_none());
        assert!(ctx.collection_id.is_none());
        assert!(ctx.search_query.is_none());
        assert!(ctx.conversation_history.is_none());
    }

    /// HotM sends full context with all fields populated.
    #[test]
    fn test_chat_request_full_hotm_payload() {
        let json = r#"{
            "input": "find notes about quantum computing",
            "context": {
                "note_id": "uuid-of-active-note",
                "collection_id": "uuid-of-active-collection",
                "search_query": "last search the user ran",
                "conversation_history": [
                    {"role": "user", "content": "previous message"},
                    {"role": "assistant", "content": "previous reply"},
                    {"role": "user", "content": "find notes about quantum computing"}
                ]
            }
        }"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, "find notes about quantum computing");
        let ctx = req.context.unwrap();
        assert_eq!(ctx.note_id.as_deref(), Some("uuid-of-active-note"));
        assert_eq!(
            ctx.collection_id.as_deref(),
            Some("uuid-of-active-collection")
        );
        assert_eq!(
            ctx.search_query.as_deref(),
            Some("last search the user ran")
        );
        let history = ctx.conversation_history.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[2].content, "find notes about quantum computing");
    }

    /// ChatMessage with optional timestamp field populated.
    #[test]
    fn test_chat_message_with_timestamp() {
        let json = r#"{"role": "user", "content": "hello", "timestamp": "2026-02-27T14:00:00Z"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.timestamp.as_deref(), Some("2026-02-27T14:00:00Z"));
    }

    /// ChatMessage without timestamp deserializes fine.
    #[test]
    fn test_chat_message_without_timestamp_deserializes() {
        let json = r#"{"role": "assistant", "content": "response"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert!(msg.timestamp.is_none());
    }

    // --- Response Contract Validation ---

    /// Issue #549: Verify response JSON field names exactly match HotM contract.
    /// HotM expects: `messages`, `actions`, `model_info`.
    #[test]
    fn test_chat_response_contract_field_names() {
        let response = ChatResponse {
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "test".to_string(),
                timestamp: Some("2026-02-27T00:00:00Z".to_string()),
            }],
            actions: vec![],
            model_info: ChatModelInfo {
                model: "test:latest".to_string(),
                context_window: 4096,
                estimated_available_context: 3896,
                max_output_tokens: 2048,
                supports_thinking: false,
                thinking_type: "none".to_string(),
                speed_tok_s: 50.0,
                parameter_size: None,
                family: None,
            },
        };
        let val: serde_json::Value = serde_json::to_value(&response).unwrap();
        let obj = val.as_object().unwrap();

        // Top-level fields HotM expects
        assert!(obj.contains_key("messages"), "missing 'messages' field");
        assert!(obj.contains_key("actions"), "missing 'actions' field");
        assert!(obj.contains_key("model_info"), "missing 'model_info' field");

        // Message fields
        let msg = &obj["messages"][0];
        assert!(msg.get("role").is_some(), "message missing 'role'");
        assert!(msg.get("content").is_some(), "message missing 'content'");
        assert!(
            msg.get("timestamp").is_some(),
            "message missing 'timestamp'"
        );

        // model_info fields
        let mi = obj["model_info"].as_object().unwrap();
        assert!(mi.contains_key("model"));
        assert!(mi.contains_key("context_window"));
        assert!(mi.contains_key("estimated_available_context"));
        assert!(mi.contains_key("max_output_tokens"));
        assert!(mi.contains_key("supports_thinking"));
        assert!(mi.contains_key("thinking_type"));
        assert!(mi.contains_key("speed_tok_s"));
    }

    /// Issue #549: Response with actions populated.
    #[test]
    fn test_chat_response_with_actions() {
        let response = ChatResponse {
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "Found notes".to_string(),
                timestamp: None,
            }],
            actions: vec![ChatAction {
                action_type: "search_notes".to_string(),
                payload: serde_json::json!({
                    "query": "quantum computing",
                    "results": [
                        {"note_id": "abc-123", "title": "Quantum Basics", "score": 0.92}
                    ]
                }),
            }],
            model_info: ChatModelInfo {
                model: "test:latest".to_string(),
                context_window: 0,
                estimated_available_context: 0,
                max_output_tokens: 0,
                supports_thinking: false,
                thinking_type: "none".to_string(),
                speed_tok_s: 0.0,
                parameter_size: None,
                family: None,
            },
        };
        let val: serde_json::Value = serde_json::to_value(&response).unwrap();
        let actions = val["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        // ChatAction serializes `action_type` as `type` via #[serde(rename)]
        assert_eq!(actions[0]["type"], "search_notes");
        assert!(actions[0]["payload"]["results"].is_array());
    }

    /// Issue #549 test case #11: Empty messages and actions arrays.
    #[test]
    fn test_chat_response_empty_messages_and_actions() {
        let response = ChatResponse {
            messages: vec![],
            actions: vec![],
            model_info: ChatModelInfo {
                model: "test:latest".to_string(),
                context_window: 0,
                estimated_available_context: 0,
                max_output_tokens: 0,
                supports_thinking: false,
                thinking_type: "none".to_string(),
                speed_tok_s: 0.0,
                parameter_size: None,
                family: None,
            },
        };
        let val: serde_json::Value = serde_json::to_value(&response).unwrap();
        assert_eq!(val["messages"].as_array().unwrap().len(), 0);
        assert_eq!(val["actions"].as_array().unwrap().len(), 0);
    }

    /// Issue #549 test case #15: Message without timestamp — timestamp field absent.
    #[test]
    fn test_chat_response_message_without_timestamp_omits_field() {
        let response = ChatResponse {
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "response text".to_string(),
                timestamp: None,
            }],
            actions: vec![],
            model_info: ChatModelInfo {
                model: "test:latest".to_string(),
                context_window: 0,
                estimated_available_context: 0,
                max_output_tokens: 0,
                supports_thinking: false,
                thinking_type: "none".to_string(),
                speed_tok_s: 0.0,
                parameter_size: None,
                family: None,
            },
        };
        let val: serde_json::Value = serde_json::to_value(&response).unwrap();
        let msg = &val["messages"][0];
        // timestamp should be absent (skip_serializing_if = None)
        assert!(
            msg.get("timestamp").is_none(),
            "None timestamp should be omitted"
        );
        // But role and content must still be present
        assert_eq!(msg["role"], "assistant");
        assert_eq!(msg["content"], "response text");
    }

    /// model_info optional fields (`parameter_size`, `family`) are omitted when None.
    #[test]
    fn test_chat_model_info_optional_fields_omitted() {
        let info = ChatModelInfo {
            model: "unknown:latest".to_string(),
            context_window: 0,
            estimated_available_context: 0,
            max_output_tokens: 0,
            supports_thinking: false,
            thinking_type: "unknown".to_string(),
            speed_tok_s: 0.0,
            parameter_size: None,
            family: None,
        };
        let val: serde_json::Value = serde_json::to_value(&info).unwrap();
        let obj = val.as_object().unwrap();
        assert!(
            !obj.contains_key("parameter_size"),
            "None parameter_size should be omitted"
        );
        assert!(!obj.contains_key("family"), "None family should be omitted");
    }

    /// model_info optional fields present when Some.
    #[test]
    fn test_chat_model_info_optional_fields_present() {
        let info = ChatModelInfo {
            model: "qwen3:8b".to_string(),
            context_window: 40960,
            estimated_available_context: 40760,
            max_output_tokens: 4096,
            supports_thinking: false,
            thinking_type: "not_tested".to_string(),
            speed_tok_s: 144.3,
            parameter_size: Some("8.2B".to_string()),
            family: Some("qwen3".to_string()),
        };
        let val: serde_json::Value = serde_json::to_value(&info).unwrap();
        assert_eq!(val["parameter_size"], "8.2B");
        assert_eq!(val["family"], "qwen3");
    }

    // =========================================================================
    // Streaming Chat Contract Tests (Issue #812, #814, #816)
    // =========================================================================

    use matric_core::Error as CoreError;

    /// Collect all frames a stream produces by running the pump to completion
    /// (the bounded channel must be wide enough to never block), then draining.
    async fn run_pump_to_completion(
        chunks: Vec<matric_core::Result<String>>,
        capacity: usize,
        metrics: Arc<ChatStreamMetrics>,
    ) -> Vec<ChatStreamFrame> {
        let (tx, mut rx) = mpsc::channel::<ChatStreamFrame>(capacity);
        let source = futures::stream::iter(chunks);
        pump_chat_stream(
            source,
            tx,
            metrics,
            "test-model:latest".to_string(),
            Duration::from_secs(5),
        )
        .await;
        let mut frames = Vec::new();
        while let Some(f) = rx.recv().await {
            frames.push(f);
        }
        frames
    }

    /// #814: the metrics snapshot exposes every chat-stream counter, including
    /// the headline `chat_stream_dropped_tokens_total`.
    #[test]
    fn chat_stream_metrics_snapshot_has_all_counters() {
        let metrics = ChatStreamMetrics::default();
        let snap = metrics.snapshot();
        let obj = snap.as_object().unwrap();
        for key in [
            "chat_stream_started_total",
            "chat_stream_completed_total",
            "chat_stream_errored_total",
            "chat_stream_client_disconnect_total",
            "chat_stream_tokens_total",
            "chat_stream_dropped_tokens_total",
        ] {
            assert!(obj.contains_key(key), "snapshot missing {key}");
            assert_eq!(obj[key]["type"], "counter", "{key} should be a counter");
            assert_eq!(obj[key]["value"], 0, "{key} should start at 0");
        }
    }

    /// #814: counters surface their live values in the snapshot.
    #[test]
    fn chat_stream_metrics_snapshot_reflects_increments() {
        let metrics = ChatStreamMetrics::default();
        metrics.streams_started.fetch_add(3, Ordering::Relaxed);
        metrics.dropped_tokens_total.fetch_add(7, Ordering::Relaxed);
        metrics
            .tokens_streamed_total
            .fetch_add(42, Ordering::Relaxed);
        let snap = metrics.snapshot();
        assert_eq!(snap["chat_stream_started_total"]["value"], 3);
        assert_eq!(snap["chat_stream_dropped_tokens_total"]["value"], 7);
        assert_eq!(snap["chat_stream_tokens_total"]["value"], 42);
    }

    /// #816: frame constructors produce the documented SSE shapes.
    #[test]
    fn chat_stream_frame_shapes() {
        let delta = ChatStreamFrame::delta("hello");
        assert_eq!(delta.event, "delta");
        let v: serde_json::Value = serde_json::from_str(&delta.data).unwrap();
        assert_eq!(v["content"], "hello");

        let done = ChatStreamFrame::done("qwen3:8b");
        assert_eq!(done.event, "done");
        let v: serde_json::Value = serde_json::from_str(&done.data).unwrap();
        assert_eq!(v["finish_reason"], "stop");
        assert_eq!(v["model"], "qwen3:8b");

        let err = ChatStreamFrame::error("boom".to_string());
        assert_eq!(err.event, "error");
        let v: serde_json::Value = serde_json::from_str(&err.data).unwrap();
        assert_eq!(v["error"], "boom");
        assert_eq!(v["code"], "GENERATION_FAILED");
    }

    /// #816: a clean stream emits one `delta` per non-empty chunk followed by a
    /// terminal `done`, and accounts every delivered token.
    #[tokio::test]
    async fn pump_emits_deltas_then_done() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let frames = run_pump_to_completion(
            vec![
                Ok("Hel".to_string()),
                Ok("lo".to_string()),
                Ok("!".to_string()),
            ],
            16,
            metrics.clone(),
        )
        .await;

        assert_eq!(frames.len(), 4, "3 deltas + 1 done");
        assert_eq!(frames[0].event, "delta");
        assert_eq!(frames[1].event, "delta");
        assert_eq!(frames[2].event, "delta");
        assert_eq!(frames[3].event, "done", "last frame must be the terminator");

        let reassembled: String = frames
            .iter()
            .filter(|f| f.event == "delta")
            .map(|f| {
                serde_json::from_str::<serde_json::Value>(&f.data).unwrap()["content"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(reassembled, "Hello!");

        assert_eq!(metrics.streams_completed.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.streams_errored.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.dropped_tokens_total.load(Ordering::Relaxed), 0);
    }

    /// #816: empty content chunks (e.g. keep-alive lines) are not emitted.
    #[tokio::test]
    async fn pump_skips_empty_chunks() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let frames = run_pump_to_completion(
            vec![Ok("a".to_string()), Ok(String::new()), Ok("b".to_string())],
            16,
            metrics.clone(),
        )
        .await;

        let deltas = frames.iter().filter(|f| f.event == "delta").count();
        assert_eq!(deltas, 2, "empty chunk must be skipped");
        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 2);
    }

    /// #816: a mid-stream generation error emits an `error` frame, terminates
    /// the stream, and increments the error counter.
    #[tokio::test]
    async fn pump_emits_error_frame_on_chunk_error() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let frames = run_pump_to_completion(
            vec![
                Ok("partial".to_string()),
                Err(CoreError::Inference("upstream exploded".to_string())),
                // Anything after the error must never be emitted.
                Ok("never".to_string()),
            ],
            16,
            metrics.clone(),
        )
        .await;

        assert_eq!(frames.len(), 2, "1 delta + 1 error, nothing after");
        assert_eq!(frames[0].event, "delta");
        assert_eq!(frames[1].event, "error");
        let v: serde_json::Value = serde_json::from_str(&frames[1].data).unwrap();
        assert!(v["error"].as_str().unwrap().contains("upstream exploded"));

        assert_eq!(metrics.streams_errored.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.streams_completed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 1);
    }

    /// #814/#816: when the client disconnects mid-stream (receiver dropped),
    /// undelivered tokens are counted as dropped and the disconnect is recorded.
    #[tokio::test]
    async fn pump_counts_dropped_tokens_on_client_disconnect() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let (tx, rx) = mpsc::channel::<ChatStreamFrame>(16);
        drop(rx); // client gone before any token is delivered

        let source = futures::stream::iter(vec![
            Ok("a".to_string()),
            Ok("b".to_string()),
            Ok("c".to_string()),
        ]);
        pump_chat_stream(
            source,
            tx,
            metrics.clone(),
            "test-model:latest".to_string(),
            Duration::from_secs(5),
        )
        .await;

        assert_eq!(metrics.client_disconnects.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.dropped_tokens_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.streams_completed.load(Ordering::Relaxed), 0);
    }

    /// #814/#816: when a client stops draining the SSE buffer, the pump sheds
    /// the stalled token after the send window rather than blocking forever.
    #[tokio::test]
    async fn pump_counts_dropped_tokens_on_backpressure() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        // Capacity 1, never drained: the first token buffers, the second stalls
        // and is shed after the (tiny) send window elapses.
        let (tx, _rx) = mpsc::channel::<ChatStreamFrame>(1);
        let source = futures::stream::iter(vec![
            Ok("first".to_string()),
            Ok("second".to_string()),
            Ok("third".to_string()),
        ]);
        pump_chat_stream(
            source,
            tx,
            metrics.clone(),
            "test-model:latest".to_string(),
            Duration::from_millis(50),
        )
        .await;

        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.dropped_tokens_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.client_disconnects.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.streams_completed.load(Ordering::Relaxed), 0);
        // _rx kept alive to scope end so the channel stays "full" rather than closed.
        drop(_rx);
    }
}

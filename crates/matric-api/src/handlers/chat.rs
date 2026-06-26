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
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

use matric_api::services::chat_stream_store::{ResumeCursor, StoredFrame};
use matric_api::services::ChatStreamStore;

use crate::{ApiError, AppState};
use matric_inference::discovery::ModelDiscovery;
use matric_inference::profiles::ModelRegistry;
use matric_inference::OllamaBackend;
use uuid::Uuid;

/// Bounded capacity of the SSE event channel for a streaming chat response.
/// Large enough that a reasonably-paced client never sees drops, small enough
/// to bound memory if a client stalls.
const CHAT_STREAM_CHANNEL_CAPACITY: usize = 256;
const CHAT_GENERATION_FAILURE_MESSAGE: &str =
    "Chat generation failed. Check server logs for diagnostics.";
const CHAT_MODEL_UNAVAILABLE_MESSAGE: &str = "Requested chat model is not available.";
const CHAT_MODEL_UNSUPPORTED_MESSAGE: &str = "Requested model cannot be used for chat.";

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
        return ApiError::BadRequest("input must not be empty".to_string()).into_response();
    }

    // 2. Check backend is configured
    let backend = match state.generation_backend() {
        Some(b) => b,
        None => {
            return chat_service_unavailable("Chat generation backend is not available", 30);
        }
    };

    // 2b. Check the provider is currently reachable (#630).
    // The periodic reachability probe sets this flag; if the provider (e.g.
    // Ollama) is down or not yet started, fail fast with a retry hint rather
    // than blocking on a generation request that will time out.
    if !state.inference_available.load(Ordering::Relaxed) {
        return chat_service_unavailable("Chat provider is not reachable", 30);
    }

    // 3. Try to acquire a semaphore permit (non-blocking)
    let semaphore = match &state.chat_semaphore {
        Some(s) => s,
        None => {
            return chat_service_unavailable("Chat generation backend is not available", 30);
        }
    };

    let _permit = match semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return chat_service_unavailable("Chat service is currently at capacity", 5);
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
            ApiError::ProviderFailure {
                capability: "Chat generation",
                detail: "chat generation request failed".to_string(),
            }
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
        return Err(
            ApiError::BadRequest(CHAT_MODEL_UNAVAILABLE_MESSAGE.to_string()).into_response(),
        );
    }

    // Check it's not an embedding-only model
    let is_embed_only = models
        .models
        .iter()
        .find(|m| m.name == model_slug)
        .map(|m| m.is_likely_embedding())
        .unwrap_or(false);

    if is_embed_only {
        return Err(
            ApiError::BadRequest(CHAT_MODEL_UNSUPPORTED_MESSAGE.to_string()).into_response(),
        );
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

fn chat_service_unavailable(msg: &str, retry_after: u64) -> Response {
    let mut response = ApiError::ServiceUnavailable(msg.to_string()).into_response();
    if let Ok(value) = retry_after.to_string().parse() {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
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
    /// SSE event id of the form `{session}-{seq}`, set by the pump (or the
    /// resumption replay path). Carried as the SSE `id:` field so a reconnecting
    /// `EventSource` echoes it back via `Last-Event-ID` (#815). `None` on a bare
    /// frame before the pump assigns a sequence.
    pub id: Option<String>,
}

impl ChatStreamFrame {
    fn delta(content: &str) -> Self {
        Self {
            event: "delta",
            data: serde_json::json!({ "content": content }).to_string(),
            id: None,
        }
    }

    fn done(model_name: &str) -> Self {
        Self {
            event: "done",
            data: serde_json::json!({ "finish_reason": "stop", "model": model_name }).to_string(),
            id: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            event: "error",
            data: serde_json::json!({ "error": message, "code": "GENERATION_FAILED" }).to_string(),
            id: None,
        }
    }

    fn generation_failed() -> Self {
        Self::error(CHAT_GENERATION_FAILURE_MESSAGE.to_string())
    }

    /// An `error` frame signalling that a resumed stream's buffer ended before a
    /// terminal frame — the original generation was interrupted (e.g. the client
    /// disconnected mid-generation) and cannot be continued. The client should
    /// resend the request to regenerate (#815).
    fn interrupted() -> Self {
        Self {
            event: "error",
            data: serde_json::json!({
                "error": "stream interrupted before completion; resend the request to regenerate",
                "code": "STREAM_INTERRUPTED"
            })
            .to_string(),
            id: None,
        }
    }

    /// Attach the SSE event id (`{session}-{seq}`).
    fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    fn into_event(self) -> Event {
        let mut ev = Event::default().event(self.event).data(self.data);
        if let Some(id) = self.id {
            ev = ev.id(id);
        }
        ev
    }

    /// Project to a [`StoredFrame`] for Redis buffering (#815).
    fn to_stored(&self, seq: u64) -> StoredFrame {
        StoredFrame {
            seq,
            event: self.event.to_string(),
            data: self.data.clone(),
        }
    }

    /// Reconstruct an emittable frame from a buffered [`StoredFrame`] during
    /// resumption replay, re-attaching the `{session}-{seq}` id (#815).
    fn from_stored(sf: &StoredFrame, session: &str) -> Self {
        let event: &'static str = match sf.event.as_str() {
            "done" => "done",
            "error" => "error",
            _ => "delta",
        };
        Self {
            event,
            data: sf.data.clone(),
            id: Some(format!("{}-{}", session, sf.seq)),
        }
    }
}

/// Resume an interrupted chat stream by replaying buffered frames after the
/// client's `Last-Event-ID` cursor — no GPU permit and no new generation (#815).
///
/// Replays every buffered frame with `seq > cursor.after_seq`, preserving event
/// ids so the client can resume again if needed. If the buffer has no terminal
/// frame (the original generation was interrupted, or the session has expired /
/// is unknown), emits a terminal `STREAM_INTERRUPTED` error so the client knows
/// to resend the request rather than wait forever.
async fn resume_chat_stream(store: ChatStreamStore, cursor: ResumeCursor) -> Response {
    let frames = store.read_after(&cursor.session, cursor.after_seq).await;
    let replay = build_resume_frames(&frames, &cursor.session);

    let (tx, rx) = mpsc::channel::<ChatStreamFrame>(CHAT_STREAM_CHANNEL_CAPACITY);
    tokio::spawn(async move {
        for frame in replay {
            if tx.send(frame).await.is_err() {
                return; // client disconnected again
            }
        }
    });

    let event_stream =
        ReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(event_stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

/// Build the replay frame sequence for a resumed stream: every buffered frame
/// after the cursor (with its `{session}-{seq}` id re-attached), plus a terminal
/// `STREAM_INTERRUPTED` error when the buffer did not already end with a terminal
/// frame — so a client resuming an interrupted generation gets a clear signal
/// rather than an open stream that never closes (#815).
fn build_resume_frames(frames: &[StoredFrame], session: &str) -> Vec<ChatStreamFrame> {
    let mut out: Vec<ChatStreamFrame> = frames
        .iter()
        .map(|sf| ChatStreamFrame::from_stored(sf, session))
        .collect();
    let had_terminal = frames.last().map(StoredFrame::is_terminal).unwrap_or(false);
    if !had_terminal {
        out.push(ChatStreamFrame::interrupted());
    }
    out
}

/// Pump a backend content stream into the SSE frame channel, emitting one
/// `delta` frame per content chunk, a terminal `done` frame on completion, or
/// an `error` frame on failure — while accounting delivered vs dropped tokens
/// in `metrics`.
///
/// Extracted from the handler so the SSE framing, terminator, error path, and
/// backpressure/dropped-token accounting are unit-testable without a live
/// model (#816).
#[allow(clippy::too_many_arguments)]
async fn pump_chat_stream<S>(
    mut chunks: S,
    tx: mpsc::Sender<ChatStreamFrame>,
    metrics: Arc<ChatStreamMetrics>,
    model_name: String,
    send_timeout: Duration,
    session: String,
    store: ChatStreamStore,
) where
    S: futures::Stream<Item = matric_core::Result<String>> + Unpin,
{
    let mut delivered: u64 = 0;
    let mut dropped: u64 = 0;
    // Monotonic per-session sequence; every persisted/emitted frame gets one so
    // the SSE event id is `{session}-{seq}` and replay can be cursor-based (#815).
    let mut seq: u64 = 0;

    while let Some(item) = chunks.next().await {
        match item {
            Ok(content) => {
                if content.is_empty() {
                    continue;
                }
                seq += 1;
                let frame = ChatStreamFrame::delta(&content);
                // Persist BEFORE the send attempt so a frame shed by backpressure
                // is still replayable on reconnect within the TTL window (#815).
                store.append(&session, &frame.to_stored(seq)).await;
                let frame = frame.with_id(format!("{session}-{seq}"));
                match tokio::time::timeout(send_timeout, tx.send(frame)).await {
                    Ok(Ok(())) => {
                        delivered += 1;
                    }
                    Ok(Err(_closed)) => {
                        // Receiver dropped — client disconnected mid-stream. The
                        // frame is buffered; a reconnect within the TTL window
                        // replays from here. Generation stops (GPU released) per
                        // the #814 backpressure decision.
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
                warn!(error = %e, "Streaming chat generation failed");
                seq += 1;
                let frame = ChatStreamFrame::generation_failed();
                store.append(&session, &frame.to_stored(seq)).await;
                let _ = tx.send(frame.with_id(format!("{session}-{seq}"))).await;
                metrics.streams_errored.fetch_add(1, Ordering::Relaxed);
                metrics
                    .tokens_streamed_total
                    .fetch_add(delivered, Ordering::Relaxed);
                return;
            }
        }
    }

    // Stream exhausted normally — emit the terminal `done` frame.
    seq += 1;
    let frame = ChatStreamFrame::done(&model_name);
    store.append(&session, &frame.to_stored(seq)).await;
    let _ = tx.send(frame.with_id(format!("{session}-{seq}"))).await;
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
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Response {
    // 0. Resumption: a `Last-Event-ID` header means the client is reconnecting to
    //    an existing stream. Replay the buffered tail after the cursor instead of
    //    starting a fresh generation — no GPU permit required (#815). The request
    //    body is re-sent by the client but ignored on this path.
    if let Some(cursor) = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(ResumeCursor::parse)
    {
        return resume_chat_stream(state.chat_stream_store.clone(), cursor).await;
    }

    // 1. Validate input
    let input = req.input.trim();
    if input.is_empty() {
        return ApiError::BadRequest("input must not be empty".to_string()).into_response();
    }

    // 2. Backend configured?
    let backend = match state.generation_backend() {
        Some(b) => b,
        None => return chat_service_unavailable("Chat generation backend is not available", 30),
    };

    // 2b. Provider reachable? (#630)
    if !state.inference_available.load(Ordering::Relaxed) {
        return chat_service_unavailable("Chat provider is not reachable", 30);
    }

    // 3. Acquire an OWNED permit — held for the full stream lifetime.
    let semaphore = match &state.chat_semaphore {
        Some(s) => s.clone(),
        None => return chat_service_unavailable("Chat generation backend is not available", 30),
    };
    let permit = match semaphore.try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return chat_service_unavailable("Chat service is currently at capacity", 5),
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

    // 6. Begin streaming. Each stream gets a session id; emitted frames carry an
    //    `id` of `{session}-{seq}` and are buffered in Redis for 60s so a
    //    reconnecting client can resume after its `Last-Event-ID` (#815).
    let metrics = state.chat_stream_metrics.clone();
    metrics.streams_started.fetch_add(1, Ordering::Relaxed);

    let session = Uuid::new_v4().to_string();
    let store = state.chat_stream_store.clone();
    let (tx, rx) = mpsc::channel::<ChatStreamFrame>(CHAT_STREAM_CHANNEL_CAPACITY);
    let send_timeout = chat_stream_send_timeout();

    tokio::spawn(async move {
        // The owned permit lives until this task ends, releasing the GPU slot
        // only when the stream completes, errors, or the client disconnects.
        let _permit = permit;

        match chat_backend.chat_multi_turn_stream(messages).await {
            Ok(chunks) => {
                pump_chat_stream(
                    chunks,
                    tx,
                    metrics,
                    model_name,
                    send_timeout,
                    session,
                    store,
                )
                .await;
            }
            Err(e) => {
                warn!(error = %e, model = %model_name, "Streaming chat failed to start");
                let frame = ChatStreamFrame::generation_failed();
                store.append(&session, &frame.to_stored(1)).await;
                let _ = tx.send(frame.with_id(format!("{session}-1"))).await;
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
            warn!(error = %e, "Failed to discover chat models");
            return ApiError::ProviderFailure {
                capability: "Chat model discovery",
                detail: "chat model discovery request failed".to_string(),
            }
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

    #[tokio::test]
    async fn chat_service_unavailable_returns_problem_without_legacy_error_shape() {
        let response = chat_service_unavailable("Chat provider is not reachable", 30);

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("30")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/service-unavailable"
        );
        assert_eq!(problem["detail"], "Chat provider is not reachable");
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
        assert!(problem.get("retry_after").is_none());
    }

    #[tokio::test]
    async fn chat_model_validation_problems_do_not_echo_model_slug() {
        let private_model_slug = "tenant-alpha/private-router/token-secret-model:latest";
        for detail in [
            CHAT_MODEL_UNAVAILABLE_MESSAGE,
            CHAT_MODEL_UNSUPPORTED_MESSAGE,
        ] {
            let response = ApiError::BadRequest(detail.to_string()).into_response();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                response
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok()),
                Some("application/problem+json")
            );

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

            assert_eq!(
                problem["type"],
                "https://fortemi.com/problems/validation-error"
            );
            assert_eq!(problem["detail"], detail);

            let serialized = problem.to_string();
            assert!(!serialized.contains(private_model_slug));
            assert!(!serialized.contains("tenant-alpha"));
            assert!(!serialized.contains("token-secret"));
            assert!(problem.get("error").is_none());
            assert!(problem.get("error_description").is_none());
        }
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
            "test-session".to_string(),
            ChatStreamStore::disabled(),
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

        let generic_err = ChatStreamFrame::generation_failed();
        assert_eq!(generic_err.event, "error");
        let v: serde_json::Value = serde_json::from_str(&generic_err.data).unwrap();
        assert_eq!(v["error"], CHAT_GENERATION_FAILURE_MESSAGE);
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
        assert_eq!(v["error"], CHAT_GENERATION_FAILURE_MESSAGE);
        assert_eq!(v["code"], "GENERATION_FAILED");
        assert!(!frames[1].data.contains("upstream exploded"));

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
            "test-session".to_string(),
            ChatStreamStore::disabled(),
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
            "test-session".to_string(),
            ChatStreamStore::disabled(),
        )
        .await;

        assert_eq!(metrics.tokens_streamed_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.dropped_tokens_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.client_disconnects.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.streams_completed.load(Ordering::Relaxed), 0);
        // _rx kept alive to scope end so the channel stays "full" rather than closed.
        drop(_rx);
    }

    /// #815: every emitted frame carries a sequential `{session}-{seq}` SSE id so
    /// a reconnecting client's `Last-Event-ID` resolves to a cursor.
    #[tokio::test]
    async fn pump_assigns_sequential_event_ids() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let frames =
            run_pump_to_completion(vec![Ok("a".to_string()), Ok("b".to_string())], 16, metrics)
                .await;
        // 2 deltas + done, ids 1..=3 under the helper's "test-session".
        let ids: Vec<Option<String>> = frames.iter().map(|f| f.id.clone()).collect();
        assert_eq!(
            ids,
            vec![
                Some("test-session-1".to_string()),
                Some("test-session-2".to_string()),
                Some("test-session-3".to_string()),
            ]
        );
    }

    /// #815: a buffered frame replays with its original event/data and a re-built
    /// `{session}-{seq}` id.
    #[test]
    fn from_stored_reattaches_event_id() {
        let sf = StoredFrame {
            seq: 5,
            event: "delta".to_string(),
            data: r#"{"content":"hi"}"#.to_string(),
        };
        let frame = ChatStreamFrame::from_stored(&sf, "sess-uuid");
        assert_eq!(frame.event, "delta");
        assert_eq!(frame.data, r#"{"content":"hi"}"#);
        assert_eq!(frame.id.as_deref(), Some("sess-uuid-5"));

        // Unknown stored event names degrade to "delta" rather than panicking.
        let weird = StoredFrame {
            seq: 1,
            event: "bogus".to_string(),
            data: "{}".to_string(),
        };
        assert_eq!(ChatStreamFrame::from_stored(&weird, "s").event, "delta");
    }

    /// #815: replaying a buffer that ends with a terminal frame yields exactly the
    /// post-cursor frames — no synthetic interruption appended.
    #[test]
    fn build_resume_frames_completed_buffer_replays_verbatim() {
        let frames = vec![
            StoredFrame {
                seq: 3,
                event: "delta".into(),
                data: r#"{"content":"lo"}"#.into(),
            },
            StoredFrame {
                seq: 4,
                event: "done".into(),
                data: r#"{"finish_reason":"stop","model":"m"}"#.into(),
            },
        ];
        let out = build_resume_frames(&frames, "S");
        assert_eq!(
            out.len(),
            2,
            "no interruption appended for a complete buffer"
        );
        assert_eq!(out[0].id.as_deref(), Some("S-3"));
        assert_eq!(out[1].event, "done");
        assert_eq!(out[1].id.as_deref(), Some("S-4"));
    }

    /// #815: replaying a buffer with no terminal frame (generation was interrupted
    /// or the session expired) appends a terminal STREAM_INTERRUPTED error so the
    /// client stops waiting and resends.
    #[test]
    fn build_resume_frames_interrupted_buffer_appends_terminator() {
        let frames = vec![StoredFrame {
            seq: 2,
            event: "delta".into(),
            data: r#"{"content":"par"}"#.into(),
        }];
        let out = build_resume_frames(&frames, "S");
        assert_eq!(out.len(), 2, "delta replay + synthetic interruption");
        assert_eq!(out[0].event, "delta");
        assert_eq!(out[1].event, "error");
        let v: serde_json::Value = serde_json::from_str(&out[1].data).unwrap();
        assert_eq!(v["code"], "STREAM_INTERRUPTED");
        assert!(
            out[1].id.is_none(),
            "synthetic terminator has no buffered id"
        );

        // An empty buffer (unknown / expired session) is purely the terminator.
        let empty = build_resume_frames(&[], "S");
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].event, "error");
        let v: serde_json::Value = serde_json::from_str(&empty[0].data).unwrap();
        assert_eq!(v["code"], "STREAM_INTERRUPTED");
    }

    // =========================================================================
    // Async resource discipline: leak / dangling-reference guards
    //
    // The streaming path holds an owned GPU permit, an `Arc<ChatStreamMetrics>`,
    // and an `mpsc` sender across a spawned task. A clone the pump forgets to
    // drop is a metrics/permit leak; a sender the pump forgets to drop holds the
    // SSE channel open forever (the client hangs, the connection fd leaks). These
    // tests pin the release discipline at the pump boundary.
    // =========================================================================

    /// The pump must consume the `Arc<ChatStreamMetrics>` it is handed and drop
    /// it on return — it must not stash a clone in a lingering task or closure.
    /// `strong_count` returning to its pre-call value proves no leaked reference.
    #[tokio::test]
    async fn pump_does_not_leak_metrics_arc() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let before = Arc::strong_count(&metrics);
        // The clone is moved into the pump; a correct pump drops it on return.
        let _frames = run_pump_to_completion(
            vec![Ok("a".to_string()), Ok("b".to_string())],
            16,
            metrics.clone(),
        )
        .await;
        assert_eq!(
            Arc::strong_count(&metrics),
            before,
            "pump leaked a metrics Arc clone — async resource not released"
        );
    }

    /// On completion the pump must drop its `mpsc::Sender` so the SSE channel
    /// closes and the client's stream ends. A dangling sender would keep the
    /// channel open and hang the receiver; the bounded `timeout` converts any
    /// such regression into a fast failure instead of a hung test.
    #[tokio::test]
    async fn pump_closes_channel_on_completion_no_dangling_sender() {
        let metrics = Arc::new(ChatStreamMetrics::default());
        let (tx, mut rx) = mpsc::channel::<ChatStreamFrame>(16);
        let source = futures::stream::iter(vec![Ok("x".to_string())]);
        // `tx` is moved into the pump; on return it must be dropped.
        pump_chat_stream(
            source,
            tx,
            metrics,
            "test-model:latest".to_string(),
            Duration::from_secs(5),
            "test-session".to_string(),
            ChatStreamStore::disabled(),
        )
        .await;

        let drain = async {
            let mut saw_done = false;
            while let Some(frame) = rx.recv().await {
                if frame.event == "done" {
                    saw_done = true;
                }
            }
            (saw_done, rx)
        };
        let (saw_done, mut rx) = tokio::time::timeout(Duration::from_secs(2), drain)
            .await
            .expect("draining hung — the pump left a dangling sender (resource leak)");

        assert!(saw_done, "stream should have emitted a terminal done frame");
        assert!(
            rx.recv().await.is_none(),
            "channel must be closed once the pump returns"
        );
    }

    /// Stream DTOs own their data (`String` / `&'static str`) with no `Rc`/`Arc`,
    /// so reference cycles are impossible by construction. A `to_stored` →
    /// `from_stored` round-trip preserves the wire content and yields a frame
    /// whose data is independent of the sources: dropping the originals leaves
    /// the restored frame fully valid (no shared or dangling ownership).
    #[test]
    fn stored_frame_roundtrip_is_value_independent() {
        let original = ChatStreamFrame::delta("payload");
        let stored = original.to_stored(7);
        let restored = ChatStreamFrame::from_stored(&stored, "sess");

        assert_eq!(restored.event, "delta");
        assert_eq!(restored.data, original.data);
        assert_eq!(restored.id.as_deref(), Some("sess-7"));

        // Drop both sources; the restored frame must remain wholly intact.
        drop(original);
        drop(stored);
        assert_eq!(restored.data, r#"{"content":"payload"}"#);
        assert_eq!(restored.event, "delta");
    }
}

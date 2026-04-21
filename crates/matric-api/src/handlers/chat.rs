//! Synchronous chat handler — calls Ollama directly, bypassing the job queue.
//!
//! GPU availability is gated by a `tokio::Semaphore` in AppState. When all
//! permits are taken (by other chat requests or concurrent GPU usage), the
//! endpoint returns 503 immediately rather than queuing.

use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::AppState;
use matric_inference::discovery::ModelDiscovery;
use matric_inference::profiles::ModelRegistry;
use matric_inference::OllamaBackend;

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
}

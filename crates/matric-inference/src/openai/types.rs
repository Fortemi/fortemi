//! OpenAI API request and response types.

use serde::{Deserialize, Serialize};
use std::fmt;

// =============================================================================
// EMBEDDING TYPES
// =============================================================================

/// Request body for the embeddings endpoint.
#[derive(Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
}

impl fmt::Debug for EmbeddingRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingRequest")
            .field("model_len", &self.model.len())
            .field("input_count", &self.input.len())
            .field(
                "input_total_len",
                &self.input.iter().map(String::len).sum::<usize>(),
            )
            .field(
                "encoding_format_len",
                &self.encoding_format.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Response from the embeddings endpoint.
#[derive(Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

impl fmt::Debug for EmbeddingResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingResponse")
            .field("data_count", &self.data.len())
            .field("model_len", &self.model.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// Single embedding data point.
#[derive(Deserialize)]
pub struct EmbeddingData {
    pub embedding: Vec<f32>,
    pub index: usize,
}

impl fmt::Debug for EmbeddingData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingData")
            .field("embedding_len", &self.embedding.len())
            .field("index", &self.index)
            .finish()
    }
}

/// Token usage for embedding request.
#[derive(Debug, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

// =============================================================================
// CHAT COMPLETION TYPES
// =============================================================================

/// OpenAI `response_format` for structured output (JSON mode).
#[derive(Debug, Clone, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

/// Request body for chat completions endpoint.
#[derive(Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(default)]
    pub stream: bool,
}

impl fmt::Debug for ChatCompletionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatCompletionRequest")
            .field("model_len", &self.model.len())
            .field("message_count", &self.messages.len())
            .field(
                "message_content_total_len",
                &self
                    .messages
                    .iter()
                    .map(|message| message.content.len())
                    .sum::<usize>(),
            )
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("response_format", &self.response_format)
            .field("stream", &self.stream)
            .finish()
    }
}

/// A single chat message.
#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl fmt::Debug for ChatMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatMessage")
            .field("role_len", &self.role.len())
            .field("content_len", &self.content.len())
            .finish()
    }
}

/// Response from chat completions endpoint.
#[derive(Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<ChatUsage>,
}

impl fmt::Debug for ChatCompletionResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatCompletionResponse")
            .field("id_len", &self.id.len())
            .field("choice_count", &self.choices.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// Single chat completion choice.
#[derive(Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

impl fmt::Debug for ChatChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatChoice")
            .field("index", &self.index)
            .field("message", &self.message)
            .field(
                "finish_reason_len",
                &self.finish_reason.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Token usage for chat completion request.
#[derive(Debug, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// =============================================================================
// STREAMING TYPES
// =============================================================================

/// Streaming chunk for chat completions.
#[derive(Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub choices: Vec<ChatChunkChoice>,
}

impl fmt::Debug for ChatCompletionChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatCompletionChunk")
            .field("id_len", &self.id.len())
            .field("choice_count", &self.choices.len())
            .finish()
    }
}

/// Single choice in a streaming chunk.
#[derive(Deserialize)]
pub struct ChatChunkChoice {
    pub index: usize,
    pub delta: ChatDelta,
    pub finish_reason: Option<String>,
}

impl fmt::Debug for ChatChunkChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatChunkChoice")
            .field("index", &self.index)
            .field("delta", &self.delta)
            .field(
                "finish_reason_len",
                &self.finish_reason.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Delta content in streaming response.
#[derive(Deserialize)]
pub struct ChatDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

impl fmt::Debug for ChatDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatDelta")
            .field("role_len", &self.role.as_ref().map(String::len))
            .field("content_len", &self.content.as_ref().map(String::len))
            .finish()
    }
}

// =============================================================================
// ERROR TYPES
// =============================================================================

/// Error response from OpenAI API.
#[derive(Deserialize)]
pub struct OpenAIErrorResponse {
    pub error: OpenAIError,
}

impl fmt::Debug for OpenAIErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIErrorResponse")
            .field("error", &self.error)
            .finish()
    }
}

/// Detailed error information.
#[derive(Deserialize)]
pub struct OpenAIError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
}

impl fmt::Debug for OpenAIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIError")
            .field("message_len", &self.message.len())
            .field("error_type_len", &self.error_type.len())
            .field("code_len", &self.code.as_ref().map(String::len))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_request_serialization() {
        let request = EmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: vec!["hello".to_string(), "world".to_string()],
            encoding_format: Some("float".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("text-embedding-3-small"));
        assert!(json.contains("hello"));
        assert!(json.contains("float"));
    }

    #[test]
    fn test_embedding_request_without_format() {
        let request = EmbeddingRequest {
            model: "test".to_string(),
            input: vec!["test".to_string()],
            encoding_format: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("encoding_format"));
    }

    #[test]
    fn test_embedding_response_deserialization() {
        let json = r#"{
            "data": [
                {"embedding": [0.1, 0.2, 0.3], "index": 0}
            ],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        }"#;

        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.data[0].index, 0);
        assert_eq!(response.usage.prompt_tokens, 2);
    }

    #[test]
    fn test_chat_completion_request_serialization() {
        let request = ChatCompletionRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are helpful.".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
            ],
            temperature: Some(0.7),
            max_tokens: None,
            response_format: None,
            stream: false,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4o-mini"));
        assert!(json.contains("system"));
        assert!(json.contains("user"));
        assert!(json.contains("0.7"));
        assert!(!json.contains("max_tokens"));
    }

    #[test]
    fn test_chat_completion_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello!");
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_chat_completion_chunk_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        }"#;

        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_openai_error_response_deserialization() {
        let json = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;

        let response: OpenAIErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error.message, "Invalid API key");
        assert_eq!(response.error.error_type, "invalid_request_error");
        assert_eq!(response.error.code, Some("invalid_api_key".to_string()));
    }

    #[test]
    fn openai_wire_debug_redacts_prompts_models_ids_embeddings_and_errors() {
        let embedding_request = EmbeddingRequest {
            model: "private-embedding-model".to_string(),
            input: vec![
                "customer email jane@example.com".to_string(),
                "token sk-private-token".to_string(),
            ],
            encoding_format: Some("float".to_string()),
        };
        let embedding_response = EmbeddingResponse {
            data: vec![EmbeddingData {
                embedding: vec![0.123456, 0.654321],
                index: 0,
            }],
            model: "private-embedding-model".to_string(),
            usage: EmbeddingUsage {
                prompt_tokens: 4,
                total_tokens: 4,
            },
        };
        let chat_request = ChatCompletionRequest {
            model: "private-chat-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Please summarize jane@example.com using sk-private-token".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(128),
            response_format: Some(ResponseFormat {
                format_type: "json_object".to_string(),
            }),
            stream: true,
        };
        let chat_response = ChatCompletionResponse {
            id: "chatcmpl-secret-id".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "Summary includes jane@example.com".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 4,
                completion_tokens: 5,
                total_tokens: 9,
            }),
        };
        let chunk = ChatCompletionChunk {
            id: "chunk-secret-id".to_string(),
            choices: vec![ChatChunkChoice {
                index: 0,
                delta: ChatDelta {
                    role: Some("assistant".to_string()),
                    content: Some("streamed sk-private-token".to_string()),
                },
                finish_reason: Some("stop".to_string()),
            }],
        };
        let error = OpenAIErrorResponse {
            error: OpenAIError {
                message: "Invalid key sk-private-token for https://provider.example".to_string(),
                error_type: "invalid_request_error".to_string(),
                code: Some("invalid_api_key".to_string()),
            },
        };

        let debug = format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            embedding_request,
            embedding_response.data[0],
            embedding_response,
            chat_request.messages[0],
            chat_request,
            chat_response.choices[0].message,
            chat_response.choices[0],
            chat_response,
            chunk.choices[0].delta,
            chunk.choices[0],
            chunk,
            error
        );

        assert!(debug.contains("model_len"));
        assert!(debug.contains("input_count"));
        assert!(debug.contains("embedding_len"));
        assert!(debug.contains("message_content_total_len"));
        assert!(debug.contains("content_len"));
        assert!(debug.contains("id_len"));
        assert!(debug.contains("message_len"));
        assert!(!debug.contains("private-embedding-model"));
        assert!(!debug.contains("private-chat-model"));
        assert!(!debug.contains("jane@example.com"));
        assert!(!debug.contains("sk-private-token"));
        assert!(!debug.contains("chatcmpl-secret-id"));
        assert!(!debug.contains("chunk-secret-id"));
        assert!(!debug.contains("0.123456"));
        assert!(!debug.contains("provider.example"));
        assert!(!debug.contains("Invalid key"));
    }

    #[test]
    fn test_chat_message_clone() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "test".to_string(),
        };
        let cloned = msg.clone();
        assert_eq!(msg.role, cloned.role);
        assert_eq!(msg.content, cloned.content);
    }
}

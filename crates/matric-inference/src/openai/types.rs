//! OpenAI API request and response types.

use serde::{Deserialize, Serialize};

// =============================================================================
// EMBEDDING TYPES
// =============================================================================

/// Request body for the embeddings endpoint.
#[derive(Debug, Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
}

/// Response from the embeddings endpoint.
#[derive(Debug, Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

/// Single embedding data point.
#[derive(Debug, Deserialize)]
pub struct EmbeddingData {
    pub embedding: Vec<f32>,
    pub index: usize,
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

/// Request body for chat completions endpoint.
#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Response from chat completions endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<ChatUsage>,
}

/// Single chat completion choice.
#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
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
#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub choices: Vec<ChatChunkChoice>,
}

/// Single choice in a streaming chunk.
#[derive(Debug, Deserialize)]
pub struct ChatChunkChoice {
    pub index: usize,
    pub delta: ChatDelta,
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response.
#[derive(Debug, Deserialize)]
pub struct ChatDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

// =============================================================================
// ERROR TYPES
// =============================================================================

/// Error response from OpenAI API.
#[derive(Debug, Deserialize)]
pub struct OpenAIErrorResponse {
    pub error: OpenAIError,
}

/// Detailed error information.
#[derive(Debug, Deserialize)]
pub struct OpenAIError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
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

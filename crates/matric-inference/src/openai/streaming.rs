//! SSE stream parsing for OpenAI-compatible streaming responses.

use futures::{Stream, StreamExt};
use std::pin::Pin;

use matric_core::{Error, Result};

use super::types::ChatCompletionChunk;

/// Stream of generation tokens.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

/// Parse SSE stream from OpenAI-compatible endpoint.
pub fn parse_sse_stream(
    stream: impl Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> TokenStream {
    let token_stream = stream
        .map(|chunk_result| {
            chunk_result.map_err(|e| Error::Inference(format!("Stream error: {}", e)))
        })
        .filter_map(|result| async move {
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    parse_sse_chunk(&text)
                }
                Err(e) => Some(Err(e)),
            }
        });

    Box::pin(token_stream)
}

/// Parse a single SSE chunk and extract content.
fn parse_sse_chunk(chunk: &str) -> Option<Result<String>> {
    let mut content = String::new();

    for line in chunk.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        // End of stream marker
        if line == "data: [DONE]" {
            return None;
        }

        // Parse data lines
        if let Some(data) = line.strip_prefix("data: ") {
            match serde_json::from_str::<ChatCompletionChunk>(data) {
                Ok(chunk) => {
                    for choice in chunk.choices {
                        if let Some(c) = choice.delta.content {
                            content.push_str(&c);
                        }
                    }
                }
                Err(e) => {
                    return Some(Err(Error::Inference(format!(
                        "Failed to parse SSE chunk: {}",
                        e
                    ))));
                }
            }
        }
    }

    if content.is_empty() {
        None
    } else {
        Some(Ok(content))
    }
}

/// Streaming generation trait extension.
#[async_trait::async_trait]
pub trait StreamingGeneration: Send + Sync {
    /// Generate text with streaming response.
    async fn generate_stream(&self, prompt: &str) -> Result<TokenStream>;

    /// Generate text with system context and streaming response.
    async fn generate_with_system_stream(&self, system: &str, prompt: &str) -> Result<TokenStream>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_chunk_with_content() {
        let chunk = r#"data: {"id":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let result = parse_sse_chunk(chunk);
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap(), "Hello");
    }

    #[test]
    fn test_parse_sse_chunk_done() {
        let chunk = "data: [DONE]";
        let result = parse_sse_chunk(chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_chunk_empty_delta() {
        let chunk =
            r#"data: {"id":"test","choices":[{"index":0,"delta":{},"finish_reason":null}]}"#;
        let result = parse_sse_chunk(chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_chunk_with_role_only() {
        let chunk = r#"data: {"id":"test","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}"#;
        let result = parse_sse_chunk(chunk);
        assert!(result.is_none()); // No content, just role
    }

    #[test]
    fn test_parse_sse_chunk_comment() {
        let chunk = ": this is a comment";
        let result = parse_sse_chunk(chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_chunk_empty_line() {
        let chunk = "";
        let result = parse_sse_chunk(chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_chunk_multiple_lines() {
        let chunk = r#"data: {"id":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"test","choices":[{"index":0,"delta":{"content":" World"},"finish_reason":null}]}"#;
        let result = parse_sse_chunk(chunk);
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap(), "Hello World");
    }

    #[test]
    fn test_parse_sse_chunk_invalid_json() {
        let chunk = "data: {invalid json}";
        let result = parse_sse_chunk(chunk);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_parse_sse_chunk_finish_reason() {
        let chunk = r#"data: {"id":"test","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}"#;
        let result = parse_sse_chunk(chunk);
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap(), "!");
    }
}

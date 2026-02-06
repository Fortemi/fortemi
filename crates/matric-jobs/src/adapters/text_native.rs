//! TextNative extraction adapter - handles plain text files.

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Adapter for extracting content from plain text files.
///
/// Reads bytes as UTF-8 (with lossy conversion for invalid sequences)
/// and returns the text with basic metadata (char count, line count).
pub struct TextNativeAdapter;

#[async_trait]
impl ExtractionAdapter for TextNativeAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::TextNative
    }

    async fn extract(
        &self,
        data: &[u8],
        _filename: &str,
        _mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        let text = String::from_utf8_lossy(data).into_owned();
        let char_count = text.len();
        let line_count = text.lines().count();

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata: serde_json::json!({
                "char_count": char_count,
                "line_count": line_count,
            }),
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // No external dependencies
    }

    fn name(&self) -> &str {
        "text_native"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_native_extraction() {
        let adapter = TextNativeAdapter;
        let result = adapter
            .extract(
                b"Hello, world!\nLine two.",
                "test.txt",
                "text/plain",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(
            result.extracted_text.as_deref(),
            Some("Hello, world!\nLine two.")
        );
        assert_eq!(result.metadata["char_count"], 23);
        assert_eq!(result.metadata["line_count"], 2);
        assert!(result.ai_description.is_none());
        assert!(result.preview_data.is_none());
    }

    #[tokio::test]
    async fn test_text_native_empty_input() {
        let adapter = TextNativeAdapter;
        let result = adapter
            .extract(b"", "empty.txt", "text/plain", &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result.extracted_text.as_deref(), Some(""));
        assert_eq!(result.metadata["char_count"], 0);
        assert_eq!(result.metadata["line_count"], 0);
    }

    #[tokio::test]
    async fn test_text_native_invalid_utf8() {
        let adapter = TextNativeAdapter;
        let data: &[u8] = &[0xFF, 0xFE, b'h', b'i'];
        let result = adapter
            .extract(
                data,
                "binary.bin",
                "application/octet-stream",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        // Should use lossy conversion (replacement characters)
        let text = result.extracted_text.unwrap();
        assert!(text.contains("hi"));
        assert!(text.contains('\u{FFFD}')); // replacement character
    }

    #[tokio::test]
    async fn test_text_native_strategy() {
        let adapter = TextNativeAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::TextNative);
    }

    #[tokio::test]
    async fn test_text_native_name() {
        let adapter = TextNativeAdapter;
        assert_eq!(adapter.name(), "text_native");
    }

    #[tokio::test]
    async fn test_text_native_health_check() {
        let adapter = TextNativeAdapter;
        assert!(adapter.health_check().await.unwrap());
    }
}

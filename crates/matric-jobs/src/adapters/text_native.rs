//! TextNative extraction adapter - handles plain text files.

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use matric_core::{
    defaults::TEXT_EXTRACTION_MAX_BYTES, ExtractionAdapter, ExtractionResult, ExtractionStrategy,
    Result,
};

/// Adapter for extracting content from plain text files.
///
/// Reads bytes as UTF-8 (with lossy conversion for invalid sequences)
/// and returns the text with basic metadata (char count, line count).
///
/// For large files (> 10MB by default), the adapter truncates to the
/// configured threshold and sets metadata flags.
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
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        // Read max_bytes from config or use default
        let max_bytes = config
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(TEXT_EXTRACTION_MAX_BYTES);

        let original_size = data.len();
        let truncated = original_size > max_bytes;
        let data_slice = if truncated { &data[..max_bytes] } else { data };

        let text = String::from_utf8_lossy(data_slice).into_owned();
        let has_replacement = text.contains('\u{FFFD}');
        let char_count = text.len();
        let line_count = text.lines().count();

        let mut metadata = serde_json::json!({
            "char_count": char_count,
            "line_count": line_count,
            "encoding": if has_replacement { "utf-8-lossy" } else { "utf-8" },
        });

        if truncated {
            metadata["truncated"] = serde_json::json!(true);
            metadata["original_size"] = serde_json::json!(original_size);
            metadata["truncated_at"] = serde_json::json!(max_bytes);
        }

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata,
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
        assert_eq!(result.metadata["encoding"], "utf-8");
        assert!(result.metadata.get("truncated").is_none());
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
        assert_eq!(result.metadata["encoding"], "utf-8");
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
        assert_eq!(result.metadata["encoding"], "utf-8-lossy");
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

    #[tokio::test]
    async fn test_text_native_large_file_truncation() {
        let adapter = TextNativeAdapter;
        // Create a file larger than the default threshold (10MB)
        let large_data = vec![b'A'; TEXT_EXTRACTION_MAX_BYTES + 1000];
        let result = adapter
            .extract(
                &large_data,
                "large.txt",
                "text/plain",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        // Should be truncated
        assert_eq!(result.metadata["truncated"], true);
        assert_eq!(
            result.metadata["original_size"],
            TEXT_EXTRACTION_MAX_BYTES + 1000
        );
        assert_eq!(result.metadata["truncated_at"], TEXT_EXTRACTION_MAX_BYTES);

        // Extracted text should be exactly max_bytes
        let text = result.extracted_text.unwrap();
        assert_eq!(text.len(), TEXT_EXTRACTION_MAX_BYTES);
        assert_eq!(result.metadata["char_count"], TEXT_EXTRACTION_MAX_BYTES);
    }

    #[tokio::test]
    async fn test_text_native_config_max_bytes() {
        let adapter = TextNativeAdapter;
        let custom_max = 1000;
        let data = vec![b'B'; custom_max + 500];

        let result = adapter
            .extract(
                &data,
                "custom.txt",
                "text/plain",
                &serde_json::json!({ "max_bytes": custom_max }),
            )
            .await
            .unwrap();

        // Should be truncated at custom threshold
        assert_eq!(result.metadata["truncated"], true);
        assert_eq!(result.metadata["original_size"], custom_max + 500);
        assert_eq!(result.metadata["truncated_at"], custom_max);

        let text = result.extracted_text.unwrap();
        assert_eq!(text.len(), custom_max);
        assert_eq!(result.metadata["char_count"], custom_max);
    }

    #[tokio::test]
    async fn test_text_native_exact_threshold() {
        let adapter = TextNativeAdapter;
        // Create data exactly at the threshold (should NOT truncate)
        let data = vec![b'C'; TEXT_EXTRACTION_MAX_BYTES];

        let result = adapter
            .extract(&data, "exact.txt", "text/plain", &serde_json::json!({}))
            .await
            .unwrap();

        // Should NOT be truncated
        assert!(result.metadata.get("truncated").is_none());
        assert!(result.metadata.get("original_size").is_none());
        assert!(result.metadata.get("truncated_at").is_none());

        let text = result.extracted_text.unwrap();
        assert_eq!(text.len(), TEXT_EXTRACTION_MAX_BYTES);
    }

    #[tokio::test]
    async fn test_text_native_just_over_threshold() {
        let adapter = TextNativeAdapter;
        // Create data 1 byte over threshold (should truncate)
        let data = vec![b'D'; TEXT_EXTRACTION_MAX_BYTES + 1];

        let result = adapter
            .extract(&data, "over.txt", "text/plain", &serde_json::json!({}))
            .await
            .unwrap();

        // Should be truncated
        assert_eq!(result.metadata["truncated"], true);
        assert_eq!(
            result.metadata["original_size"],
            TEXT_EXTRACTION_MAX_BYTES + 1
        );
        assert_eq!(result.metadata["truncated_at"], TEXT_EXTRACTION_MAX_BYTES);

        let text = result.extracted_text.unwrap();
        assert_eq!(text.len(), TEXT_EXTRACTION_MAX_BYTES);
    }
}

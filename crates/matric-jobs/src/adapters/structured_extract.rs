//! StructuredExtract adapter - handles JSON, YAML, TOML, CSV, XML.

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Adapter for extracting content from structured data formats.
///
/// Handles JSON, YAML, TOML, CSV, and XML. Validates the format,
/// extracts schema metadata, and returns the text with format info.
pub struct StructuredExtractAdapter;

impl StructuredExtractAdapter {
    fn detect_format(filename: &str, mime_type: &str) -> &'static str {
        // Check MIME type first
        let mime = mime_type.to_lowercase();
        if mime.contains("json") {
            return "json";
        }
        if mime.contains("yaml") {
            return "yaml";
        }
        if mime.contains("toml") {
            return "toml";
        }
        if mime.contains("csv") {
            return "csv";
        }
        if mime.contains("xml") {
            return "xml";
        }

        // Fall back to extension
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "json" | "geojson" | "ndjson" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "csv" | "tsv" => "csv",
            "xml" | "svg" | "drawio" => "xml",
            _ => "text",
        }
    }

    fn extract_json_metadata(text: &str) -> JsonValue {
        match serde_json::from_str::<JsonValue>(text) {
            Ok(val) => {
                let type_name = match &val {
                    JsonValue::Object(map) => {
                        let keys: Vec<&String> = map.keys().take(10).collect();
                        serde_json::json!({
                            "valid": true,
                            "type": "object",
                            "top_level_keys": keys,
                            "key_count": map.len(),
                        })
                    }
                    JsonValue::Array(arr) => {
                        serde_json::json!({
                            "valid": true,
                            "type": "array",
                            "element_count": arr.len(),
                        })
                    }
                    _ => serde_json::json!({
                        "valid": true,
                        "type": "primitive",
                    }),
                };
                type_name
            }
            Err(e) => serde_json::json!({
                "valid": false,
                "parse_error": e.to_string(),
            }),
        }
    }

    fn extract_csv_metadata(text: &str) -> JsonValue {
        let lines: Vec<&str> = text.lines().collect();
        let row_count = lines.len();
        let headers: Option<Vec<&str>> = lines.first().map(|line| line.split(',').collect());

        serde_json::json!({
            "row_count": row_count,
            "headers": headers,
            "column_count": headers.as_ref().map(|h| h.len()).unwrap_or(0),
        })
    }
}

#[async_trait]
impl ExtractionAdapter for StructuredExtractAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::StructuredExtract
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        let text = String::from_utf8_lossy(data).into_owned();
        let format = Self::detect_format(filename, mime_type);

        let format_metadata = match format {
            "json" => Self::extract_json_metadata(&text),
            "csv" => Self::extract_csv_metadata(&text),
            _ => serde_json::json!({ "format": format }),
        };

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata: serde_json::json!({
                "format": format,
                "format_metadata": format_metadata,
            }),
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // No external dependencies
    }

    fn name(&self) -> &str {
        "structured_extract"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_structured_extract_json() {
        let adapter = StructuredExtractAdapter;
        let json_data = r#"{"name": "test", "count": 42}"#;
        let result = adapter
            .extract(
                json_data.as_bytes(),
                "data.json",
                "application/json",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert!(result.extracted_text.is_some());
        assert_eq!(result.metadata["format"], "json");
        assert_eq!(result.metadata["format_metadata"]["valid"], true);
        assert_eq!(result.metadata["format_metadata"]["type"], "object");
        assert_eq!(result.metadata["format_metadata"]["key_count"], 2);
    }

    #[tokio::test]
    async fn test_structured_extract_json_array() {
        let adapter = StructuredExtractAdapter;
        let json_data = r#"[1, 2, 3]"#;
        let result = adapter
            .extract(
                json_data.as_bytes(),
                "data.json",
                "application/json",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["format"], "json");
        assert_eq!(result.metadata["format_metadata"]["type"], "array");
        assert_eq!(result.metadata["format_metadata"]["element_count"], 3);
    }

    #[tokio::test]
    async fn test_structured_extract_csv() {
        let adapter = StructuredExtractAdapter;
        let csv_data = "name,age,city\nAlice,30,NYC\nBob,25,LA\n";
        let result = adapter
            .extract(
                csv_data.as_bytes(),
                "data.csv",
                "text/csv",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["format"], "csv");
        assert_eq!(result.metadata["format_metadata"]["row_count"], 3);
        assert_eq!(result.metadata["format_metadata"]["column_count"], 3);
    }

    #[tokio::test]
    async fn test_structured_extract_invalid_json_fallback() {
        let adapter = StructuredExtractAdapter;
        let bad_json = "not valid json {{{";
        let result = adapter
            .extract(
                bad_json.as_bytes(),
                "bad.json",
                "application/json",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        // Should still succeed, with parse_error in metadata
        assert!(result.extracted_text.is_some());
        assert_eq!(result.metadata["format"], "json");
        assert_eq!(result.metadata["format_metadata"]["valid"], false);
    }

    #[tokio::test]
    async fn test_structured_extract_yaml() {
        let adapter = StructuredExtractAdapter;
        let yaml_data = "name: test\ncount: 42";
        let result = adapter
            .extract(
                yaml_data.as_bytes(),
                "data.yaml",
                "application/yaml",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["format"], "yaml");
    }

    #[tokio::test]
    async fn test_structured_extract_strategy() {
        let adapter = StructuredExtractAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::StructuredExtract);
    }

    #[tokio::test]
    async fn test_structured_extract_name() {
        let adapter = StructuredExtractAdapter;
        assert_eq!(adapter.name(), "structured_extract");
    }

    #[tokio::test]
    async fn test_structured_extract_health_check() {
        let adapter = StructuredExtractAdapter;
        assert!(adapter.health_check().await.unwrap());
    }

    #[test]
    fn test_detect_format_from_mime() {
        assert_eq!(
            StructuredExtractAdapter::detect_format("f.txt", "application/json"),
            "json"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("f.txt", "text/yaml"),
            "yaml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("f.txt", "text/csv"),
            "csv"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("f.txt", "text/xml"),
            "xml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("f.txt", "application/toml"),
            "toml"
        );
    }

    #[test]
    fn test_detect_format_from_extension() {
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.json", "application/octet-stream"),
            "json"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.yaml", "application/octet-stream"),
            "yaml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.csv", "application/octet-stream"),
            "csv"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.xml", "application/octet-stream"),
            "xml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.toml", "application/octet-stream"),
            "toml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("data.unknown", "application/octet-stream"),
            "text"
        );
    }
}

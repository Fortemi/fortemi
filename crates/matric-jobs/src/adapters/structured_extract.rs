//! StructuredExtract adapter - handles JSON, YAML, TOML, CSV, XML.

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Strip HTML tags from a string, preserving text content.
fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Adapter for extracting content from structured data formats.
///
/// Handles JSON, YAML, TOML, CSV, XML, and diagram-specific formats
/// (Draw.io, Excalidraw, SVG). Validates the format, extracts schema
/// metadata, and returns the text with format info.
pub struct StructuredExtractAdapter;

impl StructuredExtractAdapter {
    fn detect_format(filename: &str, mime_type: &str) -> &'static str {
        // Check MIME type first
        let mime = mime_type.to_lowercase();
        if mime.contains("excalidraw") {
            return "excalidraw";
        }
        if mime.contains("drawio") {
            return "xml";
        }
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
            "excalidraw" => "excalidraw",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "csv" | "tsv" => "csv",
            "xml" | "svg" | "drawio" | "graffle" => "xml",
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

    /// Extract human-readable text from Excalidraw JSON.
    ///
    /// Excalidraw stores diagram elements as a JSON array. Text content lives
    /// in the `text` field of elements with `type: "text"`.
    fn extract_excalidraw_text(text: &str) -> (String, JsonValue) {
        let mut labels = Vec::new();
        if let Ok(val) = serde_json::from_str::<JsonValue>(text) {
            if let Some(elements) = val.get("elements").and_then(|e| e.as_array()) {
                for elem in elements {
                    // Direct text elements
                    if let Some(t) = elem.get("text").and_then(|t| t.as_str()) {
                        let trimmed = t.trim();
                        if !trimmed.is_empty() {
                            labels.push(trimmed.to_string());
                        }
                    }
                }
            }
            let metadata = serde_json::json!({
                "format": "excalidraw",
                "format_metadata": {
                    "valid": true,
                    "element_count": val.get("elements").and_then(|e| e.as_array()).map(|a| a.len()).unwrap_or(0),
                    "text_labels": labels.len(),
                }
            });
            let extracted = if labels.is_empty() {
                text.to_string()
            } else {
                labels.join("\n")
            };
            (extracted, metadata)
        } else {
            (
                text.to_string(),
                serde_json::json!({
                    "format": "excalidraw",
                    "format_metadata": { "valid": false }
                }),
            )
        }
    }

    /// Extract human-readable text from Draw.io / SVG XML.
    ///
    /// Draw.io stores labels in `value` attributes of `<mxCell>` elements and
    /// in `<UserObject label="...">` wrappers. SVG has `<text>` elements.
    /// Uses simple regex-based extraction to avoid pulling in an XML parser dep.
    fn extract_xml_labels(text: &str) -> Vec<String> {
        let mut labels = Vec::new();

        // Draw.io: value="label text" on mxCell elements
        for cap in text.split("value=\"").skip(1) {
            if let Some(end) = cap.find('"') {
                let val = &cap[..end];
                // Decode common HTML entities and strip tags
                let decoded = val
                    .replace("&amp;", "&")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&#xa;", "\n")
                    .replace("&#10;", "\n");
                // Strip HTML tags (Draw.io labels can contain <b>, <br>, etc.)
                let stripped = strip_html_tags(&decoded);
                let trimmed = stripped.trim();
                if !trimmed.is_empty() {
                    labels.push(trimmed.to_string());
                }
            }
        }

        // Draw.io: label="text" on UserObject elements
        for cap in text.split("label=\"").skip(1) {
            if let Some(end) = cap.find('"') {
                let val = &cap[..end];
                let decoded = val
                    .replace("&amp;", "&")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"");
                let stripped = strip_html_tags(&decoded);
                let trimmed = stripped.trim();
                if !trimmed.is_empty() && !labels.contains(&trimmed.to_string()) {
                    labels.push(trimmed.to_string());
                }
            }
        }

        // SVG: content between <text> tags
        for cap in text.split("<text").skip(1) {
            // Find the closing > of the opening tag
            if let Some(tag_end) = cap.find('>') {
                let rest = &cap[tag_end + 1..];
                if let Some(close) = rest.find("</text>") {
                    let content = &rest[..close];
                    // Strip nested tspan tags
                    let stripped = strip_html_tags(content);
                    let trimmed = stripped.trim();
                    if !trimmed.is_empty() {
                        labels.push(trimmed.to_string());
                    }
                }
            }
        }

        labels
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

        // Excalidraw: extract text labels from JSON elements
        if format == "excalidraw" {
            let (extracted, metadata) = Self::extract_excalidraw_text(&text);
            return Ok(ExtractionResult {
                extracted_text: Some(extracted),
                metadata,
                ai_description: None,
                preview_data: None,
                derived_files: vec![],
            });
        }

        // XML-based diagrams (Draw.io, SVG): extract text labels
        if format == "xml" {
            let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
            let is_diagram = ext == "drawio"
                || ext == "svg"
                || mime_type.contains("drawio")
                || mime_type == "image/svg+xml";

            if is_diagram {
                let labels = Self::extract_xml_labels(&text);
                let extracted = if labels.is_empty() {
                    text.clone()
                } else {
                    labels.join("\n")
                };
                return Ok(ExtractionResult {
                    extracted_text: Some(extracted),
                    metadata: serde_json::json!({
                        "format": format,
                        "format_metadata": {
                            "diagram_type": ext,
                            "text_labels": labels.len(),
                        },
                    }),
                    ai_description: None,
                    preview_data: None,
                    derived_files: vec![],
                });
            }
        }

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
            derived_files: vec![],
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

    #[test]
    fn test_detect_format_diagram_types() {
        assert_eq!(
            StructuredExtractAdapter::detect_format("arch.drawio", "application/x-drawio"),
            "xml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("arch.drawio", "application/x-drawio+xml"),
            "xml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format(
                "board.excalidraw",
                "application/x-excalidraw+json"
            ),
            "excalidraw"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("icon.svg", "image/svg+xml"),
            "xml"
        );
        assert_eq!(
            StructuredExtractAdapter::detect_format("doc.graffle", "application/x-omnigraffle"),
            "xml"
        );
    }

    #[tokio::test]
    async fn test_extract_excalidraw() {
        let adapter = StructuredExtractAdapter;
        let excalidraw_data = r#"{
            "type": "excalidraw",
            "elements": [
                {"type": "text", "text": "Hello World", "id": "1"},
                {"type": "rectangle", "id": "2"},
                {"type": "text", "text": "Another Label", "id": "3"}
            ]
        }"#;
        let result = adapter
            .extract(
                excalidraw_data.as_bytes(),
                "board.excalidraw",
                "application/x-excalidraw+json",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(text.contains("Hello World"));
        assert!(text.contains("Another Label"));
        assert_eq!(result.metadata["format"], "excalidraw");
        assert_eq!(result.metadata["format_metadata"]["text_labels"], 2);
    }

    #[tokio::test]
    async fn test_extract_drawio_xml() {
        let adapter = StructuredExtractAdapter;
        let drawio_data = r#"<?xml version="1.0" encoding="UTF-8"?>
<mxfile>
  <diagram>
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="2" value="Start" style="ellipse" vertex="1" parent="1"/>
        <mxCell id="3" value="Process &amp; Transform" style="rounded" vertex="1" parent="1"/>
        <mxCell id="4" value="" style="edgeStyle=orthogonal" edge="1" parent="1"/>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>"#;
        let result = adapter
            .extract(
                drawio_data.as_bytes(),
                "flow.drawio",
                "application/x-drawio",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(text.contains("Start"));
        assert!(text.contains("Process & Transform"));
        assert_eq!(result.metadata["format"], "xml");
        assert_eq!(result.metadata["format_metadata"]["diagram_type"], "drawio");
    }

    #[tokio::test]
    async fn test_extract_svg_text() {
        let adapter = StructuredExtractAdapter;
        let svg_data = r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
  <text x="10" y="30" font-size="20">Hello SVG</text>
  <text x="10" y="60"><tspan>Nested Text</tspan></text>
  <rect x="0" y="0" width="200" height="100"/>
</svg>"#;
        let result = adapter
            .extract(
                svg_data.as_bytes(),
                "diagram.svg",
                "image/svg+xml",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(text.contains("Hello SVG"));
        assert!(text.contains("Nested Text"));
        assert_eq!(result.metadata["format_metadata"]["diagram_type"], "svg");
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("hello"), "hello");
        assert_eq!(strip_html_tags("<b>bold</b>"), "bold");
        assert_eq!(strip_html_tags("a<br/>b"), "ab");
        assert_eq!(
            strip_html_tags("<div style=\"color:red\">text</div>"),
            "text"
        );
    }
}

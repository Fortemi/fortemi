//! OfficeConvertAdapter — converts office documents to plain text using pandoc.
//!
//! Supports: docx, pptx, rtf, odt, tex, epub, eml, mbox
//! For xlsx/csv: falls back to TextNativeAdapter behavior (read as text)

use std::io::Write;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tokio::process::Command;
use tracing::debug;

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

pub struct OfficeConvertAdapter;

/// Determine the pandoc input format from filename extension.
fn pandoc_input_format(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "docx" => Some("docx"),
        "pptx" => Some("pptx"),
        "rtf" => Some("rtf"),
        "odt" => Some("odt"),
        "tex" | "latex" => Some("latex"),
        "epub" => Some("epub"),
        "html" | "htm" => Some("html"),
        "rst" => Some("rst"),
        "org" => Some("org"),
        "mediawiki" => Some("mediawiki"),
        "textile" => Some("textile"),
        _ => None,
    }
}

/// Determine the pandoc input format from MIME type.
fn pandoc_format_from_mime(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        "application/rtf" => Some("rtf"),
        "application/vnd.oasis.opendocument.text" => Some("odt"),
        "application/x-latex" | "text/x-tex" => Some("latex"),
        "application/epub+zip" => Some("epub"),
        "text/html" => Some("html"),
        "message/rfc822" => None, // eml handled separately
        _ => None,
    }
}

/// Run a command with a timeout, returning stdout as a string.
async fn run_cmd_with_timeout(cmd: &mut Command, timeout_secs: u64) -> Result<String> {
    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| {
            matric_core::Error::Internal(format!(
                "External command timed out after {}s",
                timeout_secs
            ))
        })?
        .map_err(|e| matric_core::Error::Internal(format!("Failed to execute command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(matric_core::Error::Internal(format!(
            "Command failed (exit {}): {}",
            output.status,
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[async_trait]
impl ExtractionAdapter for OfficeConvertAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::OfficeConvert
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot convert empty document".to_string(),
            ));
        }

        // Determine pandoc input format
        let format = pandoc_input_format(filename).or_else(|| pandoc_format_from_mime(mime_type));

        let format = match format {
            Some(f) => f,
            None => {
                // Fallback: try as plain text
                let text = String::from_utf8_lossy(data).into_owned();
                return Ok(ExtractionResult {
                    extracted_text: Some(text.clone()),
                    metadata: json!({
                        "fallback": true,
                        "reason": "unsupported_format",
                        "char_count": text.len(),
                        "line_count": text.lines().count(),
                    }),
                    ai_description: None,
                    preview_data: None,
                });
            }
        };

        // Write to temp file
        let suffix = filename
            .rsplit('.')
            .next()
            .map(|e| format!(".{}", e))
            .unwrap_or_default();
        let mut tmpfile = tempfile::Builder::new()
            .suffix(&suffix)
            .tempfile()
            .map_err(|e| {
                matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
            })?;
        tmpfile.write_all(data).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
        })?;
        let tmp_path = tmpfile.path().to_string_lossy().to_string();

        debug!(filename, format, "Converting with pandoc");

        // Run pandoc: pandoc -f FORMAT -t plain --wrap=none INPUT
        let text = run_cmd_with_timeout(
            Command::new("pandoc")
                .arg("-f")
                .arg(format)
                .arg("-t")
                .arg("plain")
                .arg("--wrap=none")
                .arg(&tmp_path),
            EXTRACTION_CMD_TIMEOUT_SECS,
        )
        .await?;

        let char_count = text.len();
        let line_count = text.lines().count();

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata: json!({
                "format": format,
                "char_count": char_count,
                "line_count": line_count,
                "converter": "pandoc",
            }),
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        match Command::new("pandoc").arg("--version").output().await {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false),
        }
    }

    fn name(&self) -> &str {
        "office_convert"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_office_convert_strategy() {
        let adapter = OfficeConvertAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::OfficeConvert);
    }

    #[test]
    fn test_office_convert_name() {
        let adapter = OfficeConvertAdapter;
        assert_eq!(adapter.name(), "office_convert");
    }

    #[tokio::test]
    async fn test_office_convert_health_check() {
        let adapter = OfficeConvertAdapter;
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_office_convert_empty_input() {
        let adapter = OfficeConvertAdapter;
        let result = adapter
            .extract(
                b"",
                "empty.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                &json!({}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_office_convert_unsupported_format_fallback() {
        let adapter = OfficeConvertAdapter;
        let result = adapter
            .extract(
                b"plain text content",
                "file.xyz",
                "application/octet-stream",
                &json!({}),
            )
            .await;
        assert!(result.is_ok());
        let extraction = result.unwrap();
        assert_eq!(extraction.metadata["fallback"], true);
        assert_eq!(
            extraction.extracted_text.as_deref(),
            Some("plain text content")
        );
    }

    #[test]
    fn test_pandoc_input_format_detection() {
        assert_eq!(pandoc_input_format("doc.docx"), Some("docx"));
        assert_eq!(pandoc_input_format("slides.pptx"), Some("pptx"));
        assert_eq!(pandoc_input_format("doc.rtf"), Some("rtf"));
        assert_eq!(pandoc_input_format("doc.odt"), Some("odt"));
        assert_eq!(pandoc_input_format("paper.tex"), Some("latex"));
        assert_eq!(pandoc_input_format("book.epub"), Some("epub"));
        assert_eq!(pandoc_input_format("page.html"), Some("html"));
        assert_eq!(pandoc_input_format("unknown.bin"), None);
    }

    #[test]
    fn test_pandoc_format_from_mime() {
        assert_eq!(
            pandoc_format_from_mime(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            ),
            Some("docx")
        );
        assert_eq!(pandoc_format_from_mime("application/rtf"), Some("rtf"));
        assert_eq!(pandoc_format_from_mime("application/octet-stream"), None);
    }

    #[tokio::test]
    async fn test_office_convert_html_extraction() {
        let adapter = OfficeConvertAdapter;
        if !adapter.health_check().await.unwrap_or(false) {
            eprintln!("Skipping test: pandoc not installed");
            return;
        }

        let html = b"<html><body><h1>Title</h1><p>Hello world</p></body></html>";
        let result = adapter
            .extract(html, "test.html", "text/html", &json!({}))
            .await;
        assert!(result.is_ok(), "Extraction failed: {:?}", result.err());
        let extraction = result.unwrap();
        let text = extraction.extracted_text.unwrap();
        assert!(
            text.contains("Title"),
            "Should contain title, got: {}",
            text
        );
        assert!(
            text.contains("Hello world"),
            "Should contain content, got: {}",
            text
        );
    }
}

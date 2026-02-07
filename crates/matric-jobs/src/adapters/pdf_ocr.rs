//! PdfOcrAdapter — OCRs scanned PDFs using pdftoppm + tesseract.
//!
//! Pipeline: PDF → pdftoppm (render pages to PNG) → tesseract (OCR each page) → concatenate.
//! Triggered when PdfTextAdapter flags `needs_ocr: true` in metadata (< 50 chars extracted).

use std::fs;
use std::io::Write;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tempfile::{NamedTempFile, TempDir};
use tokio::process::Command;
use tracing::{debug, warn};

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

pub struct PdfOcrAdapter;

#[allow(dead_code)]
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

/// Run a command that may output to files rather than stdout.
async fn run_cmd_status(cmd: &mut Command, timeout_secs: u64) -> Result<()> {
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

    Ok(())
}

#[async_trait]
impl ExtractionAdapter for PdfOcrAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::PdfOcr
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot OCR empty PDF data".to_string(),
            ));
        }

        // Validate PDF magic bytes
        if data.len() < 4 || &data[0..4] != b"%PDF" {
            return Err(matric_core::Error::InvalidInput(format!(
                "File '{}' is not a valid PDF (missing %PDF header)",
                filename
            )));
        }

        // Read config
        let dpi = config.get("dpi").and_then(|v| v.as_u64()).unwrap_or(300);
        let language = config
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("eng");

        // Write PDF to temp file
        let mut tmpfile = NamedTempFile::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
        })?;
        tmpfile.write_all(data).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
        })?;
        let pdf_path = tmpfile.path().to_string_lossy().to_string();

        // Create temp dir for rendered page images
        let img_dir = TempDir::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp dir: {}", e))
        })?;
        let img_prefix = img_dir.path().join("page").to_string_lossy().to_string();

        debug!(filename, dpi, language, "Rendering PDF pages for OCR");

        // Step 1: Render PDF pages to PNG using pdftoppm
        run_cmd_status(
            Command::new("pdftoppm")
                .arg("-png")
                .arg("-r")
                .arg(dpi.to_string())
                .arg(&pdf_path)
                .arg(&img_prefix),
            EXTRACTION_CMD_TIMEOUT_SECS * 3, // Allow more time for rendering
        )
        .await?;

        // Find all rendered page images (sorted by name for correct order)
        let mut page_images: Vec<String> = Vec::new();
        let entries = fs::read_dir(img_dir.path())
            .map_err(|e| matric_core::Error::Internal(format!("Failed to read temp dir: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                matric_core::Error::Internal(format!("Failed to read dir entry: {}", e))
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                page_images.push(path.to_string_lossy().to_string());
            }
        }
        page_images.sort();

        if page_images.is_empty() {
            return Ok(ExtractionResult {
                extracted_text: Some(String::new()),
                metadata: json!({
                    "ocr_pages": 0,
                    "dpi": dpi,
                    "language": language,
                    "warning": "No pages rendered from PDF",
                }),
                ai_description: None,
                preview_data: None,
            });
        }

        debug!(filename, pages = page_images.len(), "OCRing rendered pages");

        // Step 2: OCR each page with tesseract
        let mut page_texts = Vec::new();

        for (i, img_path) in page_images.iter().enumerate() {
            // tesseract INPUT OUTPUT -l LANG -- outputs OUTPUT.txt
            let output_base = img_dir.path().join(format!("ocr_{}", i));
            let output_path = format!("{}.txt", output_base.to_string_lossy());

            let result = run_cmd_status(
                Command::new("tesseract")
                    .arg(img_path)
                    .arg(output_base.to_string_lossy().as_ref())
                    .arg("-l")
                    .arg(language),
                EXTRACTION_CMD_TIMEOUT_SECS,
            )
            .await;

            match result {
                Ok(()) => {
                    if let Ok(text) = fs::read_to_string(&output_path) {
                        page_texts.push(text);
                    }
                }
                Err(e) => {
                    warn!(page = i + 1, error = %e, "OCR failed for page, skipping");
                    page_texts.push(format!("[OCR failed for page {}]", i + 1));
                }
            }
        }

        let full_text = page_texts.join("\n\n--- Page Break ---\n\n");
        let char_count = full_text.len();
        let line_count = full_text.lines().count();
        let page_count = page_images.len();

        Ok(ExtractionResult {
            extracted_text: Some(full_text),
            metadata: json!({
                "ocr_pages": page_count,
                "dpi": dpi,
                "language": language,
                "char_count": char_count,
                "line_count": line_count,
                "engine": "tesseract",
            }),
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Check both pdftoppm and tesseract are available
        let pdftoppm_ok = match Command::new("pdftoppm").arg("-v").output().await {
            Ok(output) => output.status.success() || output.status.code() == Some(99),
            Err(_) => false,
        };
        let tesseract_ok = match Command::new("tesseract").arg("--version").output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        };
        Ok(pdftoppm_ok && tesseract_ok)
    }

    fn name(&self) -> &str {
        "pdf_ocr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_ocr_strategy() {
        let adapter = PdfOcrAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::PdfOcr);
    }

    #[test]
    fn test_pdf_ocr_name() {
        let adapter = PdfOcrAdapter;
        assert_eq!(adapter.name(), "pdf_ocr");
    }

    #[tokio::test]
    async fn test_pdf_ocr_health_check() {
        let adapter = PdfOcrAdapter;
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        // Result depends on whether tesseract/pdftoppm are installed
    }

    #[tokio::test]
    async fn test_pdf_ocr_empty_input() {
        let adapter = PdfOcrAdapter;
        let result = adapter
            .extract(b"", "empty.pdf", "application/pdf", &json!({}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn test_pdf_ocr_invalid_pdf() {
        let adapter = PdfOcrAdapter;
        let result = adapter
            .extract(b"not a pdf", "bad.pdf", "application/pdf", &json!({}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a valid PDF"));
    }

    #[tokio::test]
    async fn test_pdf_ocr_config_parsing() {
        // Test that config values are read (even if OCR tools aren't installed)
        let adapter = PdfOcrAdapter;
        let config = json!({ "dpi": 150, "language": "deu" });

        // This will fail because the PDF is invalid, but config parsing happens first
        let result = adapter
            .extract(b"%PDF-1.0\n", "test.pdf", "application/pdf", &config)
            .await;
        // The error will be about pdftoppm failing, not config parsing
        // Just verify it doesn't panic
        let _ = result;
    }
}

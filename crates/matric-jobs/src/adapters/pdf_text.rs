//! PdfText extraction adapter — extracts text from PDFs using `pdftotext` (poppler-utils).

use std::io::Write;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tempfile::NamedTempFile;
use tokio::process::Command;
use tracing::{debug, warn};

use matric_core::defaults::{
    EXTRACTION_CMD_TIMEOUT_SECS, LARGE_PDF_PAGE_THRESHOLD, PDF_BATCH_PAGES,
};
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Adapter for extracting text from PDF files using `pdftotext` (poppler-utils).
///
/// For large PDFs (> 100 pages), extraction is batched in 50-page chunks to
/// bound memory usage. Each `pdftotext` invocation is guarded by a per-command
/// timeout.
///
/// If extraction yields empty/near-empty text, `metadata["needs_ocr"]` is set
/// to `true` as a signal for a future `PdfOcrAdapter`.
pub struct PdfTextAdapter;

/// Parse `pdfinfo` output into a JSON metadata object.
///
/// Strips null bytes (`\0`) from values — old PDFs (e.g. Acrobat 3.0/4.0
/// scans) embed `\u0000` in metadata fields like `Creator`, which PostgreSQL
/// rejects in JSON/text columns.
fn parse_pdfinfo(output: &str) -> JsonValue {
    let mut metadata = serde_json::Map::new();

    for line in output.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase().replace(' ', "_");
            let value = value.trim().replace('\0', "");
            if !value.is_empty() {
                // Parse page count as number
                if key == "pages" {
                    if let Ok(pages) = value.parse::<u64>() {
                        metadata.insert(key, JsonValue::Number(pages.into()));
                        continue;
                    }
                }
                metadata.insert(key, JsonValue::String(value));
            }
        }
    }

    JsonValue::Object(metadata)
}

/// Get page count from pdfinfo metadata, defaulting to 0.
fn page_count(metadata: &JsonValue) -> usize {
    metadata.get("pages").and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

fn pdf_text_len(text: &str) -> usize {
    text.len()
}

fn pdf_text_io_error_kind(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::TimedOut => "timed_out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::WouldBlock => "would_block",
        _ => "io_error",
    }
}

fn pdf_text_stderr_reason_code(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("syntax error")
        || text.contains("invalid")
        || text.contains("xref")
        || text.contains("not a pdf")
    {
        "invalid_pdf"
    } else if text.contains("not found") || text.contains("no such") {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else {
        "command_failed"
    }
}

fn pdf_text_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("invalid") || text.contains("xref") || text.contains("not a pdf") {
        "invalid_pdf"
    } else if text.contains("not found") || text.contains("no such") {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else {
        "operation_failed"
    }
}

fn pdf_text_command_failure_detail(
    command: &'static str,
    phase: &'static str,
    status_code: Option<i32>,
    stderr: &[u8],
) -> String {
    format!(
        "{command} {phase} failed; status={}; stderr_len={}; stderr_reason={}",
        status_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
        stderr.len(),
        pdf_text_stderr_reason_code(stderr)
    )
}

fn pdf_text_io_failure_detail(phase: &'static str, error: &std::io::Error) -> String {
    format!(
        "PDF temp IO failed; phase={phase}; io_error_kind={}",
        pdf_text_io_error_kind(error)
    )
}

/// Run a command with a timeout, returning stdout as a string.
async fn run_cmd_with_timeout(
    cmd: &mut Command,
    timeout_secs: u64,
    command: &'static str,
    phase: &'static str,
) -> Result<String> {
    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| {
            matric_core::Error::Internal(format!(
                "External command timed out after {}s",
                timeout_secs
            ))
        })?
        .map_err(|e| {
            matric_core::Error::Internal(format!(
                "{command} {phase} failed to start; io_error_kind={}",
                pdf_text_io_error_kind(&e)
            ))
        })?;

    if !output.status.success() {
        return Err(matric_core::Error::Internal(
            pdf_text_command_failure_detail(command, phase, output.status.code(), &output.stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[async_trait]
impl ExtractionAdapter for PdfTextAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::PdfText
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot extract text from empty PDF data".to_string(),
            ));
        }

        // Validate PDF magic bytes (%PDF)
        if data.len() < 4 || &data[0..4] != b"%PDF" {
            return Err(matric_core::Error::InvalidInput(format!(
                "File is not a valid PDF; filename_len={}; reason=missing_pdf_header",
                pdf_text_len(filename)
            )));
        }

        // Write data to a temporary file (pdftotext reads from file path)
        let mut tmpfile = NamedTempFile::new().map_err(|e| {
            matric_core::Error::Internal(pdf_text_io_failure_detail("create_temp_file", &e))
        })?;
        tmpfile.write_all(data).map_err(|e| {
            matric_core::Error::Internal(pdf_text_io_failure_detail("write_temp_file", &e))
        })?;
        let tmp_path = tmpfile.path().to_string_lossy().to_string();

        // Get metadata from pdfinfo
        let pdfinfo_output = run_cmd_with_timeout(
            Command::new("pdfinfo").arg(&tmp_path),
            EXTRACTION_CMD_TIMEOUT_SECS,
            "pdfinfo",
            "metadata",
        )
        .await;

        let mut metadata = match pdfinfo_output {
            Ok(output) => parse_pdfinfo(&output),
            Err(e) => {
                let error_text = e.to_string();
                warn!(
                    filename_len = pdf_text_len(filename),
                    error_len = pdf_text_len(&error_text),
                    error_reason = pdf_text_error_reason_code(&error_text),
                    "pdfinfo failed, continuing without metadata"
                );
                serde_json::json!({})
            }
        };

        // Extract text
        let pages = page_count(&metadata);
        let text = if pages > LARGE_PDF_PAGE_THRESHOLD {
            // Batch extraction for large PDFs
            debug!(
                filename_len = pdf_text_len(filename),
                pages, "Large PDF detected, extracting in batches"
            );
            let mut chunks = Vec::new();
            let mut start = 1usize;
            while start <= pages {
                let end = (start + PDF_BATCH_PAGES - 1).min(pages);
                let chunk = run_cmd_with_timeout(
                    Command::new("pdftotext")
                        .arg("-f")
                        .arg(start.to_string())
                        .arg("-l")
                        .arg(end.to_string())
                        .arg(&tmp_path)
                        .arg("-"),
                    EXTRACTION_CMD_TIMEOUT_SECS,
                    "pdftotext",
                    "batch_extract",
                )
                .await?;
                chunks.push(chunk);
                start = end + 1;
            }
            chunks.join("")
        } else {
            // Single extraction for small PDFs (or when page count is unknown)
            run_cmd_with_timeout(
                Command::new("pdftotext").arg(&tmp_path).arg("-"),
                EXTRACTION_CMD_TIMEOUT_SECS,
                "pdftotext",
                "extract",
            )
            .await?
        };

        // Strip null bytes — old PDFs can embed \0 in text content too
        let text = text.replace('\0', "");

        // Signal if OCR might be needed (text-layer PDFs with scanned content)
        let trimmed_len = text.trim().len();
        if trimmed_len < 50 && pages > 0 {
            metadata
                .as_object_mut()
                .unwrap()
                .insert("needs_ocr".to_string(), JsonValue::Bool(true));
        }

        // Add char/line count
        let char_count = text.len();
        let line_count = text.lines().count();
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "char_count".to_string(),
                JsonValue::Number(char_count.into()),
            );
            obj.insert(
                "line_count".to_string(),
                JsonValue::Number(line_count.into()),
            );
        }

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata,
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        })
    }

    async fn health_check(&self) -> Result<bool> {
        match Command::new("pdftotext").arg("-v").output().await {
            Ok(output) => {
                // pdftotext -v prints version to stderr and exits with 0 or 99
                // depending on the version. Both indicate the binary exists.
                Ok(output.status.success() || output.status.code() == Some(99))
            }
            Err(_) => Ok(false),
        }
    }

    fn name(&self) -> &str {
        "pdf_text"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_input_message(error: matric_core::Error) -> String {
        match error {
            matric_core::Error::InvalidInput(message) => message,
            other => panic!("expected invalid input error, got {other:?}"),
        }
    }

    #[test]
    fn test_pdf_text_strategy() {
        let adapter = PdfTextAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::PdfText);
    }

    #[test]
    fn test_pdf_text_name() {
        let adapter = PdfTextAdapter;
        assert_eq!(adapter.name(), "pdf_text");
    }

    #[test]
    fn pdf_text_command_failure_detail_redacts_stderr() {
        let stderr = b"Syntax Error at /srv/fortemi/private/file.pdf token=mm_key_secret";
        let detail = pdf_text_command_failure_detail("pdftotext", "extract", Some(1), stderr);

        assert!(detail.contains("pdftotext extract failed"));
        assert!(detail.contains("status=1"));
        assert!(detail.contains("stderr_len="));
        assert!(detail.contains("stderr_reason=invalid_pdf"));
        assert!(!detail.contains("/srv/fortemi"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("Syntax Error"));
    }

    #[test]
    fn pdf_text_log_metadata_omits_raw_filename_and_error() {
        let filename = "customer-token-mm_key_secret.pdf";
        let error = "permission denied at /srv/fortemi/private/customer-token-mm_key_secret.pdf";
        let detail = format!(
            "filename_len={}; error_len={}; error_reason={}",
            pdf_text_len(filename),
            pdf_text_len(error),
            pdf_text_error_reason_code(error)
        );

        assert!(detail.contains("filename_len="));
        assert!(detail.contains("error_len="));
        assert!(detail.contains("permission_denied"));
        assert!(!detail.contains("customer-token"));
        assert!(!detail.contains("mm_key_secret"));
        assert!(!detail.contains("/srv/fortemi"));
    }

    #[test]
    fn pdf_text_stderr_reason_code_uses_stable_classes() {
        assert_eq!(
            pdf_text_stderr_reason_code(b"Permission denied"),
            "permission_denied"
        );
        assert_eq!(pdf_text_stderr_reason_code(b"xref invalid"), "invalid_pdf");
        assert_eq!(pdf_text_stderr_reason_code(b"No such file"), "not_found");
        assert_eq!(
            pdf_text_stderr_reason_code(b"request timed out"),
            "timed_out"
        );
        assert_eq!(
            pdf_text_stderr_reason_code(b"opaque backend detail"),
            "command_failed"
        );
    }

    #[test]
    fn pdf_text_io_failure_detail_redacts_os_diagnostics() {
        let error = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied at /srv/fortemi/private/sk-live.pdf",
        );
        let detail = pdf_text_io_failure_detail("write_temp_file", &error);

        assert!(detail.contains("PDF temp IO failed"));
        assert!(detail.contains("phase=write_temp_file"));
        assert!(detail.contains("io_error_kind=permission_denied"));
        assert!(!detail.contains("/srv/fortemi"));
        assert!(!detail.contains("sk-live"));
        assert!(!detail.contains("permission denied at"));
    }

    #[tokio::test]
    async fn test_pdf_text_health_check() {
        let adapter = PdfTextAdapter;
        // This test passes if pdftotext is installed (CI) or returns false if not
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pdf_text_empty_input() {
        let adapter = PdfTextAdapter;
        let result = adapter
            .extract(b"", "empty.pdf", "application/pdf", &serde_json::json!({}))
            .await;
        assert!(result.is_err());
        // Error Display is redacted; assert on the typed variant's inner message.
        let err = match result.unwrap_err() {
            matric_core::Error::InvalidInput(msg) => msg,
            other => panic!("expected InvalidInput, got: {other:?}"),
        };
        assert!(
            err.contains("empty"),
            "Error should mention empty data: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_pdf_text_invalid_pdf() {
        let adapter = PdfTextAdapter;
        let filename = "bad-token-sk-live.pdf";
        let result = adapter
            .extract(
                b"not a pdf at all",
                filename,
                "application/pdf",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_err());
        let err = invalid_input_message(result.unwrap_err());
        assert!(
            err.contains("not a valid PDF"),
            "Error should mention invalid PDF: {}",
            err
        );
        assert!(err.contains("filename_len="));
        assert!(err.contains("reason=missing_pdf_header"));
        assert!(!err.contains(filename));
        assert!(!err.contains("sk-live"));
    }

    #[tokio::test]
    async fn test_pdf_text_extraction() {
        // Minimal valid PDF that contains the text "Hello World"
        // Generated from the PDF spec: header, catalog, page, content stream, xref
        let pdf_bytes = b"%PDF-1.0
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj

2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj

3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]
   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj

4 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 100 700 Td (Hello World) Tj ET
endstream
endobj

5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj

xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000266 00000 n
0000000360 00000 n

trailer
<< /Size 6 /Root 1 0 R >>
startxref
434
%%EOF";

        let adapter = PdfTextAdapter;
        // Only run if pdftotext is available
        if !adapter.health_check().await.unwrap_or(false) {
            eprintln!("Skipping test_pdf_text_extraction: pdftotext not installed");
            return;
        }

        let result = adapter
            .extract(
                pdf_bytes,
                "hello.pdf",
                "application/pdf",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_ok(), "Extraction failed: {:?}", result.err());
        let extraction = result.unwrap();
        let text = extraction.extracted_text.unwrap();
        assert!(
            text.contains("Hello World"),
            "Extracted text should contain 'Hello World', got: {}",
            text
        );
        assert!(extraction.metadata.get("char_count").is_some());
        assert!(extraction.metadata.get("line_count").is_some());
    }

    #[test]
    fn test_pdfinfo_metadata_parsing() {
        let pdfinfo_output = "\
Title:          Test Document
Author:         John Doe
Creator:        LaTeX
Producer:       pdfTeX-1.40.25
CreationDate:   Tue Jan  7 10:30:00 2025
Pages:          42
Page size:      612 x 792 pts (letter)
";
        let metadata = parse_pdfinfo(pdfinfo_output);
        assert_eq!(metadata["title"], "Test Document");
        assert_eq!(metadata["author"], "John Doe");
        assert_eq!(metadata["producer"], "pdfTeX-1.40.25");
        assert_eq!(metadata["pages"], 42);
        assert_eq!(metadata["page_size"], "612 x 792 pts (letter)");
    }

    #[test]
    fn test_pdfinfo_empty_output() {
        let metadata = parse_pdfinfo("");
        assert!(metadata.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_page_count_extraction() {
        let meta = serde_json::json!({"pages": 150});
        assert_eq!(page_count(&meta), 150);

        let meta_no_pages = serde_json::json!({});
        assert_eq!(page_count(&meta_no_pages), 0);

        let meta_string_pages = serde_json::json!({"pages": "not a number"});
        assert_eq!(page_count(&meta_string_pages), 0);
    }
}

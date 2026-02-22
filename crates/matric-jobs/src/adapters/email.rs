//! Email extraction adapter - handles .eml and .mbox files.
//!
//! Parses RFC 2822/MIME email messages using the `mailparse` crate.
//! For `.mbox` files, splits on `^From ` separator lines and processes
//! each message individually, aggregating results into a single text body
//! and unified metadata.
//!
//! **Extracted text** is assembled as a human-readable transcript:
//! - Per-message headers (From, To, Subject, Date, ...)
//! - Body (text/plain preferred; text/html stripped of tags as fallback)
//! - Messages separated by a rule line in mbox mode
//!
//! **Metadata** shape:
//! ```json
//! {
//!   "format": "eml" | "mbox",
//!   "message_count": 1,
//!   "messages": [
//!     {
//!       "from":        "...",
//!       "to":          "...",
//!       "cc":          "...",
//!       "bcc":         "...",
//!       "subject":     "...",
//!       "date":        "...",
//!       "message_id":  "...",
//!       "in_reply_to": "...",
//!       "attachments": [
//!         { "filename": "...", "content_type": "...", "size": 0 }
//!       ]
//!     }
//!   ]
//! }
//! ```

use async_trait::async_trait;
use mailparse::{parse_mail, MailHeaderMap};
use serde_json::Value as JsonValue;

use matric_core::{DerivedFile, ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Public adapter struct
// ─────────────────────────────────────────────────────────────────────────────

/// Adapter for extracting content from email files (`.eml`, `.mbox`).
///
/// Pure-Rust implementation with no external process dependencies.
/// Uses `mailparse` for RFC 2822/MIME parsing.
pub struct EmailAdapter;

// ─────────────────────────────────────────────────────────────────────────────
// ExtractionAdapter impl
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ExtractionAdapter for EmailAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Email
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        // Determine format from filename extension; default to eml.
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_mbox = ext == "mbox";

        if is_mbox {
            extract_mbox(data)
        } else {
            extract_eml(data)
        }
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // Pure-Rust, no external dependencies
    }

    fn name(&self) -> &str {
        "email"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EML extraction (single message)
// ─────────────────────────────────────────────────────────────────────────────

fn extract_eml(data: &[u8]) -> Result<ExtractionResult> {
    if data.is_empty() {
        return Ok(ExtractionResult {
            extracted_text: None,
            metadata: serde_json::json!({
                "format": "eml",
                "message_count": 0,
                "messages": []
            }),
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        });
    }

    let parsed = parse_mail(data)
        .map_err(|e| matric_core::Error::Internal(format!("Failed to parse email: {e}")))?;

    let msg_meta = extract_message_metadata(&parsed);
    let body = extract_body(&parsed);
    let derived_files = extract_derived_files(&parsed);

    let text = format_message_text(&msg_meta, &body);

    Ok(ExtractionResult {
        extracted_text: if text.is_empty() { None } else { Some(text) },
        metadata: serde_json::json!({
            "format": "eml",
            "message_count": 1,
            "messages": [msg_meta]
        }),
        ai_description: None,
        preview_data: None,
        derived_files,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Mbox extraction (multiple messages)
// ─────────────────────────────────────────────────────────────────────────────

fn extract_mbox(data: &[u8]) -> Result<ExtractionResult> {
    if data.is_empty() {
        return Ok(ExtractionResult {
            extracted_text: None,
            metadata: serde_json::json!({
                "format": "mbox",
                "message_count": 0,
                "messages": []
            }),
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        });
    }

    let messages = split_mbox(data);

    if messages.is_empty() {
        // No valid From_ lines — treat as a single bare message
        return extract_single_mbox_message(data, "mbox");
    }

    let mut text_parts: Vec<String> = Vec::new();
    let mut messages_meta: Vec<JsonValue> = Vec::new();
    let mut all_derived_files: Vec<DerivedFile> = Vec::new();

    for (i, msg_bytes) in messages.iter().enumerate() {
        match parse_mail(msg_bytes) {
            Ok(parsed) => {
                let msg_meta = extract_message_metadata(&parsed);
                let body = extract_body(&parsed);
                let msg_text = format_message_text(&msg_meta, &body);
                let mut derived = extract_derived_files(&parsed);

                if !msg_text.is_empty() {
                    if i > 0 {
                        text_parts.push("─".repeat(72));
                    }
                    text_parts.push(msg_text);
                }
                messages_meta.push(msg_meta);
                all_derived_files.append(&mut derived);
            }
            Err(_) => {
                // Skip unparseable individual messages — log nothing (silent skip)
            }
        }
    }

    let full_text = text_parts.join("\n\n");

    Ok(ExtractionResult {
        extracted_text: if full_text.is_empty() {
            None
        } else {
            Some(full_text)
        },
        metadata: serde_json::json!({
            "format": "mbox",
            "message_count": messages_meta.len(),
            "messages": messages_meta
        }),
        ai_description: None,
        preview_data: None,
        derived_files: all_derived_files,
    })
}

/// Fallback when mbox data has no From_ separators — treat as single message.
fn extract_single_mbox_message(data: &[u8], format: &str) -> Result<ExtractionResult> {
    match parse_mail(data) {
        Ok(parsed) => {
            let msg_meta = extract_message_metadata(&parsed);
            let body = extract_body(&parsed);
            let derived_files = extract_derived_files(&parsed);
            let text = format_message_text(&msg_meta, &body);

            Ok(ExtractionResult {
                extracted_text: if text.is_empty() { None } else { Some(text) },
                metadata: serde_json::json!({
                    "format": format,
                    "message_count": 1,
                    "messages": [msg_meta]
                }),
                ai_description: None,
                preview_data: None,
                derived_files,
            })
        }
        Err(_) => Ok(ExtractionResult {
            extracted_text: None,
            metadata: serde_json::json!({
                "format": format,
                "message_count": 0,
                "messages": []
            }),
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mbox splitting
// ─────────────────────────────────────────────────────────────────────────────

/// Splits raw mbox bytes into individual message byte slices.
///
/// mbox format: each message begins with a line that starts with `"From "`
/// (followed by an email address and a date). This function uses a
/// byte-level line scan rather than UTF-8 conversion to handle arbitrary
/// binary attachments embedded in messages.
fn split_mbox(data: &[u8]) -> Vec<Vec<u8>> {
    let mut messages: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut is_first_separator = true;

    // Walk through lines without converting the entire buffer to UTF-8
    for line in data.split(|&b| b == b'\n') {
        let is_from_line = line.starts_with(b"From ");

        if is_from_line && !is_first_separator {
            // End current message, start a new one
            if !current.is_empty() {
                messages.push(std::mem::take(&mut current));
            }
            // Don't include the "From " separator line itself in the message body
            continue;
        }

        if is_from_line && is_first_separator {
            // First separator found — don't include it in the message body
            is_first_separator = false;
            continue;
        }

        is_first_separator = false;

        // Re-add the newline that split() consumed
        if !current.is_empty() || !line.is_empty() {
            current.extend_from_slice(line);
            current.push(b'\n');
        }
    }

    // Push the last message if it has content
    if !current.is_empty() {
        messages.push(current);
    }

    messages
}

// ─────────────────────────────────────────────────────────────────────────────
// Header and body extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extracts well-known headers from a parsed email into a JSON object.
fn extract_message_metadata(mail: &mailparse::ParsedMail<'_>) -> JsonValue {
    let headers = &mail.headers;

    let get =
        |name: &str| -> Option<String> { headers.get_first_value(name).filter(|s| !s.is_empty()) };

    let attachments = enumerate_attachments(mail);

    serde_json::json!({
        "from":        get("From"),
        "to":          get("To"),
        "cc":          get("Cc"),
        "bcc":         get("Bcc"),
        "subject":     get("Subject"),
        "date":        get("Date"),
        "message_id":  get("Message-ID"),
        "in_reply_to": get("In-Reply-To"),
        "attachments": attachments
    })
}

/// Walks the MIME tree and collects attachment metadata.
///
/// An attachment is any `multipart/*` subpart that is NOT `text/plain`
/// or `text/html` at the top level, or that has a `Content-Disposition: attachment`
/// header. We record filename, content-type, and size in bytes.
fn enumerate_attachments(mail: &mailparse::ParsedMail<'_>) -> Vec<JsonValue> {
    let mut attachments = Vec::new();
    collect_attachments(mail, &mut attachments, true);
    attachments
}

fn collect_attachments(
    part: &mailparse::ParsedMail<'_>,
    out: &mut Vec<JsonValue>,
    is_top_level: bool,
) {
    let content_type = part.ctype.mimetype.to_ascii_lowercase();

    if content_type.starts_with("multipart/") {
        // Recurse into multipart subparts (they are no longer top-level)
        for subpart in &part.subparts {
            collect_attachments(subpart, out, false);
        }
        return;
    }

    // Check Content-Disposition header
    let disposition = part
        .headers
        .get_first_value("Content-Disposition")
        .unwrap_or_default()
        .to_ascii_lowercase();

    let is_attachment_disposition = disposition.starts_with("attachment");

    // A part is a body part (not an attachment) if:
    // - It has no "attachment" disposition, AND
    // - It is a text/plain or text/html type (whether top-level or in multipart/alternative)
    let is_text_body = content_type == "text/plain" || content_type == "text/html";

    if is_attachment_disposition || (!is_top_level && !is_text_body) {
        // This part is an attachment: record its metadata
        let filename = extract_part_filename(part);
        let size = part.get_body_raw().map(|b| b.len()).unwrap_or(0);

        out.push(serde_json::json!({
            "filename":     filename,
            "content_type": &part.ctype.mimetype,
            "size":         size
        }));
    }
}

/// Extracts binary attachment data from a parsed email as `DerivedFile` entries.
///
/// Walks the MIME tree using the same logic as `collect_attachments` but also
/// reads the decoded body bytes. Each attachment becomes a `DerivedFile` that
/// the extraction handler will store as a derived attachment on the note.
fn extract_derived_files(mail: &mailparse::ParsedMail<'_>) -> Vec<DerivedFile> {
    let mut files = Vec::new();
    collect_derived_files(mail, &mut files, true);
    files
}

fn collect_derived_files(
    part: &mailparse::ParsedMail<'_>,
    out: &mut Vec<DerivedFile>,
    is_top_level: bool,
) {
    let content_type = part.ctype.mimetype.to_ascii_lowercase();

    if content_type.starts_with("multipart/") {
        for subpart in &part.subparts {
            collect_derived_files(subpart, out, false);
        }
        return;
    }

    let disposition = part
        .headers
        .get_first_value("Content-Disposition")
        .unwrap_or_default()
        .to_ascii_lowercase();

    let is_attachment_disposition = disposition.starts_with("attachment");
    let is_text_body = content_type == "text/plain" || content_type == "text/html";

    if is_attachment_disposition || (!is_top_level && !is_text_body) {
        if let Ok(data) = part.get_body_raw() {
            if !data.is_empty() {
                let filename = extract_part_filename(part)
                    .unwrap_or_else(|| format!("attachment_{}", out.len()));
                out.push(DerivedFile {
                    filename,
                    content_type: part.ctype.mimetype.clone(),
                    data,
                    derivation_type: "email_attachment".to_string(),
                    ai_description: None,
                });
            }
        }
    }
}

/// Extracts a filename for a MIME part from its disposition or content-type params.
fn extract_part_filename(part: &mailparse::ParsedMail<'_>) -> Option<String> {
    // Try Content-Disposition "filename=" parameter first
    let disposition = part
        .headers
        .get_first_value("Content-Disposition")
        .unwrap_or_default();

    if let Some(name) = extract_param_value(&disposition, "filename") {
        return Some(name);
    }

    // Fall back to Content-Type "name=" parameter
    let ct_header = part
        .headers
        .get_first_value("Content-Type")
        .unwrap_or_default();

    if let Some(name) = extract_param_value(&ct_header, "name") {
        return Some(name);
    }

    None
}

/// Parses `key=value` or `key="value"` from a header field string.
fn extract_param_value(header: &str, param: &str) -> Option<String> {
    let needle = format!("{param}=");
    let lower = header.to_ascii_lowercase();
    let start = lower.find(needle.as_str())?;
    let rest = &header[start + needle.len()..];

    if let Some(rest) = rest.strip_prefix('"') {
        // Quoted string — take until closing quote
        let end = rest.find('"').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    } else {
        // Unquoted — take until semicolon or end
        let end = rest.find(';').unwrap_or(rest.len());
        let val = rest[..end].trim();
        if val.is_empty() {
            None
        } else {
            Some(val.to_string())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Body extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Extracts the human-readable body from a parsed email.
///
/// Strategy:
/// 1. Walk the MIME tree to find a `text/plain` part — use it directly.
/// 2. If no `text/plain` is found, look for `text/html` and strip tags.
/// 3. If neither is found, return an empty string.
fn extract_body(mail: &mailparse::ParsedMail<'_>) -> String {
    // Try text/plain first
    if let Some(text) = find_part_body(mail, "text/plain") {
        return text;
    }
    // Fall back to text/html
    if let Some(html) = find_part_body(mail, "text/html") {
        return strip_html_tags(&html);
    }
    String::new()
}

/// Recursively searches the MIME tree for the first part matching `target_mime`.
fn find_part_body(mail: &mailparse::ParsedMail<'_>, target_mime: &str) -> Option<String> {
    let mime = mail.ctype.mimetype.to_ascii_lowercase();

    if mime == target_mime {
        return mail.get_body().ok().filter(|s| !s.is_empty());
    }

    if mime.starts_with("multipart/") {
        for subpart in &mail.subparts {
            if let Some(body) = find_part_body(subpart, target_mime) {
                return Some(body);
            }
        }
    }

    None
}

/// Strips HTML tags from a string, leaving only the text nodes.
///
/// Handles `<` and `>` as tag delimiters. Decodes a small set of common
/// HTML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&nbsp;`, `&#39;`).
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ─────────────────────────────────────────────────────────────────────────────
// Text formatting
// ─────────────────────────────────────────────────────────────────────────────

/// Formats a single message's headers and body into a human-readable string.
fn format_message_text(meta: &JsonValue, body: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    let headers = [
        ("From", "from"),
        ("To", "to"),
        ("Cc", "cc"),
        ("Bcc", "bcc"),
        ("Subject", "subject"),
        ("Date", "date"),
        ("Message-ID", "message_id"),
        ("In-Reply-To", "in_reply_to"),
    ];

    for (label, key) in &headers {
        if let Some(val) = meta.get(*key).and_then(|v| v.as_str()) {
            if !val.is_empty() {
                parts.push(format!("{label}: {val}"));
            }
        }
    }

    if !parts.is_empty() && !body.is_empty() {
        parts.push(String::new()); // blank line between headers and body
    }

    if !body.is_empty() {
        parts.push(body.to_string());
    }

    parts.join("\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper fixtures ────────────────────────────────────────────────────

    /// Minimal RFC 2822 .eml message with text/plain body.
    fn simple_eml() -> Vec<u8> {
        concat!(
            "From: Alice <alice@example.com>\r\n",
            "To: Bob <bob@example.com>\r\n",
            "Subject: Hello\r\n",
            "Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n",
            "Message-ID: <abc123@example.com>\r\n",
            "\r\n",
            "Hello Bob, this is a test message.\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    /// .eml with CC, BCC, and In-Reply-To headers.
    fn full_headers_eml() -> Vec<u8> {
        concat!(
            "From: Alice <alice@example.com>\r\n",
            "To: Bob <bob@example.com>\r\n",
            "Cc: Carol <carol@example.com>\r\n",
            "Bcc: Dave <dave@example.com>\r\n",
            "Subject: RE: Test\r\n",
            "Date: Tue, 2 Jan 2024 09:00:00 +0000\r\n",
            "Message-ID: <reply123@example.com>\r\n",
            "In-Reply-To: <orig123@example.com>\r\n",
            "\r\n",
            "This is a reply.\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    /// Multipart .eml with text/plain and text/html alternatives.
    fn multipart_eml() -> Vec<u8> {
        concat!(
            "From: Sender <sender@example.com>\r\n",
            "To: Recipient <recipient@example.com>\r\n",
            "Subject: Multipart test\r\n",
            "Date: Wed, 3 Jan 2024 10:00:00 +0000\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/alternative; boundary=\"boundary42\"\r\n",
            "\r\n",
            "--boundary42\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "\r\n",
            "Plain text body.\r\n",
            "--boundary42\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "\r\n",
            "<html><body><p>HTML body.</p></body></html>\r\n",
            "--boundary42--\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    /// .eml with only a text/html body (no text/plain).
    fn html_only_eml() -> Vec<u8> {
        concat!(
            "From: Sender <sender@example.com>\r\n",
            "To: Recipient <recipient@example.com>\r\n",
            "Subject: HTML only\r\n",
            "Date: Thu, 4 Jan 2024 11:00:00 +0000\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "\r\n",
            "<html><body><p>Hello &amp; world!</p></body></html>\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    /// .eml with a text/plain body and an attachment.
    fn eml_with_attachment() -> Vec<u8> {
        concat!(
            "From: Sender <sender@example.com>\r\n",
            "To: Recipient <recipient@example.com>\r\n",
            "Subject: Has Attachment\r\n",
            "Date: Fri, 5 Jan 2024 12:00:00 +0000\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=\"bound99\"\r\n",
            "\r\n",
            "--bound99\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "\r\n",
            "See the attached file.\r\n",
            "--bound99\r\n",
            "Content-Type: application/pdf\r\n",
            "Content-Disposition: attachment; filename=\"report.pdf\"\r\n",
            "Content-Transfer-Encoding: base64\r\n",
            "\r\n",
            "AAAA\r\n",
            "--bound99--\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    /// Minimal mbox with two messages.
    fn simple_mbox() -> Vec<u8> {
        concat!(
            "From alice@example.com Mon Jan  1 12:00:00 2024\r\n",
            "From: Alice <alice@example.com>\r\n",
            "To: Bob <bob@example.com>\r\n",
            "Subject: First message\r\n",
            "\r\n",
            "First message body.\r\n",
            "\r\n",
            "From bob@example.com Tue Jan  2 09:00:00 2024\r\n",
            "From: Bob <bob@example.com>\r\n",
            "To: Alice <alice@example.com>\r\n",
            "Subject: Second message\r\n",
            "\r\n",
            "Second message body.\r\n",
        )
        .as_bytes()
        .to_vec()
    }

    // ── Adapter metadata tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_email_strategy() {
        let adapter = EmailAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::Email);
    }

    #[tokio::test]
    async fn test_email_name() {
        let adapter = EmailAdapter;
        assert_eq!(adapter.name(), "email");
    }

    #[tokio::test]
    async fn test_email_health_check() {
        let adapter = EmailAdapter;
        assert!(adapter.health_check().await.unwrap());
    }

    // ── Empty data handling ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_eml_returns_no_text() {
        let adapter = EmailAdapter;
        let result = adapter
            .extract(b"", "empty.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();

        assert!(result.extracted_text.is_none());
        assert_eq!(result.metadata["format"], "eml");
        assert_eq!(result.metadata["message_count"], 0);
        assert!(result.metadata["messages"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_empty_mbox_returns_no_text() {
        let adapter = EmailAdapter;
        let result = adapter
            .extract(
                b"",
                "empty.mbox",
                "application/mbox",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert!(result.extracted_text.is_none());
        assert_eq!(result.metadata["format"], "mbox");
        assert_eq!(result.metadata["message_count"], 0);
    }

    // ── Single .eml header extraction ─────────────────────────────────────

    #[tokio::test]
    async fn test_eml_basic_headers_extracted() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "test.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["format"], "eml");
        assert_eq!(result.metadata["message_count"], 1);

        let msg = &result.metadata["messages"][0];
        assert_eq!(msg["from"].as_str().unwrap(), "Alice <alice@example.com>");
        assert_eq!(msg["to"].as_str().unwrap(), "Bob <bob@example.com>");
        assert_eq!(msg["subject"].as_str().unwrap(), "Hello");
        assert!(msg["date"].as_str().is_some());
        assert_eq!(msg["message_id"].as_str().unwrap(), "<abc123@example.com>");
    }

    #[tokio::test]
    async fn test_eml_full_headers_extracted() {
        let adapter = EmailAdapter;
        let data = full_headers_eml();
        let result = adapter
            .extract(&data, "reply.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();

        let msg = &result.metadata["messages"][0];
        assert_eq!(msg["cc"].as_str().unwrap(), "Carol <carol@example.com>");
        assert_eq!(msg["bcc"].as_str().unwrap(), "Dave <dave@example.com>");
        assert_eq!(
            msg["in_reply_to"].as_str().unwrap(),
            "<orig123@example.com>"
        );
    }

    // ── Body extraction: text/plain preferred ──────────────────────────────

    #[tokio::test]
    async fn test_eml_plain_text_body_extracted() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "test.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(
            text.contains("Hello Bob, this is a test message."),
            "Body text missing; got: {text}"
        );
    }

    #[tokio::test]
    async fn test_multipart_prefers_plain_over_html() {
        let adapter = EmailAdapter;
        let data = multipart_eml();
        let result = adapter
            .extract(
                &data,
                "multipart.eml",
                "message/rfc822",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        // Should contain the text/plain part
        assert!(
            text.contains("Plain text body."),
            "Expected plain text body; got: {text}"
        );
        // Should NOT contain raw HTML tags
        assert!(
            !text.contains("<html>"),
            "Should not contain raw HTML tags; got: {text}"
        );
    }

    // ── Body extraction: text/html fallback ───────────────────────────────

    #[tokio::test]
    async fn test_html_only_email_strips_tags() {
        let adapter = EmailAdapter;
        let data = html_only_eml();
        let result = adapter
            .extract(
                &data,
                "htmlonly.eml",
                "message/rfc822",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        // Tags should be stripped
        assert!(
            !text.contains("<html>") && !text.contains("<body>"),
            "HTML tags should be stripped; got: {text}"
        );
        // Content should be present
        assert!(
            text.contains("Hello") && text.contains("world"),
            "Text content should be present; got: {text}"
        );
        // HTML entity &amp; should be decoded
        assert!(
            text.contains('&'),
            "&amp; should be decoded to &; got: {text}"
        );
    }

    // ── Attachment enumeration ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_eml_attachment_enumerated_in_metadata() {
        let adapter = EmailAdapter;
        let data = eml_with_attachment();
        let result = adapter
            .extract(
                &data,
                "attached.eml",
                "message/rfc822",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let msg = &result.metadata["messages"][0];
        let attachments = msg["attachments"].as_array().unwrap();
        assert_eq!(
            attachments.len(),
            1,
            "Expected 1 attachment; found: {attachments:?}"
        );

        let att = &attachments[0];
        assert_eq!(att["filename"].as_str().unwrap(), "report.pdf");
        assert_eq!(att["content_type"].as_str().unwrap(), "application/pdf");
        // Size should be a non-negative integer
        assert!(att["size"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_simple_eml_has_no_attachments() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(
                &data,
                "simple.eml",
                "message/rfc822",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let msg = &result.metadata["messages"][0];
        let attachments = msg["attachments"].as_array().unwrap();
        assert!(
            attachments.is_empty(),
            "Simple email should have no attachments; found: {attachments:?}"
        );
    }

    // ── Mbox parsing ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_mbox_split_into_two_messages() {
        let adapter = EmailAdapter;
        let data = simple_mbox();
        let result = adapter
            .extract(
                &data,
                "test.mbox",
                "application/mbox",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["format"], "mbox");
        assert_eq!(
            result.metadata["message_count"].as_u64().unwrap(),
            2,
            "Expected 2 messages; metadata: {:?}",
            result.metadata
        );

        let messages = result.metadata["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);

        // Verify first message
        assert_eq!(messages[0]["subject"].as_str().unwrap(), "First message");
        // Verify second message
        assert_eq!(messages[1]["subject"].as_str().unwrap(), "Second message");
    }

    #[tokio::test]
    async fn test_mbox_extracted_text_contains_both_bodies() {
        let adapter = EmailAdapter;
        let data = simple_mbox();
        let result = adapter
            .extract(
                &data,
                "test.mbox",
                "application/mbox",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(
            text.contains("First message body."),
            "Missing first body; got: {text}"
        );
        assert!(
            text.contains("Second message body."),
            "Missing second body; got: {text}"
        );
    }

    #[tokio::test]
    async fn test_mbox_text_contains_headers() {
        let adapter = EmailAdapter;
        let data = simple_mbox();
        let result = adapter
            .extract(
                &data,
                "test.mbox",
                "application/mbox",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(
            text.contains("Subject: First message"),
            "Missing first subject in text; got: {text}"
        );
        assert!(
            text.contains("Subject: Second message"),
            "Missing second subject in text; got: {text}"
        );
    }

    // ── split_mbox unit tests ──────────────────────────────────────────────

    #[test]
    fn test_split_mbox_two_messages() {
        let data = simple_mbox();
        let parts = split_mbox(&data);
        assert_eq!(parts.len(), 2, "Expected 2 parts from simple mbox");
    }

    #[test]
    fn test_split_mbox_empty() {
        let parts = split_mbox(b"");
        assert!(parts.is_empty());
    }

    #[test]
    fn test_split_mbox_no_separator() {
        // Data with no "From " line — split_mbox returns 0 (caller handles as bare message)
        let data = b"From: alice@example.com\r\nSubject: Test\r\n\r\nBody";
        let parts = split_mbox(data);
        assert_eq!(parts.len(), 1, "Single message without From_ separator");
    }

    #[test]
    fn test_split_mbox_single_message_with_separator() {
        let data = b"From alice@example.com Mon Jan 1 00:00:00 2024\r\nFrom: alice@example.com\r\nSubject: X\r\n\r\nBody\r\n";
        let parts = split_mbox(data);
        assert_eq!(parts.len(), 1);
        let text = String::from_utf8_lossy(&parts[0]);
        assert!(text.contains("Subject: X"), "Subject should be in part");
    }

    // ── strip_html_tags unit tests ─────────────────────────────────────────

    #[test]
    fn test_strip_html_removes_tags() {
        let result = strip_html_tags("<p>Hello <b>world</b>!</p>");
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_strip_html_decodes_entities() {
        let result = strip_html_tags("&amp; &lt;tag&gt; &quot;quote&quot; &#39;apos&#39; &nbsp;");
        assert_eq!(result, "& <tag> \"quote\" 'apos'  ");
    }

    #[test]
    fn test_strip_html_empty_input() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_strip_html_no_tags() {
        let plain = "Just plain text.";
        assert_eq!(strip_html_tags(plain), plain);
    }

    #[test]
    fn test_strip_html_nested_tags() {
        let html = "<div class=\"foo\"><span>Text</span></div>";
        assert_eq!(strip_html_tags(html), "Text");
    }

    // ── extract_param_value unit tests ────────────────────────────────────

    #[test]
    fn test_extract_param_value_quoted() {
        let header = "attachment; filename=\"my file.pdf\"";
        assert_eq!(
            extract_param_value(header, "filename"),
            Some("my file.pdf".to_string())
        );
    }

    #[test]
    fn test_extract_param_value_unquoted() {
        let header = "attachment; filename=report.pdf";
        assert_eq!(
            extract_param_value(header, "filename"),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn test_extract_param_value_missing() {
        let header = "attachment";
        assert_eq!(extract_param_value(header, "filename"), None);
    }

    #[test]
    fn test_extract_param_value_empty_value() {
        let header = "attachment; filename=";
        // Empty unquoted value — returns None
        assert_eq!(extract_param_value(header, "filename"), None);
    }

    // ── Format metadata tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_eml_format_field_is_eml() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "x.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.metadata["format"], "eml");
    }

    #[tokio::test]
    async fn test_mbox_format_field_is_mbox() {
        let adapter = EmailAdapter;
        let data = simple_mbox();
        let result = adapter
            .extract(&data, "x.mbox", "application/mbox", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.metadata["format"], "mbox");
    }

    #[tokio::test]
    async fn test_unknown_extension_defaults_to_eml() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        // No extension — should default to eml behaviour
        let result = adapter
            .extract(&data, "message", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.metadata["format"], "eml");
    }

    // ── Text structure tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_extracted_text_includes_headers_and_body() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "test.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        assert!(text.contains("From: Alice"), "Missing From header in text");
        assert!(text.contains("Subject: Hello"), "Missing Subject in text");
        assert!(
            text.contains("Hello Bob, this is a test message."),
            "Missing body in text"
        );
    }

    #[tokio::test]
    async fn test_ai_description_is_none() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "test.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.ai_description.is_none());
    }

    #[tokio::test]
    async fn test_preview_data_is_none() {
        let adapter = EmailAdapter;
        let data = simple_eml();
        let result = adapter
            .extract(&data, "test.eml", "message/rfc822", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.preview_data.is_none());
    }
}

//! Redaction-safe diagnostics for inference backend errors.

use reqwest::StatusCode;
use std::fmt::Display;

pub(crate) fn text_len(value: &str) -> usize {
    value.chars().count()
}

pub(crate) fn backend_body_reason(body: &str) -> &'static str {
    let lower = body.to_ascii_lowercase();

    if lower.contains("permission") || lower.contains("unauthorized") || lower.contains("forbidden")
    {
        "auth_or_permission"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("not found") || lower.contains("missing") {
        "not_found"
    } else if lower.contains("invalid")
        || lower.contains("parse")
        || lower.contains("json")
        || lower.contains("schema")
    {
        "invalid_response"
    } else if lower.contains("too large") || lower.contains("payload") || lower.contains("size") {
        "payload_size"
    } else if lower.contains("unavailable")
        || lower.contains("overloaded")
        || lower.contains("busy")
        || lower.contains("rate")
    {
        "unavailable"
    } else if body.trim().is_empty() {
        "empty"
    } else {
        "other"
    }
}

pub(crate) fn backend_status_error(service: &str, status: StatusCode, body: &str) -> String {
    format!(
        "{service} API returned status={}; body_len={}; body_reason={}",
        status.as_u16(),
        text_len(body),
        backend_body_reason(body)
    )
}

#[allow(dead_code)]
pub(crate) fn backend_request_error(context: &str, err: &reqwest::Error) -> String {
    match err.status() {
        Some(status) => format!(
            "{context}; error_reason={}; error_len={}; status={}",
            reqwest_error_reason(err),
            text_len(&err.to_string()),
            status.as_u16()
        ),
        None => format!(
            "{context}; error_reason={}; error_len={}",
            reqwest_error_reason(err),
            text_len(&err.to_string())
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn backend_parse_error(context: &str, err: impl Display) -> String {
    let rendered = err.to_string();
    format!("{context}; error_len={}", text_len(&rendered))
}

#[allow(dead_code)]
fn reqwest_error_reason(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_decode() {
        "decode"
    } else if err.is_status() {
        "status"
    } else if err.is_request() {
        "request"
    } else if err.is_body() {
        "body"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_status_error_redacts_response_body() {
        let body = "permission denied for /srv/private/audio.wav token=sk-private";
        let rendered = backend_status_error("Whisper", StatusCode::FORBIDDEN, body);

        assert!(rendered.contains("status=403"));
        assert!(rendered.contains("body_len="));
        assert!(rendered.contains("body_reason=auth_or_permission"));
        assert!(!rendered.contains("/srv/private/audio.wav"));
        assert!(!rendered.contains("sk-private"));
        assert!(!rendered.contains("permission denied"));
    }

    #[test]
    fn backend_body_reason_uses_stable_classes() {
        assert_eq!(backend_body_reason("request timed out"), "timeout");
        assert_eq!(backend_body_reason("not found"), "not_found");
        assert_eq!(backend_body_reason("invalid json"), "invalid_response");
        assert_eq!(backend_body_reason("payload too large"), "payload_size");
        assert_eq!(backend_body_reason("server unavailable"), "unavailable");
        assert_eq!(backend_body_reason(""), "empty");
        assert_eq!(backend_body_reason("opaque backend detail"), "other");
    }

    #[test]
    fn backend_parse_error_redacts_parser_details() {
        let rendered = backend_parse_error(
            "OpenAI chat response parse failed",
            "expected value at /private/path token=sk-private",
        );

        assert!(rendered.contains("OpenAI chat response parse failed"));
        assert!(rendered.contains("error_len="));
        assert!(!rendered.contains("/private/path"));
        assert!(!rendered.contains("sk-private"));
        assert!(!rendered.contains("expected value"));
    }

    #[tokio::test]
    async fn backend_request_error_redacts_reqwest_diagnostics() {
        let err = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(50))
            .build()
            .unwrap()
            .get("http://127.0.0.1:1/private/path?token=sk-private")
            .send()
            .await
            .unwrap_err();

        let rendered = backend_request_error("OpenAI chat request failed", &err);

        assert!(rendered.contains("OpenAI chat request failed"));
        assert!(rendered.contains("error_reason="));
        assert!(rendered.contains("error_len="));
        assert!(!rendered.contains("127.0.0.1"));
        assert!(!rendered.contains("/private/path"));
        assert!(!rendered.contains("sk-private"));
    }
}

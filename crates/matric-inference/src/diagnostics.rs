//! Redaction-safe diagnostics for inference backend errors.

use reqwest::StatusCode;

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
}

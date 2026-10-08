//! Operator switch that disables attachment ingestion, storage and derivative routes (#1160).
//!
//! `FORTEMI_ATTACHMENTS_ENABLED=false` applies in every deployment mode. When it is off:
//! attachment REST routes fail closed with a stable problem response, the served OpenAPI
//! document omits them, the scanner and attachment storage stop being startup
//! prerequisites, and attachment-backed background jobs fail closed.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use matric_jobs::AttachmentScanConfig;

/// Environment variable controlling the switch. Defaults to `true`.
pub const ATTACHMENTS_ENABLED_ENV: &str = "FORTEMI_ATTACHMENTS_ENABLED";
/// Stable machine-readable code carried by every attachments-disabled response.
pub const ATTACHMENTS_DISABLED_CODE: &str = "attachments_disabled";
/// Public, non-sensitive detail text for attachments-disabled responses.
pub const ATTACHMENTS_DISABLED_DETAIL: &str =
    "Attachment ingestion is disabled on this deployment (FORTEMI_ATTACHMENTS_ENABLED=false).";

/// Middleware state carrying the resolved operator switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentsSwitch {
    enabled: bool,
}

impl AttachmentsSwitch {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }
}

/// Parse `FORTEMI_ATTACHMENTS_ENABLED` with the strict boolean grammar.
pub fn parse_attachments_enabled<F>(env: F) -> anyhow::Result<bool>
where
    F: Fn(&str) -> Option<String>,
{
    crate::strict_bool_value(
        ATTACHMENTS_ENABLED_ENV,
        env(ATTACHMENTS_ENABLED_ENV).as_deref(),
        true,
    )
}

/// Startup admission for attachment scanning.
///
/// With attachments enabled the existing contract is unchanged: the scan mode must be
/// explicit and hosted multi-tenant mode requires `required` plus a clamd address.
/// With attachments disabled no scanner configuration is consulted or required.
pub fn resolve_attachment_scan_config<F>(
    attachments_enabled: bool,
    hosted_multi_tenant: bool,
    max_upload_bytes: usize,
    env: F,
) -> anyhow::Result<AttachmentScanConfig>
where
    F: Fn(&str) -> Option<String>,
{
    if !attachments_enabled {
        return Ok(AttachmentScanConfig::attachments_disabled(max_upload_bytes));
    }
    let optional = |name: &str| env(name).filter(|value| !value.trim().is_empty());
    AttachmentScanConfig::from_values(
        hosted_multi_tenant,
        max_upload_bytes,
        env("MATRIC_ATTACHMENT_SCAN_MODE"),
        optional("MATRIC_ATTACHMENT_CLAMD_ADDR"),
        optional("MATRIC_ATTACHMENT_SCAN_TIMEOUT_MS"),
        optional("MATRIC_ATTACHMENT_SCAN_MAX_BYTES"),
    )
}

/// True for every attachment upload, download and derivative route, including TUS
/// uploads, thumbnails, sprites and subtitles. Works on concrete and templated paths.
pub fn is_attachment_path(path: &str) -> bool {
    let mut segments = path.trim_start_matches('/').split('/');
    match (segments.next(), segments.next(), segments.next()) {
        (Some("api"), Some("v1"), Some("attachments")) => true,
        (Some("api"), Some("v1"), Some("notes")) => {
            segments.next().is_some_and(|id| !id.is_empty())
                && segments.next() == Some("attachments")
        }
        _ => false,
    }
}

/// Problem response returned for every attachment route while the switch is off.
pub fn attachments_disabled_response() -> Response {
    let status = StatusCode::NOT_FOUND;
    (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(serde_json::json!({
            "type": "https://fortemi.com/problems/not-found",
            "title": "Not Found",
            "status": status.as_u16(),
            "detail": ATTACHMENTS_DISABLED_DETAIL,
            "code": ATTACHMENTS_DISABLED_CODE,
        })),
    )
        .into_response()
}

/// Reject attachment routes before any handler, storage or authorization lookup runs.
pub async fn attachments_gate_middleware(
    State(switch): State<AttachmentsSwitch>,
    request: Request,
    next: Next,
) -> Response {
    if !switch.enabled() && is_attachment_path(request.uri().path()) {
        return attachments_disabled_response();
    }
    next.run(request).await
}

/// Remove attachment operations from a served OpenAPI document and mark the state.
pub fn strip_attachment_paths_from_openapi(yaml: &str) -> String {
    let Ok(mut value) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return yaml.to_string();
    };
    let Some(root) = value.as_mapping_mut() else {
        return yaml.to_string();
    };
    if let Some(paths) = root
        .get_mut(serde_yaml::Value::String("paths".to_string()))
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        paths.retain(|key, _| !key.as_str().is_some_and(is_attachment_path));
    }
    root.insert(
        serde_yaml::Value::String("x-fortemi-attachments".to_string()),
        serde_yaml::to_value(serde_json::json!({
            "enabled": false,
            "disabled_code": ATTACHMENTS_DISABLED_CODE,
        }))
        .expect("attachments extension must serialize"),
    );
    serde_yaml::to_string(&value).unwrap_or_else(|_| yaml.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use matric_jobs::AttachmentScanMode;
    use std::collections::HashMap;
    use tower::ServiceExt;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn switch_defaults_to_enabled_and_is_strict() {
        assert!(parse_attachments_enabled(env_from(&[])).unwrap());
        assert!(
            !parse_attachments_enabled(env_from(&[(ATTACHMENTS_ENABLED_ENV, "false")])).unwrap()
        );
        assert!(parse_attachments_enabled(env_from(&[(ATTACHMENTS_ENABLED_ENV, "1")])).unwrap());
        assert!(parse_attachments_enabled(env_from(&[(ATTACHMENTS_ENABLED_ENV, "no")])).is_err());
    }

    #[test]
    fn hosted_admission_succeeds_without_scanner_when_attachments_disabled() {
        let config = resolve_attachment_scan_config(false, true, 1024, env_from(&[])).unwrap();
        assert_eq!(config.mode, AttachmentScanMode::Disabled);
        assert!(config.clamd_addr.is_none());
        // Scanner settings are not consulted at all while attachments are off.
        let config = resolve_attachment_scan_config(
            false,
            true,
            1024,
            env_from(&[("MATRIC_ATTACHMENT_SCAN_MODE", "bogus")]),
        )
        .unwrap();
        assert_eq!(config.mode, AttachmentScanMode::Disabled);
    }

    #[test]
    fn hosted_admission_restores_scanner_prerequisites_when_enabled() {
        let missing = resolve_attachment_scan_config(true, true, 1024, env_from(&[]));
        assert!(missing.is_err(), "scan mode must be explicit");
        let bypass = resolve_attachment_scan_config(
            true,
            true,
            1024,
            env_from(&[("MATRIC_ATTACHMENT_SCAN_MODE", "disabled")]),
        );
        assert!(bypass.is_err(), "hosted mode requires required scanning");
        let no_addr = resolve_attachment_scan_config(
            true,
            true,
            1024,
            env_from(&[("MATRIC_ATTACHMENT_SCAN_MODE", "required")]),
        );
        assert!(no_addr.is_err(), "required scanning needs a clamd address");
        let ok = resolve_attachment_scan_config(
            true,
            true,
            1024,
            env_from(&[
                ("MATRIC_ATTACHMENT_SCAN_MODE", "required"),
                ("MATRIC_ATTACHMENT_CLAMD_ADDR", "127.0.0.1:3310"),
            ]),
        )
        .unwrap();
        assert_eq!(ok.mode, AttachmentScanMode::Required);
    }

    #[test]
    fn attachment_path_matcher_covers_route_groups_only() {
        for path in [
            "/api/v1/attachments",
            "/api/v1/attachments/abc",
            "/api/v1/attachments/abc/download",
            "/api/v1/attachments/abc/thumbnail",
            "/api/v1/attachments/abc/thumbnails.vtt",
            "/api/v1/attachments/abc/sprites/0",
            "/api/v1/attachments/abc/subtitles",
            "/api/v1/notes/n1/attachments",
            "/api/v1/notes/n1/attachments/upload",
            "/api/v1/notes/n1/attachments/tus",
            "/api/v1/notes/{id}/attachments/tus/{upload_id}",
        ] {
            assert!(is_attachment_path(path), "{path} must be gated");
        }
        for path in [
            "/api/v1/notes",
            "/api/v1/notes/n1",
            "/api/v1/notes/n1/links",
            "/api/v1/attachmentsx",
            "/health",
            "/api/v1/backup/knowledge-shard/upload",
        ] {
            assert!(!is_attachment_path(path), "{path} must not be gated");
        }
    }

    fn gated_router(enabled: bool) -> Router {
        Router::new()
            .route("/api/v1/notes/{id}/attachments", get(|| async { "list" }))
            .route(
                "/api/v1/notes/{id}/attachments/tus/{upload_id}",
                get(|| async { "tus" }),
            )
            .route(
                "/api/v1/attachments/{attachment_id}/download",
                get(|| async { "download" }),
            )
            .route(
                "/api/v1/attachments/{attachment_id}/sprites/{sprite_index}",
                get(|| async { "sprite" }),
            )
            .route("/api/v1/notes/{id}", get(|| async { "note" }))
            .layer(axum::middleware::from_fn_with_state(
                AttachmentsSwitch::new(enabled),
                attachments_gate_middleware,
            ))
    }

    async fn call(router: Router, path: &str) -> (StatusCode, serde_json::Value) {
        let response = router
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = if content_type.starts_with("application/problem+json") {
            serde_json::from_slice(&bytes).unwrap()
        } else {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
        };
        (status, body)
    }

    #[tokio::test]
    async fn disabled_switch_fails_attachment_routes_closed() {
        for path in [
            "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/attachments",
            "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/attachments/tus/u1",
            "/api/v1/attachments/018fd1a0-0000-7000-8000-000000000002/download",
            "/api/v1/attachments/018fd1a0-0000-7000-8000-000000000002/sprites/0",
        ] {
            let (status, body) = call(gated_router(false), path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(body["code"], ATTACHMENTS_DISABLED_CODE);
            assert_eq!(body["type"], "https://fortemi.com/problems/not-found");
            assert_eq!(body["status"], 404);
        }
        let (status, body) = call(gated_router(false), "/api/v1/notes/n1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "note");
    }

    #[tokio::test]
    async fn enabled_switch_passes_attachment_routes_through() {
        let (status, body) = call(
            gated_router(true),
            "/api/v1/attachments/018fd1a0-0000-7000-8000-000000000002/download",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "download");
    }

    #[test]
    fn openapi_strip_removes_only_attachment_paths() {
        let yaml = "openapi: 3.1.0\npaths:\n  /api/v1/notes:\n    get: {}\n  /api/v1/attachments/{attachment_id}:\n    get: {}\n  /api/v1/notes/{id}/attachments/tus:\n    post: {}\n";
        let stripped = strip_attachment_paths_from_openapi(yaml);
        assert!(stripped.contains("/api/v1/notes:"));
        assert!(!stripped.contains("/api/v1/attachments"));
        assert!(!stripped.contains("attachments/tus"));
        assert!(stripped.contains("x-fortemi-attachments"));
    }
}

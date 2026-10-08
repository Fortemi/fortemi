//! Community auth-exempt routes that hosted mode closes behind a verified bearer (#1163).
//!
//! The legacy WebSocket (`/api/v1/ws`) and the NDJSON ingest stream
//! (`/api/v1/ingest/stream`) bypass bearer authentication in community mode.
//! Neither runs on a tenant transaction, so in hosted mode
//! (`FORTEMI_MULTI_TENANT=true`) `auth_middleware` stops exempting them: a
//! request without a valid bearer gets `401`, and a verified bearer reaches the
//! hosted tenant-transaction gate, which answers `503` because neither
//! operation is migrated. This holds even when `REQUIRE_AUTH=false`.
//!
//! Liveness, readiness and the aggregate `/api/v1/health/streaming` probe stay
//! public in every mode.

use axum::http::Method;

pub const LEGACY_WEBSOCKET_PATH: &str = "/api/v1/ws";
pub const INGEST_STREAM_PATH: &str = "/api/v1/ingest/stream";

/// Whether hosted mode requires a verified bearer on an otherwise auth-exempt route.
///
/// CORS preflights stay exempt so browsers still receive the CORS answer.
pub fn hosted_requires_bearer(method: &Method, path: &str) -> bool {
    *method != Method::OPTIONS && (path == LEGACY_WEBSOCKET_PATH || path == INGEST_STREAM_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_policy;

    #[test]
    fn hosted_bearer_set_is_the_realtime_transport_routes() {
        for path in [LEGACY_WEBSOCKET_PATH, INGEST_STREAM_PATH] {
            assert!(
                route_policy::route_policy_for_path(path).is_some(),
                "{path} must be inventoried"
            );
            assert!(
                route_policy::is_public_without_bearer(path),
                "{path} stays exempt in community mode"
            );
            assert!(hosted_requires_bearer(&Method::GET, path));
            assert!(hosted_requires_bearer(&Method::POST, path));
            assert!(!hosted_requires_bearer(&Method::OPTIONS, path));
        }
    }

    #[test]
    fn probes_and_discovery_stay_public_in_hosted_mode() {
        for path in [
            "/health",
            "/health/live",
            "/livez",
            "/readyz",
            "/api/v1/health/streaming",
            "/api/v1/system/compatibility",
            "/api/v1/events",
        ] {
            assert!(!hosted_requires_bearer(&Method::GET, path), "{path}");
        }
    }
}

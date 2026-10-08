//! Community auth-exempt routes that hosted mode closes behind a verified bearer (#1163, #1164).
//!
//! The legacy WebSocket (`/api/v1/ws`), the NDJSON ingest stream
//! (`/api/v1/ingest/stream`) and the knowledge diagnostics under
//! `/api/v1/health/*` bypass bearer authentication in community mode. None of
//! them runs on a tenant transaction, so in hosted mode (`FORTEMI_MULTI_TENANT=true`)
//! `auth_middleware` stops exempting them: a request without a valid bearer
//! gets `401`, and a verified bearer reaches the hosted tenant-transaction gate,
//! which answers `503` because none of these operations is migrated. This holds
//! even when `REQUIRE_AUTH=false`.
//!
//! Liveness, readiness and the aggregate `/api/v1/health/streaming` probe stay
//! public in every mode.

use axum::http::Method;

pub const LEGACY_WEBSOCKET_PATH: &str = "/api/v1/ws";
pub const INGEST_STREAM_PATH: &str = "/api/v1/ingest/stream";

/// Knowledge diagnostics: they read tenant notes, tags and links.
pub const KNOWLEDGE_DIAGNOSTIC_PATHS: [&str; 6] = [
    "/api/v1/health/knowledge",
    "/api/v1/health/orphan-tags",
    "/api/v1/health/stale-notes",
    "/api/v1/health/unlinked-notes",
    "/api/v1/health/tag-cooccurrence",
    "/api/v1/health/access-frequency",
];

/// Whether `path` is a knowledge diagnostic rather than a liveness/readiness probe.
pub fn is_knowledge_diagnostic(path: &str) -> bool {
    KNOWLEDGE_DIAGNOSTIC_PATHS.contains(&path)
}

/// Whether hosted mode requires a verified bearer on an otherwise auth-exempt route.
///
/// CORS preflights stay exempt so browsers still receive the CORS answer.
pub fn hosted_requires_bearer(method: &Method, path: &str) -> bool {
    *method != Method::OPTIONS
        && (path == LEGACY_WEBSOCKET_PATH
            || path == INGEST_STREAM_PATH
            || is_knowledge_diagnostic(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_policy;

    #[test]
    fn hosted_bearer_set_is_realtime_transport_and_knowledge_diagnostics_only() {
        for path in [LEGACY_WEBSOCKET_PATH, INGEST_STREAM_PATH]
            .into_iter()
            .chain(KNOWLEDGE_DIAGNOSTIC_PATHS)
        {
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
            assert!(!is_knowledge_diagnostic(path), "{path}");
        }
    }

    #[test]
    fn every_inventoried_health_diagnostic_except_streaming_is_listed() {
        let inventoried: Vec<&str> = route_policy::ROUTE_POLICY_INVENTORY
            .iter()
            .filter(|policy| policy.action_family == "health_diagnostics")
            .map(|policy| policy.path)
            .filter(|path| *path != "/api/v1/health/streaming")
            .collect();
        assert_eq!(inventoried.len(), KNOWLEDGE_DIAGNOSTIC_PATHS.len());
        for path in inventoried {
            assert!(is_knowledge_diagnostic(path), "{path} must be listed");
        }
    }
}

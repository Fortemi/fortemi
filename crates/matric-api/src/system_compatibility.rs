//! Public API compatibility and authentication discovery.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{hosted_route_qualification, route_policy, AppState};

const CONTRACT_REVISION: &str = "2026-07-06";
// fortemi-auth v2026.10.3, conformance/v4/manifest.json and release-policy.json.
// That release has no exported Rust constants for its claim contract metadata.
const VERIFIER_CLAIM_CONTRACT_VERSION: &str = "4.0.0";
const VERIFIER_CLAIM_CONTRACT_PROFILE: &str = "rust-node-jwt-v1";

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct CompatibilityResponse {
    schema_version: u8,
    /// Remains 2026-07-06 in every auth mode; external OIDC is an additive extension.
    #[schema(example = "2026-07-06")]
    contract_revision: &'static str,
    api: CompatibilityApi,
    deployment: CompatibilityDeployment,
    auth: CompatibilityAuth,
    capabilities: serde_json::Value,
    backoffice: BackofficeCompatibility,
    links: CompatibilityLinks,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct CompatibilityApi {
    name: &'static str,
    version: &'static str,
    minimum_hotm_enterprise_client: &'static str,
    git_sha_present: bool,
    build_date_present: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct CompatibilityDeployment {
    mode: &'static str,
    edition: &'static str,
    hosted_multi_tenant_ready: bool,
    /// Route-inventory-derived profile a dedicated hosted deployment can rely on.
    #[serde(skip_serializing_if = "Option::is_none")]
    hosted_profile: Option<hosted_route_qualification::HostedProfileSummary>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct CompatibilityAuth {
    required: bool,
    mode: CompatibilityAuthMode,
    oauth_issuer_configured: bool,
    tenant_context_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    claim_contract_version: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claim_contract_profile: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority_release: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority_commit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_sha256: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_policy_sha256: Option<&'static str>,
    /// Claim contract of the pinned fortemi-auth verifier in external OIDC mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    verifier_claim_contract_version: Option<&'static str>,
    /// Verifier profile, independent of the hosted authority tuple.
    #[serde(skip_serializing_if = "Option::is_none")]
    verifier_claim_contract_profile: Option<&'static str>,
    /// Exact issuer trusted by the external OIDC verifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    issuer: Option<String>,
    /// Complete, sorted set of accepted resource identifiers, as loaded at startup.
    #[serde(skip_serializing_if = "Option::is_none")]
    audiences: Option<Vec<String>>,
    /// Whether local OAuth authorization, registration and token issuance are disabled.
    /// Revocation and introspection of existing local tokens remain registered.
    #[serde(skip_serializing_if = "Option::is_none")]
    local_oauth_disabled: Option<bool>,
}

/// Consumers must fail closed for authentication modes they do not support.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum CompatibilityAuthMode {
    AnonymousLocal,
    Oauth,
    HostedOauth,
    ExternalOidc,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct CompatibilityLinks {
    openapi: &'static str,
    asyncapi: &'static str,
    health: &'static str,
    streaming_health: &'static str,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct BackofficeCompatibility {
    contract_revision: &'static str,
    production_enabled: bool,
    preview_responses_enabled: bool,
    preview_gate: &'static str,
    surfaces: Vec<BackofficeSurface>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct BackofficeSurface {
    key: &'static str,
    state: &'static str,
    reason_code: &'static str,
    endpoint: &'static str,
    required_scopes: &'static [&'static str],
    actions: &'static [&'static str],
    audit_events: &'static [&'static str],
    response_contract: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompatibilityInputs<'a> {
    pub(crate) require_auth: bool,
    pub(crate) multi_tenant: bool,
    pub(crate) attachments_enabled: bool,
    pub(crate) key_provider_configured: bool,
    pub(crate) git_sha: &'a str,
    pub(crate) build_date: &'a str,
    pub(crate) external_oidc: Option<ExternalOidcInputs<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExternalOidcInputs<'a> {
    issuer: Option<&'a str>,
    audiences: &'a BTreeSet<String>,
    local_oauth_disabled: bool,
}

fn capability_state(state: &'static str) -> serde_json::Value {
    serde_json::json!({ "state": state })
}

fn capability_state_with_reason(
    state: &'static str,
    reason_code: &'static str,
) -> serde_json::Value {
    serde_json::json!({
        "state": state,
        "reason_code": reason_code,
    })
}

fn build_compatibility_auth(inputs: &CompatibilityInputs<'_>) -> CompatibilityAuth {
    let external_oidc = inputs.external_oidc.filter(|_| !inputs.multi_tenant);
    let mode = if inputs.multi_tenant {
        CompatibilityAuthMode::HostedOauth
    } else if external_oidc.is_some() {
        CompatibilityAuthMode::ExternalOidc
    } else if inputs.require_auth {
        CompatibilityAuthMode::Oauth
    } else {
        CompatibilityAuthMode::AnonymousLocal
    };
    CompatibilityAuth {
        required: inputs.require_auth,
        mode,
        oauth_issuer_configured: inputs.require_auth,
        tenant_context_available: inputs.multi_tenant,
        claim_contract_version: inputs.multi_tenant.then_some("1.1.0"),
        claim_contract_profile: inputs.multi_tenant.then_some("rust-node-jwt-v1"),
        authority_release: inputs.multi_tenant.then_some("v2026.9.0"),
        authority_commit: inputs
            .multi_tenant
            .then_some("cff36d293d73080c186e2f35115dd92a594982d2"),
        manifest_sha256: inputs
            .multi_tenant
            .then_some("2df0a35edad67cc3e8869286183a4d098b1eb8fc2161432ed0b54ba69b17e242"),
        release_policy_sha256: inputs
            .multi_tenant
            .then_some("bd77fbeba24991f969d07e5f45c5259504e52bb5ab9d7c4137abb22270c4abe8"),
        verifier_claim_contract_version: external_oidc.map(|_| VERIFIER_CLAIM_CONTRACT_VERSION),
        verifier_claim_contract_profile: external_oidc.map(|_| VERIFIER_CLAIM_CONTRACT_PROFILE),
        issuer: external_oidc.and_then(|config| config.issuer.map(str::to_owned)),
        audiences: external_oidc.map(|config| config.audiences.iter().cloned().collect()),
        local_oauth_disabled: external_oidc.map(|config| config.local_oauth_disabled),
    }
}

fn compatibility_deployment_mode(require_auth: bool, multi_tenant: bool) -> &'static str {
    if multi_tenant {
        "hosted_multi_tenant"
    } else if require_auth {
        "single_tenant_server"
    } else {
        "local_sidecar"
    }
}

fn build_backoffice_compatibility() -> BackofficeCompatibility {
    BackofficeCompatibility {
        contract_revision: "2026-07-10",
        production_enabled: false,
        preview_responses_enabled: false,
        preview_gate: "FORTEMI_BACKOFFICE_PREVIEW=true",
        surfaces: vec![
            BackofficeSurface {
                key: "tenant_health",
                state: "unavailable",
                reason_code: "backoffice_contract_preview_disabled",
                endpoint: "/api/v1/admin/tenant/health",
                required_scopes: &["admin:tenant:read"],
                actions: &["tenant.health.read"],
                audit_events: &["backoffice.tenant_health.read"],
                response_contract: "health_state, degraded_reasons, checked_at",
            },
            BackofficeSurface {
                key: "audit_posture",
                state: "preview",
                reason_code: "hosted_audit_gate_open",
                endpoint: "/api/v1/admin/audit/posture",
                required_scopes: &["admin:audit:read"],
                actions: &["audit.posture.read"],
                audit_events: &["backoffice.audit_posture.read"],
                response_contract: "coverage_state, sink_state, retention_state, missing_events",
            },
            BackofficeSurface {
                key: "quota_status",
                state: "unavailable",
                reason_code: "quota_policy_not_implemented",
                endpoint: "/api/v1/admin/quota/status",
                required_scopes: &["admin:quota:read"],
                actions: &["quota.status.read"],
                audit_events: &["backoffice.quota_status.read"],
                response_contract: "metering_state, period, limits, usage, reset_at",
            },
            BackofficeSurface {
                key: "kms_status",
                state: "unavailable",
                reason_code: "kms_backoffice_not_implemented",
                endpoint: "/api/v1/admin/kms/status",
                required_scopes: &["admin:kms:read"],
                actions: &["kms.status.read"],
                audit_events: &["backoffice.kms_status.read"],
                response_contract:
                    "provider_state, keyring_state, rotation_state, degraded_reasons",
            },
            BackofficeSurface {
                key: "premium_components",
                state: "preview",
                reason_code: "capability_catalog_preview_only",
                endpoint: "/api/v1/admin/premium/components",
                required_scopes: &["admin:components:read"],
                actions: &["premium.components.read"],
                audit_events: &["backoffice.premium_components.read"],
                response_contract: "component_key, state, reason_code, required_entitlements",
            },
            BackofficeSurface {
                key: "support_diagnostics",
                state: "unavailable",
                reason_code: "support_diagnostics_not_implemented",
                endpoint: "/api/v1/admin/support/diagnostics",
                required_scopes: &["admin:support:read"],
                actions: &["support.diagnostics.read"],
                audit_events: &["backoffice.support_diagnostics.read"],
                response_contract: "diagnostic_key, state, redacted_summary, collected_at",
            },
        ],
    }
}

pub(crate) fn build_compatibility_response_from_inputs(
    inputs: CompatibilityInputs<'_>,
) -> CompatibilityResponse {
    CompatibilityResponse {
        schema_version: 1,
        contract_revision: CONTRACT_REVISION,
        api: CompatibilityApi {
            name: "fortemi",
            version: env!("CARGO_PKG_VERSION"),
            minimum_hotm_enterprise_client: "0.0.0-checkpoint",
            git_sha_present: !inputs.git_sha.trim().is_empty() && inputs.git_sha != "unknown",
            build_date_present: !inputs.build_date.trim().is_empty()
                && inputs.build_date != "unknown",
        },
        deployment: CompatibilityDeployment {
            mode: compatibility_deployment_mode(inputs.require_auth, inputs.multi_tenant),
            edition: if inputs.multi_tenant {
                "internal"
            } else {
                "community"
            },
            hosted_multi_tenant_ready: false,
            hosted_profile: inputs
                .multi_tenant
                .then(hosted_route_qualification::hosted_profile_summary),
        },
        auth: build_compatibility_auth(&inputs),
        capabilities: serde_json::json!({
            "core_notes": capability_state("available"),
            "attachments": if inputs.attachments_enabled {
                capability_state("available")
            } else {
                capability_state_with_reason("unavailable", "attachments_disabled_by_operator")
            },
            "search": capability_state("available"),
            "jobs": capability_state("available"),
            "realtime_activity": capability_state("available"),
            "hosted_auth": capability_state_with_reason(
                if inputs.multi_tenant { "preview" } else { "unavailable" },
                if inputs.multi_tenant {
                    "tenant_route_coverage_incomplete"
                } else if inputs.require_auth {
                    "rls_gate_open"
                } else {
                    "hosted_auth_not_configured"
                },
            ),
            "premium_components": capability_state_with_reason(
                "preview",
                "capability_catalog_preview_only",
            ),
            "backoffice_api": capability_state_with_reason(
                "unavailable",
                "contract_not_implemented",
            ),
            "audit_posture": capability_state_with_reason(
                "preview",
                "hosted_audit_gate_open",
            ),
            "quota_status": capability_state_with_reason(
                "unavailable",
                "quota_policy_not_implemented",
            ),
            "kms_status": capability_state_with_reason(
                if inputs.key_provider_configured { "preview" } else { "unavailable" },
                if inputs.key_provider_configured {
                    "kms_runtime_configured_rotation_gate_open"
                } else {
                    "key_provider_not_configured"
                },
            ),
            "mcp_scope_gate": capability_state_with_reason(
                "preview",
                "enterprise_gate_not_implemented",
            ),
        }),
        backoffice: build_backoffice_compatibility(),
        links: CompatibilityLinks {
            openapi: route_policy::OPERATOR_OPENAPI_PATH,
            asyncapi: route_policy::OPERATOR_ASYNCAPI_PATH,
            health: "/health",
            streaming_health: "/api/v1/health/streaming",
        },
    }
}

pub(crate) fn build_compatibility_response(state: &AppState) -> CompatibilityResponse {
    #[cfg(feature = "hosted-auth")]
    let external_oidc = state
        .external_oidc
        .as_ref()
        .filter(|_| state.auth_mode.external_oidc())
        .map(|config| ExternalOidcInputs {
            issuer: state.oidc_issuer.as_deref(),
            audiences: &config.audiences,
            local_oauth_disabled: !state.oauth_local_as_enabled,
        });
    #[cfg(not(feature = "hosted-auth"))]
    let external_oidc = None;
    build_compatibility_response_from_inputs(CompatibilityInputs {
        require_auth: state.require_auth,
        multi_tenant: state.multi_tenant,
        attachments_enabled: state.attachments_enabled,
        key_provider_configured: state.key_provider.is_some(),
        git_sha: &state.git_sha,
        build_date: &state.build_date,
        external_oidc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_inputs() -> CompatibilityInputs<'static> {
        CompatibilityInputs {
            require_auth: false,
            multi_tenant: false,
            attachments_enabled: true,
            key_provider_configured: false,
            git_sha: "unknown",
            build_date: "unknown",
            external_oidc: None,
        }
    }

    fn assert_auth_fixture(inputs: CompatibilityInputs<'_>, mode: &str) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/openapi/fixtures/system-compatibility-auth.json"
        ))
        .unwrap();
        let response =
            serde_json::to_value(build_compatibility_response_from_inputs(inputs)).unwrap();
        assert_eq!(response["schema_version"], fixture["schema_version"]);
        assert_eq!(response["contract_revision"], fixture["contract_revision"]);
        assert_eq!(response["auth"], fixture["modes"][mode]);
    }

    #[test]
    fn auth_object_for_anonymous_local() {
        assert_auth_fixture(local_inputs(), "anonymous_local");
    }

    #[test]
    fn auth_object_for_own_oauth() {
        assert_auth_fixture(
            CompatibilityInputs {
                require_auth: true,
                ..local_inputs()
            },
            "oauth",
        );
    }

    #[test]
    fn auth_object_for_external_oidc_single_tenant() {
        let audiences = [
            "https://fortemi.example.com/mcp".to_string(),
            "https://fortemi.example.com".to_string(),
        ]
        .into_iter()
        .collect();
        assert_auth_fixture(
            CompatibilityInputs {
                require_auth: true,
                external_oidc: Some(ExternalOidcInputs {
                    issuer: Some("https://idp.example.com/realms/example"),
                    audiences: &audiences,
                    local_oauth_disabled: true,
                }),
                ..local_inputs()
            },
            "external_oidc",
        );
    }

    #[test]
    fn auth_object_for_hosted_multi_tenant() {
        assert_auth_fixture(
            CompatibilityInputs {
                require_auth: true,
                multi_tenant: true,
                key_provider_configured: true,
                ..local_inputs()
            },
            "hosted_oauth",
        );
    }

    #[test]
    fn contract_revision_is_compatible_in_all_auth_modes() {
        let audiences = ["https://fortemi.example.com".to_string()]
            .into_iter()
            .collect();
        let external_oidc = ExternalOidcInputs {
            issuer: Some("https://idp.example.com/realms/example"),
            audiences: &audiences,
            local_oauth_disabled: true,
        };
        for (mode, require_auth, multi_tenant, external_oidc) in [
            ("anonymous_local", false, false, None),
            ("oauth", true, false, None),
            ("hosted_oauth", true, true, None),
            ("external_oidc", true, false, Some(external_oidc)),
        ] {
            let response = serde_json::to_value(build_compatibility_response_from_inputs(
                CompatibilityInputs {
                    require_auth,
                    multi_tenant,
                    external_oidc,
                    ..local_inputs()
                },
            ))
            .unwrap();
            assert_eq!(response["auth"]["mode"], mode);
            assert_eq!(response["contract_revision"], "2026-07-06", "{mode}");
        }
    }

    #[test]
    fn external_oidc_reports_local_oauth_opt_in() {
        let audiences = ["https://fortemi.example.com".to_string()]
            .into_iter()
            .collect();
        let response = serde_json::to_value(build_compatibility_response_from_inputs(
            CompatibilityInputs {
                require_auth: true,
                external_oidc: Some(ExternalOidcInputs {
                    issuer: Some("https://idp.example.com/realms/example"),
                    audiences: &audiences,
                    local_oauth_disabled: false,
                }),
                ..local_inputs()
            },
        ))
        .unwrap();
        assert_eq!(
            response["auth"],
            serde_json::json!({
                "required": true,
                "mode": "external_oidc",
                "oauth_issuer_configured": true,
                "tenant_context_available": false,
                "verifier_claim_contract_version": "4.0.0",
                "verifier_claim_contract_profile": "rust-node-jwt-v1",
                "issuer": "https://idp.example.com/realms/example",
                "audiences": ["https://fortemi.example.com"],
                "local_oauth_disabled": false,
            })
        );
    }

    #[test]
    fn verifier_contract_metadata_is_bound_to_dependency_release() {
        // Update the mirrored contract metadata when either verifier dependency changes.
        let manifest = include_str!("../../../Cargo.toml");
        let lockfile = include_str!("../../../Cargo.lock");
        let policy: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fortemi-auth-consumer/fixtures/fortemi-auth-release-policy-v1.json"
        ))
        .unwrap();
        assert_eq!(VERIFIER_CLAIM_CONTRACT_VERSION, "4.0.0");
        assert_eq!(
            policy["current_release"]["contract_version"],
            VERIFIER_CLAIM_CONTRACT_VERSION
        );
        assert_eq!(
            policy["current_release"]["profile"],
            VERIFIER_CLAIM_CONTRACT_PROFILE
        );
        assert_eq!(policy["current_release"]["tag"], "v2026.10.3");
        assert_eq!(policy["manifest_path"], "conformance/v4/manifest.json");
        for dependency in ["fortemi-auth-core", "fortemi-auth-clerk"] {
            assert!(manifest.contains(&format!(
                "{dependency} = {{ version = \"=2026.10.3\", git = \"https://git.integrolabs.net/Fortemi/fortemi-auth.git\", tag = \"v2026.10.3\" }}"
            )));
            assert!(lockfile.contains(&format!(
                "name = \"{dependency}\"\nversion = \"2026.10.3\"\nsource = \"git+https://git.integrolabs.net/Fortemi/fortemi-auth.git?tag=v2026.10.3#cc273293a6f0300259efa02b5572bbdce9f95f36\""
            )));
        }
    }

    #[tokio::test]
    async fn deployment_mode_is_independent_of_key_provider_presence() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
            .unwrap();
        let mut state =
            crate::tests::build_call_api_test_state(matric_db::Database::new(pool), "unused").await;
        state.require_auth = true;
        state.multi_tenant = true;
        assert!(state.key_provider.is_none());
        let response = serde_json::to_value(build_compatibility_response(&state)).unwrap();
        assert_eq!(response["deployment"]["mode"], "hosted_multi_tenant");
        assert_eq!(response["auth"]["mode"], "hosted_oauth");
        assert_eq!(
            response["capabilities"]["kms_status"]["state"],
            "unavailable"
        );
    }

    #[cfg(feature = "hosted-auth")]
    #[tokio::test]
    async fn external_oidc_endpoint_reports_verifier_state_without_authentication() {
        use axum::{body::Body, http::Request, routing::get, Router};
        use tower::ServiceExt;

        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
            .unwrap();
        let mut state =
            crate::tests::build_call_api_test_state(matric_db::Database::new(pool), "unused").await;
        state.require_auth = true;
        state.auth_mode = crate::AuthMode::ExternalOidc;
        state.issuer = "https://fortemi.example.com".to_string();
        state.oidc_issuer = Some("https://idp.example.com/realms/example".to_string());
        state.oauth_local_as_enabled = false;
        state.external_oidc = Some(crate::ExternalOidcConfig {
            audiences: [
                "https://fortemi.example.com/mcp".to_string(),
                "https://fortemi.example.com".to_string(),
            ]
            .into_iter()
            .collect(),
            default_tenant_id: uuid::Uuid::nil(),
            allow_multi_audience: false,
            allow_token_scopes: false,
            allow_legacy_tokens: false,
            legacy_scope_ceiling: Default::default(),
            max_token_lifetime_seconds: 3600,
        });
        let app = Router::new()
            .route(
                "/api/v1/system/compatibility",
                get(crate::system_compatibility),
            )
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::auth_middleware,
            ))
            .with_state(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/system/compatibility")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CACHE_CONTROL],
            "no-store"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/openapi/fixtures/system-compatibility-auth.json"
        ))
        .unwrap();
        assert_eq!(response["auth"], fixture["modes"]["external_oidc"]);
        assert_eq!(response["deployment"]["mode"], "single_tenant_server");
    }
}

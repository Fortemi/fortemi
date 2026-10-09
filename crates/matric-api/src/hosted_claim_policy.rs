//! Loads the hosted OIDC claim policy (IdP group/role to Fortemi scope mapping, #1152).
//!
//! The policy file is read once when the verifier is built. Any problem fails startup:
//! an unreadable file, invalid JSON, unknown fields, a mapping to a scope outside the
//! Fortemi vocabulary, or anything `ClaimPolicy::from_config` rejects. Errors never
//! include the file path contents, group values or client ids.

use anyhow::Context;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use fortemi_auth_core::{ClaimPolicy, ClaimPolicyConfig, ScopeSource};

/// Scopes an external IdP mapping may grant. `system:*` scopes are internal and are
/// never derivable from directory groups.
pub const MAPPABLE_SCOPES: &[&str] = &["read", "write", "admin", "mcp"];

static EMPTY_FORBIDDEN_AUDIENCES: LazyLock<BTreeSet<String>> = LazyLock::new(BTreeSet::new);

#[derive(Clone, Debug)]
pub struct ClaimPolicyLoadOptions<'a> {
    pub required: bool,
    pub allow_token_scopes: bool,
    pub forbidden_audiences: &'a BTreeSet<String>,
}

impl ClaimPolicyLoadOptions<'_> {
    pub fn hosted_default() -> Self {
        Self {
            required: false,
            allow_token_scopes: true,
            forbidden_audiences: &EMPTY_FORBIDDEN_AUDIENCES,
        }
    }
}

/// Build the claim policy from `FORTEMI_AUTH_CLAIM_POLICY_FILE`, or the default
/// token-scope policy when unset and allowed for the selected mode.
pub fn load_claim_policy(path: Option<&str>) -> anyhow::Result<ClaimPolicy> {
    load_claim_policy_with_options(path, &ClaimPolicyLoadOptions::hosted_default())
}

pub fn load_claim_policy_with_options(
    path: Option<&str>,
    options: &ClaimPolicyLoadOptions<'_>,
) -> anyhow::Result<ClaimPolicy> {
    let Some(path) = path else {
        anyhow::ensure!(
            !options.required,
            "FORTEMI_AUTH_CLAIM_POLICY_FILE is required in external OIDC mode"
        );
        return Ok(ClaimPolicy::default());
    };
    anyhow::ensure!(
        !path.trim().is_empty(),
        "FORTEMI_AUTH_CLAIM_POLICY_FILE must name a nonempty JSON policy file"
    );
    let bytes = std::fs::read(path).context(
        "FORTEMI_AUTH_CLAIM_POLICY_FILE could not be read; mount the policy file readable by the server user",
    )?;
    claim_policy_from_json_with_options(&bytes, options)
}

/// Parse and validate policy JSON.
pub fn claim_policy_from_json(bytes: &[u8]) -> anyhow::Result<ClaimPolicy> {
    claim_policy_from_json_with_options(bytes, &ClaimPolicyLoadOptions::hosted_default())
}

pub fn claim_policy_from_json_with_options(
    bytes: &[u8],
    options: &ClaimPolicyLoadOptions<'_>,
) -> anyhow::Result<ClaimPolicy> {
    let config: ClaimPolicyConfig = serde_json::from_slice(bytes).map_err(|err| {
        anyhow::anyhow!(
            "FORTEMI_AUTH_CLAIM_POLICY_FILE is not a valid claim policy (line {}, column {}); see docs/content/authentication.md",
            err.line(),
            err.column()
        )
    })?;
    if let Some(mapping) = &config.scope_mapping {
        let unknown = mapping
            .rules
            .iter()
            .flat_map(|rule| rule.scopes.iter())
            .any(|scope| !MAPPABLE_SCOPES.contains(&scope.as_str()));
        anyhow::ensure!(
            !unknown,
            "FORTEMI_AUTH_CLAIM_POLICY_FILE maps a scope outside the Fortemi vocabulary (read, write, admin, mcp)"
        );
    }
    let uses_token_scopes = matches!(
        config.scope_source,
        Some(ScopeSource::Token | ScopeSource::Union)
    ) || (config.scope_source.is_none() && config.scope_mapping.is_none());
    anyhow::ensure!(
        !uses_token_scopes || options.allow_token_scopes,
        "FORTEMI_AUTH_ALLOW_TOKEN_SCOPES=true is required for claim policies using token or union scope sources"
    );
    if let Some(clients) = &config.clients {
        for client in clients
            .allowed
            .iter()
            .flatten()
            .chain(clients.service.iter())
        {
            anyhow::ensure!(
                !options.forbidden_audiences.contains(client),
                "FORTEMI_AUTH_AUDIENCES must not contain a claim-policy client id"
            );
        }
    }
    ClaimPolicy::from_config(config).map_err(|_| {
        anyhow::anyhow!(
            "FORTEMI_AUTH_CLAIM_POLICY_FILE failed validation: check rule ids, values, scope lists, the claim path, client lists and scope_source"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fortemi_auth_core::{AuthError, PrincipalKind};
    use serde_json::json;

    fn policy(value: serde_json::Value) -> anyhow::Result<ClaimPolicy> {
        claim_policy_from_json(value.to_string().as_bytes())
    }

    #[test]
    fn unset_policy_keeps_token_scopes() {
        let policy = load_claim_policy(None).unwrap();
        let decision = policy.evaluate(&json!({}), &["read".to_string()]).unwrap();
        assert_eq!(decision.scopes, vec!["read"]);
    }

    #[test]
    fn keycloak_realm_roles_map_to_fortemi_scopes() {
        let policy = policy(json!({
            "scope_mapping": {"claim": "realm_access.roles", "rules": [
                {"id": "kc-read", "value": "fortemi-read", "scopes": ["read"]},
                {"id": "kc-agent", "value": "fortemi-agent", "scopes": ["read", "mcp"]}
            ]},
            "clients": {"allowed": ["fortemi-web"], "service": ["fortemi-etl"]}
        }))
        .unwrap();
        let claims = json!({"azp": "fortemi-etl", "realm_access": {"roles": ["fortemi-agent", "uma_authorization"]}});
        let decision = policy.evaluate(&claims, &["openid".to_string()]).unwrap();
        assert_eq!(decision.scopes, vec!["mcp", "read"]);
        assert_eq!(decision.scope_grants, vec!["kc-agent"]);
        assert_eq!(decision.principal_kind, PrincipalKind::Service);
        assert_eq!(
            policy.evaluate(&json!({"azp": "other"}), &[]),
            Err(AuthError::ClientNotAllowed)
        );
    }

    #[test]
    fn invalid_policies_fail_startup_without_echoing_values() {
        for (value, needle) in [
            (
                json!({"scope_mapping": {"claim": "groups", "rules": [
                    {"id": "x", "value": "secret-group-sentinel", "scopes": ["system:admin"]}
                ]}}),
                "outside the Fortemi vocabulary",
            ),
            (
                json!({"scope_mapping": {"claim": "groups", "rules": []}}),
                "failed validation",
            ),
            (
                json!({"unknown": "secret-group-sentinel"}),
                "not a valid claim policy",
            ),
        ] {
            let error = policy(value).unwrap_err().to_string();
            assert!(error.contains(needle), "{error}");
            assert!(!error.contains("secret-group-sentinel"), "{error}");
        }
        let error = claim_policy_from_json(b"{not json")
            .unwrap_err()
            .to_string();
        assert!(error.contains("line 1"));
        assert!(load_claim_policy(Some(" ")).is_err());
        assert!(load_claim_policy(Some("/nonexistent/fortemi-policy.json")).is_err());
    }

    #[test]
    fn external_options_require_policy_and_token_scope_opt_in() {
        let audiences = BTreeSet::from(["https://fortemi.example".to_string()]);
        let options = ClaimPolicyLoadOptions {
            required: true,
            allow_token_scopes: false,
            forbidden_audiences: &audiences,
        };
        assert!(load_claim_policy_with_options(None, &options).is_err());
        assert!(claim_policy_from_json_with_options(
            json!({"scope_source": "token", "allow_token_scopes": true})
                .to_string()
                .as_bytes(),
            &options,
        )
        .is_err());

        let allowed = ClaimPolicyLoadOptions {
            allow_token_scopes: true,
            ..options
        };
        claim_policy_from_json_with_options(
            json!({"scope_source": "token", "allow_token_scopes": true})
                .to_string()
                .as_bytes(),
            &allowed,
        )
        .unwrap();
    }

    #[test]
    fn external_options_refuse_audience_equal_to_client_id() {
        let audiences = BTreeSet::from(["fortemi-web".to_string()]);
        let options = ClaimPolicyLoadOptions {
            required: true,
            allow_token_scopes: false,
            forbidden_audiences: &audiences,
        };
        let error = claim_policy_from_json_with_options(
            json!({
                "scope_mapping": {"claim": "roles", "rules": [
                    {"id": "read", "value": "reader", "scopes": ["read"]}
                ]},
                "clients": {"allowed": ["fortemi-web"]}
            })
            .to_string()
            .as_bytes(),
            &options,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("must not contain a claim-policy client id"));
        assert!(!error.contains("fortemi-web"));
    }
}

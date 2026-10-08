//! Loads the hosted OIDC claim policy (IdP group/role to Fortemi scope mapping, #1152).
//!
//! The policy file is read once when the verifier is built. Any problem fails startup:
//! an unreadable file, invalid JSON, unknown fields, a mapping to a scope outside the
//! Fortemi vocabulary, or anything `ClaimPolicy::from_config` rejects. Errors never
//! include the file path contents, group values or client ids.

use anyhow::Context;
use fortemi_auth_core::{ClaimPolicy, ClaimPolicyConfig};

/// Scopes an external IdP mapping may grant. `system:*` scopes are internal and are
/// never derivable from directory groups.
pub const MAPPABLE_SCOPES: &[&str] = &["read", "write", "admin", "mcp"];

/// Build the claim policy from `FORTEMI_AUTH_CLAIM_POLICY_FILE`, or the default
/// token-scope policy when unset.
pub fn load_claim_policy(path: Option<&str>) -> anyhow::Result<ClaimPolicy> {
    let Some(path) = path else {
        return Ok(ClaimPolicy::default());
    };
    anyhow::ensure!(
        !path.trim().is_empty(),
        "FORTEMI_AUTH_CLAIM_POLICY_FILE must name a nonempty JSON policy file"
    );
    let bytes = std::fs::read(path).context(
        "FORTEMI_AUTH_CLAIM_POLICY_FILE could not be read; mount the policy file readable by the server user",
    )?;
    claim_policy_from_json(&bytes)
}

/// Parse and validate policy JSON.
pub fn claim_policy_from_json(bytes: &[u8]) -> anyhow::Result<ClaimPolicy> {
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
}

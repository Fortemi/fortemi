//! Authorization policy contract for hosted and plugin-backed deployments.
//!
//! The first #710 contract slice keeps this independent of HTTP middleware. It
//! defines the decision shape and the scope-family split that prevents MCP
//! transport/session scope from becoming generic REST mutation authority.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::AuthPrincipal;

#[async_trait]
pub trait AuthorizationPolicy: Send + Sync {
    /// Decide whether `principal` may perform `action` on `resource` in `ctx`.
    ///
    /// Implementations must be deterministic for the same inputs within a
    /// policy epoch, safe to call concurrently, and fail closed on internal
    /// errors at the caller boundary.
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &Action,
        resource: &Resource,
        ctx: &AuthzContext,
    ) -> Result<Decision, AuthzError>;

    fn policy_id(&self) -> &'static str;

    fn policy_version(&self) -> &'static str;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub name: String,
    pub scope_family: ScopeFamily,
    pub required_scopes: Vec<String>,
}

impl Action {
    pub fn rest(name: impl Into<String>, required_scope: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope_family: ScopeFamily::Rest,
            required_scopes: vec![required_scope.into()],
        }
    }

    pub fn admin(name: impl Into<String>, required_scope: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope_family: ScopeFamily::Admin,
            required_scopes: vec![required_scope.into()],
        }
    }

    pub fn mcp_tool(
        name: impl Into<String>,
        mcp_scope: impl Into<String>,
        underlying_scope: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            scope_family: ScopeFamily::McpTool,
            required_scopes: vec![mcp_scope.into(), underlying_scope.into()],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScopeFamily {
    Rest,
    Admin,
    McpTransport,
    McpTool,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: Option<String>,
    pub tenant_id: Option<String>,
    pub attrs: HashMap<String, Value>,
}

impl Resource {
    pub fn new(kind: ResourceKind) -> Self {
        Self {
            kind,
            id: None,
            tenant_id: None,
            attrs: HashMap::new(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResourceKind {
    PublicRoute,
    Note,
    Attachment,
    Archive,
    Collection,
    Template,
    Taxonomy,
    DocumentType,
    Provenance,
    Job,
    ModelConfig,
    Inference,
    Webhook,
    Backup,
    ApiKey,
    McpTool,
    Tenant,
    System,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthzContext {
    pub tenant_id: Option<String>,
    pub environment: HashMap<String, Value>,
    pub correlation_id: Option<String>,
}

impl AuthzContext {
    pub fn personal() -> Self {
        Self {
            tenant_id: None,
            environment: HashMap::new(),
            correlation_id: None,
        }
    }

    pub fn hosted(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            environment: HashMap::new(),
            correlation_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Decision {
    Allow {
        obligations: Vec<Obligation>,
        policy_id: String,
        policy_version: String,
    },
    Deny {
        reason: DenyReason,
        policy_id: String,
        policy_version: String,
    },
    Indeterminate {
        reason: DenyReason,
        policy_id: String,
        policy_version: String,
    },
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. } | Self::Indeterminate { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Obligation {
    LogPii { fields: Vec<String> },
    RequireMfa,
    RecordReason { template: String },
    EnforceTtl { seconds: u64 },
    CacheControl { value: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DenyReason {
    Anonymous,
    MissingScope,
    TenantMismatch,
    PolicyDisabled,
    InvalidResource,
    PolicyError,
    Other(String),
}

#[derive(Debug, Error)]
pub enum AuthzError {
    #[error("authorization policy error: {0}")]
    Policy(String),
}

#[derive(Clone, Debug, Default)]
pub struct AllowAllPolicy;

#[async_trait]
impl AuthorizationPolicy for AllowAllPolicy {
    async fn authorize(
        &self,
        _principal: &AuthPrincipal,
        _action: &Action,
        _resource: &Resource,
        _ctx: &AuthzContext,
    ) -> Result<Decision, AuthzError> {
        Ok(Decision::Allow {
            obligations: vec![],
            policy_id: self.policy_id().to_string(),
            policy_version: self.policy_version().to_string(),
        })
    }

    fn policy_id(&self) -> &'static str {
        "allow_all"
    }

    fn policy_version(&self) -> &'static str {
        "2026-06-25"
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoleBasedPolicy;

#[async_trait]
impl AuthorizationPolicy for RoleBasedPolicy {
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &Action,
        resource: &Resource,
        ctx: &AuthzContext,
    ) -> Result<Decision, AuthzError> {
        if !principal.is_authenticated() {
            return Ok(self.deny(DenyReason::Anonymous));
        }

        if let Some(ctx_tenant) = &ctx.tenant_id {
            match &resource.tenant_id {
                Some(resource_tenant) if resource_tenant == ctx_tenant => {}
                Some(_) => return Ok(self.deny(DenyReason::TenantMismatch)),
                None => return Ok(self.deny(DenyReason::InvalidResource)),
            }
        }

        if action
            .required_scopes
            .iter()
            .all(|scope| principal_has_family_scope(principal, &action.scope_family, scope))
        {
            Ok(Decision::Allow {
                obligations: vec![],
                policy_id: self.policy_id().to_string(),
                policy_version: self.policy_version().to_string(),
            })
        } else {
            Ok(self.deny(DenyReason::MissingScope))
        }
    }

    fn policy_id(&self) -> &'static str {
        "role_based"
    }

    fn policy_version(&self) -> &'static str {
        "2026-06-25"
    }
}

impl RoleBasedPolicy {
    fn deny(&self, reason: DenyReason) -> Decision {
        Decision::Deny {
            reason,
            policy_id: self.policy_id().to_string(),
            policy_version: self.policy_version().to_string(),
        }
    }
}

fn principal_has_family_scope(
    principal: &AuthPrincipal,
    family: &ScopeFamily,
    required_scope: &str,
) -> bool {
    let granted = principal.scope_str();

    granted.split_whitespace().any(|scope| {
        scope == "admin"
            || scope == "system:*"
            || scope == required_scope
            || family_allows_legacy_scope(family, scope, required_scope)
    })
}

fn family_allows_legacy_scope(family: &ScopeFamily, granted: &str, required: &str) -> bool {
    match family {
        ScopeFamily::Rest => {
            (granted == "write" && required == "read")
                || (granted == "write" && required == "write")
                || (granted == "read" && required == "read")
        }
        ScopeFamily::Admin => granted == "admin",
        ScopeFamily::McpTransport => granted == "mcp" || granted == required,
        ScopeFamily::McpTool => false,
        ScopeFamily::System => granted == "system:admin" || granted == "system:*",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn allow_all_policy_preserves_personal_mode_behavior() {
        let decision = AllowAllPolicy
            .authorize(
                &AuthPrincipal::Anonymous,
                &Action::rest("notes:write", "notes:write"),
                &Resource::new(ResourceKind::Note),
                &AuthzContext::personal(),
            )
            .await
            .unwrap();

        assert!(decision.is_allow());
    }

    #[tokio::test]
    async fn role_policy_allows_matching_rest_scope() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "notes:read notes:write".to_string(),
            user_id: Some("user".to_string()),
        };

        let decision = RoleBasedPolicy
            .authorize(
                &principal,
                &Action::rest("notes:update", "notes:write"),
                &Resource::new(ResourceKind::Note).with_tenant("tenant-a"),
                &AuthzContext::hosted("tenant-a"),
            )
            .await
            .unwrap();

        assert!(decision.is_allow());
    }

    #[tokio::test]
    async fn role_policy_denies_mcp_transport_scope_for_rest_write() {
        let principal = AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "mcp".to_string(),
        };

        let decision = RoleBasedPolicy
            .authorize(
                &principal,
                &Action::rest("notes:update", "notes:write"),
                &Resource::new(ResourceKind::Note).with_tenant("tenant-a"),
                &AuthzContext::hosted("tenant-a"),
            )
            .await
            .unwrap();

        assert_eq!(
            decision,
            Decision::Deny {
                reason: DenyReason::MissingScope,
                policy_id: "role_based".to_string(),
                policy_version: "2026-06-25".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn role_policy_requires_mcp_wrapper_and_underlying_resource_scope() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "mcp:read notes:read".to_string(),
            user_id: Some("user".to_string()),
        };

        let decision = RoleBasedPolicy
            .authorize(
                &principal,
                &Action::mcp_tool("mcp:search_notes", "mcp:read", "notes:read"),
                &Resource::new(ResourceKind::McpTool).with_tenant("tenant-a"),
                &AuthzContext::hosted("tenant-a"),
            )
            .await
            .unwrap();

        assert!(decision.is_allow());
    }

    #[tokio::test]
    async fn role_policy_denies_mcp_tool_when_underlying_scope_is_missing() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "mcp:read".to_string(),
            user_id: Some("user".to_string()),
        };

        let decision = RoleBasedPolicy
            .authorize(
                &principal,
                &Action::mcp_tool("mcp:search_notes", "mcp:read", "notes:read"),
                &Resource::new(ResourceKind::McpTool).with_tenant("tenant-a"),
                &AuthzContext::hosted("tenant-a"),
            )
            .await
            .unwrap();

        assert!(decision.is_deny());
    }

    #[tokio::test]
    async fn role_policy_denies_cross_tenant_resource() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "notes:read".to_string(),
            user_id: Some("user".to_string()),
        };

        let decision = RoleBasedPolicy
            .authorize(
                &principal,
                &Action::rest("notes:read", "notes:read"),
                &Resource::new(ResourceKind::Note).with_tenant("tenant-b"),
                &AuthzContext::hosted("tenant-a"),
            )
            .await
            .unwrap();

        assert_eq!(
            decision,
            Decision::Deny {
                reason: DenyReason::TenantMismatch,
                policy_id: "role_based".to_string(),
                policy_version: "2026-06-25".to_string(),
            }
        );
    }
}

//! Authorization policy contract for hosted and plugin-backed deployments.
//!
//! The first #710 contract slice keeps this independent of HTTP middleware. It
//! defines the decision shape and the scope-family split that prevents MCP
//! transport/session scope from becoming generic REST mutation authority.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::AuthPrincipal;

/// Regression budget for the first in-process policy implementation.
///
/// `RoleBasedPolicy` is intentionally a hot-path, in-process authorization
/// gate. In debug/test builds this budget is loose enough to avoid benchmark
/// flake, but it still prevents accidental network, database, filesystem, or
/// other high-latency work from entering the policy decision path.
pub const IN_PROCESS_POLICY_EVAL_TARGET_AVG_MICROS: u128 = 250;

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

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub name: String,
    pub scope_family: ScopeFamily,
    pub required_scopes: Vec<String>,
}

impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action")
            .field("name_len", &self.name.chars().count())
            .field("scope_family", &self.scope_family)
            .field("required_scope_count", &self.required_scopes.len())
            .field(
                "required_scope_total_len",
                &self
                    .required_scopes
                    .iter()
                    .map(|scope| scope.chars().count())
                    .sum::<usize>(),
            )
            .finish()
    }
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

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: Option<String>,
    pub tenant_id: Option<String>,
    pub attrs: HashMap<String, Value>,
}

impl fmt::Debug for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resource")
            .field("kind", &self.kind)
            .field("id_present", &self.id.is_some())
            .field("tenant_id_present", &self.tenant_id.is_some())
            .field("attr_count", &self.attrs.len())
            .field(
                "attr_key_total_len",
                &self
                    .attrs
                    .keys()
                    .map(|key| key.chars().count())
                    .sum::<usize>(),
            )
            .finish()
    }
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

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
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

impl fmt::Debug for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PublicRoute => f.write_str("PublicRoute"),
            Self::Note => f.write_str("Note"),
            Self::Attachment => f.write_str("Attachment"),
            Self::Archive => f.write_str("Archive"),
            Self::Collection => f.write_str("Collection"),
            Self::Template => f.write_str("Template"),
            Self::Taxonomy => f.write_str("Taxonomy"),
            Self::DocumentType => f.write_str("DocumentType"),
            Self::Provenance => f.write_str("Provenance"),
            Self::Job => f.write_str("Job"),
            Self::ModelConfig => f.write_str("ModelConfig"),
            Self::Inference => f.write_str("Inference"),
            Self::Webhook => f.write_str("Webhook"),
            Self::Backup => f.write_str("Backup"),
            Self::ApiKey => f.write_str("ApiKey"),
            Self::McpTool => f.write_str("McpTool"),
            Self::Tenant => f.write_str("Tenant"),
            Self::System => f.write_str("System"),
            Self::Other(value) => f
                .debug_tuple("Other")
                .field(&format_args!("len={}", value.chars().count()))
                .finish(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthzContext {
    pub tenant_id: Option<String>,
    pub environment: HashMap<String, Value>,
    pub correlation_id: Option<String>,
}

impl fmt::Debug for AuthzContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthzContext")
            .field("tenant_id_present", &self.tenant_id.is_some())
            .field("environment_count", &self.environment.len())
            .field(
                "environment_key_total_len",
                &self
                    .environment
                    .keys()
                    .map(|key| key.chars().count())
                    .sum::<usize>(),
            )
            .field("correlation_id_present", &self.correlation_id.is_some())
            .finish()
    }
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

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
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

impl fmt::Debug for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow {
                obligations,
                policy_id,
                policy_version,
            } => f
                .debug_struct("Allow")
                .field("obligation_count", &obligations.len())
                .field("policy_id_len", &policy_id.chars().count())
                .field("policy_version_len", &policy_version.chars().count())
                .finish(),
            Self::Deny {
                reason,
                policy_id,
                policy_version,
            } => f
                .debug_struct("Deny")
                .field("reason", reason)
                .field("policy_id_len", &policy_id.chars().count())
                .field("policy_version_len", &policy_version.chars().count())
                .finish(),
            Self::Indeterminate {
                reason,
                policy_id,
                policy_version,
            } => f
                .debug_struct("Indeterminate")
                .field("reason", reason)
                .field("policy_id_len", &policy_id.chars().count())
                .field("policy_version_len", &policy_version.chars().count())
                .finish(),
        }
    }
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. } | Self::Indeterminate { .. })
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Obligation {
    LogPii { fields: Vec<String> },
    RequireMfa,
    RecordReason { template: String },
    EnforceTtl { seconds: u64 },
    CacheControl { value: String },
}

impl fmt::Debug for Obligation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LogPii { fields } => f
                .debug_struct("LogPii")
                .field("field_count", &fields.len())
                .field(
                    "field_total_len",
                    &fields
                        .iter()
                        .map(|field| field.chars().count())
                        .sum::<usize>(),
                )
                .finish(),
            Self::RequireMfa => f.write_str("RequireMfa"),
            Self::RecordReason { template } => f
                .debug_struct("RecordReason")
                .field("template_len", &template.chars().count())
                .finish(),
            Self::EnforceTtl { seconds } => f
                .debug_struct("EnforceTtl")
                .field("seconds", seconds)
                .finish(),
            Self::CacheControl { value } => f
                .debug_struct("CacheControl")
                .field("value_len", &value.chars().count())
                .finish(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum DenyReason {
    Anonymous,
    MissingScope,
    TenantMismatch,
    PolicyDisabled,
    InvalidResource,
    PolicyError,
    Other(String),
}

impl fmt::Debug for DenyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anonymous => f.write_str("Anonymous"),
            Self::MissingScope => f.write_str("MissingScope"),
            Self::TenantMismatch => f.write_str("TenantMismatch"),
            Self::PolicyDisabled => f.write_str("PolicyDisabled"),
            Self::InvalidResource => f.write_str("InvalidResource"),
            Self::PolicyError => f.write_str("PolicyError"),
            Self::Other(value) => f
                .debug_tuple("Other")
                .field(&format_args!("len={}", value.chars().count()))
                .finish(),
        }
    }
}

#[derive(Error)]
pub enum AuthzError {
    #[error("authorization policy error")]
    Policy(String),
}

impl fmt::Debug for AuthzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(message) => f
                .debug_struct("Policy")
                .field("message_len", &message.chars().count())
                .finish(),
        }
    }
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
            (granted == "write" && (required == "read" || required == "write"))
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

    #[test]
    fn authorization_contract_debug_reports_metadata_without_raw_policy_inputs() {
        let action = Action {
            name: "notes:update:tenant-alpha".to_string(),
            scope_family: ScopeFamily::Rest,
            required_scopes: vec![
                "notes:write".to_string(),
                "Bearer should-not-be-here".to_string(),
            ],
        };

        let mut resource = Resource::new(ResourceKind::Other("custom-secret-resource".to_string()))
            .with_id("note-raw-id-123")
            .with_tenant("tenant-alpha");
        resource.attrs.insert(
            "authorization".to_string(),
            Value::String("Bearer attr-token".to_string()),
        );
        resource.attrs.insert(
            "path".to_string(),
            Value::String("/srv/private/note.md".to_string()),
        );

        let mut ctx = AuthzContext::hosted("tenant-alpha");
        ctx.environment.insert(
            "DATABASE_URL".to_string(),
            Value::String("postgres://user:pass@localhost/db".to_string()),
        );
        ctx.correlation_id = Some("correlation-raw-id".to_string());

        let decision = Decision::Deny {
            reason: DenyReason::Other("missing secret scope for tenant-alpha".to_string()),
            policy_id: "custom-policy-id".to_string(),
            policy_version: "2026-secret-version".to_string(),
        };

        let obligation = Obligation::RecordReason {
            template: "denied tenant-alpha for notes:update".to_string(),
        };

        let debug = format!("{action:?}\n{resource:?}\n{ctx:?}\n{decision:?}\n{obligation:?}");

        assert!(debug.contains("name_len"));
        assert!(debug.contains("required_scope_count: 2"));
        assert!(debug.contains("id_present: true"));
        assert!(debug.contains("tenant_id_present: true"));
        assert!(debug.contains("environment_count: 1"));
        assert!(debug.contains("template_len"));
        assert!(!debug.contains("notes:update:tenant-alpha"));
        assert!(!debug.contains("notes:write"));
        assert!(!debug.contains("Bearer should-not-be-here"));
        assert!(!debug.contains("custom-secret-resource"));
        assert!(!debug.contains("note-raw-id-123"));
        assert!(!debug.contains("tenant-alpha"));
        assert!(!debug.contains("authorization"));
        assert!(!debug.contains("Bearer attr-token"));
        assert!(!debug.contains("/srv/private/note.md"));
        assert!(!debug.contains("DATABASE_URL"));
        assert!(!debug.contains("postgres://user:pass@localhost/db"));
        assert!(!debug.contains("correlation-raw-id"));
        assert!(!debug.contains("missing secret scope"));
        assert!(!debug.contains("custom-policy-id"));
        assert!(!debug.contains("2026-secret-version"));
    }

    #[test]
    fn authz_error_debug_and_display_do_not_echo_policy_detail() {
        let error = AuthzError::Policy(
            "backend policy failed for postgres://user:pass@localhost/db".to_string(),
        );

        let debug = format!("{error:?}");
        let display = error.to_string();

        assert!(debug.contains("message_len"));
        assert_eq!(display, "authorization policy error");
        assert!(!debug.contains("postgres://user:pass@localhost/db"));
        assert!(!display.contains("postgres://user:pass@localhost/db"));
        assert!(!debug.contains("backend policy failed"));
        assert!(!display.contains("backend policy failed"));
    }

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

    #[tokio::test]
    async fn role_policy_stays_within_in_process_eval_budget() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "notes:read notes:write mcp:read".to_string(),
            user_id: Some("user".to_string()),
        };
        let action = Action::rest("notes:update", "notes:write");
        let resource = Resource::new(ResourceKind::Note).with_tenant("tenant-a");
        let ctx = AuthzContext::hosted("tenant-a");

        let iterations = 2_000u128;
        let started = std::time::Instant::now();
        for _ in 0..iterations {
            let decision = RoleBasedPolicy
                .authorize(&principal, &action, &resource, &ctx)
                .await
                .unwrap();
            assert!(decision.is_allow());
        }

        let avg_micros = started.elapsed().as_micros() / iterations;
        assert!(
            avg_micros <= IN_PROCESS_POLICY_EVAL_TARGET_AVG_MICROS,
            "average RoleBasedPolicy evaluation took {avg_micros}us; target is {IN_PROCESS_POLICY_EVAL_TARGET_AVG_MICROS}us"
        );
    }
}

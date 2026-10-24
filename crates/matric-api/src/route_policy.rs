//! Route authorization inventory for the first AuthorizationPolicy slice (#710).
//!
//! This table is intentionally executable: authentication exemptions and
//! admin/operator checks read from it, and tests compare it with the Axum router
//! declarations in `main.rs` and production modules. The inventory records
//! policy/docs/cache/isolation metadata; runtime isolation uses exact Axum
//! matched templates and methods, defaulting unknown identities to complete catalog.

use std::collections::HashMap;
use std::fmt;

use axum::http::Method;
use matric_core::{Action, AuthPrincipal, AuthzContext, Resource, ResourceKind, ScopeFamily};
use serde_json::json;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyClass {
    Public,
    PublicWithInlineProof,
    AuthenticatedRead,
    AuthenticatedWrite,
    AdminOperator,
    TenantObject,
    SystemHealth,
    OAuth,
    RealtimeTransport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocsExposureClass {
    Public,
    Authenticated,
    Operator,
    Hidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheHeaderClass {
    PublicProbe,
    PrivateUserData,
    NoStore,
}

/// Archive authority required in addition to the ordinary route decision.
/// Methods may use separate inventory rows when their archive authority differs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveIsolation {
    /// Verified handler operations use only the authorized resolver-selected archive.
    ArchiveScoped,
    /// Explicit target/catalog filtering applies the current archive policy.
    CatalogFiltered,
    /// Shared, global, or not yet verified: require every archive in the tenant.
    CompleteCatalog,
    /// No archive data; ordinary authentication/authorization still applies.
    Public,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RoutePolicy {
    pub path: &'static str,
    pub class: PolicyClass,
    pub action_family: &'static str,
    pub docs: DocsExposureClass,
    pub cache: CacheHeaderClass,
    pub archive_isolation: ArchiveIsolation,
    pub methods: &'static [&'static str],
}

impl fmt::Debug for RoutePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutePolicy")
            .field("path_len", &self.path.chars().count())
            .field("class", &self.class)
            .field("action_family_len", &self.action_family.chars().count())
            .field("docs", &self.docs)
            .field("cache", &self.cache)
            .field("archive_isolation", &self.archive_isolation)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct RoutePolicyInput {
    pub policy: &'static RoutePolicy,
    pub action: Action,
    pub resource: Resource,
    pub context: AuthzContext,
    /// Verified token attribution for audit only; never passed to `AuthorizationPolicy`.
    /// Kept outside `context.environment`, which is an input to access decisions.
    pub audit_azp: Option<String>,
}

impl fmt::Debug for RoutePolicyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutePolicyInput")
            .field("policy", self.policy)
            .field("action", &self.action)
            .field("resource", &self.resource)
            .field("context", &self.context)
            .finish()
    }
}

struct ResourceIdCandidate {
    param_name: &'static str,
    value: String,
}

use CacheHeaderClass::{NoStore, PrivateUserData, PublicProbe};
use DocsExposureClass::{Authenticated, Hidden, Operator, Public as DocsPublic};
use PolicyClass::{
    AdminOperator, AuthenticatedRead, AuthenticatedWrite, OAuth, Public, PublicWithInlineProof,
    RealtimeTransport, SystemHealth, TenantObject,
};

use ArchiveIsolation::{ArchiveScoped, CatalogFiltered, CompleteCatalog, Public as NoArchiveData};

pub const OPERATOR_DOCS_PATH: &str = "/api/v1/operator/docs";
pub const OPERATOR_OPENAPI_PATH: &str = "/api/v1/operator/openapi.yaml";
pub const OPERATOR_ASYNCAPI_PATH: &str = "/api/v1/operator/asyncapi.yaml";

/// A deliberately closed TenantObject route which has no backing-resource
/// normalizer yet.  These records are exact route identities, not families or
/// prefixes: the role-based admission guard continues to deny each one before
/// its handler can run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClosedRouteNormalizationException {
    pub path: &'static str,
    pub methods: &'static [&'static str],
    pub reason: &'static str,
    pub follow_up: &'static str,
}

/// #1278 is limited to embedding-set normalization.  These existing routes
/// remain closed until their follow-up has a tenant-scoped lookup and a
/// non-enumerating handler qualification.
pub(crate) const CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS: &[ClosedRouteNormalizationException] = &[
    ClosedRouteNormalizationException {
        path: "/api/v1/calls/{id}",
        methods: &["GET", "HEAD"],
        reason: "Call dispatch lacks a tenant-scoped backing-resource lookup, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1280",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/graph/{id}",
        methods: &["GET", "HEAD"],
        reason: "Graph traversal needs its own tenant-scoped dispatch qualification, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1280",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/notes/{id}/concepts",
        methods: &["GET", "HEAD", "POST"],
        reason: "Note-scoped concept operations need a tenant-scoped taxonomy lookup, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1279",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/notes/{id}/concepts/{concept_id}",
        methods: &["DELETE"],
        reason: "Note-scoped concept removal needs tenant-scoped note and concept qualification, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1279",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/notes/{id}/memory-provenance",
        methods: &["GET", "HEAD"],
        reason: "Memory provenance needs a tenant-scoped provenance lookup, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1279",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/notes/{id}/provenance",
        methods: &["GET", "HEAD"],
        reason: "Provenance operations need a tenant-scoped provenance lookup, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1279",
    },
    ClosedRouteNormalizationException {
        path: "/api/v1/notes/{id}/move",
        methods: &["POST"],
        reason: "Moving a note needs tenant-scoped note and target-collection qualification, so role-based admission remains closed.",
        follow_up: "https://git.integrolabs.net/Fortemi/fortemi/issues/1280",
    },
];

pub const ROUTE_POLICY_INVENTORY: &[RoutePolicy] = &[
    r(
        "/api/v1/operator/docs/",
        &["GET", "HEAD"],
        AdminOperator,
        "docs_schema",
        Operator,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/operator/docs/{*rest}",
        &["GET", "HEAD"],
        AdminOperator,
        "docs_schema",
        Operator,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/.well-known/oauth-authorization-server",
        &["GET", "HEAD"],
        OAuth,
        "oauth_discovery",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    r(
        "/.well-known/oauth-protected-resource",
        &["GET", "HEAD"],
        OAuth,
        "oauth_discovery",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    r(
        "/.well-known/oauth-protected-resource/{*resource_path}",
        &["GET", "HEAD"],
        OAuth,
        "oauth_discovery",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    r(
        "/api/v1/api-keys",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/api-keys/{id}",
        &["DELETE"],
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/archives",
        &["GET", "HEAD"],
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/archives",
        &["POST"],
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/archives/{name}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/archives/{name}/clone",
        &["POST"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/archives/{name}/set-default",
        &["POST"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/archives/{name}/stats",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/attachments",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}",
        &["DELETE", "GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/download",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/sprites/{sprite_index}",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/subtitles",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/thumbnail",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/thumbnails.vtt",
        &["GET", "HEAD"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/audio/transcribe",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/auth/token-info",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "token_verification",
        Hidden,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/me",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "principal",
        Authenticated,
        PrivateUserData,
        NoArchiveData,
    ),
    r(
        "/api/v1/me/tokens",
        &["GET", "HEAD", "POST"],
        AuthenticatedRead,
        "credential_management",
        Authenticated,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/me/tokens/{id}",
        &["DELETE"],
        AuthenticatedWrite,
        "credential_management",
        Authenticated,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/admin/users",
        &["GET", "HEAD"],
        AdminOperator,
        "principal_admin",
        Operator,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/admin/users/{id}/disable",
        &["POST"],
        AdminOperator,
        "principal_admin",
        Operator,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/admin/tokens",
        &["GET", "HEAD"],
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/admin/tokens/{id}",
        &["DELETE"],
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/admin/users/{id}/enable",
        &["POST"],
        AdminOperator,
        "principal_admin",
        Operator,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/database",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/database/restore",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/database/snapshot",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/database/upload",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/download",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/export",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/import",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/knowledge-archive",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/knowledge-archive/{filename}",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/knowledge-shard",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/knowledge-shard/import",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/knowledge-shard/upload",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/list",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/list/{filename}",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/memory/{name}",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/backup/metadata/{filename}",
        &["GET", "HEAD", "PUT"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/status",
        &["GET", "HEAD"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/swap",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/backup/trigger",
        &["POST"],
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/calls/{id}",
        &["GET", "HEAD"],
        TenantObject,
        "realtime_call",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/chat",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/chat/models",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "ai_execution",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/chat/stream",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/collections",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/collections/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/collections/{id}/export",
        &["GET", "HEAD"],
        TenantObject,
        "collection",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/collections/{id}/notes",
        &["GET", "HEAD"],
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/autocomplete",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/collections",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/collections/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/collections/{id}/members",
        &["PUT"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/collections/{id}/members/{concept_id}",
        &["DELETE", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/governance",
        &["GET", "HEAD"],
        AdminOperator,
        "taxonomy",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/schemes",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/schemes/export/turtle",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/schemes/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/schemes/{id}/export/turtle",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/schemes/{id}/top-concepts",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/ancestors",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/broader",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/broader/{target_id}",
        &["DELETE"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/descendants",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/full",
        &["GET", "HEAD"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/narrower",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/narrower/{target_id}",
        &["DELETE"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/related",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/concepts/{id}/related/{target_id}",
        &["DELETE"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/document-types",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "document_type_catalog",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/document-types/detect",
        &["POST"],
        AuthenticatedWrite,
        "document_type_catalog",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/document-types/{name}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        AdminOperator,
        "document_type_catalog",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-configs",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-configs/default",
        &["GET", "HEAD"],
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-configs/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/members",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/members/{note_id}",
        &["DELETE"],
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/build-index",
        &["POST"],
        TenantObject,
        "embedding_control",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/index-health",
        &["GET", "HEAD"],
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/runs",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/runs/{run_id}",
        &["GET", "HEAD"],
        TenantObject,
        "embedding_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/refresh",
        &["POST"],
        TenantObject,
        "embedding_control",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/entities/similar",
        &["GET", "HEAD"],
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/events",
        &["GET", "HEAD"],
        RealtimeTransport,
        "event_stream",
        Authenticated,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/events/tokens",
        &["POST"],
        AdminOperator,
        "event_stream",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/events/tokens/{token_id}",
        &["DELETE"],
        AdminOperator,
        "event_stream",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/extraction/stats",
        &["GET", "HEAD"],
        AdminOperator,
        "system_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/cold-spots",
        &["GET", "HEAD"],
        TenantObject,
        "graph_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/community/coarse",
        &["POST"],
        TenantObject,
        "graph_control",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/diagnostics",
        &["GET", "HEAD"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/diagnostics/compare",
        &["GET", "HEAD"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/diagnostics/history",
        &["GET", "HEAD"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/diagnostics/snapshot",
        &["POST"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/maintenance",
        &["POST"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/pfnet/sparsify",
        &["POST"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/snn/recompute",
        &["POST"],
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/topology/stats",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "graph_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/graph/{id}",
        &["GET", "HEAD"],
        TenantObject,
        "graph_control",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/access-frequency",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/knowledge",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/orphan-tags",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/stale-notes",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/streaming",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        DocsPublic,
        PublicProbe,
        CompleteCatalog,
    ),
    r(
        "/api/v1/system/compatibility",
        &["GET", "HEAD"],
        SystemHealth,
        "compatibility_discovery",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/health/tag-cooccurrence",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/health/unlinked-notes",
        &["GET", "HEAD"],
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inbound-sources",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "inbound_connector",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inbound-sources/{name}",
        &["DELETE"],
        AdminOperator,
        "inbound_connector",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/complete",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/catalog",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "ai_execution",
        Hidden,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/config",
        &["DELETE", "GET", "HEAD", "POST"],
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/config/audit",
        &["GET", "HEAD"],
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/embed",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/providers",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "ai_execution",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/stream",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/inference/test-connection",
        &["POST"],
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/ingest/stream",
        &["POST"],
        RealtimeTransport,
        "ingest_stream",
        Hidden,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/ingest/tokens",
        &["POST"],
        AdminOperator,
        "ingest_stream",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/ingest/tokens/{token_id}",
        &["DELETE"],
        AdminOperator,
        "ingest_stream",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/jobs/pause",
        &["POST"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs/pause/{archive}",
        &["POST"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/jobs/pending",
        &["GET", "HEAD"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs/resume",
        &["POST"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs/resume/{archive}",
        &["POST"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/jobs/stats",
        &["GET", "HEAD"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs/status",
        &["GET", "HEAD"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/jobs/{id}",
        &["GET", "HEAD"],
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/memories",
        &["GET", "HEAD"],
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/memories",
        &["POST"],
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memories/available",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        NoStore,
        CatalogFiltered,
    ),
    r(
        "/api/v1/memories/overview",
        &["GET", "HEAD"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/memories/search",
        &["GET", "HEAD"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memories/{name}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/memories/{name}/clone",
        &["POST"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memories/{name}/set-default",
        &["POST"],
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memories/{name}/stats",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/lifecycle-purge",
        &["POST"],
        AuthenticatedWrite,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/lifecycle-purge/preview",
        &["POST"],
        AuthenticatedWrite,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/lifecycle-purge/{operation_id}",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/lifecycle-purge/{operation_id}/resume",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memory/context",
        &["GET", "HEAD"],
        TenantObject,
        "memory_management",
        Authenticated,
        NoStore,
        ArchiveScoped,
    ),
    r(
        "/api/v1/memory/export",
        &["POST"],
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/memory/info",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/models",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "model_catalog",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/activity",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/bulk",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/source-upsert",
        &["POST"],
        AuthenticatedWrite,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/reprocess",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/timeline",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        ArchiveScoped,
    ),
    r(
        "/api/v1/notes/{id}/attachments",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/attachments/tus",
        &["OPTIONS", "POST"],
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/attachments/tus/{upload_id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/attachments/upload",
        &["POST"],
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/backlinks",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/concepts",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/concepts/{concept_id}",
        &["DELETE"],
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/export",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/full",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/links",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/memory-provenance",
        &["GET", "HEAD"],
        TenantObject,
        "provenance",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/move",
        &["POST"],
        TenantObject,
        "collection",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/provenance",
        &["GET", "HEAD"],
        TenantObject,
        "provenance",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/purge",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/related",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/similar",
        &["GET", "HEAD"],
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/reprocess",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/restore",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/status",
        &["PATCH"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        ArchiveScoped,
    ),
    r(
        "/api/v1/notes/{id}/tags",
        &["GET", "HEAD", "PUT"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/versions",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/versions/diff",
        &["GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/versions/{version}",
        &["DELETE", "GET", "HEAD"],
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/notes/{id}/versions/{version}/restore",
        &["POST"],
        TenantObject,
        "note",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/address",
        &["POST"],
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/decrypt",
        &["POST"],
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/encrypt",
        &["POST"],
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keygen",
        &["POST"],
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets/active",
        &["GET", "HEAD"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets/import",
        &["POST"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}",
        &["DELETE"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}/active",
        &["PUT"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}/export",
        &["GET", "HEAD"],
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/recipients",
        &["POST"],
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/pke/verify/{address}",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "pke",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/provenance/devices",
        &["POST"],
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/provenance/files",
        &["POST"],
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/provenance/locations",
        &["POST"],
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/provenance/named-locations",
        &["POST"],
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/provenance/notes",
        &["POST"],
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/rate-limit/status",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "rate_limit",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/realtime/twilio/{provider_call_id}",
        &["GET", "HEAD"],
        PublicWithInlineProof,
        "realtime_provider_callback",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/search",
        &["GET", "HEAD"],
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/search/evidence/resolve",
        &["POST"],
        AuthenticatedRead,
        "search",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/search/federated",
        &["POST"],
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
        CatalogFiltered,
    ),
    r(
        "/api/v1/tags",
        &["GET", "HEAD"],
        AuthenticatedRead,
        "taxonomy",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/templates",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "template",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/templates/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        TenantObject,
        "template",
        Authenticated,
        PrivateUserData,
        CompleteCatalog,
    ),
    r(
        "/api/v1/templates/{id}/instantiate",
        &["POST"],
        TenantObject,
        "template",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/user/secrets",
        &["GET", "HEAD", "POST"],
        TenantObject,
        "user_secret",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/user/secrets/{id}",
        &["DELETE"],
        TenantObject,
        "user_secret",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/vision/describe",
        &["POST"],
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/incoming",
        &["GET", "HEAD", "POST"],
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/incoming/validate",
        &["POST"],
        PublicWithInlineProof,
        "webhook_receiver",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/incoming/{slug}",
        &["DELETE", "GET", "HEAD", "PATCH", "POST"],
        PublicWithInlineProof,
        "webhook_receiver",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/{id}",
        &["DELETE", "GET", "HEAD", "PATCH"],
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/{id}/deliveries",
        &["GET", "HEAD"],
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/webhooks/{id}/test",
        &["POST"],
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
        CompleteCatalog,
    ),
    r(
        "/api/v1/ws",
        &["GET", "HEAD"],
        RealtimeTransport,
        "event_stream",
        Hidden,
        NoStore,
        CompleteCatalog,
    ),
    r(
        OPERATOR_ASYNCAPI_PATH,
        &["GET", "HEAD"],
        AdminOperator,
        "docs_schema",
        Operator,
        NoStore,
        NoArchiveData,
    ),
    r(
        OPERATOR_DOCS_PATH,
        &["GET", "HEAD"],
        AdminOperator,
        "docs_schema",
        Operator,
        NoStore,
        NoArchiveData,
    ),
    r(
        OPERATOR_OPENAPI_PATH,
        &["GET", "HEAD"],
        AdminOperator,
        "docs_schema",
        Operator,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/health",
        &["GET", "HEAD", "POST"],
        Public,
        "health_probe",
        DocsPublic,
        PublicProbe,
        CatalogFiltered,
    ),
    r(
        "/health/live",
        &["GET", "HEAD"],
        Public,
        "health_probe",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    r(
        "/livez",
        &["GET", "HEAD"],
        Public,
        "health_probe",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    r(
        "/readyz",
        &["GET", "HEAD"],
        Public,
        "health_probe",
        DocsPublic,
        PublicProbe,
        NoArchiveData,
    ),
    // Bypasses bearer middleware, but the handler authenticates the resource owner
    // and binds consent to a server-side transaction (#943).
    r(
        "/oauth/authorize",
        &["GET", "HEAD", "POST"],
        OAuth,
        "oauth_owner_consent",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/oauth/introspect",
        &["POST"],
        OAuth,
        "oauth_flow",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/oauth/register",
        &["POST"],
        OAuth,
        "oauth_flow",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/oauth/revoke",
        &["POST"],
        OAuth,
        "oauth_flow",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/oauth/token",
        &["POST"],
        OAuth,
        "oauth_flow",
        DocsPublic,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/recording.wav",
        &["GET", "HEAD"],
        Public,
        "test_fixture",
        Hidden,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/problem-contract-test",
        &["GET", "HEAD"],
        Public,
        "test_fixture",
        Hidden,
        NoStore,
        NoArchiveData,
    ),
    r(
        "/api/v1/problem-contract-status/{status}",
        &["GET", "HEAD"],
        Public,
        "test_fixture",
        Hidden,
        NoStore,
        NoArchiveData,
    ),
];

const fn r(
    path: &'static str,
    methods: &'static [&'static str],
    class: PolicyClass,
    action_family: &'static str,
    docs: DocsExposureClass,
    cache: CacheHeaderClass,
    archive_isolation: ArchiveIsolation,
) -> RoutePolicy {
    RoutePolicy {
        path,
        class,
        action_family,
        docs,
        cache,
        archive_isolation,
        methods,
    }
}

fn closed_route_normalization_exception(
    policy: &RoutePolicy,
    method: &Method,
) -> Option<&'static ClosedRouteNormalizationException> {
    CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS
        .iter()
        .find(|exception| {
            exception.path == policy.path && exception.methods.contains(&method.as_str())
        })
}

/// True only for one of the exact #1278 closure records and its listed method.
/// This is intentionally independent of `requires_backing_resource_normalization`:
/// that flag is false for inventory accounting, but cannot make a closed route
/// admissible under the hosted role-based policy.
pub(crate) fn role_based_route_is_intentionally_closed(input: &RoutePolicyInput) -> bool {
    let Some((_, method)) = input.action.name.rsplit_once(':') else {
        return false;
    };
    CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS
        .iter()
        .any(|exception| {
            exception.path == input.policy.path
                && exception
                    .methods
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(method))
        })
}

/// Exact Axum route identity. Unknown templates or methods fail closed at admission.
pub fn route_policy_for_matched_path(
    method: &Method,
    template: &str,
) -> Option<&'static RoutePolicy> {
    ROUTE_POLICY_INVENTORY
        .iter()
        .find(|route| route.path == template && route.methods.contains(&method.as_str()))
}

pub fn route_policy_for_path(path: &str) -> Option<&'static RoutePolicy> {
    ROUTE_POLICY_INVENTORY
        .iter()
        .find(|route| route_template_matches(route.path, path))
}

/// Whether an authenticated hosted route has completed request-transaction migration.
///
/// Unknown and unreleased routes fail closed in `auth_middleware`; community mode
/// does not consult this gate.
pub fn hosted_tenant_transaction_ready(method: &Method, path: &str) -> bool {
    let Some(policy) = route_policy_for_path(path) else {
        return false;
    };

    matches!(
        (method, policy.path),
        (&Method::GET, "/api/v1/events")
            | (&Method::GET, "/api/v1/me")
            | (&Method::GET, "/api/v1/memory/context")
            | (&Method::GET, "/api/v1/memories/available")
            | (&Method::POST, "/api/v1/memory/export")
            | (&Method::GET, "/api/v1/search")
            | (&Method::GET, "/api/v1/entities/similar")
            | (&Method::POST, "/api/v1/search/evidence/resolve")
            | (&Method::GET, "/api/v1/notes")
            | (&Method::POST, "/api/v1/notes")
            | (&Method::POST, "/api/v1/notes/source-upsert")
            | (&Method::POST, "/api/v1/lifecycle-purge/preview")
            | (&Method::POST, "/api/v1/lifecycle-purge")
            | (&Method::GET, "/api/v1/lifecycle-purge/{operation_id}")
            | (
                &Method::POST,
                "/api/v1/lifecycle-purge/{operation_id}/resume"
            )
            | (&Method::GET, "/api/v1/notes/{id}")
            | (&Method::GET, "/api/v1/notes/{id}/similar")
            | (&Method::DELETE, "/api/v1/notes/{id}")
            | (&Method::PATCH, "/api/v1/notes/{id}/status")
            | (&Method::GET, "/api/v1/notes/{id}/tags")
            | (&Method::PUT, "/api/v1/notes/{id}/tags")
            | (&Method::POST, "/api/v1/notes/{id}/links")
            | (&Method::GET, "/api/v1/tags")
            | (&Method::GET, "/api/v1/collections")
            | (&Method::POST, "/api/v1/collections")
            | (&Method::GET, "/api/v1/collections/{id}")
            | (&Method::PATCH, "/api/v1/collections/{id}")
            | (&Method::DELETE, "/api/v1/collections/{id}")
            | (&Method::GET, "/api/v1/collections/{id}/export")
            | (&Method::GET, "/api/v1/collections/{id}/notes")
            | (&Method::POST, "/api/v1/inference/complete")
            | (&Method::GET, "/api/v1/inference/catalog")
            | (&Method::POST, "/api/v1/inference/embed")
            | (&Method::GET, "/api/v1/inference/providers")
            | (&Method::POST, "/api/v1/inference/stream")
            | (&Method::GET, "/api/v1/auth/token-info")
            | (&Method::GET, "/api/v1/user/secrets")
            | (&Method::POST, "/api/v1/user/secrets")
            | (&Method::DELETE, "/api/v1/user/secrets/{id}")
    )
    // POST /api/v1/notes/{id}/move stays closed: its note ID has no collection
    // normalizer, so the role-based policy always denies it, and the handler does
    // not prove note or target-collection visibility (#1154 qualification gap).
}

pub fn is_operator_docs_route(path: &str) -> bool {
    route_policy_for_path(path).is_some_and(|policy| {
        policy.action_family == "docs_schema"
            && policy.docs == DocsExposureClass::Operator
            && policy.cache == CacheHeaderClass::NoStore
    })
}

pub fn is_public_without_bearer(path: &str) -> bool {
    route_policy_for_path(path).is_some_and(|policy| {
        matches!(
            policy.class,
            Public | OAuth | RealtimeTransport | SystemHealth
        ) || (policy.class == PublicWithInlineProof
            && policy.action_family == "realtime_provider_callback")
    })
}

pub fn authorization_input_for_request(
    method: &Method,
    path: &str,
    tenant_id: Option<&str>,
) -> Option<RoutePolicyInput> {
    let policy = route_policy_for_path(path)?;
    let policy = route_policy_for_matched_path(method, policy.path).unwrap_or(policy);
    let params = route_params(policy.path, path);
    let action_family = action_family_for_request(policy, method);
    let policy_class = policy_class_for_request(policy, method);
    let action = Action {
        name: route_action_name(action_family, method),
        scope_family: scope_family_for_policy_class(policy_class),
        required_scopes: required_scopes_for_policy_class(policy_class, method),
    };

    let mut resource = Resource::new(resource_kind_for_action_family(action_family));
    if let Some(candidate) = resource_id_for_policy(policy, &params) {
        let requires_backing_normalization =
            requires_backing_resource_normalization(policy, method, candidate.param_name);
        resource = resource.with_id(candidate.value);
        resource
            .attrs
            .insert("resource_id_source".to_string(), json!("route_param"));
        resource
            .attrs
            .insert("resource_id_param".to_string(), json!(candidate.param_name));
        resource
            .attrs
            .insert("resource_id_normalized".to_string(), json!(false));
        resource.attrs.insert(
            "requires_backing_resource_normalization".to_string(),
            json!(requires_backing_normalization),
        );
    } else {
        resource
            .attrs
            .insert("resource_id_source".to_string(), json!("route_template"));
        resource
            .attrs
            .insert("resource_id_normalized".to_string(), json!(true));
        resource.attrs.insert(
            "requires_backing_resource_normalization".to_string(),
            json!(false),
        );
    }
    for (param_name, param_value) in &params {
        resource
            .attrs
            .insert(format!("route_param_{param_name}"), json!(param_value));
    }
    if let Some(tenant_id) = tenant_id {
        resource = resource.with_tenant(tenant_id);
    }
    resource
        .attrs
        .insert("route_template".to_string(), json!(policy.path));
    resource.attrs.insert(
        "policy_class".to_string(),
        json!(format!("{:?}", policy_class)),
    );
    resource.attrs.insert(
        "inventory_policy_class".to_string(),
        json!(format!("{:?}", policy.class)),
    );
    resource
        .attrs
        .insert("action_family".to_string(), json!(action_family));
    resource.attrs.insert(
        "docs_exposure".to_string(),
        json!(format!("{:?}", policy.docs)),
    );
    resource.attrs.insert(
        "cache_header".to_string(),
        json!(format!("{:?}", policy.cache)),
    );

    let context = match tenant_id {
        Some(tenant_id) => AuthzContext::hosted(tenant_id),
        None => AuthzContext::personal(),
    };

    Some(RoutePolicyInput {
        policy,
        action,
        resource,
        context,
        audit_azp: None,
    })
}

pub fn mark_resource_id_normalized(input: &mut RoutePolicyInput) {
    input
        .resource
        .attrs
        .insert("resource_id_normalized".to_string(), json!(true));
}

/// Human readers may subscribe to domain events without MCP transport rights.
/// The caller must establish the credential class from verified identity metadata;
/// service credentials retain the inventory's MCP requirement.
pub fn apply_event_stream_admission(
    mut input: RoutePolicyInput,
    principal: &AuthPrincipal,
    human: bool,
) -> RoutePolicyInput {
    if input.policy.path == "/api/v1/events"
        && input.action.name == "event_stream:get"
        && human
        && matches!(principal, AuthPrincipal::OAuthClient { .. })
        && principal.has_scope("read")
        && !principal.has_scope("mcp")
    {
        input.action.scope_family = ScopeFamily::Rest;
        input.action.required_scopes = vec!["read".to_string()];
    }
    input
}

fn action_family_for_request(policy: &RoutePolicy, method: &Method) -> &'static str {
    if is_incoming_webhook_receiver_slug_route(policy) && method != Method::POST {
        "webhook_control"
    } else {
        policy.action_family
    }
}

pub(crate) fn policy_class_for_request(policy: &RoutePolicy, method: &Method) -> PolicyClass {
    if is_incoming_webhook_receiver_slug_route(policy) && method != Method::POST {
        AdminOperator
    } else {
        policy.class
    }
}

/// Hosted scopes are mapped explicitly, with no implicit owner -> admin grant.
/// Keep their existing scope contract and migration gates. External deployments
/// have one administrator; inventory statistics and archive mutations need admin.
pub(crate) fn apply_external_oidc_archive_hardening(
    mut input: RoutePolicyInput,
    external: bool,
) -> RoutePolicyInput {
    let archive_mutation = input.policy.action_family == "memory_management"
        && input.policy.path.contains("{name}")
        && !input.action.name.ends_with(":get")
        && !input.action.name.ends_with(":head")
        && !input.action.name.ends_with(":options");
    if external && (archive_mutation || input.policy.path == "/api/v1/memories/overview") {
        input.action.scope_family = ScopeFamily::Admin;
        input.action.required_scopes = vec!["admin".into()];
        input
            .resource
            .attrs
            .insert("policy_class".into(), json!("AdminOperator"));
    }
    input
}

fn is_incoming_webhook_receiver_slug_route(policy: &RoutePolicy) -> bool {
    policy.path == "/api/v1/webhooks/incoming/{slug}" && policy.action_family == "webhook_receiver"
}

fn route_action_name(action_family: &str, method: &Method) -> String {
    format!("{}:{}", action_family, method.as_str().to_ascii_lowercase())
}

fn scope_family_for_policy_class(policy_class: PolicyClass) -> ScopeFamily {
    match policy_class {
        AdminOperator => ScopeFamily::Admin,
        RealtimeTransport => ScopeFamily::McpTransport,
        _ => ScopeFamily::Rest,
    }
}

fn required_scopes_for_policy_class(policy_class: PolicyClass, method: &Method) -> Vec<String> {
    match policy_class {
        Public | PublicWithInlineProof | OAuth => vec![],
        RealtimeTransport => vec!["mcp".to_string()],
        AdminOperator => vec!["admin".to_string()],
        SystemHealth | AuthenticatedRead => vec!["read".to_string()],
        AuthenticatedWrite => vec!["write".to_string()],
        TenantObject => {
            if is_read_method(method) {
                vec!["read".to_string()]
            } else {
                vec!["write".to_string()]
            }
        }
    }
}

fn is_read_method(method: &Method) -> bool {
    matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

fn resource_kind_for_action_family(action_family: &str) -> ResourceKind {
    match action_family {
        "attachment" => ResourceKind::Attachment,
        "backup_restore" => ResourceKind::Backup,
        "collection" => ResourceKind::Collection,
        "credential_management" => ResourceKind::ApiKey,
        "document_type_catalog" => ResourceKind::DocumentType,
        "health_diagnostics" | "system_diagnostics" | "token_verification" => ResourceKind::System,
        "inbound_connector" | "webhook_control" | "webhook_receiver" => ResourceKind::Webhook,
        "job_control" => ResourceKind::Job,
        "memory_management" => ResourceKind::Archive,
        "model_config" | "model_catalog" => ResourceKind::ModelConfig,
        "note" | "search" => ResourceKind::Note,
        "provenance" => ResourceKind::Provenance,
        "taxonomy" => ResourceKind::Taxonomy,
        "template" => ResourceKind::Template,
        "ai_execution" => ResourceKind::Inference,
        "event_stream" | "ingest_stream" | "realtime_call" | "realtime_provider_callback" => {
            ResourceKind::McpTool
        }
        "oauth_discovery"
        | "oauth_flow"
        | "oauth_owner_consent"
        | "docs_schema"
        | "health_probe"
        | "test_fixture" => ResourceKind::PublicRoute,
        other => ResourceKind::Other(other.to_string()),
    }
}

fn resource_id_for_policy(
    policy: &RoutePolicy,
    params: &HashMap<&'static str, &str>,
) -> Option<ResourceIdCandidate> {
    // Swagger's catch-all is an embedded asset name, not a database object.
    // Operator scope is still required for every exact documentation template.
    if policy.action_family == "docs_schema" {
        return None;
    }
    let preferred_param = match policy.action_family {
        "attachment" if policy.path.starts_with("/api/v1/notes/") => "id",
        "attachment" => "attachment_id",
        "collection"
        | "credential_management"
        | "note"
        | "provenance"
        | "realtime_call"
        | "taxonomy"
        | "template"
        | "webhook_control" => "id",
        "document_type_catalog" | "inbound_connector" | "memory_management" => "name",
        "embedding_control" => "slug",
        "pke_keyset" => "name_or_id",
        "realtime_provider_callback" => "provider_call_id",
        "webhook_receiver" => "slug",
        _ => "",
    };

    params
        .get(preferred_param)
        .map(|value| ResourceIdCandidate {
            param_name: preferred_param,
            value: (*value).to_string(),
        })
        .or_else(|| {
            params
                .iter()
                .next()
                .map(|(param_name, value)| ResourceIdCandidate {
                    param_name,
                    value: (*value).to_string(),
                })
        })
}

fn requires_backing_resource_normalization(
    policy: &RoutePolicy,
    method: &Method,
    param_name: &str,
) -> bool {
    if closed_route_normalization_exception(policy, method).is_some() {
        // The matching role-based guard is intentionally fail-closed.  This
        // false value keeps the generic normalizer inventory honest without
        // pretending a nonexistent lookup has qualified the resource.
        return false;
    }
    if policy
        .path
        .starts_with("/api/v1/lifecycle-purge/{operation_id}")
    {
        // The opaque operation ID is checked by tenant RLS in the same scoped
        // transaction as status/resume. It is not a note ID.
        return false;
    }
    if policy.action_family == "user_secret" {
        // DELETE is intentionally idempotent and non-enumerating. Tenant RLS and
        // the user predicate are the ownership boundary for this opaque UUID.
        return false;
    }
    if method == Method::POST && policy.path == "/api/v1/notes/{id}/links" {
        // Both endpoints of a manual link are checked as active notes inside the
        // same tenant/archive-scoped transaction as the insert. A pre-policy
        // backing lookup would turn only the path/source into a 403 oracle while
        // the body/target correctly receives the stable non-enumerating 404.
        return false;
    }
    matches!(
        policy.class,
        TenantObject | AdminOperator | AuthenticatedWrite
    ) || matches!(
        policy.action_family,
        "attachment"
            | "backup_restore"
            | "collection"
            | "credential_management"
            | "document_type_catalog"
            | "memory_management"
            | "note"
            | "provenance"
            | "taxonomy"
            | "template"
            | "webhook_control"
    ) || matches!(param_name, "id" | "attachment_id" | "name" | "name_or_id")
}

/// The production dispatcher for a backing-resource authorization lookup.
///
/// This is deliberately derived from the executable route-policy input rather
/// than from a second route list.  The authorization middleware consumes this
/// selector, and the inventory regression below consumes the same selector, so
/// a newly parameterized tenant object cannot silently become an unnormalised
/// role-based resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackingResourceNormalizer {
    Note,
    Credential,
    Attachment,
    Archive,
    Collection,
    Template,
    TaxonomyConcept,
    TaxonomyRelation,
    TaxonomyScheme,
    TaxonomyCollection,
    DocumentType,
    Job,
    Webhook,
    InboundSource,
    EmbeddingSet,
    PkeKeyset,
}

pub(crate) fn backing_resource_normalizer(
    input: &RoutePolicyInput,
) -> Option<BackingResourceNormalizer> {
    let required = input
        .resource
        .attrs
        .get("requires_backing_resource_normalization")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let normalized = input
        .resource
        .attrs
        .get("resource_id_normalized")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !required || normalized {
        return None;
    }

    let parameter = input
        .resource
        .attrs
        .get("resource_id_param")
        .and_then(|value| value.as_str());
    let template = input
        .resource
        .attrs
        .get("route_template")
        .and_then(|value| value.as_str())
        .unwrap_or(input.policy.path);

    match (input.policy.action_family, &input.resource.kind, parameter) {
        ("embedding_control", _, Some("slug")) => Some(BackingResourceNormalizer::EmbeddingSet),
        ("credential_management", ResourceKind::ApiKey, Some("id")) => {
            Some(BackingResourceNormalizer::Credential)
        }
        ("attachment", ResourceKind::Attachment, Some("id"))
            if template.starts_with("/api/v1/notes/") =>
        {
            Some(BackingResourceNormalizer::Note)
        }
        (_, ResourceKind::Note, Some("id")) => Some(BackingResourceNormalizer::Note),
        (_, ResourceKind::Attachment, Some("attachment_id")) => {
            Some(BackingResourceNormalizer::Attachment)
        }
        (_, ResourceKind::Archive, Some("name")) => Some(BackingResourceNormalizer::Archive),
        (_, ResourceKind::Collection, Some("id"))
            if template.starts_with("/api/v1/collections/{id}") =>
        {
            Some(BackingResourceNormalizer::Collection)
        }
        (_, ResourceKind::Template, Some("id")) => Some(BackingResourceNormalizer::Template),
        ("taxonomy", ResourceKind::Taxonomy, Some("id"))
            if template.starts_with("/api/v1/concepts/{id}/")
                && template.contains("{target_id}") =>
        {
            Some(BackingResourceNormalizer::TaxonomyRelation)
        }
        ("taxonomy", ResourceKind::Taxonomy, Some("id"))
            if template.starts_with("/api/v1/concepts/{id}") =>
        {
            Some(BackingResourceNormalizer::TaxonomyConcept)
        }
        ("taxonomy", ResourceKind::Taxonomy, Some("id"))
            if template.starts_with("/api/v1/concepts/schemes/{id}") =>
        {
            Some(BackingResourceNormalizer::TaxonomyScheme)
        }
        ("taxonomy", ResourceKind::Taxonomy, Some("id"))
            if template.starts_with("/api/v1/concepts/collections/{id}") =>
        {
            Some(BackingResourceNormalizer::TaxonomyCollection)
        }
        (_, ResourceKind::DocumentType, Some("name")) => {
            Some(BackingResourceNormalizer::DocumentType)
        }
        (_, ResourceKind::Job, Some("id")) => Some(BackingResourceNormalizer::Job),
        ("webhook_control", ResourceKind::Webhook, Some("id")) => {
            Some(BackingResourceNormalizer::Webhook)
        }
        ("inbound_connector", ResourceKind::Webhook, Some("name")) => {
            Some(BackingResourceNormalizer::InboundSource)
        }
        ("pke_keyset", _, Some("name_or_id")) => Some(BackingResourceNormalizer::PkeKeyset),
        _ => None,
    }
}

/// OpenAPI path parameters omit Axum's catch-all marker.
pub fn openapi_path(router_path: &str) -> String {
    router_path.replace("{*", "{")
}

fn route_template_matches(template: &str, path: &str) -> bool {
    let mut template_segments = template.split('/');
    let mut path_segments = path.split('/');

    loop {
        match (template_segments.next(), path_segments.next()) {
            (None, None) => return true,
            (Some(t), Some(_)) if t.starts_with("{*") && t.ends_with('}') => {
                // Keep even the empty well-known subtree public; Axum's
                // fallback then answers 404 instead of authentication's 401.
                return template_segments.next().is_none();
            }
            (Some(t), Some(p)) if is_template_param(t) && !p.is_empty() => {}
            (Some(t), Some(p)) if t == p => {}
            _ => return false,
        }
    }
}

fn route_params<'a>(template: &'static str, path: &'a str) -> HashMap<&'static str, &'a str> {
    let mut params = HashMap::new();

    for (template_segment, path_segment) in template.split('/').zip(path.split('/')) {
        if is_template_param(template_segment) {
            params.insert(
                &template_segment[1..template_segment.len() - 1],
                path_segment,
            );
        }
    }

    params
}

fn is_template_param(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}')
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_registered_route_requires_an_archive_isolation_class() {
        for path in all_registered_routes() {
            let route = ROUTE_POLICY_INVENTORY
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("Route lacks archive isolation: {path}"));
            // The constructor and field are required: omitting a class is also
            // a compile error, including for a newly inventoried route.
            assert!(matches!(
                route.archive_isolation,
                ArchiveScoped | CatalogFiltered | CompleteCatalog | NoArchiveData
            ));
        }
    }

    #[test]
    fn registered_routes_have_policy_inventory_rows() {
        let registered = all_registered_routes();
        let inventoried: BTreeSet<_> = ROUTE_POLICY_INVENTORY
            .iter()
            .map(|route| route.path)
            .collect();

        let missing: Vec<_> = registered.difference(&inventoried).copied().collect();
        assert!(
            missing.is_empty(),
            "routes missing policy inventory rows: {missing:?}"
        );
    }

    #[test]
    fn policy_inventory_paths_are_registered() {
        let registered = all_registered_routes();
        let unregistered: Vec<_> = ROUTE_POLICY_INVENTORY
            .iter()
            .map(|route| route.path)
            .filter(|path| !registered.contains(path))
            .collect();

        assert!(
            unregistered.is_empty(),
            "policy inventory rows without registered routes: {unregistered:?}"
        );
    }

    #[test]
    fn registered_route_operations_build_complete_policy_inputs() {
        let operations = all_registered_operations();
        assert!(
            operations.len() > all_registered_routes().len(),
            "method-level inventory must expand multi-method routes"
        );

        for (path, method_name) in operations {
            let method = Method::from_bytes(method_name.as_bytes()).expect("known HTTP method");
            let request_path = materialize_route_template(path);
            let input = authorization_input_for_request(&method, &request_path, Some("tenant-a"))
                .unwrap_or_else(|| panic!("{method_name} {path} has no policy input"));
            let action_family = input.resource.attrs["action_family"]
                .as_str()
                .expect("action family must be structured policy metadata");

            assert_eq!(input.policy.path, path);
            assert_eq!(
                input.action.name,
                format!("{}:{}", action_family, method_name.to_ascii_lowercase()),
                "{method_name} {path} action drifted from the route inventory"
            );
            assert_eq!(input.resource.attrs["route_template"], json!(path));
            assert_eq!(
                input.resource.attrs["inventory_policy_class"],
                json!(format!("{:?}", input.policy.class))
            );
            assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
            assert_eq!(input.context.tenant_id.as_deref(), Some("tenant-a"));

            let effective_class = input.resource.attrs["policy_class"]
                .as_str()
                .expect("effective policy class must be recorded");
            if matches!(
                effective_class,
                "Public" | "PublicWithInlineProof" | "OAuth"
            ) {
                assert!(
                    input.action.required_scopes.is_empty(),
                    "{method_name} {path} public operation unexpectedly requires bearer scopes"
                );
            } else {
                assert!(
                    !input.action.required_scopes.is_empty(),
                    "{method_name} {path} protected operation has no required scope"
                );
            }
        }
    }

    #[test]
    fn public_callback_routes_have_explicit_public_bypass_rules() {
        assert!(!is_public_without_bearer(
            "/api/v1/webhooks/incoming/example"
        ));
        for path in [
            "/.well-known/oauth-protected-resource/",
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-protected-resource/team/mcp",
            "/.well-known/oauth-protected-resource/unknown/",
        ] {
            assert!(is_public_without_bearer(path));
        }
        assert!(!is_public_without_bearer(
            "/.well-known/oauth-protected-resource-typo/mcp"
        ));
        assert!(is_public_without_bearer("/livez"));
        assert!(is_public_without_bearer("/readyz"));
        assert!(is_public_without_bearer("/api/v1/realtime/twilio/CA123"));
        assert!(is_public_without_bearer("/api/v1/health/streaming"));
        assert!(is_public_without_bearer("/api/v1/system/compatibility"));
    }

    #[test]
    fn generated_docs_are_operator_only_and_no_store() {
        for path in [
            OPERATOR_DOCS_PATH,
            "/api/v1/operator/docs/swagger-ui.css",
            OPERATOR_OPENAPI_PATH,
            OPERATOR_ASYNCAPI_PATH,
        ] {
            assert!(!is_public_without_bearer(path));
            assert!(is_operator_docs_route(path));
            let input = authorization_input_for_request(&Method::GET, path, None)
                .expect("operator docs route must have policy input");
            assert_eq!(input.policy.class, AdminOperator);
            assert_eq!(input.policy.docs, Operator);
            assert_eq!(input.policy.cache, NoStore);
            assert_eq!(input.action.scope_family, ScopeFamily::Admin);
            assert_eq!(input.action.required_scopes, vec!["admin"]);
            assert!(
                input.resource.id.is_none(),
                "embedded docs have no database resource ID"
            );
            assert_eq!(input.resource.attrs["resource_id_normalized"], true);
        }

        for legacy_path in ["/docs", "/openapi.yaml", "/asyncapi.yaml"] {
            assert!(!is_public_without_bearer(legacy_path));
            assert!(route_policy_for_path(legacy_path).is_none());
        }
    }

    #[test]
    fn generated_api_inventory_has_no_public_mount_or_execution_mode() {
        let docs_policies: Vec<_> = ROUTE_POLICY_INVENTORY
            .iter()
            .filter(|policy| policy.action_family == "docs_schema")
            .collect();
        assert_eq!(
            docs_policies.len(),
            5,
            "OpenAPI, AsyncAPI, and the three Swagger templates must be the only generated-doc routes"
        );
        assert!(docs_policies.iter().all(|policy| {
            policy.class == AdminOperator && policy.docs == Operator && policy.cache == NoStore
        }));

        let source = include_str!("main.rs");
        for forbidden in [
            format!("SwaggerUi::new({:?})", "/docs"),
            format!(".route({:?}", "/openapi.yaml"),
            format!(".route({:?}", "/asyncapi.yaml"),
            format!(".try_it_out_enabled({})", "true"),
        ] {
            assert!(
                !source.contains(&forbidden),
                "generated API inventory must not restore forbidden mount: {forbidden}"
            );
        }
        assert!(source.contains(&format!("SwaggerUi::new({:?})", OPERATOR_DOCS_PATH)));
        assert!(source.contains(&format!(".try_it_out_enabled({})", "false")));
    }

    #[test]
    fn api_key_route_builds_admin_policy_input() {
        let input = authorization_input_for_request(&Method::POST, "/api/v1/api-keys", None)
            .expect("api-key route should be inventoried");

        assert_eq!(input.policy.action_family, "credential_management");
        assert_eq!(input.action.scope_family, ScopeFamily::Admin);
        assert_eq!(input.action.required_scopes, vec!["admin"]);
        assert_eq!(input.resource.kind, ResourceKind::ApiKey);
        assert_eq!(input.context, AuthzContext::personal());
        assert_eq!(
            input.resource.attrs["resource_id_source"],
            json!("route_template")
        );
        assert_eq!(input.resource.attrs["resource_id_normalized"], json!(true));
        assert_eq!(
            input.resource.attrs["requires_backing_resource_normalization"],
            json!(false)
        );
    }

    #[test]
    fn hosted_user_secret_routes_are_hidden_tenant_objects_without_enumeration_lookup() {
        let list = authorization_input_for_request(
            &Method::GET,
            "/api/v1/user/secrets",
            Some("018fd1a0-0000-7000-8000-000000000201"),
        )
        .unwrap();
        assert_eq!(list.policy.class, TenantObject);
        assert_eq!(list.policy.docs, Hidden);
        assert_eq!(list.policy.cache, NoStore);
        assert_eq!(list.action.required_scopes, vec!["read"]);
        assert_eq!(
            list.resource.kind,
            ResourceKind::Other("user_secret".into())
        );

        let revoke = authorization_input_for_request(
            &Method::DELETE,
            "/api/v1/user/secrets/018fd1a0-0000-7000-8000-000000000202",
            Some("018fd1a0-0000-7000-8000-000000000201"),
        )
        .unwrap();
        assert_eq!(revoke.action.required_scopes, vec!["write"]);
        assert_eq!(
            revoke.resource.id.as_deref(),
            Some("018fd1a0-0000-7000-8000-000000000202")
        );
        assert_eq!(
            revoke.resource.attrs["requires_backing_resource_normalization"],
            false
        );
    }

    #[test]
    fn route_policy_debug_redacts_route_and_authorization_metadata() {
        static POLICY: RoutePolicy = RoutePolicy {
            path: "/api/v1/archives/customer-private@example.internal/{secret}",
            class: TenantObject,
            action_family: "archive-secret-action-family",
            docs: Authenticated,
            cache: PrivateUserData,
            archive_isolation: CompleteCatalog,
            methods: &["GET", "HEAD"],
        };

        let mut resource = Resource::new(ResourceKind::Other(
            "customer-private-resource-kind".to_string(),
        ))
        .with_id("archive-id-secret-123")
        .with_tenant("tenant-secret-456");
        resource.attrs.insert(
            "route_param_secret_key".to_string(),
            json!("route-param-secret-value"),
        );
        resource
            .attrs
            .insert("route_template".to_string(), json!(POLICY.path));

        let mut context = AuthzContext::hosted("tenant-secret-456");
        context.environment.insert(
            "private-context-key".to_string(),
            json!("private-context-value"),
        );
        context.correlation_id = Some("correlation-secret-789".to_string());

        let input = RoutePolicyInput {
            policy: &POLICY,
            action: Action::rest("archive-secret-action", "scope:archive-secret"),
            resource,
            context,
            audit_azp: Some("private-audit-client".to_string()),
        };

        let output = format!("{:?}\n{:?}", POLICY, input);

        for expected in [
            "RoutePolicy",
            "path_len",
            "action_family_len",
            "RoutePolicyInput",
            "Action",
            "name_len",
            "required_scope_count",
            "Resource",
            "id_present: true",
            "tenant_id_present: true",
            "AuthzContext",
            "environment_count",
            "correlation_id_present: true",
        ] {
            assert!(
                output.contains(expected),
                "debug output should contain metadata marker {expected}: {output}"
            );
        }

        for raw in [
            "private-audit-client",
            POLICY.path,
            POLICY.action_family,
            "customer-private-resource-kind",
            "archive-id-secret-123",
            "tenant-secret-456",
            "route_param_secret_key",
            "route-param-secret-value",
            "private-context-key",
            "private-context-value",
            "correlation-secret-789",
            "archive-secret-action",
            "scope:archive-secret",
        ] {
            assert!(
                !output.contains(raw),
                "debug output leaked raw policy/authz value {raw}: {output}"
            );
        }
    }

    #[test]
    fn tenant_note_mutation_builds_rest_write_policy_input() {
        let input = authorization_input_for_request(
            &Method::PATCH,
            "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001",
            Some("tenant-a"),
        )
        .expect("note route should be inventoried");

        assert_eq!(input.action.name, "note:patch");
        assert_eq!(input.action.scope_family, ScopeFamily::Rest);
        assert_eq!(input.action.required_scopes, vec!["write"]);
        assert_eq!(input.resource.kind, ResourceKind::Note);
        assert_eq!(
            input.resource.id.as_deref(),
            Some("018fd1a0-0000-7000-8000-000000000001")
        );
        assert_eq!(
            input.resource.attrs["resource_id_source"],
            json!("route_param")
        );
        assert_eq!(input.resource.attrs["resource_id_param"], json!("id"));
        assert_eq!(input.resource.attrs["resource_id_normalized"], json!(false));
        assert_eq!(
            input.resource.attrs["requires_backing_resource_normalization"],
            json!(true)
        );
        assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
        assert_eq!(input.context.tenant_id.as_deref(), Some("tenant-a"));
    }

    #[test]
    fn mark_resource_id_normalized_sets_backing_lookup_result() {
        let mut input = authorization_input_for_request(
            &Method::PATCH,
            "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001",
            Some("tenant-a"),
        )
        .expect("note route should be inventoried");

        mark_resource_id_normalized(&mut input);

        assert_eq!(input.resource.attrs["resource_id_normalized"], json!(true));
        assert_eq!(
            input.resource.attrs["requires_backing_resource_normalization"],
            json!(true)
        );
    }

    #[test]
    fn attachment_route_marks_route_param_id_as_unnormalized_candidate() {
        let input = authorization_input_for_request(
            &Method::GET,
            "/api/v1/attachments/018fd1a0-0000-7000-8000-000000000002/download",
            Some("tenant-a"),
        )
        .expect("attachment route should be inventoried");

        assert_eq!(input.policy.action_family, "attachment");
        assert_eq!(input.action.required_scopes, vec!["read"]);
        assert_eq!(input.resource.kind, ResourceKind::Attachment);
        assert_eq!(
            input.resource.id.as_deref(),
            Some("018fd1a0-0000-7000-8000-000000000002")
        );
        assert_eq!(
            input.resource.attrs["resource_id_source"],
            json!("route_param")
        );
        assert_eq!(
            input.resource.attrs["resource_id_param"],
            json!("attachment_id")
        );
        assert_eq!(input.resource.attrs["resource_id_normalized"], json!(false));
        assert_eq!(
            input.resource.attrs["requires_backing_resource_normalization"],
            json!(true)
        );
    }

    #[test]
    fn public_webhook_receiver_builds_inline_proof_policy_input() {
        let input = authorization_input_for_request(
            &Method::POST,
            "/api/v1/webhooks/incoming/customer-created",
            None,
        )
        .expect("incoming webhook route should be inventoried");

        assert_eq!(input.policy.class, PublicWithInlineProof);
        assert_eq!(input.action.required_scopes, Vec::<String>::new());
        assert_eq!(input.resource.kind, ResourceKind::Webhook);
        assert_eq!(input.resource.id.as_deref(), Some("customer-created"));
        assert_eq!(
            input.resource.attrs["policy_class"],
            json!("PublicWithInlineProof")
        );
        assert_eq!(
            input.resource.attrs["action_family"],
            json!("webhook_receiver")
        );
    }

    #[test]
    fn incoming_webhook_receiver_management_methods_build_admin_policy_input() {
        for method in [Method::GET, Method::PATCH, Method::DELETE] {
            let input = authorization_input_for_request(
                &method,
                "/api/v1/webhooks/incoming/customer-created",
                None,
            )
            .expect("incoming webhook management route should be inventoried");

            assert_eq!(input.policy.class, PublicWithInlineProof);
            assert_eq!(input.policy.action_family, "webhook_receiver");
            assert_eq!(
                input.action.name,
                format!("webhook_control:{}", method.as_str().to_ascii_lowercase())
            );
            assert_eq!(input.action.scope_family, ScopeFamily::Admin);
            assert_eq!(input.action.required_scopes, vec!["admin"]);
            assert_eq!(input.resource.kind, ResourceKind::Webhook);
            assert_eq!(input.resource.id.as_deref(), Some("customer-created"));
            assert_eq!(input.resource.attrs["policy_class"], json!("AdminOperator"));
            assert_eq!(
                input.resource.attrs["inventory_policy_class"],
                json!("PublicWithInlineProof")
            );
            assert_eq!(
                input.resource.attrs["action_family"],
                json!("webhook_control")
            );
        }
    }

    #[test]
    fn realtime_transport_defaults_to_mcp_transport_scope() {
        let input = authorization_input_for_request(&Method::GET, "/api/v1/events", None)
            .expect("event stream route should be inventoried");

        assert_eq!(input.policy.class, RealtimeTransport);
        assert_eq!(input.action.scope_family, ScopeFamily::McpTransport);
        assert_eq!(input.action.required_scopes, vec!["mcp"]);
        assert_eq!(input.resource.kind, ResourceKind::McpTool);
    }

    #[test]
    fn event_stream_accepts_human_read_without_changing_other_transports() {
        let reader = AuthPrincipal::OAuthClient {
            client_id: "synthetic-web".into(),
            user_id: Some("synthetic-reader".into()),
            scope: "read".into(),
        };
        let input =
            authorization_input_for_request(&Method::GET, "/api/v1/events", Some("tenant-a"))
                .unwrap();
        let human = apply_event_stream_admission(input.clone(), &reader, true);
        assert_eq!(human.action.scope_family, ScopeFamily::Rest);
        assert_eq!(human.action.required_scopes, vec!["read"]);
        assert_eq!(human.resource.tenant_id.as_deref(), Some("tenant-a"));
        let service = apply_event_stream_admission(input, &reader, false);
        assert_eq!(service.action.required_scopes, vec!["mcp"]);
        let ws = authorization_input_for_request(&Method::GET, "/api/v1/ws", None).unwrap();
        assert_eq!(
            apply_event_stream_admission(ws, &reader, true)
                .action
                .required_scopes,
            vec!["mcp"]
        );
    }

    #[test]
    fn evidence_resolution_is_read_authorized_and_not_cacheable() {
        let input = authorization_input_for_request(
            &Method::POST,
            "/api/v1/search/evidence/resolve",
            Some("tenant-a"),
        )
        .unwrap();
        assert_eq!(input.action.required_scopes, vec!["read"]);
        assert_eq!(input.resource.kind, ResourceKind::Note);
        assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
        assert_eq!(input.policy.cache, CacheHeaderClass::NoStore);
        assert!(!hosted_tenant_transaction_ready(
            &Method::GET,
            "/api/v1/search/evidence/resolve"
        ));
    }

    #[test]
    fn memory_context_preserves_read_and_admin_boundaries() {
        let path = "/api/v1/memory/context";
        let input = authorization_input_for_request(&Method::GET, path, Some("tenant-a")).unwrap();
        assert_eq!(input.action.required_scopes, vec!["read"]);
        assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
        assert_eq!(input.policy.cache, CacheHeaderClass::NoStore);
        assert!(!is_public_without_bearer(path));
        assert!(hosted_tenant_transaction_ready(&Method::GET, path));
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!hosted_tenant_transaction_ready(&method, path));
        }
        for path in ["/api/v1/archives", "/api/v1/memories"] {
            let input =
                authorization_input_for_request(&Method::GET, path, Some("tenant-a")).unwrap();
            assert_eq!(input.action.required_scopes, vec!["admin"]);
            assert!(!hosted_tenant_transaction_ready(&Method::GET, path));
        }
        assert!(!hosted_tenant_transaction_ready(
            &Method::GET,
            "/api/v1/archives/research"
        ));
    }

    #[test]
    fn memory_export_is_read_scoped_no_store_and_tenant_transaction_ready() {
        let path = "/api/v1/memory/export";
        let input = authorization_input_for_request(&Method::POST, path, Some("tenant-a")).unwrap();
        assert_eq!(input.policy.class, AuthenticatedRead);
        assert_eq!(input.action.required_scopes, vec!["read"]);
        assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
        assert_eq!(input.policy.cache, CacheHeaderClass::NoStore);
        assert!(!is_public_without_bearer(path));
        assert!(hosted_tenant_transaction_ready(&Method::POST, path));
        for method in [Method::GET, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!hosted_tenant_transaction_ready(&method, path));
        }
    }

    #[test]
    fn hosted_tenant_transaction_gate_admits_only_migrated_methods() {
        for (method, path) in [
            (Method::GET, "/api/v1/me"),
            (Method::GET, "/api/v1/notes"),
            (
                Method::DELETE,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001",
            ),
            (
                Method::PUT,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/tags",
            ),
            (
                Method::POST,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/links",
            ),
            (Method::GET, "/api/v1/events"),
            (Method::GET, "/api/v1/search"),
            (Method::POST, "/api/v1/search/evidence/resolve"),
            (Method::POST, "/api/v1/notes"),
            (Method::POST, "/api/v1/lifecycle-purge/preview"),
            (Method::POST, "/api/v1/lifecycle-purge"),
            (
                Method::GET,
                "/api/v1/lifecycle-purge/018fd1a0-0000-7000-8000-000000000004",
            ),
            (
                Method::POST,
                "/api/v1/lifecycle-purge/018fd1a0-0000-7000-8000-000000000004/resume",
            ),
            (Method::POST, "/api/v1/collections"),
            (
                Method::GET,
                "/api/v1/collections/018fd1a0-0000-7000-8000-000000000002/export",
            ),
            (Method::POST, "/api/v1/inference/complete"),
            (Method::POST, "/api/v1/inference/stream"),
            (Method::POST, "/api/v1/user/secrets"),
            (
                Method::DELETE,
                "/api/v1/user/secrets/018fd1a0-0000-7000-8000-000000000003",
            ),
        ] {
            assert!(
                hosted_tenant_transaction_ready(&method, path),
                "migrated hosted route was not admitted: {method} {path}"
            );
        }

        for (method, path) in [
            (Method::POST, "/api/v1/notes/bulk"),
            (Method::POST, "/api/v1/search"),
            (
                Method::PATCH,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001",
            ),
            (
                Method::POST,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/restore",
            ),
            (
                Method::POST,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/purge",
            ),
            (Method::POST, "/api/v1/notes/reprocess"),
            (
                Method::POST,
                "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001/move",
            ),
            (Method::GET, "/api/v1/concepts"),
            (Method::GET, "/api/v1/attachments"),
            (Method::GET, "/api/v1/jobs"),
            (Method::GET, "/api/v1/not-in-the-inventory"),
        ] {
            assert!(
                !hosted_tenant_transaction_ready(&method, path),
                "unmigrated hosted route was admitted: {method} {path}"
            );
        }
    }

    #[test]
    fn lifecycle_purge_operation_is_tenant_scoped_without_note_normalization() {
        let operation_id = "018fd1a0-0000-7000-8000-000000000004";
        for (method, suffix, scopes) in [
            (Method::GET, "", vec!["read"]),
            (Method::POST, "/resume", vec!["write"]),
        ] {
            let input = authorization_input_for_request(
                &method,
                &format!("/api/v1/lifecycle-purge/{operation_id}{suffix}"),
                Some("tenant-a"),
            )
            .expect("lifecycle purge route policy");
            assert_eq!(input.policy.class, TenantObject);
            assert_eq!(input.action.required_scopes, scopes);
            assert_eq!(input.resource.kind, ResourceKind::Note);
            assert_eq!(input.resource.id.as_deref(), Some(operation_id));
            assert_eq!(input.resource.tenant_id.as_deref(), Some("tenant-a"));
            assert_eq!(
                input.resource.attrs["requires_backing_resource_normalization"],
                false
            );
            assert!(hosted_tenant_transaction_ready(
                &method,
                &format!("/api/v1/lifecycle-purge/{operation_id}{suffix}")
            ));
        }
    }

    #[test]
    fn manual_note_link_authorization_binds_write_scope_source_and_tenant() {
        let tenant_id = "018fd1a0-0000-7000-8000-000000000111";
        let source_id = "018fd1a0-0000-7000-8000-000000000222";
        let input = authorization_input_for_request(
            &Method::POST,
            &format!("/api/v1/notes/{source_id}/links"),
            Some(tenant_id),
        )
        .expect("manual link route policy");

        assert_eq!(input.policy.class, TenantObject);
        assert_eq!(input.action.required_scopes, vec!["write"]);
        assert_eq!(input.resource.kind, ResourceKind::Note);
        assert_eq!(input.resource.id.as_deref(), Some(source_id));
        assert_eq!(input.resource.tenant_id.as_deref(), Some(tenant_id));
        assert_eq!(input.resource.attrs["resource_id_param"], "id");
        assert_eq!(input.resource.attrs["resource_id_normalized"], false);
        assert_eq!(
            input.resource.attrs["requires_backing_resource_normalization"],
            false
        );
        assert_eq!(
            input.resource.attrs["route_template"],
            "/api/v1/notes/{id}/links"
        );
    }

    const HOSTED_ROUTE_QUALIFICATION_DOC: &str =
        include_str!("../../../docs/deployment/hosted-route-qualification.md");

    /// The committed matrix is generated from the registered router operations
    /// and the hosted tenant-transaction gate. Regenerate with
    /// `FORTEMI_UPDATE_ROUTE_QUALIFICATION=1 cargo test -p matric-api --bin matric-api hosted_route_qualification`.
    #[test]
    fn hosted_route_qualification_matrix_doc_is_current() {
        use crate::hosted_route_qualification::matrix;

        let operations: Vec<_> = all_registered_operations().into_iter().collect();
        let expected = matrix::render(&operations);
        let committed = matrix::committed_region(HOSTED_ROUTE_QUALIFICATION_DOC)
            .expect("hosted route qualification doc must contain the generated region");
        if committed != expected
            && std::env::var("FORTEMI_UPDATE_ROUTE_QUALIFICATION").as_deref() == Ok("1")
        {
            let path = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/deployment/hosted-route-qualification.md"
            );
            let updated = HOSTED_ROUTE_QUALIFICATION_DOC.replacen(committed, &expected, 1);
            std::fs::write(path, updated).expect("rewrite hosted route qualification doc");
            return;
        }
        let first = committed
            .lines()
            .zip(expected.lines())
            .position(|(a, b)| a != b);
        assert!(
            committed == expected,
            "hosted route qualification matrix differs at region line {:?}; rerun with FORTEMI_UPDATE_ROUTE_QUALIFICATION=1",
            first.map(|line| line + 1)
        );
    }

    #[test]
    fn hosted_route_qualification_covers_every_inventory_row_once() {
        use crate::hosted_route_qualification::route_class;

        for policy in ROUTE_POLICY_INVENTORY {
            assert_eq!(
                route_policy_for_path(policy.path).map(|row| row.path),
                Some(policy.path),
                "template {} resolves to a different inventory row",
                policy.path
            );
            // Classification is total; this exercises every row.
            let _ = route_class(policy);
        }
    }

    #[test]
    fn hosted_ready_operations_are_registered_and_classified() {
        use crate::hosted_route_qualification::{qualification, Qualification};

        let operations = all_registered_operations();
        for policy in ROUTE_POLICY_INVENTORY {
            for method in [
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ] {
                if hosted_tenant_transaction_ready(&method, policy.path) {
                    assert!(
                        operations.contains(&(policy.path, method.as_str())),
                        "{method} {} is hosted-ready but not registered",
                        policy.path
                    );
                    assert_eq!(qualification(&method, policy), Qualification::Qualified);
                }
            }
        }
    }

    fn extract_registered_routes(source: &'static str) -> BTreeSet<&'static str> {
        let mut routes = BTreeSet::new();
        let mut remaining = source;

        while let Some(route_pos) = remaining.find(".route(") {
            remaining = &remaining[route_pos + ".route(".len()..];
            let Some(quoted) = remaining.trim_start().strip_prefix('"') else {
                continue;
            };
            remaining = quoted;
            let Some(second_quote) = remaining.find('"') else {
                continue;
            };
            let path = &remaining[..second_quote];
            routes.insert(path);
            remaining = &remaining[second_quote + 1..];
        }

        let mut remaining = source;
        while let Some(swagger_pos) = remaining.find("SwaggerUi::new(") {
            remaining = &remaining[swagger_pos + "SwaggerUi::new(".len()..];
            let Some(quoted) = remaining.trim_start().strip_prefix('"') else {
                continue;
            };
            remaining = quoted;
            let Some(second_quote) = remaining.find('"') else {
                continue;
            };
            let base = &remaining[..second_quote];
            routes.insert(base);
            routes.insert(Box::leak(format!("{base}/").into_boxed_str()));
            routes.insert(Box::leak(format!("{base}/{{*rest}}").into_boxed_str()));
            remaining = &remaining[second_quote + 1..];
        }

        routes
    }

    fn extract_registered_route_operations(
        source: &'static str,
    ) -> BTreeSet<(&'static str, &'static str)> {
        const METHODS: [(&str, &str); 9] = [
            ("connect(", "CONNECT"),
            ("delete(", "DELETE"),
            ("get(", "GET"),
            ("head(", "HEAD"),
            ("options(", "OPTIONS"),
            ("patch(", "PATCH"),
            ("post(", "POST"),
            ("put(", "PUT"),
            ("trace(", "TRACE"),
        ];

        const DELEGATED_METHOD_ROUTERS: [(&str, &str); 1] =
            [("evidence_resolution::route()", "POST")];

        let mut operations = BTreeSet::new();
        let mut remaining = source;
        while let Some(route_pos) = remaining.find(".route(") {
            remaining = &remaining[route_pos + ".route(".len()..];
            let Some(call_end) = matching_call_end(remaining) else {
                break;
            };
            let call = &remaining[..call_end];
            let Some(first_quote) = call
                .find('"')
                .filter(|index| call[..*index].trim().is_empty())
            else {
                remaining = &remaining[call_end + 1..];
                continue;
            };
            let path_start = first_quote + 1;
            let Some(path_len) = call[path_start..].find('"') else {
                remaining = &remaining[call_end + 1..];
                continue;
            };
            let path = &call[path_start..path_start + path_len];
            for (token, method) in METHODS {
                if call.contains(token) {
                    operations.insert((path, method));
                }
            }
            // Method routers built in focused modules are not visible as
            // `get(`/`post(` tokens in the route call.
            for (token, method) in DELEGATED_METHOD_ROUTERS {
                if call.contains(token) {
                    operations.insert((path, method));
                }
            }
            remaining = &remaining[call_end + 1..];
        }

        let mut remaining = source;
        while let Some(swagger_pos) = remaining.find("SwaggerUi::new(") {
            remaining = &remaining[swagger_pos + "SwaggerUi::new(".len()..];
            let Some(first_quote) = remaining.find('"') else {
                continue;
            };
            remaining = &remaining[first_quote + 1..];
            let Some(second_quote) = remaining.find('"') else {
                continue;
            };
            let base = &remaining[..second_quote];
            operations.insert((base, "GET"));
            operations.insert((Box::leak(format!("{base}/").into_boxed_str()), "GET"));
            operations.insert((
                Box::leak(format!("{base}/{{*rest}}").into_boxed_str()),
                "GET",
            ));
            remaining = &remaining[second_quote + 1..];
        }

        let heads: Vec<_> = operations
            .iter()
            .filter(|(_, method)| *method == "GET")
            .map(|(path, _)| (*path, "HEAD"))
            .collect();
        operations.extend(heads);
        operations
    }

    // Include registrations in merged production modules as well as main.rs.
    // Test-only modules are not part of the production registration surface.
    fn registration_sources() -> Vec<&'static str> {
        fn collect(dir: &std::path::Path, sources: &mut Vec<&'static str>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(&path, sources);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let name = path.file_name().unwrap().to_string_lossy();
                    if name == "main.rs" || name.contains("test") {
                        continue;
                    }
                    let source = std::fs::read_to_string(path).unwrap();
                    // Test-only imports/hooks can precede production code.
                    // Exclude the test module, not everything after the first
                    // cfg(test) attribute.
                    let production = source
                        .split("\n#[cfg(test)]\nmod tests")
                        .next()
                        .unwrap()
                        .split("\n#[cfg(test)]\npub(crate) mod tests")
                        .next()
                        .unwrap();
                    sources.push(Box::leak(production.to_owned().into_boxed_str()));
                }
            }
        }
        let mut sources = vec![include_str!("main.rs")];
        collect(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut sources,
        );
        sources
    }

    fn all_registered_routes() -> BTreeSet<&'static str> {
        registration_sources()
            .into_iter()
            .flat_map(extract_registered_routes)
            .collect()
    }

    pub(crate) fn all_registered_operations() -> BTreeSet<(&'static str, &'static str)> {
        registration_sources()
            .into_iter()
            .flat_map(extract_registered_route_operations)
            .collect()
    }

    #[test]
    fn matched_route_inventory_requires_exact_template_and_method() {
        let mut identities = BTreeSet::new();
        for route in ROUTE_POLICY_INVENTORY {
            for method in route.methods {
                assert!(
                    identities.insert((route.path, *method)),
                    "duplicate {method} {}",
                    route.path
                );
            }
        }
        for path in ["/api/v1/archives", "/api/v1/memories"] {
            for (method, expected) in [
                (Method::GET, CatalogFiltered),
                (Method::HEAD, CatalogFiltered),
                (Method::POST, CompleteCatalog),
            ] {
                let policy = route_policy_for_matched_path(&method, path).unwrap();
                assert_eq!(policy.archive_isolation, expected, "{method} {path}");
                let input = authorization_input_for_request(&method, path, None).unwrap();
                assert_eq!(input.policy.archive_isolation, expected);
            }
        }
        for (template, method) in all_registered_operations() {
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            let policy = route_policy_for_matched_path(&method, template)
                .unwrap_or_else(|| panic!("unclassified {method} {template}"));
            assert_eq!(policy.path, template);
        }
        assert!(route_policy_for_matched_path(&Method::GET, "/api/v1/notes/private").is_none());
        assert!(route_policy_for_matched_path(&Method::POST, "/api/v1/notes/{id}").is_none());
    }

    fn matching_call_end(call: &str) -> Option<usize> {
        let mut depth = 1_u32;
        let mut quoted = false;
        let mut escaped = false;

        for (index, byte) in call.bytes().enumerate() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
                continue;
            }
            match byte {
                b'"' => quoted = true,
                b'(' => depth = depth.saturating_add(1),
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn materialize_route_template(template: &str) -> String {
        template
            .split('/')
            .map(|segment| {
                if is_template_param(segment) {
                    "policy-test-value"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    #[test]
    fn tenant_object_backing_normalization_inventory_selects_a_production_dispatcher() {
        for policy in ROUTE_POLICY_INVENTORY
            .iter()
            .filter(|policy| policy.class == TenantObject && policy.path.contains('{'))
        {
            let path = materialize_route_template(policy.path);
            for method_name in policy.methods {
                let method = Method::from_bytes(method_name.as_bytes())
                    .expect("route inventory contains a valid HTTP method");
                let input = authorization_input_for_request(&method, &path, Some("tenant-a"))
                    .unwrap_or_else(|| panic!("{method} {} has no policy input", policy.path));
                if input
                    .resource
                    .attrs
                    .get("requires_backing_resource_normalization")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                {
                    assert!(
                        backing_resource_normalizer(&input).is_some(),
                        "{method} {} requires backing normalization but has no production dispatcher; documented route-policy exceptions must set requires_backing_resource_normalization=false",
                        policy.path
                    );
                } else if let Some(exception) =
                    closed_route_normalization_exception(policy, &method)
                {
                    assert!(
                        role_based_route_is_intentionally_closed(&input),
                        "{method} {} is a closed normalization exception without the matching role-based fail-closed guard",
                        policy.path
                    );
                    assert_eq!(exception.path, policy.path);
                }
            }
        }
    }

    #[test]
    fn closed_normalization_exceptions_are_exact_real_routes_with_follow_ups() {
        assert_eq!(CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS.len(), 7);
        assert_eq!(
            CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS
                .iter()
                .map(|exception| exception.methods.len())
                .sum::<usize>(),
            13
        );

        let mut exception_operations = BTreeSet::new();
        for exception in CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS {
            assert!(!exception.reason.contains('\n'));
            assert!(
                matches!(
                    exception.follow_up,
                    "https://git.integrolabs.net/Fortemi/fortemi/issues/1279"
                        | "https://git.integrolabs.net/Fortemi/fortemi/issues/1280"
                ),
                "{} has an unexpected follow-up link",
                exception.path
            );
            let policy = ROUTE_POLICY_INVENTORY
                .iter()
                .find(|policy| policy.path == exception.path)
                .unwrap_or_else(|| panic!("stale closed-route exception: {}", exception.path));
            assert_eq!(policy.class, TenantObject, "{}", exception.path);
            assert_eq!(
                policy.methods, exception.methods,
                "stale closed-route exception method set: {}",
                exception.path
            );

            let path = materialize_route_template(exception.path);
            for method_name in exception.methods {
                assert!(
                    exception_operations.insert((exception.path, *method_name)),
                    "duplicate closed-route exception {method_name} {}",
                    exception.path
                );
                let method = Method::from_bytes(method_name.as_bytes()).unwrap();
                let input = authorization_input_for_request(&method, &path, Some("tenant-a"))
                    .unwrap_or_else(|| panic!("{method} {} has no policy input", exception.path));
                assert_eq!(
                    input.resource.attrs["requires_backing_resource_normalization"],
                    json!(false),
                    "{method} {} must not claim an unavailable normalizer",
                    exception.path
                );
                assert!(
                    role_based_route_is_intentionally_closed(&input),
                    "{method} {} must retain its fail-closed role-based guard",
                    exception.path
                );
                assert!(
                    backing_resource_normalizer(&input).is_none(),
                    "{method} {} must not select a fictional normalizer",
                    exception.path
                );
            }
        }
    }
}

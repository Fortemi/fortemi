//! Route authorization inventory for the first AuthorizationPolicy slice (#710).
//!
//! This table is intentionally executable: authentication exemptions and
//! admin/operator checks read from it, and tests compare it with the Axum router
//! declarations in `main.rs`. The first slice keeps enforcement coarse while
//! making every externally registered route carry policy/docs/cache metadata.

use std::collections::HashMap;

use axum::http::Method;
use matric_core::{Action, AuthzContext, Resource, ResourceKind, ScopeFamily};
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
    Docs,
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
    PublicStatic,
    PublicProbe,
    PrivateUserData,
    NoStore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutePolicy {
    pub path: &'static str,
    pub class: PolicyClass,
    pub action_family: &'static str,
    pub docs: DocsExposureClass,
    pub cache: CacheHeaderClass,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePolicyInput {
    pub policy: &'static RoutePolicy,
    pub action: Action,
    pub resource: Resource,
    pub context: AuthzContext,
}

struct ResourceIdCandidate {
    param_name: &'static str,
    value: String,
}

use CacheHeaderClass::*;
use DocsExposureClass::{Authenticated, Hidden, Operator, Public as DocsPublic};
use PolicyClass::{
    AdminOperator, AuthenticatedRead, AuthenticatedWrite, Docs, OAuth, Public,
    PublicWithInlineProof, RealtimeTransport, SystemHealth, TenantObject,
};

pub const ROUTE_POLICY_INVENTORY: &[RoutePolicy] = &[
    r(
        "/.well-known/oauth-authorization-server",
        OAuth,
        "oauth_discovery",
        DocsPublic,
        PublicProbe,
    ),
    r(
        "/.well-known/oauth-protected-resource",
        OAuth,
        "oauth_discovery",
        DocsPublic,
        PublicProbe,
    ),
    r(
        "/api/v1/api-keys",
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/api-keys/{id}",
        AdminOperator,
        "credential_management",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/archives",
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/archives/{name}",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/archives/{name}/clone",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/archives/{name}/set-default",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/archives/{name}/stats",
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments/{attachment_id}",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/download",
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/sprites/{sprite_index}",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/subtitles",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/thumbnail",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/attachments/{attachment_id}/thumbnails.vtt",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/audio/transcribe",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/backup/database",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/database/restore",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/database/snapshot",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/database/upload",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/download",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/export",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/import",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/knowledge-archive",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/knowledge-archive/{filename}",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/knowledge-shard",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/knowledge-shard/import",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/knowledge-shard/upload",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/list",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/list/{filename}",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/memory/{name}",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/metadata/{filename}",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/status",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/swap",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/backup/trigger",
        AdminOperator,
        "backup_restore",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/calls/{id}",
        TenantObject,
        "realtime_call",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/chat",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/chat/models",
        AuthenticatedRead,
        "ai_execution",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/chat/stream",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/collections",
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/collections/{id}",
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/collections/{id}/export",
        TenantObject,
        "collection",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/collections/{id}/notes",
        TenantObject,
        "collection",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/autocomplete",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/collections",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/collections/{id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/collections/{id}/members",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/collections/{id}/members/{concept_id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/governance",
        AdminOperator,
        "taxonomy",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/concepts/schemes",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/schemes/export/turtle",
        TenantObject,
        "taxonomy",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/concepts/schemes/{id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/schemes/{id}/export/turtle",
        TenantObject,
        "taxonomy",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/concepts/schemes/{id}/top-concepts",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/ancestors",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/broader",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/broader/{target_id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/descendants",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/full",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/narrower",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/narrower/{target_id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/related",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/concepts/{id}/related/{target_id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/document-types",
        AdminOperator,
        "document_type_catalog",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/document-types/detect",
        AuthenticatedWrite,
        "document_type_catalog",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/document-types/{name}",
        AdminOperator,
        "document_type_catalog",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/embedding-configs",
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/embedding-configs/default",
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/embedding-configs/{id}",
        AdminOperator,
        "embedding_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/embedding-sets",
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/embedding-sets/{slug}",
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/members",
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/members/{note_id}",
        TenantObject,
        "embedding_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/embedding-sets/{slug}/refresh",
        TenantObject,
        "embedding_control",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/events",
        RealtimeTransport,
        "event_stream",
        Hidden,
        NoStore,
    ),
    r(
        "/api/v1/extraction/stats",
        AdminOperator,
        "system_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/cold-spots",
        TenantObject,
        "graph_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/graph/community/coarse",
        TenantObject,
        "graph_control",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/graph/diagnostics",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/diagnostics/compare",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/diagnostics/history",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/diagnostics/snapshot",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/maintenance",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/pfnet/sparsify",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/snn/recompute",
        AdminOperator,
        "graph_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/graph/topology/stats",
        AuthenticatedRead,
        "graph_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/graph/{id}",
        TenantObject,
        "graph_control",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/health/access-frequency",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/health/knowledge",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/health/orphan-tags",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/health/stale-notes",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/health/streaming",
        SystemHealth,
        "health_diagnostics",
        DocsPublic,
        PublicProbe,
    ),
    r(
        "/api/v1/health/tag-cooccurrence",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/health/unlinked-notes",
        SystemHealth,
        "health_diagnostics",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/inbound-sources",
        AdminOperator,
        "inbound_connector",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/inbound-sources/{name}",
        AdminOperator,
        "inbound_connector",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/inference/complete",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/inference/config",
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/inference/config/audit",
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/inference/providers",
        AuthenticatedRead,
        "ai_execution",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/inference/stream",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/inference/test-connection",
        AdminOperator,
        "model_config",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/ingest/stream",
        RealtimeTransport,
        "ingest_stream",
        Hidden,
        NoStore,
    ),
    r(
        "/api/v1/ingest/tokens",
        AdminOperator,
        "ingest_stream",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/ingest/tokens/{token_id}",
        AdminOperator,
        "ingest_stream",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/pause",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/pause/{archive}",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/pending",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/resume",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/resume/{archive}",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/stats",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/status",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/jobs/{id}",
        AdminOperator,
        "job_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/memories",
        AdminOperator,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/overview",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/search",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/{name}",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/{name}/clone",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/{name}/set-default",
        TenantObject,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memories/{name}/stats",
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/memory/info",
        AuthenticatedRead,
        "memory_management",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/models",
        AuthenticatedRead,
        "model_catalog",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/activity",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/bulk",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/reprocess",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/timeline",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/attachments",
        TenantObject,
        "attachment",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/attachments/tus",
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/attachments/tus/{upload_id}",
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/attachments/upload",
        TenantObject,
        "attachment",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/backlinks",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/concepts",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/concepts/{concept_id}",
        TenantObject,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/export",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/full",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/links",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/memory-provenance",
        TenantObject,
        "provenance",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/move",
        TenantObject,
        "collection",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/provenance",
        TenantObject,
        "provenance",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/purge",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/related",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/reprocess",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/restore",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/status",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/notes/{id}/tags",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/versions",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/versions/diff",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/versions/{version}",
        TenantObject,
        "note",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/notes/{id}/versions/{version}/restore",
        TenantObject,
        "note",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/address",
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/decrypt",
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/encrypt",
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/keygen",
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets/active",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets/import",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}/active",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/keysets/{name_or_id}/export",
        AdminOperator,
        "pke_keyset",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/pke/recipients",
        AuthenticatedWrite,
        "pke",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/pke/verify/{address}",
        AuthenticatedRead,
        "pke",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/provenance/devices",
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/provenance/files",
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/provenance/locations",
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/provenance/named-locations",
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/provenance/notes",
        TenantObject,
        "provenance",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/rate-limit/status",
        AuthenticatedRead,
        "rate_limit",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/realtime/twilio/{provider_call_id}",
        PublicWithInlineProof,
        "realtime_provider_callback",
        Hidden,
        NoStore,
    ),
    r(
        "/api/v1/search",
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/search/federated",
        TenantObject,
        "search",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/tags",
        AuthenticatedRead,
        "taxonomy",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/templates",
        TenantObject,
        "template",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/templates/{id}",
        TenantObject,
        "template",
        Authenticated,
        PrivateUserData,
    ),
    r(
        "/api/v1/templates/{id}/instantiate",
        TenantObject,
        "template",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/vision/describe",
        AuthenticatedWrite,
        "ai_execution",
        Authenticated,
        NoStore,
    ),
    r(
        "/api/v1/webhooks",
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/incoming",
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/incoming/validate",
        PublicWithInlineProof,
        "webhook_receiver",
        Hidden,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/incoming/{slug}",
        PublicWithInlineProof,
        "webhook_receiver",
        Hidden,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/{id}",
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/{id}/deliveries",
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/webhooks/{id}/test",
        AdminOperator,
        "webhook_control",
        Operator,
        NoStore,
    ),
    r(
        "/api/v1/ws",
        RealtimeTransport,
        "event_stream",
        Hidden,
        NoStore,
    ),
    r(
        "/asyncapi.yaml",
        Docs,
        "docs_schema",
        DocsPublic,
        PublicStatic,
    ),
    r("/health", Public, "health_probe", DocsPublic, PublicProbe),
    r(
        "/health/live",
        Public,
        "health_probe",
        DocsPublic,
        PublicProbe,
    ),
    r("/oauth/authorize", OAuth, "oauth_flow", DocsPublic, NoStore),
    r(
        "/oauth/introspect",
        OAuth,
        "oauth_flow",
        DocsPublic,
        NoStore,
    ),
    r("/oauth/register", OAuth, "oauth_flow", DocsPublic, NoStore),
    r("/oauth/revoke", OAuth, "oauth_flow", DocsPublic, NoStore),
    r("/oauth/token", OAuth, "oauth_flow", DocsPublic, NoStore),
    r(
        "/openapi.yaml",
        Docs,
        "docs_schema",
        DocsPublic,
        PublicStatic,
    ),
    r("/recording.wav", Public, "test_fixture", Hidden, NoStore),
];

const fn r(
    path: &'static str,
    class: PolicyClass,
    action_family: &'static str,
    docs: DocsExposureClass,
    cache: CacheHeaderClass,
) -> RoutePolicy {
    RoutePolicy {
        path,
        class,
        action_family,
        docs,
        cache,
    }
}

pub fn route_policy_for_path(path: &str) -> Option<&'static RoutePolicy> {
    ROUTE_POLICY_INVENTORY
        .iter()
        .find(|route| route_template_matches(route.path, path))
}

pub fn is_public_without_bearer(path: &str) -> bool {
    path.starts_with("/docs")
        || path.starts_with("/swagger-ui")
        || path.starts_with("/api-docs")
        || route_policy_for_path(path).is_some_and(|policy| {
            matches!(
                policy.class,
                Public | Docs | OAuth | RealtimeTransport | SystemHealth
            ) || (policy.class == PublicWithInlineProof
                && policy.action_family == "realtime_provider_callback")
        })
}

pub fn is_admin_operator_route(path: &str) -> bool {
    // Transitional #710 slice: the inventory can classify future hosted
    // admin/operator routes, but current scope enforcement remains limited to
    // credential management until the full AuthorizationPolicy middleware lands.
    route_policy_for_path(path).is_some_and(|policy| {
        policy.class == AdminOperator && policy.action_family == "credential_management"
    })
}

pub fn authorization_input_for_request(
    method: &Method,
    path: &str,
    tenant_id: Option<&str>,
) -> Option<RoutePolicyInput> {
    let policy = route_policy_for_path(path)?;
    let params = route_params(policy.path, path);
    let action = Action {
        name: route_action_name(policy, method),
        scope_family: scope_family_for_policy(policy),
        required_scopes: required_scopes_for_policy(policy, method),
    };

    let mut resource = Resource::new(resource_kind_for_policy(policy));
    if let Some(candidate) = resource_id_for_policy(policy, &params) {
        let requires_backing_normalization =
            requires_backing_resource_normalization(policy, candidate.param_name);
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
        json!(format!("{:?}", policy.class)),
    );
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
    })
}

pub fn mark_resource_id_normalized(input: &mut RoutePolicyInput) {
    input
        .resource
        .attrs
        .insert("resource_id_normalized".to_string(), json!(true));
}

fn route_action_name(policy: &RoutePolicy, method: &Method) -> String {
    format!(
        "{}:{}",
        policy.action_family,
        method.as_str().to_ascii_lowercase()
    )
}

fn scope_family_for_policy(policy: &RoutePolicy) -> ScopeFamily {
    match policy.class {
        AdminOperator => ScopeFamily::Admin,
        RealtimeTransport => ScopeFamily::McpTransport,
        _ => ScopeFamily::Rest,
    }
}

fn required_scopes_for_policy(policy: &RoutePolicy, method: &Method) -> Vec<String> {
    match policy.class {
        Public | PublicWithInlineProof | Docs | OAuth => vec![],
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

fn resource_kind_for_policy(policy: &RoutePolicy) -> ResourceKind {
    match policy.action_family {
        "attachment" => ResourceKind::Attachment,
        "backup_restore" => ResourceKind::Backup,
        "collection" => ResourceKind::Collection,
        "credential_management" => ResourceKind::ApiKey,
        "document_type_catalog" => ResourceKind::DocumentType,
        "health_diagnostics" | "system_diagnostics" => ResourceKind::System,
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
        "oauth_discovery" | "oauth_flow" | "docs_schema" | "health_probe" | "test_fixture" => {
            ResourceKind::PublicRoute
        }
        other => ResourceKind::Other(other.to_string()),
    }
}

fn resource_id_for_policy(
    policy: &RoutePolicy,
    params: &HashMap<&'static str, &str>,
) -> Option<ResourceIdCandidate> {
    let preferred_param = match policy.action_family {
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

fn requires_backing_resource_normalization(policy: &RoutePolicy, param_name: &str) -> bool {
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

fn route_template_matches(template: &str, path: &str) -> bool {
    let mut template_segments = template.split('/');
    let mut path_segments = path.split('/');

    loop {
        match (template_segments.next(), path_segments.next()) {
            (None, None) => return true,
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
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registered_routes_have_policy_inventory_rows() {
        let registered = extract_registered_routes(include_str!("main.rs"));
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
        let registered = extract_registered_routes(include_str!("main.rs"));
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
    fn api_key_management_is_one_admin_operator_family() {
        assert!(is_admin_operator_route("/api/v1/api-keys"));
        assert!(is_admin_operator_route(
            "/api/v1/api-keys/018fd1a0-0000-7000-8000-000000000000"
        ));
    }

    #[test]
    fn public_callback_routes_are_not_admin_operator_routes() {
        assert!(!is_public_without_bearer(
            "/api/v1/webhooks/incoming/example"
        ));
        assert!(is_public_without_bearer("/api/v1/realtime/twilio/CA123"));
        assert!(is_public_without_bearer("/api/v1/health/streaming"));
        assert!(!is_admin_operator_route(
            "/api/v1/webhooks/incoming/example"
        ));
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
    }

    #[test]
    fn realtime_transport_requires_mcp_transport_scope() {
        let input = authorization_input_for_request(&Method::GET, "/api/v1/events", None)
            .expect("event stream route should be inventoried");

        assert_eq!(input.policy.class, RealtimeTransport);
        assert_eq!(input.action.scope_family, ScopeFamily::McpTransport);
        assert_eq!(input.action.required_scopes, vec!["mcp"]);
        assert_eq!(input.resource.kind, ResourceKind::McpTool);
    }

    fn extract_registered_routes(source: &'static str) -> BTreeSet<&'static str> {
        let mut routes = BTreeSet::new();
        let mut remaining = source;

        while let Some(route_pos) = remaining.find(".route(") {
            remaining = &remaining[route_pos + ".route(".len()..];
            let Some(first_quote) = remaining.find('"') else {
                continue;
            };
            remaining = &remaining[first_quote + 1..];
            let Some(second_quote) = remaining.find('"') else {
                continue;
            };
            let path = &remaining[..second_quote];
            routes.insert(path);
            remaining = &remaining[second_quote + 1..];
        }

        routes
    }
}

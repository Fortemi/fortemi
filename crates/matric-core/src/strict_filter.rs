//! Unified Strict Filter system for multi-dimensional note filtering.
//!
//! This module provides the `StrictFilter` type that composes all filtering
//! dimensions into a single, cohesive filtering interface:
//!
//! - **Tags**: SKOS concept-based semantic filtering
//! - **Temporal**: Time-based filtering with UUIDv7 optimization
//! - **Collections**: Hierarchical organization filtering
//! - **Security**: Access control and visibility filtering (future)
//! - **Semantic Scope**: Embedding set isolation (future)
//!
//! # Design Philosophy
//!
//! The Unified Strict Filter system follows these principles:
//!
//! 1. **Composable**: Each dimension can be used independently or combined
//! 2. **Type-Safe**: Compile-time guarantees prevent invalid filter states
//! 3. **Efficient**: Optimized SQL generation with proper index usage
//! 4. **Extensible**: New dimensions can be added without breaking changes
//!
//! # Example
//!
//! ```
//! use matric_core::{StrictFilter, StrictTagFilter, StrictTemporalFilter, StrictCollectionFilter};
//! use matric_core::temporal::NamedTemporalRange;
//! use uuid::Uuid;
//!
//! // Create a multi-dimensional filter
//! let filter = StrictFilter::new()
//!     .with_tags(
//!         StrictTagFilter::new()
//!             .require_concept(Uuid::nil()) // Require "programming" tag
//!             .exclude_concept(Uuid::nil()) // Exclude "archived" tag
//!     )
//!     .with_temporal(
//!         StrictTemporalFilter::new()
//!             .created_within(NamedTemporalRange::ThisMonth)
//!     )
//!     .with_collections(
//!         StrictCollectionFilter::new()
//!             .in_collection(Uuid::nil())
//!             .with_descendants(true)
//!     );
//!
//! // Check active dimensions
//! assert!(filter.has_tag_constraints());
//! assert!(filter.has_temporal_constraints());
//! assert!(filter.has_collection_constraints());
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::collection_filter::StrictCollectionFilter;
use crate::search::StrictTagFilter;
use crate::temporal::StrictTemporalFilter;

// =============================================================================
// UNIFIED STRICT FILTER
// =============================================================================

/// Unified strict filter composing all filtering dimensions.
///
/// This is the primary entry point for multi-dimensional note filtering,
/// combining tags, temporal, collections, security, and semantic scope.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StrictFilter {
    /// Tag-based filtering using SKOS concepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<StrictTagFilter>,

    /// Temporal filtering with UUIDv7 optimization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal: Option<StrictTemporalFilter>,

    /// Collection-based hierarchical filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collections: Option<StrictCollectionFilter>,

    /// Security filtering (owner, visibility, tenant).
    /// Reserved for Phase 4 implementation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<StrictSecurityFilter>,

    /// Semantic scope filtering (embedding set isolation).
    /// Reserved for Phase 4 implementation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_scope: Option<SemanticScopeFilter>,

    /// Additional metadata filters (starred, archived, format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataFilter>,
}

impl fmt::Debug for StrictFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrictFilter")
            .field("active_dimension_count", &self.active_dimension_count())
            .field("has_tag_constraints", &self.has_tag_constraints())
            .field("has_temporal_constraints", &self.has_temporal_constraints())
            .field(
                "has_collection_constraints",
                &self.has_collection_constraints(),
            )
            .field("has_security_constraints", &self.has_security_constraints())
            .field(
                "has_semantic_scope_constraints",
                &self.has_semantic_scope_constraints(),
            )
            .field("has_metadata_constraints", &self.has_metadata_constraints())
            .field("requires_recursive_cte", &self.requires_recursive_cte())
            .field(
                "can_use_uuid_temporal_optimization",
                &self.can_use_uuid_temporal_optimization(),
            )
            .finish()
    }
}

impl StrictFilter {
    /// Create a new empty filter (matches all notes).
    pub fn new() -> Self {
        Self::default()
    }

    // =========================================================================
    // BUILDER METHODS
    // =========================================================================

    /// Set tag filter dimension.
    pub fn with_tags(mut self, tags: StrictTagFilter) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Set temporal filter dimension.
    pub fn with_temporal(mut self, temporal: StrictTemporalFilter) -> Self {
        self.temporal = Some(temporal);
        self
    }

    /// Set collection filter dimension.
    pub fn with_collections(mut self, collections: StrictCollectionFilter) -> Self {
        self.collections = Some(collections);
        self
    }

    /// Set security filter dimension.
    pub fn with_security(mut self, security: StrictSecurityFilter) -> Self {
        self.security = Some(security);
        self
    }

    /// Set semantic scope filter dimension.
    pub fn with_semantic_scope(mut self, scope: SemanticScopeFilter) -> Self {
        self.semantic_scope = Some(scope);
        self
    }

    /// Set metadata filter dimension.
    pub fn with_metadata(mut self, metadata: MetadataFilter) -> Self {
        self.metadata = Some(metadata);
        self
    }

    // =========================================================================
    // CONSTRAINT CHECKS
    // =========================================================================

    /// Check if the filter is completely empty (matches all notes).
    pub fn is_empty(&self) -> bool {
        !self.has_tag_constraints()
            && !self.has_temporal_constraints()
            && !self.has_collection_constraints()
            && !self.has_security_constraints()
            && !self.has_semantic_scope_constraints()
            && !self.has_metadata_constraints()
    }

    /// Check if there are any tag constraints.
    pub fn has_tag_constraints(&self) -> bool {
        self.tags.as_ref().map(|t| !t.is_empty()).unwrap_or(false)
    }

    /// Check if there are any temporal constraints.
    pub fn has_temporal_constraints(&self) -> bool {
        self.temporal
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }

    /// Check if there are any collection constraints.
    pub fn has_collection_constraints(&self) -> bool {
        self.collections
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    }

    /// Check if there are any security constraints.
    pub fn has_security_constraints(&self) -> bool {
        self.security
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    /// Check if there are any semantic scope constraints.
    pub fn has_semantic_scope_constraints(&self) -> bool {
        self.semantic_scope
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    /// Check if there are any metadata constraints.
    pub fn has_metadata_constraints(&self) -> bool {
        self.metadata
            .as_ref()
            .map(|m| !m.is_empty())
            .unwrap_or(false)
    }

    // =========================================================================
    // DIMENSION ACCESS
    // =========================================================================

    /// Get tag filter, creating default if not set.
    pub fn tags_or_default(&self) -> StrictTagFilter {
        self.tags.clone().unwrap_or_default()
    }

    /// Get temporal filter, creating default if not set.
    pub fn temporal_or_default(&self) -> StrictTemporalFilter {
        self.temporal.clone().unwrap_or_default()
    }

    /// Get collection filter, creating default if not set.
    pub fn collections_or_default(&self) -> StrictCollectionFilter {
        self.collections.clone().unwrap_or_default()
    }

    // =========================================================================
    // QUERY OPTIMIZATION HINTS
    // =========================================================================

    /// Check if the filter requires recursive CTE for collections.
    pub fn requires_recursive_cte(&self) -> bool {
        self.collections
            .as_ref()
            .map(|c| c.requires_recursive_query())
            .unwrap_or(false)
    }

    /// Check if UUIDv7 temporal optimization can be applied.
    ///
    /// Returns true if there are created time constraints that can
    /// be translated to primary key range queries.
    pub fn can_use_uuid_temporal_optimization(&self) -> bool {
        self.temporal
            .as_ref()
            .map(|t| t.has_created_constraints())
            .unwrap_or(false)
    }

    /// Get the number of active filter dimensions.
    pub fn active_dimension_count(&self) -> usize {
        let mut count = 0;
        if self.has_tag_constraints() {
            count += 1;
        }
        if self.has_temporal_constraints() {
            count += 1;
        }
        if self.has_collection_constraints() {
            count += 1;
        }
        if self.has_security_constraints() {
            count += 1;
        }
        if self.has_semantic_scope_constraints() {
            count += 1;
        }
        if self.has_metadata_constraints() {
            count += 1;
        }
        count
    }
}

// =============================================================================
// SECURITY FILTER (Phase 4 placeholder)
// =============================================================================

/// Security filter for access control and visibility.
///
/// This filter supports:
/// - Owner-based filtering
/// - Visibility levels (private, shared, public)
/// - Tenant isolation for multi-tenant deployments
/// - Share grant verification
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StrictSecurityFilter {
    /// Required owner ID (exact match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Uuid>,

    /// Required tenant ID for multi-tenant isolation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Uuid>,

    /// Allowed visibility levels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visibility: Vec<Visibility>,

    /// User ID for share grant verification.
    /// If set, includes notes shared with this user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_with_user: Option<Uuid>,

    /// Whether to include notes the user owns.
    #[serde(default = "default_true")]
    pub include_owned: bool,

    /// Whether to include notes shared with the user.
    #[serde(default = "default_true")]
    pub include_shared: bool,
}

impl fmt::Debug for StrictSecurityFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrictSecurityFilter")
            .field("owner_id_set", &self.owner_id.is_some())
            .field("tenant_id_set", &self.tenant_id.is_some())
            .field("visibility_count", &self.visibility.len())
            .field("shared_with_user_set", &self.shared_with_user.is_some())
            .field("include_owned", &self.include_owned)
            .field("include_shared", &self.include_shared)
            .finish()
    }
}

fn default_true() -> bool {
    true
}

/// Visibility level for notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Only visible to owner.
    Private,
    /// Visible to specific users via share grants.
    Shared,
    /// Visible to all users in tenant.
    Internal,
    /// Visible to everyone.
    Public,
}

impl StrictSecurityFilter {
    /// Create a new empty security filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set owner filter.
    pub fn for_owner(mut self, owner_id: Uuid) -> Self {
        self.owner_id = Some(owner_id);
        self
    }

    /// Set tenant filter.
    pub fn for_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Allow specific visibility levels.
    pub fn with_visibility(mut self, vis: Visibility) -> Self {
        self.visibility.push(vis);
        self
    }

    /// Check access for a specific user (includes owned and shared).
    pub fn for_user(mut self, user_id: Uuid) -> Self {
        self.owner_id = Some(user_id);
        self.shared_with_user = Some(user_id);
        self.include_owned = true;
        self.include_shared = true;
        self
    }

    /// Check if the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.owner_id.is_none()
            && self.tenant_id.is_none()
            && self.visibility.is_empty()
            && self.shared_with_user.is_none()
    }
}

// =============================================================================
// SEMANTIC SCOPE FILTER (Phase 4 placeholder)
// =============================================================================

/// Semantic scope filter for embedding set isolation.
///
/// This filter restricts search to specific embedding sets,
/// enabling isolated semantic namespaces for different use cases.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SemanticScopeFilter {
    /// Embedding set ID to search within.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_set_id: Option<Uuid>,

    /// Multiple embedding sets (OR logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_embedding_sets: Vec<Uuid>,

    /// Excluded embedding sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_embedding_sets: Vec<Uuid>,

    /// Whether to include the default embedding set.
    #[serde(default = "default_true")]
    pub include_default_set: bool,
}

impl fmt::Debug for SemanticScopeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticScopeFilter")
            .field("embedding_set_id_set", &self.embedding_set_id.is_some())
            .field("any_embedding_sets_count", &self.any_embedding_sets.len())
            .field(
                "excluded_embedding_sets_count",
                &self.excluded_embedding_sets.len(),
            )
            .field("include_default_set", &self.include_default_set)
            .finish()
    }
}

impl SemanticScopeFilter {
    /// Create a new empty semantic scope filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to a specific embedding set.
    pub fn in_set(mut self, set_id: Uuid) -> Self {
        self.embedding_set_id = Some(set_id);
        self
    }

    /// Include an embedding set (OR logic).
    pub fn include_set(mut self, set_id: Uuid) -> Self {
        self.any_embedding_sets.push(set_id);
        self
    }

    /// Exclude an embedding set.
    pub fn exclude_set(mut self, set_id: Uuid) -> Self {
        self.excluded_embedding_sets.push(set_id);
        self
    }

    /// Set whether to include the default set.
    pub fn with_default_set(mut self, include: bool) -> Self {
        self.include_default_set = include;
        self
    }

    /// Check if the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.embedding_set_id.is_none()
            && self.any_embedding_sets.is_empty()
            && self.excluded_embedding_sets.is_empty()
    }
}

// =============================================================================
// METADATA FILTER
// =============================================================================

/// Metadata filter for common note attributes.
///
/// This filter handles boolean flags and simple attributes that
/// don't fit into other filter dimensions.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct MetadataFilter {
    /// Filter by starred status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,

    /// Filter by archived status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,

    /// Filter by format (e.g., "markdown", "plaintext").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Filter by source (e.g., "manual", "import", "api").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Whether to include soft-deleted notes.
    #[serde(default)]
    pub include_deleted: bool,

    /// Minimum access count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_access_count: Option<i32>,
}

impl fmt::Debug for MetadataFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetadataFilter")
            .field("starred", &self.starred)
            .field("archived", &self.archived)
            .field(
                "format_len",
                &self.format.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "source_len",
                &self.source.as_ref().map(|value| value.chars().count()),
            )
            .field("include_deleted", &self.include_deleted)
            .field("min_access_count", &self.min_access_count)
            .finish()
    }
}

impl MetadataFilter {
    /// Create a new empty metadata filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to only starred notes.
    pub fn starred_only(mut self) -> Self {
        self.starred = Some(true);
        self
    }

    /// Filter to non-archived notes.
    pub fn exclude_archived(mut self) -> Self {
        self.archived = Some(false);
        self
    }

    /// Filter by format.
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Filter by source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Include soft-deleted notes.
    pub fn include_deleted(mut self) -> Self {
        self.include_deleted = true;
        self
    }

    /// Require minimum access count (popular notes).
    pub fn with_min_access_count(mut self, count: i32) -> Self {
        self.min_access_count = Some(count);
        self
    }

    /// Check if the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.starred.is_none()
            && self.archived.is_none()
            && self.format.is_none()
            && self.source.is_none()
            && !self.include_deleted
            && self.min_access_count.is_none()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::NamedTemporalRange;

    #[test]
    fn test_empty_filter() {
        let filter = StrictFilter::new();
        assert!(filter.is_empty());
        assert_eq!(filter.active_dimension_count(), 0);
    }

    #[test]
    fn test_single_dimension() {
        let filter =
            StrictFilter::new().with_tags(StrictTagFilter::new().require_concept(Uuid::new_v4()));

        assert!(!filter.is_empty());
        assert!(filter.has_tag_constraints());
        assert!(!filter.has_temporal_constraints());
        assert_eq!(filter.active_dimension_count(), 1);
    }

    #[test]
    fn test_multi_dimension() {
        let filter = StrictFilter::new()
            .with_tags(StrictTagFilter::new().require_concept(Uuid::new_v4()))
            .with_temporal(StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek))
            .with_collections(StrictCollectionFilter::new().in_collection(Uuid::new_v4()));

        assert!(!filter.is_empty());
        assert!(filter.has_tag_constraints());
        assert!(filter.has_temporal_constraints());
        assert!(filter.has_collection_constraints());
        assert_eq!(filter.active_dimension_count(), 3);
    }

    #[test]
    fn test_uuid_temporal_optimization() {
        // Without temporal - no optimization
        let filter = StrictFilter::new();
        assert!(!filter.can_use_uuid_temporal_optimization());

        // With created constraints - optimization available
        let filter = StrictFilter::new().with_temporal(
            StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek),
        );
        assert!(filter.can_use_uuid_temporal_optimization());

        // With only updated constraints - no optimization (uses separate column)
        let filter = StrictFilter::new().with_temporal(
            StrictTemporalFilter::new().updated_within(NamedTemporalRange::ThisWeek),
        );
        assert!(!filter.can_use_uuid_temporal_optimization());
    }

    #[test]
    fn test_recursive_cte_detection() {
        // Without descendants - no CTE
        let filter = StrictFilter::new()
            .with_collections(StrictCollectionFilter::new().in_collection(Uuid::new_v4()));
        assert!(!filter.requires_recursive_cte());

        // With descendants - needs CTE
        let filter = StrictFilter::new().with_collections(
            StrictCollectionFilter::new()
                .in_collection(Uuid::new_v4())
                .with_descendants(true),
        );
        assert!(filter.requires_recursive_cte());
    }

    #[test]
    fn test_security_filter() {
        let user_id = Uuid::new_v4();
        let filter = StrictSecurityFilter::new().for_user(user_id);

        assert!(!filter.is_empty());
        assert_eq!(filter.owner_id, Some(user_id));
        assert_eq!(filter.shared_with_user, Some(user_id));
    }

    #[test]
    fn test_semantic_scope_filter() {
        let set_id = Uuid::new_v4();
        let filter = SemanticScopeFilter::new().in_set(set_id);

        assert!(!filter.is_empty());
        assert_eq!(filter.embedding_set_id, Some(set_id));
    }

    #[test]
    fn test_metadata_filter() {
        let filter = MetadataFilter::new()
            .starred_only()
            .exclude_archived()
            .with_format("markdown");

        assert!(!filter.is_empty());
        assert_eq!(filter.starred, Some(true));
        assert_eq!(filter.archived, Some(false));
        assert_eq!(filter.format.as_deref(), Some("markdown"));
    }

    #[test]
    fn strict_filter_debug_redacts_nested_filter_values() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let embedding_set_id = Uuid::new_v4();
        let collection_id = Uuid::new_v4();
        let excluded_collection_id = Uuid::new_v4();

        let mut tag_filter = StrictTagFilter::new();
        tag_filter
            .required_string_tags
            .push("private-tag@example.test".to_string());
        tag_filter
            .any_string_tags
            .push("/tmp/private/tag".to_string());
        tag_filter
            .excluded_string_tags
            .push("sk-live-tag-secret".to_string());

        let filter = StrictFilter::new()
            .with_tags(tag_filter)
            .with_temporal(StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek))
            .with_collections(
                StrictCollectionFilter::new()
                    .in_collection(collection_id)
                    .exclude_collection(excluded_collection_id)
                    .with_descendants(true),
            )
            .with_security(
                StrictSecurityFilter::new()
                    .for_owner(owner_id)
                    .for_tenant(tenant_id)
                    .with_visibility(Visibility::Private),
            )
            .with_semantic_scope(SemanticScopeFilter::new().in_set(embedding_set_id))
            .with_metadata(
                MetadataFilter::new()
                    .with_format("private-format@example.test")
                    .with_source("https://example.test/source?token=secret")
                    .include_deleted()
                    .with_min_access_count(3),
            );

        let debug = format!("{filter:?}");

        for secret in [
            owner_id.to_string(),
            tenant_id.to_string(),
            embedding_set_id.to_string(),
            collection_id.to_string(),
            excluded_collection_id.to_string(),
            "private-tag@example.test".to_string(),
            "/tmp/private/tag".to_string(),
            "sk-live-tag-secret".to_string(),
            "private-format@example.test".to_string(),
            "https://example.test/source?token=secret".to_string(),
            "token=secret".to_string(),
        ] {
            assert!(
                !debug.contains(&secret),
                "StrictFilter Debug output leaked sensitive value {secret:?}: {debug}"
            );
        }

        for expected in [
            "active_dimension_count",
            "has_tag_constraints",
            "has_temporal_constraints",
            "has_collection_constraints",
            "has_security_constraints",
            "has_semantic_scope_constraints",
            "has_metadata_constraints",
            "requires_recursive_cte",
            "can_use_uuid_temporal_optimization",
        ] {
            assert!(
                debug.contains(expected),
                "StrictFilter Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn strict_filter_component_debug_redacts_ids_and_strings() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let embedding_set_id = Uuid::new_v4();
        let included_set_id = Uuid::new_v4();
        let excluded_set_id = Uuid::new_v4();

        let security = StrictSecurityFilter::new()
            .for_owner(owner_id)
            .for_tenant(tenant_id)
            .with_visibility(Visibility::Internal);
        let semantic_scope = SemanticScopeFilter::new()
            .in_set(embedding_set_id)
            .include_set(included_set_id)
            .exclude_set(excluded_set_id);
        let metadata = MetadataFilter::new()
            .with_format("private-format-秘密@example.test")
            .with_source("/tmp/private/source-秘密@example.test")
            .include_deleted()
            .with_min_access_count(9);

        let debug = format!("{security:?}\n{semantic_scope:?}\n{metadata:?}");

        for secret in [
            owner_id.to_string(),
            tenant_id.to_string(),
            embedding_set_id.to_string(),
            included_set_id.to_string(),
            excluded_set_id.to_string(),
            "private-format-秘密@example.test".to_string(),
            "/tmp/private/source-秘密@example.test".to_string(),
            "秘密".to_string(),
        ] {
            assert!(
                !debug.contains(&secret),
                "Strict filter component Debug output leaked sensitive value {secret:?}: {debug}"
            );
        }

        for expected in [
            "owner_id_set",
            "tenant_id_set",
            "visibility_count",
            "embedding_set_id_set",
            "any_embedding_sets_count",
            "excluded_embedding_sets_count",
            "format_len",
            "source_len",
        ] {
            assert!(
                debug.contains(expected),
                "Strict filter component Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }

        assert!(debug.contains("format_len: Some(30)"));
        assert!(debug.contains("source_len: Some(35)"));
    }
}

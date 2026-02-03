//! Unified Strict Filter Query Builder
//!
//! This module provides SQL query generation for the unified multi-dimensional
//! filtering system, composing tag, temporal, collection, security, and
//! semantic scope filters into efficient SQL queries.
//!
//! # Query Optimization
//!
//! The builder applies several optimizations:
//!
//! 1. **UUIDv7 Temporal Optimization**: Uses primary key range queries for
//!    created time filtering instead of separate timestamp columns.
//!
//! 2. **Recursive CTE for Collections**: Generates efficient recursive queries
//!    for hierarchical collection filtering.
//!
//! 3. **Parameterized Queries**: All values are parameterized to prevent SQL
//!    injection and enable prepared statement caching.
//!
//! 4. **Short-Circuit Evaluation**: Empty filter dimensions generate no SQL.

use chrono::{DateTime, Utc};

use matric_core::{
    MetadataFilter, SemanticScopeFilter, StrictCollectionFilter, StrictFilter,
    StrictSecurityFilter, StrictTagFilter, StrictTemporalFilter,
};

// Re-export QueryParam from strict_filter for consistency
pub use crate::strict_filter::QueryParam;

// =============================================================================
// UNIFIED FILTER QUERY BUILDER
// =============================================================================

/// Generates SQL WHERE clauses from a unified StrictFilter.
///
/// This builder composes multiple filter dimensions into a single SQL query,
/// optimizing for index usage and query performance.
///
/// # Example
///
/// ```rust,ignore
/// use matric_db::unified_filter::UnifiedFilterQueryBuilder;
/// use matric_core::{StrictFilter, StrictTagFilter, StrictTemporalFilter};
/// use matric_core::temporal::NamedTemporalRange;
/// use uuid::Uuid;
///
/// let filter = StrictFilter::new()
///     .with_tags(StrictTagFilter::new().require_concept(Uuid::nil()))
///     .with_temporal(StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek));
///
/// let builder = UnifiedFilterQueryBuilder::new(filter, 0);
/// let result = builder.build();
///
/// // result.where_clause: "n.id >= $1 AND n.id < $2 AND EXISTS (...)"
/// // result.cte_clause: None (no recursive collection query)
/// // result.params: [floor_uuid, ceiling_uuid, concept_id]
/// ```
pub struct UnifiedFilterQueryBuilder {
    filter: StrictFilter,
    param_offset: usize,
}

/// Result of building a unified filter query.
#[derive(Debug, Clone)]
pub struct UnifiedFilterResult {
    /// The WHERE clause fragment (without "WHERE" keyword).
    pub where_clause: String,

    /// Optional CTE clause for recursive collection queries.
    /// Should be prepended to the main query as "WITH RECURSIVE ...".
    pub cte_clause: Option<String>,

    /// Query parameters in the order they appear in the SQL.
    pub params: Vec<QueryParam>,

    /// Whether UUIDv7 temporal optimization was applied.
    pub used_uuid_temporal_opt: bool,

    /// Whether recursive CTE was generated.
    pub used_recursive_cte: bool,

    /// Number of active filter dimensions.
    pub active_dimensions: usize,
}

impl UnifiedFilterQueryBuilder {
    /// Create a new builder for the given unified filter.
    ///
    /// # Parameters
    ///
    /// * `filter` - The unified strict filter configuration
    /// * `param_offset` - Starting parameter index (number of parameters already in query)
    pub fn new(filter: StrictFilter, param_offset: usize) -> Self {
        Self {
            filter,
            param_offset,
        }
    }

    /// Build the complete filter query.
    ///
    /// Returns a `UnifiedFilterResult` containing:
    /// - WHERE clause fragment
    /// - Optional CTE clause for recursive queries
    /// - Query parameters
    /// - Optimization flags
    pub fn build(&self) -> UnifiedFilterResult {
        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = self.param_offset;
        let mut cte_clause = None;
        let mut used_uuid_temporal_opt = false;
        let mut used_recursive_cte = false;

        // =====================================================================
        // TEMPORAL FILTER (with UUIDv7 optimization)
        // =====================================================================
        if let Some(ref temporal) = self.filter.temporal {
            let (temporal_clauses, temporal_params, next_idx, uuid_opt) =
                self.build_temporal_filter(temporal, param_idx);
            clauses.extend(temporal_clauses);
            params.extend(temporal_params);
            param_idx = next_idx;
            used_uuid_temporal_opt = uuid_opt;
        }

        // =====================================================================
        // COLLECTION FILTER (with optional recursive CTE)
        // =====================================================================
        if let Some(ref collections) = self.filter.collections {
            let (coll_clauses, coll_params, next_idx, cte, recursive) =
                self.build_collection_filter(collections, param_idx);
            clauses.extend(coll_clauses);
            params.extend(coll_params);
            param_idx = next_idx;
            if let Some(cte_sql) = cte {
                cte_clause = Some(cte_sql);
                used_recursive_cte = recursive;
            }
        }

        // =====================================================================
        // TAG FILTER (delegated to existing StrictFilterQueryBuilder)
        // =====================================================================
        if let Some(ref tags) = self.filter.tags {
            let (tag_clauses, tag_params, next_idx) = self.build_tag_filter(tags, param_idx);
            clauses.extend(tag_clauses);
            params.extend(tag_params);
            param_idx = next_idx;
        }

        // =====================================================================
        // SECURITY FILTER
        // =====================================================================
        if let Some(ref security) = self.filter.security {
            let (sec_clauses, sec_params, next_idx) =
                self.build_security_filter(security, param_idx);
            clauses.extend(sec_clauses);
            params.extend(sec_params);
            param_idx = next_idx;
        }

        // =====================================================================
        // SEMANTIC SCOPE FILTER
        // =====================================================================
        if let Some(ref scope) = self.filter.semantic_scope {
            let (scope_clauses, scope_params, next_idx) =
                self.build_semantic_scope_filter(scope, param_idx);
            clauses.extend(scope_clauses);
            params.extend(scope_params);
            param_idx = next_idx;
        }

        // =====================================================================
        // METADATA FILTER
        // =====================================================================
        if let Some(ref metadata) = self.filter.metadata {
            let (meta_clauses, meta_params, next_idx) =
                self.build_metadata_filter(metadata, param_idx);
            clauses.extend(meta_clauses);
            params.extend(meta_params);
            let _ = next_idx; // Silence unused warning
        }

        // Generate final WHERE clause
        let where_clause = if clauses.is_empty() {
            "TRUE".to_string()
        } else {
            clauses.join(" AND ")
        };

        UnifiedFilterResult {
            where_clause,
            cte_clause,
            params,
            used_uuid_temporal_opt,
            used_recursive_cte,
            active_dimensions: self.filter.active_dimension_count(),
        }
    }

    // =========================================================================
    // TEMPORAL FILTER BUILDER
    // =========================================================================

    fn build_temporal_filter(
        &self,
        temporal: &StrictTemporalFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize, bool) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut used_uuid_opt = false;

        // Created time constraints (can use UUIDv7 optimization)
        if temporal.has_created_constraints() {
            let (after, before) = temporal.get_created_boundaries();

            // Use UUIDv7 optimization: filter on id column instead of created_at_utc
            if after.is_some() || before.is_some() {
                used_uuid_opt = true;

                if let Some(after_time) = after {
                    param_idx += 1;
                    clauses.push(format!("n.id >= ${}", param_idx));
                    let floor_uuid = matric_core::uuid_utils::v7_from_timestamp(&after_time);
                    params.push(QueryParam::Uuid(floor_uuid));
                }

                if let Some(before_time) = before {
                    param_idx += 1;
                    clauses.push(format!("n.id < ${}", param_idx));
                    let ceiling_uuid =
                        matric_core::uuid_utils::v7_ceiling_from_timestamp(&before_time);
                    params.push(QueryParam::Uuid(ceiling_uuid));
                }
            }
        }

        // Updated time constraints (uses updated_at_utc column)
        if temporal.has_updated_constraints() {
            let (after, before) = temporal.get_updated_boundaries();

            if let Some(after_time) = after {
                param_idx += 1;
                clauses.push(format!("n.updated_at_utc >= ${}", param_idx));
                params.push(QueryParam::Timestamp(after_time));
            }

            if let Some(before_time) = before {
                param_idx += 1;
                clauses.push(format!("n.updated_at_utc < ${}", param_idx));
                params.push(QueryParam::Timestamp(before_time));
            }
        }

        // Accessed time constraints (uses last_accessed_at column)
        if temporal.has_accessed_constraints() {
            let (after, before) = temporal.get_accessed_boundaries();

            // Handle include_never_accessed flag
            if !temporal.include_never_accessed {
                clauses.push("n.last_accessed_at IS NOT NULL".to_string());
            }

            if let Some(after_time) = after {
                param_idx += 1;
                if temporal.include_never_accessed {
                    clauses.push(format!(
                        "(n.last_accessed_at IS NULL OR n.last_accessed_at >= ${})",
                        param_idx
                    ));
                } else {
                    clauses.push(format!("n.last_accessed_at >= ${}", param_idx));
                }
                params.push(QueryParam::Timestamp(after_time));
            }

            if let Some(before_time) = before {
                param_idx += 1;
                if temporal.include_never_accessed {
                    clauses.push(format!(
                        "(n.last_accessed_at IS NULL OR n.last_accessed_at < ${})",
                        param_idx
                    ));
                } else {
                    clauses.push(format!("n.last_accessed_at < ${}", param_idx));
                }
                params.push(QueryParam::Timestamp(before_time));
            }
        }

        (clauses, params, param_idx, used_uuid_opt)
    }

    // =========================================================================
    // COLLECTION FILTER BUILDER
    // =========================================================================

    fn build_collection_filter(
        &self,
        collections: &StrictCollectionFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize, Option<String>, bool) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut cte = None;
        let recursive = collections.requires_recursive_query();

        let inclusion_ids = collections.get_inclusion_ids();

        if !inclusion_ids.is_empty() {
            if recursive {
                // Generate recursive CTE for descendant inclusion
                param_idx += 1;
                let cte_param_idx = param_idx;

                cte = Some(format!(
                    r#"collection_tree AS (
    SELECT id FROM collection WHERE id = ANY(${}::uuid[])
    UNION ALL
    SELECT c.id FROM collection c
    JOIN collection_tree ct ON c.parent_id = ct.id
)"#,
                    cte_param_idx
                ));

                clauses.push(
                    "(n.collection_id IN (SELECT id FROM collection_tree) OR n.collection_id IS NULL)"
                        .to_string(),
                );
                params.push(QueryParam::UuidArray(inclusion_ids));

                // Override if uncategorized should be excluded
                if !collections.include_uncategorized {
                    clauses.pop();
                    clauses.push("n.collection_id IN (SELECT id FROM collection_tree)".to_string());
                }
            } else {
                // Simple collection filtering without recursion
                param_idx += 1;

                if collections.include_uncategorized {
                    clauses.push(format!(
                        "(n.collection_id = ANY(${}::uuid[]) OR n.collection_id IS NULL)",
                        param_idx
                    ));
                } else {
                    clauses.push(format!("n.collection_id = ANY(${}::uuid[])", param_idx));
                }
                params.push(QueryParam::UuidArray(inclusion_ids));
            }
        } else if !collections.include_uncategorized {
            // No specific collections, but exclude uncategorized
            clauses.push("n.collection_id IS NOT NULL".to_string());
        }

        // Excluded collections
        if !collections.excluded_collections.is_empty() {
            param_idx += 1;

            if recursive {
                // Excluded collections also need recursive handling
                // For simplicity, we just exclude exact matches (not descendants)
                // Full recursive exclusion would need another CTE
                clauses.push(format!(
                    "(n.collection_id IS NULL OR n.collection_id != ALL(${}::uuid[]))",
                    param_idx
                ));
            } else {
                clauses.push(format!(
                    "(n.collection_id IS NULL OR n.collection_id != ALL(${}::uuid[]))",
                    param_idx
                ));
            }
            params.push(QueryParam::UuidArray(
                collections.excluded_collections.clone(),
            ));
        }

        (clauses, params, param_idx, cte, recursive)
    }

    // =========================================================================
    // TAG FILTER BUILDER
    // =========================================================================

    fn build_tag_filter(
        &self,
        tags: &StrictTagFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        // Required concepts (AND): note must have ALL of these
        for concept_id in &tags.required_concepts {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ${})",
                param_idx
            ));
            params.push(QueryParam::Uuid(*concept_id));
        }

        // Any concepts (OR): note must have AT LEAST ONE
        if !tags.any_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(tags.any_concepts.clone()));
        }

        // Excluded concepts (NOT): note must have NONE of these
        if !tags.excluded_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(tags.excluded_concepts.clone()));
        }

        // Required schemes (isolation)
        if !tags.required_schemes.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "(EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[])) AND NOT EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id != ALL(${}::uuid[])))",
                param_idx, param_idx
            ));
            params.push(QueryParam::UuidArray(tags.required_schemes.clone()));
        }

        // Excluded schemes
        if !tags.excluded_schemes.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(tags.excluded_schemes.clone()));
        }

        // Minimum tag count
        if let Some(min_count) = tags.min_tag_count {
            param_idx += 1;
            clauses.push(format!(
                "(SELECT COUNT(*) FROM note_skos_concept nsc WHERE nsc.note_id = n.id) >= ${}",
                param_idx
            ));
            params.push(QueryParam::Int(min_count));
        }

        // Untagged notes handling
        if !tags.include_untagged
            && tags.required_concepts.is_empty()
            && tags.any_concepts.is_empty()
        {
            clauses.push(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id)".to_string(),
            );
        }

        (clauses, params, param_idx)
    }

    // =========================================================================
    // SECURITY FILTER BUILDER
    // =========================================================================

    fn build_security_filter(
        &self,
        security: &StrictSecurityFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        // Owner filter
        if let Some(owner_id) = security.owner_id {
            if security.include_shared && security.shared_with_user.is_some() {
                // Include owned OR shared (with active grant check)
                param_idx += 1;
                let owner_param = param_idx;
                param_idx += 1;
                let shared_param = param_idx;

                // Check for active share grants: not revoked and not expired
                clauses.push(format!(
                    "(n.owner_id = ${} OR EXISTS (SELECT 1 FROM note_share_grant nsg WHERE nsg.note_id = n.id AND nsg.grantee_id = ${} AND nsg.revoked_at IS NULL AND (nsg.expires_at IS NULL OR nsg.expires_at > NOW())))",
                    owner_param, shared_param
                ));
                params.push(QueryParam::Uuid(owner_id));
                params.push(QueryParam::Uuid(security.shared_with_user.unwrap()));
            } else {
                param_idx += 1;
                clauses.push(format!("n.owner_id = ${}", param_idx));
                params.push(QueryParam::Uuid(owner_id));
            }
        }

        // Tenant filter
        if let Some(tenant_id) = security.tenant_id {
            param_idx += 1;
            clauses.push(format!("n.tenant_id = ${}", param_idx));
            params.push(QueryParam::Uuid(tenant_id));
        }

        // Visibility filter
        if !security.visibility.is_empty() {
            let vis_strings: Vec<String> = security
                .visibility
                .iter()
                .map(|v| match v {
                    matric_core::Visibility::Private => "'private'".to_string(),
                    matric_core::Visibility::Shared => "'shared'".to_string(),
                    matric_core::Visibility::Internal => "'internal'".to_string(),
                    matric_core::Visibility::Public => "'public'".to_string(),
                })
                .collect();

            clauses.push(format!("n.visibility IN ({})", vis_strings.join(", ")));
        }

        (clauses, params, param_idx)
    }

    // =========================================================================
    // SEMANTIC SCOPE FILTER BUILDER
    // =========================================================================

    fn build_semantic_scope_filter(
        &self,
        scope: &SemanticScopeFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        // Exact embedding set
        if let Some(set_id) = scope.embedding_set_id {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM embedding_set_member esm WHERE esm.note_id = n.id AND esm.set_id = ${})",
                param_idx
            ));
            params.push(QueryParam::Uuid(set_id));
        } else if !scope.any_embedding_sets.is_empty() {
            // Any embedding sets (OR logic)
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM embedding_set_member esm WHERE esm.note_id = n.id AND esm.set_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(scope.any_embedding_sets.clone()));
        }

        // Excluded embedding sets
        if !scope.excluded_embedding_sets.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM embedding_set_member esm WHERE esm.note_id = n.id AND esm.set_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(scope.excluded_embedding_sets.clone()));
        }

        (clauses, params, param_idx)
    }

    // =========================================================================
    // METADATA FILTER BUILDER
    // =========================================================================

    fn build_metadata_filter(
        &self,
        metadata: &MetadataFilter,
        mut param_idx: usize,
    ) -> (Vec<String>, Vec<QueryParam>, usize) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        // Starred filter
        if let Some(starred) = metadata.starred {
            param_idx += 1;
            clauses.push(format!("n.starred = ${}", param_idx));
            params.push(QueryParam::Bool(starred));
        }

        // Archived filter
        if let Some(archived) = metadata.archived {
            param_idx += 1;
            clauses.push(format!("n.archived = ${}", param_idx));
            params.push(QueryParam::Bool(archived));
        }

        // Format filter
        if let Some(ref format) = metadata.format {
            param_idx += 1;
            clauses.push(format!("n.format = ${}", param_idx));
            params.push(QueryParam::String(format.clone()));
        }

        // Source filter
        if let Some(ref source) = metadata.source {
            param_idx += 1;
            clauses.push(format!("n.source = ${}", param_idx));
            params.push(QueryParam::String(source.clone()));
        }

        // Deleted filter
        if !metadata.include_deleted {
            clauses.push("n.deleted_at IS NULL".to_string());
        }

        // Minimum access count
        if let Some(min_count) = metadata.min_access_count {
            param_idx += 1;
            clauses.push(format!("n.access_count >= ${}", param_idx));
            params.push(QueryParam::Int(min_count));
        }

        (clauses, params, param_idx)
    }
}

// =============================================================================
// EXTENDED QUERY PARAM
// =============================================================================

// Extend QueryParam to support additional types
impl QueryParam {
    /// Create a timestamp parameter.
    pub fn timestamp(ts: DateTime<Utc>) -> Self {
        QueryParam::Timestamp(ts)
    }

    /// Create a boolean parameter.
    pub fn bool(val: bool) -> Self {
        QueryParam::Bool(val)
    }

    /// Create a string parameter.
    pub fn string(val: impl Into<String>) -> Self {
        QueryParam::String(val.into())
    }
}

// Add new variants to QueryParam (defined in strict_filter.rs)
// We need to update the original definition

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::temporal::NamedTemporalRange;
    use uuid::Uuid;

    #[test]
    fn test_empty_filter() {
        let filter = StrictFilter::new();
        let builder = UnifiedFilterQueryBuilder::new(filter, 0);
        let result = builder.build();

        assert_eq!(result.where_clause, "TRUE");
        assert!(result.params.is_empty());
        assert!(result.cte_clause.is_none());
        assert_eq!(result.active_dimensions, 0);
    }

    #[test]
    fn test_temporal_uuid_optimization() {
        let filter = StrictFilter::new().with_temporal(
            StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek),
        );

        let builder = UnifiedFilterQueryBuilder::new(filter, 0);
        let result = builder.build();

        assert!(result.used_uuid_temporal_opt);
        assert!(result.where_clause.contains("n.id >="));
        assert!(result.where_clause.contains("n.id <"));
        assert_eq!(result.params.len(), 2); // floor and ceiling UUIDs
    }

    #[test]
    fn test_collection_recursive_cte() {
        let filter = StrictFilter::new().with_collections(
            StrictCollectionFilter::new()
                .in_collection(Uuid::new_v4())
                .with_descendants(true),
        );

        let builder = UnifiedFilterQueryBuilder::new(filter, 0);
        let result = builder.build();

        assert!(result.used_recursive_cte);
        assert!(result.cte_clause.is_some());
        assert!(result.cte_clause.unwrap().contains("UNION ALL"));
    }

    #[test]
    fn test_multi_dimension_filter() {
        let filter = StrictFilter::new()
            .with_tags(StrictTagFilter::new().require_concept(Uuid::new_v4()))
            .with_temporal(
                StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisMonth),
            )
            .with_collections(StrictCollectionFilter::new().in_collection(Uuid::new_v4()))
            .with_metadata(MetadataFilter::new().starred_only().exclude_archived());

        let builder = UnifiedFilterQueryBuilder::new(filter, 0);
        let result = builder.build();

        assert_eq!(result.active_dimensions, 4);
        assert!(!result.where_clause.is_empty());
        assert!(result.where_clause.contains("n.id >="));
        assert!(result.where_clause.contains("EXISTS"));
        assert!(result.where_clause.contains("n.starred"));
    }

    #[test]
    fn test_param_offset() {
        let filter =
            StrictFilter::new().with_tags(StrictTagFilter::new().require_concept(Uuid::new_v4()));

        // Start with offset 5 (as if there are already 5 params in the query)
        let builder = UnifiedFilterQueryBuilder::new(filter, 5);
        let result = builder.build();

        // First tag param should be $6
        assert!(result.where_clause.contains("$6"));
    }
}

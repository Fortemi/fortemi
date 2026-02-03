//! Collection filtering types for the Unified Strict Filter system.
//!
//! This module provides hierarchical collection-based filtering that supports:
//! - Single collection filtering
//! - Multiple collection filtering (OR logic)
//! - Recursive descendant inclusion
//! - Collection exclusion
//!
//! # Recursive Collection Queries
//!
//! Collections in matric-memory support hierarchical organization. The
//! `include_descendants` option enables queries that automatically include
//! notes from all child collections, implemented via PostgreSQL recursive CTE:
//!
//! ```sql
//! WITH RECURSIVE collection_tree AS (
//!     SELECT id FROM collection WHERE id = ANY($1)
//!     UNION ALL
//!     SELECT c.id FROM collection c
//!     JOIN collection_tree ct ON c.parent_id = ct.id
//! )
//! SELECT * FROM note WHERE collection_id IN (SELECT id FROM collection_tree)
//! ```

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// STRICT COLLECTION FILTER
// =============================================================================

/// Strict collection filter for hierarchical organization-based filtering.
///
/// This filter supports:
/// - `any_collections`: OR logic - notes must be in ANY of these collections
/// - `excluded_collections`: NOT logic - notes must NOT be in ANY of these
/// - `include_descendants`: Whether to include child collections recursively
/// - `include_uncategorized`: Whether to include notes without a collection
///
/// # Example
///
/// ```
/// use matric_core::StrictCollectionFilter;
/// use uuid::Uuid;
///
/// // Find notes in "projects" or "archive" collections
/// let projects_id = Uuid::nil(); // placeholder
/// let archive_id = Uuid::nil();  // placeholder
///
/// let filter = StrictCollectionFilter::new()
///     .in_collection(projects_id)
///     .in_collection(archive_id)
///     .with_descendants(true);
/// ```
///
/// # Collection Hierarchy
///
/// When `include_descendants` is true, the filter will include notes from:
/// - The specified collections
/// - All child collections (recursively)
///
/// This is useful for queries like "all notes in the 'Work' folder and subfolders".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrictCollectionFilter {
    /// Collections to include (OR logic) - notes must be in ANY of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_collections: Vec<Uuid>,

    /// Collections to exclude (NOT logic) - notes must NOT be in ANY of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_collections: Vec<Uuid>,

    /// Whether to include descendant collections recursively.
    /// Default: false (exact collection match only).
    #[serde(default)]
    pub include_descendants: bool,

    /// Whether to include notes without a collection assignment.
    /// Default: true (include uncategorized notes).
    #[serde(default = "default_true")]
    pub include_uncategorized: bool,

    /// Require notes to be in a specific collection (exact match, no OR logic).
    /// This takes precedence over `any_collections` when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_collection: Option<Uuid>,
}

fn default_true() -> bool {
    true
}

impl StrictCollectionFilter {
    /// Create a new empty collection filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a collection to include (OR logic with other collections).
    pub fn in_collection(mut self, collection_id: Uuid) -> Self {
        self.any_collections.push(collection_id);
        self
    }

    /// Add multiple collections to include (OR logic).
    pub fn in_collections(mut self, collection_ids: impl IntoIterator<Item = Uuid>) -> Self {
        self.any_collections.extend(collection_ids);
        self
    }

    /// Set exact collection match (overrides `any_collections`).
    pub fn in_exact_collection(mut self, collection_id: Uuid) -> Self {
        self.exact_collection = Some(collection_id);
        self
    }

    /// Add a collection to exclude (NOT logic).
    pub fn exclude_collection(mut self, collection_id: Uuid) -> Self {
        self.excluded_collections.push(collection_id);
        self
    }

    /// Add multiple collections to exclude (NOT logic).
    pub fn exclude_collections(mut self, collection_ids: impl IntoIterator<Item = Uuid>) -> Self {
        self.excluded_collections.extend(collection_ids);
        self
    }

    /// Set whether to include descendant collections recursively.
    pub fn with_descendants(mut self, include: bool) -> Self {
        self.include_descendants = include;
        self
    }

    /// Set whether to include notes without a collection.
    pub fn with_uncategorized(mut self, include: bool) -> Self {
        self.include_uncategorized = include;
        self
    }

    /// Check if the filter is empty (no constraints).
    pub fn is_empty(&self) -> bool {
        self.any_collections.is_empty()
            && self.excluded_collections.is_empty()
            && self.exact_collection.is_none()
    }

    /// Check if the filter requires recursive CTE processing.
    pub fn requires_recursive_query(&self) -> bool {
        self.include_descendants
            && (!self.any_collections.is_empty() || self.exact_collection.is_some())
    }

    /// Get the effective collection IDs for inclusion.
    ///
    /// Returns `exact_collection` if set, otherwise `any_collections`.
    pub fn get_inclusion_ids(&self) -> Vec<Uuid> {
        if let Some(exact) = self.exact_collection {
            vec![exact]
        } else {
            self.any_collections.clone()
        }
    }

    /// Check if using exact collection match mode.
    pub fn is_exact_match(&self) -> bool {
        self.exact_collection.is_some()
    }
}

// =============================================================================
// COLLECTION PATH FILTER
// =============================================================================

/// Filter by collection path notation (e.g., "Work/Projects/Rust").
///
/// This provides a string-based alternative to UUID-based filtering,
/// useful for API endpoints and human-readable queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionPathFilter {
    /// Collection paths to include (OR logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_paths: Vec<String>,

    /// Collection paths to exclude (NOT logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_paths: Vec<String>,

    /// Whether to include descendant collections.
    #[serde(default)]
    pub include_descendants: bool,

    /// Whether to include uncategorized notes.
    #[serde(default = "default_true")]
    pub include_uncategorized: bool,
}

impl CollectionPathFilter {
    /// Create a new empty path filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a collection path to include.
    pub fn include_path(mut self, path: impl Into<String>) -> Self {
        self.include_paths.push(path.into());
        self
    }

    /// Add a collection path to exclude.
    pub fn exclude_path(mut self, path: impl Into<String>) -> Self {
        self.exclude_paths.push(path.into());
        self
    }

    /// Set whether to include descendants.
    pub fn with_descendants(mut self, include: bool) -> Self {
        self.include_descendants = include;
        self
    }

    /// Check if the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.include_paths.is_empty() && self.exclude_paths.is_empty()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_filter() {
        let filter = StrictCollectionFilter::new();
        assert!(filter.is_empty());
        assert!(!filter.requires_recursive_query());
    }

    #[test]
    fn test_single_collection() {
        let id = Uuid::new_v4();
        let filter = StrictCollectionFilter::new().in_collection(id);

        assert!(!filter.is_empty());
        assert_eq!(filter.any_collections, vec![id]);
    }

    #[test]
    fn test_multiple_collections() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let filter = StrictCollectionFilter::new()
            .in_collection(id1)
            .in_collection(id2);

        assert_eq!(filter.any_collections.len(), 2);
    }

    #[test]
    fn test_exact_collection_overrides() {
        let id1 = Uuid::new_v4();
        let exact_id = Uuid::new_v4();

        let filter = StrictCollectionFilter::new()
            .in_collection(id1)
            .in_exact_collection(exact_id);

        assert!(filter.is_exact_match());
        assert_eq!(filter.get_inclusion_ids(), vec![exact_id]);
    }

    #[test]
    fn test_recursive_query_detection() {
        let id = Uuid::new_v4();

        // Without descendants - no recursive query needed
        let filter = StrictCollectionFilter::new().in_collection(id);
        assert!(!filter.requires_recursive_query());

        // With descendants - recursive query needed
        let filter = filter.with_descendants(true);
        assert!(filter.requires_recursive_query());
    }

    #[test]
    fn test_exclusion() {
        let include_id = Uuid::new_v4();
        let exclude_id = Uuid::new_v4();

        let filter = StrictCollectionFilter::new()
            .in_collection(include_id)
            .exclude_collection(exclude_id);

        assert_eq!(filter.any_collections.len(), 1);
        assert_eq!(filter.excluded_collections.len(), 1);
    }

    #[test]
    fn test_builder_pattern() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let filter = StrictCollectionFilter::new()
            .in_collections(vec![id1, id2])
            .with_descendants(true)
            .with_uncategorized(false);

        assert_eq!(filter.any_collections.len(), 2);
        assert!(filter.include_descendants);
        assert!(!filter.include_uncategorized);
    }

    #[test]
    fn test_path_filter() {
        let filter = CollectionPathFilter::new()
            .include_path("Work/Projects")
            .exclude_path("Work/Archive")
            .with_descendants(true);

        assert!(!filter.is_empty());
        assert_eq!(filter.include_paths.len(), 1);
        assert_eq!(filter.exclude_paths.len(), 1);
        assert!(filter.include_descendants);
    }
}

//! Search filtering types for issue #146.
//!
//! This module provides strict tag filtering support for note search operations,
//! using SKOS concepts for precise taxonomy-based filtering.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// STRICT TAG FILTER (UUID-BASED)
// =============================================================================

/// Strict tag filter using SKOS concept UUIDs for precise filtering.
///
/// This filter operates at the database query level using SKOS concept IDs,
/// providing exact control over which notes are included in search results.
///
/// # Filtering Logic
///
/// - `required_concepts`: AND logic - notes MUST have ALL of these concepts
/// - `any_concepts`: OR logic - notes MUST have AT LEAST ONE of these concepts
/// - `excluded_concepts`: NOT logic - notes MUST NOT have ANY of these concepts
/// - `required_schemes`: Scheme isolation - notes MUST have concepts from ALL these schemes
/// - `excluded_schemes`: Scheme exclusion - notes MUST NOT have concepts from ANY of these schemes
/// - `min_tag_count`: Minimum number of tags required (None = no minimum)
/// - `include_untagged`: Whether to include notes with no tags (default: true)
///
/// # Example
///
/// ```
/// use matric_core::StrictTagFilter;
/// use uuid::Uuid;
///
/// // Find notes tagged with "rust" AND ("programming" OR "tutorial")
/// // but NOT "archive"
/// let filter = StrictTagFilter::new()
///     .require_concept(Uuid::new_v4())
///     .any_concept(Uuid::new_v4())
///     .any_concept(Uuid::new_v4())
///     .exclude_concept(Uuid::new_v4());
///
/// assert!(!filter.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StrictTagFilter {
    /// Required concepts (AND logic) - must have ALL.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_concepts: Vec<Uuid>,

    /// Any concepts (OR logic) - must have AT LEAST ONE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_concepts: Vec<Uuid>,

    /// Excluded concepts (NOT logic) - must NOT have ANY.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_concepts: Vec<Uuid>,

    /// Required schemes (scheme isolation) - must have concepts from ALL schemes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_schemes: Vec<Uuid>,

    /// Excluded schemes (scheme exclusion) - must NOT have concepts from ANY schemes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_schemes: Vec<Uuid>,

    /// Required simple string tags (AND logic) - must have ALL.
    /// These are matched against the note_tag table, not SKOS concepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_string_tags: Vec<String>,

    /// Any simple string tags (OR logic) - must have AT LEAST ONE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_string_tags: Vec<String>,

    /// Excluded simple string tags (NOT logic) - must NOT have ANY.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_string_tags: Vec<String>,

    /// Minimum number of tags required (None = no minimum).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tag_count: Option<i32>,

    /// Whether to include notes with no tags (default: true).
    #[serde(default = "default_true")]
    pub include_untagged: bool,

    /// When true, the filter is unsatisfiable (e.g. any_tags requested but none
    /// resolved). Search should return empty results immediately.
    #[serde(default, skip_serializing)]
    pub match_none: bool,
}

fn default_true() -> bool {
    true
}

impl Default for StrictTagFilter {
    fn default() -> Self {
        Self {
            required_concepts: Vec::new(),
            any_concepts: Vec::new(),
            excluded_concepts: Vec::new(),
            required_schemes: Vec::new(),
            excluded_schemes: Vec::new(),
            required_string_tags: Vec::new(),
            any_string_tags: Vec::new(),
            excluded_string_tags: Vec::new(),
            min_tag_count: None,
            include_untagged: true,
            match_none: false,
        }
    }
}

impl StrictTagFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a required concept (AND logic).
    pub fn require_concept(mut self, concept_id: Uuid) -> Self {
        self.required_concepts.push(concept_id);
        self
    }

    /// Add an "any" concept (OR logic).
    pub fn any_concept(mut self, concept_id: Uuid) -> Self {
        self.any_concepts.push(concept_id);
        self
    }

    /// Add an excluded concept (NOT logic).
    pub fn exclude_concept(mut self, concept_id: Uuid) -> Self {
        self.excluded_concepts.push(concept_id);
        self
    }

    /// Add a required scheme (scheme isolation).
    pub fn require_scheme(mut self, scheme_id: Uuid) -> Self {
        self.required_schemes.push(scheme_id);
        self
    }

    /// Add an excluded scheme (scheme exclusion).
    pub fn exclude_scheme(mut self, scheme_id: Uuid) -> Self {
        self.excluded_schemes.push(scheme_id);
        self
    }

    /// Set minimum tag count.
    pub fn with_min_tag_count(mut self, count: i32) -> Self {
        self.min_tag_count = Some(count);
        self
    }

    /// Set whether to include untagged notes.
    pub fn with_include_untagged(mut self, include: bool) -> Self {
        self.include_untagged = include;
        self
    }

    /// Check if the filter is empty (no constraints).
    pub fn is_empty(&self) -> bool {
        self.required_concepts.is_empty()
            && self.any_concepts.is_empty()
            && self.excluded_concepts.is_empty()
            && self.required_schemes.is_empty()
            && self.excluded_schemes.is_empty()
            && self.required_string_tags.is_empty()
            && self.any_string_tags.is_empty()
            && self.excluded_string_tags.is_empty()
            && self.min_tag_count.is_none()
    }

    /// Check if the filter has scheme-level constraints.
    pub fn has_scheme_filter(&self) -> bool {
        !self.required_schemes.is_empty() || !self.excluded_schemes.is_empty()
    }
}

// =============================================================================
// STRICT TAG FILTER INPUT (NOTATION-BASED)
// =============================================================================

/// API input for strict tag filtering using notation strings.
///
/// This is the user-facing API format that accepts notation strings
/// (e.g., "programming/rust", "topics:machine-learning") which are
/// resolved to UUIDs before database queries.
///
/// # Notation Format
///
/// - Simple: `"rust"` - matches concept with notation "rust" in default scheme
/// - Hierarchical: `"programming/rust"` - matches hierarchical path
/// - Scheme-qualified: `"topics:machine-learning"` - explicit scheme namespace
///
/// # Example
///
/// ```json
/// {
///   "required_tags": ["programming/rust"],
///   "any_tags": ["tutorial", "guide"],
///   "excluded_tags": ["archive", "draft"],
///   "required_schemes": ["topics"],
///   "min_tag_count": 2,
///   "include_untagged": false
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StrictTagFilterInput {
    /// Required tag notations (AND logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tags: Vec<String>,

    /// Any tag notations (OR logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_tags: Vec<String>,

    /// Excluded tag notations (NOT logic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_tags: Vec<String>,

    /// Required scheme notations (scheme isolation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_schemes: Vec<String>,

    /// Excluded scheme notations (scheme exclusion).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_schemes: Vec<String>,

    /// Minimum number of tags required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tag_count: Option<i32>,

    /// Whether to include notes with no tags (default: true).
    #[serde(default = "default_true")]
    pub include_untagged: bool,
}

impl Default for StrictTagFilterInput {
    fn default() -> Self {
        Self {
            required_tags: Vec::new(),
            any_tags: Vec::new(),
            excluded_tags: Vec::new(),
            required_schemes: Vec::new(),
            excluded_schemes: Vec::new(),
            min_tag_count: None,
            include_untagged: true,
        }
    }
}

impl StrictTagFilterInput {
    /// Create a new empty filter input.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the filter input is empty (no constraints).
    pub fn is_empty(&self) -> bool {
        self.required_tags.is_empty()
            && self.any_tags.is_empty()
            && self.excluded_tags.is_empty()
            && self.required_schemes.is_empty()
            && self.excluded_schemes.is_empty()
            && self.min_tag_count.is_none()
    }

    /// Check if the filter has scheme-level constraints.
    pub fn has_scheme_filter(&self) -> bool {
        !self.required_schemes.is_empty() || !self.excluded_schemes.is_empty()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // StrictTagFilter Tests
    // =========================================================================

    #[test]
    fn test_strict_tag_filter_new() {
        let filter = StrictTagFilter::new();
        assert!(filter.is_empty());
        assert!(!filter.has_scheme_filter());
        assert!(filter.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_default() {
        let filter = StrictTagFilter::default();
        assert!(filter.is_empty());
        assert!(filter.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_builder_required() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let filter = StrictTagFilter::new()
            .require_concept(id1)
            .require_concept(id2);

        assert!(!filter.is_empty());
        assert_eq!(filter.required_concepts.len(), 2);
        assert!(filter.required_concepts.contains(&id1));
        assert!(filter.required_concepts.contains(&id2));
    }

    #[test]
    fn test_strict_tag_filter_builder_any() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let filter = StrictTagFilter::new().any_concept(id1).any_concept(id2);

        assert!(!filter.is_empty());
        assert_eq!(filter.any_concepts.len(), 2);
        assert!(filter.any_concepts.contains(&id1));
        assert!(filter.any_concepts.contains(&id2));
    }

    #[test]
    fn test_strict_tag_filter_builder_excluded() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let filter = StrictTagFilter::new()
            .exclude_concept(id1)
            .exclude_concept(id2);

        assert!(!filter.is_empty());
        assert_eq!(filter.excluded_concepts.len(), 2);
        assert!(filter.excluded_concepts.contains(&id1));
        assert!(filter.excluded_concepts.contains(&id2));
    }

    #[test]
    fn test_strict_tag_filter_builder_schemes() {
        let scheme1 = Uuid::new_v4();
        let scheme2 = Uuid::new_v4();
        let filter = StrictTagFilter::new()
            .require_scheme(scheme1)
            .exclude_scheme(scheme2);

        assert!(!filter.is_empty());
        assert!(filter.has_scheme_filter());
        assert_eq!(filter.required_schemes.len(), 1);
        assert_eq!(filter.excluded_schemes.len(), 1);
        assert!(filter.required_schemes.contains(&scheme1));
        assert!(filter.excluded_schemes.contains(&scheme2));
    }

    #[test]
    fn test_strict_tag_filter_builder_min_tag_count() {
        let filter = StrictTagFilter::new().with_min_tag_count(3);

        assert!(!filter.is_empty());
        assert_eq!(filter.min_tag_count, Some(3));
    }

    #[test]
    fn test_strict_tag_filter_builder_include_untagged() {
        let filter = StrictTagFilter::new().with_include_untagged(false);

        assert!(filter.is_empty()); // No actual filters, just untagged flag
        assert!(!filter.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_combined() {
        let required = Uuid::new_v4();
        let any1 = Uuid::new_v4();
        let any2 = Uuid::new_v4();
        let excluded = Uuid::new_v4();
        let scheme = Uuid::new_v4();

        let filter = StrictTagFilter::new()
            .require_concept(required)
            .any_concept(any1)
            .any_concept(any2)
            .exclude_concept(excluded)
            .require_scheme(scheme)
            .with_min_tag_count(2)
            .with_include_untagged(false);

        assert!(!filter.is_empty());
        assert!(filter.has_scheme_filter());
        assert_eq!(filter.required_concepts.len(), 1);
        assert_eq!(filter.any_concepts.len(), 2);
        assert_eq!(filter.excluded_concepts.len(), 1);
        assert_eq!(filter.required_schemes.len(), 1);
        assert_eq!(filter.min_tag_count, Some(2));
        assert!(!filter.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_is_empty_with_only_untagged() {
        // Just setting include_untagged doesn't make it non-empty
        let filter = StrictTagFilter::new().with_include_untagged(false);
        assert!(filter.is_empty());
    }

    #[test]
    fn test_strict_tag_filter_serialization() {
        let filter = StrictTagFilter::new()
            .require_concept(Uuid::new_v4())
            .any_concept(Uuid::new_v4())
            .exclude_concept(Uuid::new_v4())
            .with_min_tag_count(2)
            .with_include_untagged(false);

        let json = serde_json::to_string(&filter).unwrap();
        let deserialized: StrictTagFilter = serde_json::from_str(&json).unwrap();

        assert_eq!(
            filter.required_concepts.len(),
            deserialized.required_concepts.len()
        );
        assert_eq!(filter.any_concepts.len(), deserialized.any_concepts.len());
        assert_eq!(
            filter.excluded_concepts.len(),
            deserialized.excluded_concepts.len()
        );
        assert_eq!(filter.min_tag_count, deserialized.min_tag_count);
        assert_eq!(filter.include_untagged, deserialized.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_skip_serializing_empty() {
        let filter = StrictTagFilter::new();
        let json = serde_json::to_value(&filter).unwrap();
        let obj = json.as_object().unwrap();

        // Empty vecs should be skipped
        assert!(!obj.contains_key("required_concepts"));
        assert!(!obj.contains_key("any_concepts"));
        assert!(!obj.contains_key("excluded_concepts"));
        assert!(!obj.contains_key("required_schemes"));
        assert!(!obj.contains_key("excluded_schemes"));
        assert!(!obj.contains_key("min_tag_count"));
        // include_untagged is true by default, should be present
        assert!(obj.contains_key("include_untagged"));
    }

    // =========================================================================
    // StrictTagFilterInput Tests
    // =========================================================================

    #[test]
    fn test_strict_tag_filter_input_new() {
        let input = StrictTagFilterInput::new();
        assert!(input.is_empty());
        assert!(!input.has_scheme_filter());
        assert!(input.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_input_default() {
        let input = StrictTagFilterInput::default();
        assert!(input.is_empty());
        assert!(input.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_input_is_empty() {
        let mut input = StrictTagFilterInput::new();
        assert!(input.is_empty());

        input.required_tags.push("rust".to_string());
        assert!(!input.is_empty());
    }

    #[test]
    fn test_strict_tag_filter_input_has_scheme_filter() {
        let mut input = StrictTagFilterInput::new();
        assert!(!input.has_scheme_filter());

        input.required_schemes.push("topics".to_string());
        assert!(input.has_scheme_filter());

        let mut input2 = StrictTagFilterInput::new();
        input2.excluded_schemes.push("archive".to_string());
        assert!(input2.has_scheme_filter());
    }

    #[test]
    fn test_strict_tag_filter_input_serialization() {
        let input = StrictTagFilterInput {
            required_tags: vec!["programming/rust".to_string()],
            any_tags: vec!["tutorial".to_string(), "guide".to_string()],
            excluded_tags: vec!["archive".to_string()],
            required_schemes: vec!["topics".to_string()],
            excluded_schemes: vec![],
            min_tag_count: Some(2),
            include_untagged: false,
        };

        let json = serde_json::to_string(&input).unwrap();
        let deserialized: StrictTagFilterInput = serde_json::from_str(&json).unwrap();

        assert_eq!(input.required_tags, deserialized.required_tags);
        assert_eq!(input.any_tags, deserialized.any_tags);
        assert_eq!(input.excluded_tags, deserialized.excluded_tags);
        assert_eq!(input.required_schemes, deserialized.required_schemes);
        assert_eq!(input.excluded_schemes, deserialized.excluded_schemes);
        assert_eq!(input.min_tag_count, deserialized.min_tag_count);
        assert_eq!(input.include_untagged, deserialized.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_input_skip_serializing_empty() {
        let input = StrictTagFilterInput::new();
        let json = serde_json::to_value(&input).unwrap();
        let obj = json.as_object().unwrap();

        // Empty vecs should be skipped
        assert!(!obj.contains_key("required_tags"));
        assert!(!obj.contains_key("any_tags"));
        assert!(!obj.contains_key("excluded_tags"));
        assert!(!obj.contains_key("required_schemes"));
        assert!(!obj.contains_key("excluded_schemes"));
        assert!(!obj.contains_key("min_tag_count"));
        // include_untagged is true by default, should be present
        assert!(obj.contains_key("include_untagged"));
    }

    #[test]
    fn test_strict_tag_filter_input_json_deserialization() {
        let json = r#"{
            "required_tags": ["programming/rust"],
            "any_tags": ["tutorial", "guide"],
            "excluded_tags": ["archive"],
            "min_tag_count": 2,
            "include_untagged": false
        }"#;

        let input: StrictTagFilterInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.required_tags, vec!["programming/rust"]);
        assert_eq!(input.any_tags, vec!["tutorial", "guide"]);
        assert_eq!(input.excluded_tags, vec!["archive"]);
        assert_eq!(input.min_tag_count, Some(2));
        assert!(!input.include_untagged);
    }

    #[test]
    fn test_strict_tag_filter_input_default_include_untagged() {
        let json = r#"{
            "required_tags": ["rust"]
        }"#;

        let input: StrictTagFilterInput = serde_json::from_str(json).unwrap();
        // Should default to true
        assert!(input.include_untagged);
    }
}

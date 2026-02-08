//! Strict tag filter query builder for guaranteed result segregation.
//!
//! This module provides SQL query generation for strict SKOS concept filtering,
//! enabling precise control over which notes are included in search results.

use uuid::Uuid;

/// Type-safe parameter binding for SQL queries.
#[derive(Debug, Clone)]
pub enum QueryParam {
    /// Single UUID parameter.
    Uuid(Uuid),
    /// Array of UUIDs (for ANY/ALL operations).
    UuidArray(Vec<Uuid>),
    /// Integer parameter.
    Int(i32),
    /// Timestamp parameter.
    Timestamp(chrono::DateTime<chrono::Utc>),
    /// Boolean parameter.
    Bool(bool),
    /// String parameter.
    String(String),
    /// Array of strings (for simple tag filtering).
    StringArray(Vec<String>),
}

/// Generates SQL WHERE clause fragments for strict tag filtering.
///
/// This builder converts a `StrictTagFilter` into SQL WHERE clauses with
/// parameterized queries for safe database execution.
///
/// # Example
///
/// ```rust,ignore
/// use matric_db::strict_filter::{StrictFilterQueryBuilder, QueryParam};
/// use matric_core::StrictTagFilter;
/// use uuid::Uuid;
///
/// let filter = StrictTagFilter {
///     required_concepts: vec![Uuid::new_v4()],
///     any_concepts: vec![],
///     excluded_concepts: vec![],
///     required_schemes: vec![],
///     excluded_schemes: vec![],
///     min_tag_count: None,
///     include_untagged: true,
/// };
///
/// let builder = StrictFilterQueryBuilder::new(filter, 1);
/// let (sql, params) = builder.build();
/// // sql: "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = $2)"
/// // params: [QueryParam::Uuid(concept_id)]
/// ```
pub struct StrictFilterQueryBuilder {
    filter: matric_core::StrictTagFilter,
    param_offset: usize,
}

impl StrictFilterQueryBuilder {
    /// Create a new builder for the given filter.
    ///
    /// # Parameters
    ///
    /// * `filter` - The strict tag filter configuration
    /// * `param_offset` - The starting parameter index (number of parameters already in the query)
    pub fn new(filter: matric_core::StrictTagFilter, param_offset: usize) -> Self {
        Self {
            filter,
            param_offset,
        }
    }

    /// Build the complete WHERE clause fragment.
    ///
    /// Returns a tuple of:
    /// - SQL fragment (e.g., "EXISTS (...) AND NOT EXISTS (...)")
    /// - Vector of query parameters in the order they appear in the SQL
    ///
    /// If the filter is empty, returns ("TRUE", empty vec).
    /// Maximum number of elements across all filter arrays (prevents DoS via #218).
    const MAX_FILTER_ELEMENTS: usize = 1000;

    pub fn build(&self) -> (String, Vec<QueryParam>) {
        // Enforce total filter array size limit (fixes #218)
        let total_elements = self.filter.required_concepts.len()
            + self.filter.any_concepts.len()
            + self.filter.excluded_concepts.len()
            + self.filter.required_schemes.len()
            + self.filter.excluded_schemes.len()
            + self.filter.required_string_tags.len()
            + self.filter.any_string_tags.len()
            + self.filter.excluded_string_tags.len();
        if total_elements > Self::MAX_FILTER_ELEMENTS {
            // Return match-nothing clause instead of erroring — safe degradation
            return ("FALSE".to_string(), vec![]);
        }

        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = self.param_offset;

        // Required concepts (AND): note must have ALL of these
        // Each concept gets its own EXISTS clause to ensure ALL are present
        for concept_id in &self.filter.required_concepts {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ${})",
                param_idx
            ));
            params.push(QueryParam::Uuid(*concept_id));
        }

        // Any concepts (OR): note must have AT LEAST ONE
        // Use ANY with array parameter for efficient OR matching
        if !self.filter.any_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.any_concepts.clone()));
        }

        // Excluded concepts (NOT): note must have NONE of these
        // Use NOT EXISTS with ANY for efficient exclusion
        if !self.filter.excluded_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.excluded_concepts.clone()));
        }

        // Required schemes: notes must ONLY have concepts from these schemes
        // This is an isolation filter - two conditions:
        // 1. Must have at least one concept from required schemes (positive)
        // 2. Must NOT have any concepts from other schemes (negative)
        if !self.filter.required_schemes.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "(EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[])) AND NOT EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id != ALL(${}::uuid[])))",
                param_idx, param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.required_schemes.clone()));
        }

        // Excluded schemes: must NOT have concepts from these schemes
        if !self.filter.excluded_schemes.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.excluded_schemes.clone()));
        }

        // Required string tags (AND): note must have ALL of these simple tags
        // Each tag gets its own EXISTS clause to ensure ALL are present
        for tag_name in &self.filter.required_string_tags {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND (LOWER(nt.tag_name) = LOWER(${}::text) OR LOWER(nt.tag_name) LIKE LOWER(${}::text) || '/%' ESCAPE '\\'))",
                param_idx, param_idx
            ));
            params.push(QueryParam::String(tag_name.clone()));
        }

        // Any string tags (OR): note must have AT LEAST ONE
        // Use ANY with array parameter for efficient OR matching
        if !self.filter.any_string_tags.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND (LOWER(nt.tag_name) = ANY(SELECT LOWER(unnest(${}::text[]))) OR EXISTS (SELECT 1 FROM unnest(${}::text[]) AS t WHERE LOWER(nt.tag_name) LIKE LOWER(t) || '/%' ESCAPE '\\')))",
                param_idx, param_idx
            ));
            params.push(QueryParam::StringArray(self.filter.any_string_tags.clone()));
        }

        // Excluded string tags (NOT): note must have NONE of these
        // Use NOT EXISTS with ANY for efficient exclusion
        if !self.filter.excluded_string_tags.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND (LOWER(nt.tag_name) = ANY(SELECT LOWER(unnest(${}::text[]))) OR EXISTS (SELECT 1 FROM unnest(${}::text[]) AS t WHERE LOWER(nt.tag_name) LIKE LOWER(t) || '/%' ESCAPE '\\')))",
                param_idx, param_idx
            ));
            params.push(QueryParam::StringArray(
                self.filter.excluded_string_tags.clone(),
            ));
        }

        // Minimum tag count
        if let Some(min_count) = self.filter.min_tag_count {
            param_idx += 1;
            clauses.push(format!(
                "(SELECT COUNT(*) FROM note_skos_concept nsc WHERE nsc.note_id = n.id) >= ${}",
                param_idx
            ));
            params.push(QueryParam::Int(min_count));
        }

        // Untagged notes handling
        // If include_untagged is false, exclude notes with no tags
        if !self.filter.include_untagged {
            // Only add this if we don't already have required/any conditions that implicitly exclude untagged
            if self.filter.required_concepts.is_empty() && self.filter.any_concepts.is_empty() {
                clauses.push(
                    "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id)"
                        .to_string(),
                );
            }
        }

        // Generate final SQL fragment
        let sql = if clauses.is_empty() {
            "TRUE".to_string()
        } else {
            clauses.join(" AND ")
        };

        (sql, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::StrictTagFilter;

    #[test]
    fn test_empty_filter_returns_true() {
        let filter = StrictTagFilter::default();
        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert_eq!(sql, "TRUE");
        assert!(params.is_empty());
    }

    #[test]
    fn test_single_required_concept() {
        let concept_id = Uuid::new_v4();
        let filter = StrictTagFilter {
            required_concepts: vec![concept_id],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert_eq!(
            sql,
            "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = $1)"
        );
        assert_eq!(params.len(), 1);
        match &params[0] {
            QueryParam::Uuid(id) => assert_eq!(*id, concept_id),
            _ => panic!("Expected Uuid param"),
        }
    }

    #[test]
    fn test_multiple_required_concepts() {
        let concept1 = Uuid::new_v4();
        let concept2 = Uuid::new_v4();
        let filter = StrictTagFilter {
            required_concepts: vec![concept1, concept2],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should have two separate EXISTS clauses (AND logic)
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains(" AND "));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_any_concepts() {
        let concept1 = Uuid::new_v4();
        let concept2 = Uuid::new_v4();
        let filter = StrictTagFilter {
            any_concepts: vec![concept1, concept2],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should use ANY with array parameter
        assert!(sql.contains("ANY($1::uuid[])"));
        assert_eq!(params.len(), 1);
        match &params[0] {
            QueryParam::UuidArray(ids) => {
                assert_eq!(ids.len(), 2);
                assert!(ids.contains(&concept1));
                assert!(ids.contains(&concept2));
            }
            _ => panic!("Expected UuidArray param"),
        }
    }

    #[test]
    fn test_excluded_concepts() {
        let concept1 = Uuid::new_v4();
        let concept2 = Uuid::new_v4();
        let filter = StrictTagFilter {
            excluded_concepts: vec![concept1, concept2],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should use NOT EXISTS with ANY
        assert!(sql.contains("NOT EXISTS"));
        assert!(sql.contains("ANY($1::uuid[])"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_required_schemes_isolation() {
        let scheme1 = Uuid::new_v4();
        let scheme2 = Uuid::new_v4();
        let filter = StrictTagFilter {
            required_schemes: vec![scheme1, scheme2],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should have both EXISTS (positive) and NOT EXISTS (negative) for isolation
        assert!(sql.contains("EXISTS"));
        assert!(sql.contains("NOT EXISTS"));
        assert!(sql.contains("primary_scheme_id = ANY($1::uuid[])"));
        assert!(sql.contains("primary_scheme_id != ALL($1::uuid[])"));
        assert_eq!(params.len(), 1);
        match &params[0] {
            QueryParam::UuidArray(ids) => {
                assert_eq!(ids.len(), 2);
            }
            _ => panic!("Expected UuidArray param"),
        }
    }

    #[test]
    fn test_excluded_schemes() {
        let scheme1 = Uuid::new_v4();
        let filter = StrictTagFilter {
            excluded_schemes: vec![scheme1],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert!(sql.contains("NOT EXISTS"));
        assert!(sql.contains("primary_scheme_id = ANY($1::uuid[])"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_min_tag_count() {
        let filter = StrictTagFilter {
            min_tag_count: Some(3),
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert!(sql.contains("COUNT(*)"));
        assert!(sql.contains(">= $1"));
        assert_eq!(params.len(), 1);
        match &params[0] {
            QueryParam::Int(count) => assert_eq!(*count, 3),
            _ => panic!("Expected Int param"),
        }
    }

    #[test]
    fn test_exclude_untagged() {
        let filter = StrictTagFilter {
            include_untagged: false,
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert!(
            sql.contains("EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id)")
        );
        assert!(params.is_empty());
    }

    #[test]
    fn test_exclude_untagged_with_required_concepts() {
        // When we have required concepts, include_untagged=false is redundant
        // (required concepts already exclude untagged notes)
        let concept_id = Uuid::new_v4();
        let filter = StrictTagFilter {
            required_concepts: vec![concept_id],
            include_untagged: false,
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should only have the required concept clause, not the extra EXISTS for untagged
        assert_eq!(params.len(), 1);
        assert!(!sql
            .contains("AND EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id)"));
    }

    #[test]
    fn test_combined_filters() {
        let required = Uuid::new_v4();
        let any1 = Uuid::new_v4();
        let any2 = Uuid::new_v4();
        let excluded = Uuid::new_v4();

        let filter = StrictTagFilter {
            required_concepts: vec![required],
            any_concepts: vec![any1, any2],
            excluded_concepts: vec![excluded],
            min_tag_count: Some(2),
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should have 4 clauses joined with AND
        assert_eq!(params.len(), 4);
        assert!(sql.contains(" AND "));

        // Check parameter order
        match &params[0] {
            QueryParam::Uuid(_) => {} // required concept
            _ => panic!("Expected Uuid for first param"),
        }
        match &params[1] {
            QueryParam::UuidArray(_) => {} // any concepts
            _ => panic!("Expected UuidArray for second param"),
        }
        match &params[2] {
            QueryParam::UuidArray(_) => {} // excluded concepts
            _ => panic!("Expected UuidArray for third param"),
        }
        match &params[3] {
            QueryParam::Int(2) => {}
            _ => panic!("Expected Int(2) for fourth param"),
        }
    }

    #[test]
    fn test_param_offset() {
        let concept_id = Uuid::new_v4();
        let filter = StrictTagFilter {
            required_concepts: vec![concept_id],
            ..Default::default()
        };

        // Start with offset 5 (simulating 5 existing parameters)
        let builder = StrictFilterQueryBuilder::new(filter, 5);
        let (sql, params) = builder.build();

        // Should use $6 (offset 5 + 1)
        assert!(sql.contains("$6"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_complex_scheme_and_concept_filtering() {
        let req_concept = Uuid::new_v4();
        let req_scheme = Uuid::new_v4();
        let excl_scheme = Uuid::new_v4();

        let filter = StrictTagFilter {
            required_concepts: vec![req_concept],
            required_schemes: vec![req_scheme],
            excluded_schemes: vec![excl_scheme],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        // Should have 3 clauses: required concept, required scheme isolation, excluded scheme
        assert_eq!(params.len(), 3);
        assert!(sql.contains("$1")); // required concept
        assert!(sql.contains("$2")); // required scheme (used twice in isolation check)
        assert!(sql.contains("$3")); // excluded scheme
    }
}

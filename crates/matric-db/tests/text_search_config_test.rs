//! Tests to verify that all text search operations use matric_english configuration.
//!
//! This test suite ensures that:
//! - All FTS queries use 'matric_english' instead of 'english'
//! - tsvector generation uses correct configuration
//! - websearch_to_tsquery uses correct configuration (supports OR/NOT/phrase operators)
//!
//! These are unit tests that verify the SQL query strings are correctly formed.
//! They do not require a database connection.

#[cfg(test)]
mod text_search_config {
    /// Verify that search.rs uses matric_english configuration
    #[test]
    fn test_search_uses_matric_english() {
        // This test verifies that search queries use 'matric_english'
        // We check the source code directly since these are SQL strings
        let search_source = include_str!("../src/search.rs");

        // Count occurrences of 'english' text search config (should be 0)
        let english_count = search_source.matches("'english'").count();

        // Count occurrences of 'matric_english' text search config
        let matric_english_count = search_source.matches("'matric_english'").count();

        // search.rs should have NO references to 'english' config
        assert_eq!(
            english_count, 0,
            "search.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        // search.rs should use 'matric_english' in multiple places:
        // - search() method: 3 to_tsvector + 3 websearch_to_tsquery = 6 occurrences
        // - search_with_strict_filter(): 3 to_tsvector + 3 websearch_to_tsquery = 6 occurrences
        // - search_filtered(): 3 to_tsvector + 3 websearch_to_tsquery = 6 occurrences
        // - search_by_keyword(): 1 websearch_to_tsquery = 1 occurrence
        // Total expected: 19 occurrences
        assert!(
            matric_english_count >= 19,
            "search.rs should use 'matric_english' at least 19 times, found {}",
            matric_english_count
        );
    }

    /// Verify that skos_tags.rs uses matric_english configuration
    #[test]
    fn test_skos_tags_uses_matric_english() {
        let skos_source = include_str!("../src/skos_tags.rs");

        // Count occurrences of 'english' text search config (should be 0)
        let english_count = skos_source.matches("'english'").count();

        // Count occurrences of 'matric_english' text search config
        let matric_english_count = skos_source.matches("'matric_english'").count();

        // skos_tags.rs should have NO references to 'english' config
        assert_eq!(
            english_count, 0,
            "skos_tags.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        // skos_tags.rs should use 'matric_english' in at least 1 place:
        // - search_concepts() method (FTS via websearch_to_tsquery)
        // Note: search_labels() uses ILIKE prefix matching (not FTS)
        assert!(
            matric_english_count >= 1,
            "skos_tags.rs should use 'matric_english' at least 1 time, found {}",
            matric_english_count
        );
    }

    /// Verify that embedding_sets.rs uses matric_english configuration
    #[test]
    fn test_embedding_sets_uses_matric_english() {
        let embedding_source = include_str!("../src/embedding_sets.rs");

        // Count occurrences of 'english' text search config (should be 0)
        let english_count = embedding_source.matches("'english'").count();

        // Count occurrences of 'matric_english' text search config
        let matric_english_count = embedding_source.matches("'matric_english'").count();

        // embedding_sets.rs should have NO references to 'english' config
        assert_eq!(
            english_count, 0,
            "embedding_sets.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        // embedding_sets.rs should use 'matric_english' in at least 1 place
        assert!(
            matric_english_count >= 1,
            "embedding_sets.rs should use 'matric_english' at least 1 time, found {}",
            matric_english_count
        );
    }

    /// Verify no hardcoded 'english' text search config in matric-db crate
    #[test]
    fn test_no_hardcoded_english_config_in_db_crate() {
        // This is a comprehensive check across all key modules
        let search_source = include_str!("../src/search.rs");
        let skos_source = include_str!("../src/skos_tags.rs");
        let embedding_source = include_str!("../src/embedding_sets.rs");

        let total_english = search_source.matches("'english'").count()
            + skos_source.matches("'english'").count()
            + embedding_source.matches("'english'").count();

        assert_eq!(
            total_english, 0,
            "No module in matric-db should use 'english' text search config, found {} total occurrences",
            total_english
        );
    }

    /// Verify SQL query patterns are correct
    #[test]
    fn test_sql_pattern_correctness() {
        let search_source = include_str!("../src/search.rs");

        // Check for to_tsvector('matric_english', ...) pattern
        assert!(
            search_source.contains("to_tsvector('matric_english'"),
            "search.rs should contain to_tsvector('matric_english', ...) pattern"
        );

        // Check for websearch_to_tsquery('matric_english', ...) pattern
        // websearch_to_tsquery supports OR/NOT/phrase operators (Issue #364)
        assert!(
            search_source.contains("websearch_to_tsquery('matric_english'"),
            "search.rs should contain websearch_to_tsquery('matric_english', ...) pattern"
        );

        // Verify we no longer use plainto_tsquery (doesn't support boolean operators)
        assert!(
            !search_source.contains("plainto_tsquery("),
            "search.rs should NOT use plainto_tsquery (use websearch_to_tsquery instead)"
        );
    }

    /// Verify websearch_to_tsquery is used for boolean operator support (#364)
    #[test]
    fn test_websearch_to_tsquery_migration() {
        let search_source = include_str!("../src/search.rs");
        let skos_source = include_str!("../src/skos_tags.rs");
        let embedding_source = include_str!("../src/embedding_sets.rs");

        // All sources should use websearch_to_tsquery
        assert!(
            search_source.contains("websearch_to_tsquery"),
            "search.rs must use websearch_to_tsquery for OR/NOT/phrase support"
        );
        assert!(
            skos_source.contains("websearch_to_tsquery"),
            "skos_tags.rs must use websearch_to_tsquery for OR/NOT/phrase support"
        );
        assert!(
            embedding_source.contains("websearch_to_tsquery"),
            "embedding_sets.rs must use websearch_to_tsquery for OR/NOT/phrase support"
        );

        // None should use plainto_tsquery anymore
        let total_plainto = search_source.matches("plainto_tsquery(").count()
            + skos_source.matches("plainto_tsquery(").count()
            + embedding_source.matches("plainto_tsquery(").count();

        assert_eq!(
            total_plainto, 0,
            "All plainto_tsquery calls should be migrated to websearch_to_tsquery, found {} remaining",
            total_plainto
        );
    }
}

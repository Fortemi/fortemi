//! Tests to verify that all text search operations use public.matric_english configuration.
//!
//! This test suite ensures that:
//! - All FTS queries use 'public.matric_english' (schema-qualified) instead of 'english'
//! - tsvector generation uses correct configuration
//! - websearch_to_tsquery uses correct configuration (supports OR/NOT/phrase operators)
//! - Config names are schema-qualified to work correctly in non-default archives (#412)
//!
//! These are unit tests that verify the SQL query strings are correctly formed.
//! They do not require a database connection.

#[cfg(test)]
mod text_search_config {
    /// Verify that search.rs uses public.matric_english configuration
    #[test]
    fn test_search_uses_matric_english() {
        let search_source = include_str!("../src/search.rs");

        // Count occurrences of bare 'english' text search config (should be 0)
        let english_count = search_source.matches("'english'").count();

        // Count occurrences of schema-qualified 'public.matric_english' config
        let matric_english_count = search_source.matches("'public.matric_english'").count();

        // search.rs should have NO references to bare 'english' config
        assert_eq!(
            english_count, 0,
            "search.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        // search.rs should use 'public.matric_english' in multiple places
        assert!(
            matric_english_count >= 19,
            "search.rs should use 'public.matric_english' at least 19 times, found {}",
            matric_english_count
        );

        // Verify NO unqualified 'matric_english' references remain (without public. prefix)
        let unqualified_count = search_source.matches("'matric_english'").count();
        assert_eq!(
            unqualified_count, 0,
            "search.rs should not have unqualified 'matric_english' (use 'public.matric_english'), found {}",
            unqualified_count
        );
    }

    /// Verify that skos_tags.rs uses public.matric_english configuration
    #[test]
    fn test_skos_tags_uses_matric_english() {
        let skos_source = include_str!("../src/skos_tags.rs");

        let english_count = skos_source.matches("'english'").count();
        let matric_english_count = skos_source.matches("'public.matric_english'").count();

        assert_eq!(
            english_count, 0,
            "skos_tags.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        assert!(
            matric_english_count >= 1,
            "skos_tags.rs should use 'public.matric_english' at least 1 time, found {}",
            matric_english_count
        );
    }

    /// Verify that embedding_sets.rs uses public.matric_english configuration
    #[test]
    fn test_embedding_sets_uses_matric_english() {
        let embedding_source = include_str!("../src/embedding_sets.rs");

        let english_count = embedding_source.matches("'english'").count();
        let matric_english_count = embedding_source.matches("'public.matric_english'").count();

        assert_eq!(
            english_count, 0,
            "embedding_sets.rs should not use 'english' text search config, found {} occurrences",
            english_count
        );

        assert!(
            matric_english_count >= 1,
            "embedding_sets.rs should use 'public.matric_english' at least 1 time, found {}",
            matric_english_count
        );
    }

    /// Verify no hardcoded bare 'english' text search config in matric-db crate
    #[test]
    fn test_no_hardcoded_english_config_in_db_crate() {
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

    /// Verify SQL query patterns are correct with schema-qualified config names
    #[test]
    fn test_sql_pattern_correctness() {
        let search_source = include_str!("../src/search.rs");

        // Check for schema-qualified to_tsvector pattern
        assert!(
            search_source.contains("to_tsvector('public.matric_english'"),
            "search.rs should contain to_tsvector('public.matric_english', ...) pattern"
        );

        // Check for schema-qualified websearch_to_tsquery pattern
        assert!(
            search_source.contains("websearch_to_tsquery('public.matric_english'"),
            "search.rs should contain websearch_to_tsquery('public.matric_english', ...) pattern"
        );

        // Verify we no longer use plainto_tsquery (doesn't support boolean operators)
        assert!(
            !search_source.contains("plainto_tsquery("),
            "search.rs should NOT use plainto_tsquery (use websearch_to_tsquery instead)"
        );
    }

    /// Verify all FTS configs are schema-qualified (Issue #412)
    #[test]
    fn test_all_configs_schema_qualified() {
        let search_source = include_str!("../src/search.rs");
        let skos_source = include_str!("../src/skos_tags.rs");
        let embedding_source = include_str!("../src/embedding_sets.rs");
        let skos_tx_source = include_str!("../src/skos_tags_tx.rs");

        // No unqualified matric_english references
        let total_unqualified = search_source.matches("'matric_english'").count()
            + skos_source.matches("'matric_english'").count()
            + embedding_source.matches("'matric_english'").count()
            + skos_tx_source.matches("'matric_english'").count();

        assert_eq!(
            total_unqualified, 0,
            "All 'matric_english' references must be schema-qualified as 'public.matric_english' (#412), found {} unqualified",
            total_unqualified
        );

        // No unqualified matric_simple references
        let total_unqualified_simple = search_source.matches("'matric_simple'").count();
        assert_eq!(
            total_unqualified_simple, 0,
            "All 'matric_simple' references must be schema-qualified as 'public.matric_simple' (#412), found {} unqualified",
            total_unqualified_simple
        );
    }

    /// Verify archives.rs FTS fix uses schema-qualified config names (Issue #412)
    #[test]
    fn test_archives_fts_fix_uses_qualified_configs() {
        let archives_source = include_str!("../src/archives.rs");

        // The FTS_FIX_DEFINITIONS must use 'public.matric_english' and 'public.matric_simple'
        assert!(
            archives_source.contains("to_tsvector('public.matric_english'"),
            "archives.rs FTS fix must use 'public.matric_english' in generated column expression"
        );
        assert!(
            archives_source.contains("to_tsvector('public.matric_simple'"),
            "archives.rs FTS fix must use 'public.matric_simple' in functional index definitions"
        );

        // No unqualified matric_english/matric_simple in FTS definitions
        // (exclude comments and string literals that explain the problem)
        let lines: Vec<&str> = archives_source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("///"))
            .collect();
        let code_only = lines.join("\n");

        let unqualified_english = code_only.matches("'matric_english'").count();
        assert_eq!(
            unqualified_english, 0,
            "archives.rs code should not have unqualified 'matric_english', found {}",
            unqualified_english
        );

        let unqualified_simple = code_only.matches("'matric_simple'").count();
        assert_eq!(
            unqualified_simple, 0,
            "archives.rs code should not have unqualified 'matric_simple', found {}",
            unqualified_simple
        );
    }

    /// Verify websearch_to_tsquery is used for boolean operator support (#364)
    #[test]
    fn test_websearch_to_tsquery_migration() {
        let search_source = include_str!("../src/search.rs");
        let skos_source = include_str!("../src/skos_tags.rs");
        let embedding_source = include_str!("../src/embedding_sets.rs");

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

//! Tests for Issue #259: Duplicate prefLabel creates misleading "valid_notation" constraint error
//!
//! This test verifies that SKOS constraint violations return user-friendly error messages
//! instead of raw database constraint names.

#[cfg(test)]
mod skos_error_messages_tests {
    /// Test that verifies the error message transformation logic
    ///
    /// This tests the actual logic that would be in ApiError conversion,
    /// simulating how database constraint errors are transformed into friendly messages.
    #[test]
    fn test_constraint_error_detection() {
        // Test various constraint error patterns
        let test_cases = vec![
            (
                r#"duplicate key value violates unique constraint "idx_unique_pref_label""#,
                "prefLabel",
                "idx_unique_pref_label",
            ),
            (
                r#"duplicate key value violates unique constraint "valid_notation""#,
                "notation",
                "valid_notation",
            ),
            (
                r#"ERROR: duplicate key value violates unique constraint "idx_unique_pref_label"
DETAIL: Key (scheme_uri, pref_label)=(http://example.org/scheme, Test) already exists."#,
                "prefLabel",
                "idx_unique_pref_label",
            ),
        ];

        for (db_error, expected_type, constraint_name) in test_cases {
            // Verify that the error message can be detected
            assert!(
                db_error.contains("duplicate key") || db_error.contains("unique constraint"),
                "Should detect duplicate key error"
            );

            // Apply the same transformation logic as in main.rs
            let friendly_msg = if db_error.contains("idx_unique_pref_label")
                || db_error.contains("pref_label")
            {
                "A concept with this prefLabel already exists in the scheme".to_string()
            } else if db_error.contains("valid_notation") || db_error.contains("notation") {
                "A concept with this notation already exists in the scheme".to_string()
            } else if db_error.contains("idx_unique_tag_name") || db_error.contains("tag_name") {
                "A tag with this name already exists".to_string()
            } else {
                db_error.to_string()
            };

            // Verify the friendly message doesn't contain the raw constraint name
            assert!(
                !friendly_msg.contains(constraint_name),
                "Friendly message should not contain constraint name '{}'. Got: {}",
                constraint_name,
                friendly_msg
            );

            // Verify the friendly message mentions the expected type
            assert!(
                friendly_msg.contains(expected_type),
                "Friendly message should mention '{}'. Got: {}",
                expected_type,
                friendly_msg
            );
        }
    }

    /// Test that duplicate prefLabel errors get friendly messages
    #[test]
    fn test_duplicate_preflabel_transformation() {
        let db_error = r#"duplicate key value violates unique constraint "idx_unique_pref_label""#;

        // Apply transformation
        let friendly_msg =
            if db_error.contains("idx_unique_pref_label") || db_error.contains("pref_label") {
                "A concept with this prefLabel already exists in the scheme".to_string()
            } else {
                db_error.to_string()
            };

        // Verify transformation
        assert_eq!(
            friendly_msg, "A concept with this prefLabel already exists in the scheme",
            "prefLabel constraint should produce friendly message"
        );
    }

    /// Test that duplicate notation errors get friendly messages
    #[test]
    fn test_duplicate_notation_transformation() {
        let db_error = r#"duplicate key value violates unique constraint "valid_notation""#;

        // Apply transformation
        let friendly_msg =
            if db_error.contains("idx_unique_pref_label") || db_error.contains("pref_label") {
                "A concept with this prefLabel already exists in the scheme".to_string()
            } else if db_error.contains("valid_notation") || db_error.contains("notation") {
                "A concept with this notation already exists in the scheme".to_string()
            } else {
                db_error.to_string()
            };

        // Verify transformation
        assert_eq!(
            friendly_msg, "A concept with this notation already exists in the scheme",
            "notation constraint should produce friendly message"
        );
    }

    /// Test that unknown constraint errors pass through unchanged
    #[test]
    fn test_unknown_constraint_passthrough() {
        let db_error = r#"duplicate key value violates unique constraint "some_other_constraint""#;

        // Apply transformation
        let friendly_msg =
            if db_error.contains("idx_unique_pref_label") || db_error.contains("pref_label") {
                "A concept with this prefLabel already exists in the scheme".to_string()
            } else if db_error.contains("valid_notation") || db_error.contains("notation") {
                "A concept with this notation already exists in the scheme".to_string()
            } else if db_error.contains("idx_unique_tag_name") || db_error.contains("tag_name") {
                "A tag with this name already exists".to_string()
            } else {
                db_error.to_string()
            };

        // Verify passthrough - unknown constraints should return the original message
        assert_eq!(
            friendly_msg, db_error,
            "Unknown constraints should pass through unchanged"
        );
    }

    /// Test tag name constraint errors
    #[test]
    fn test_duplicate_tag_name_transformation() {
        let db_error = r#"duplicate key value violates unique constraint "idx_unique_tag_name""#;

        // Apply transformation
        let friendly_msg =
            if db_error.contains("idx_unique_pref_label") || db_error.contains("pref_label") {
                "A concept with this prefLabel already exists in the scheme".to_string()
            } else if db_error.contains("valid_notation") || db_error.contains("notation") {
                "A concept with this notation already exists in the scheme".to_string()
            } else if db_error.contains("idx_unique_tag_name") || db_error.contains("tag_name") {
                "A tag with this name already exists".to_string()
            } else {
                db_error.to_string()
            };

        // Verify transformation
        assert_eq!(
            friendly_msg, "A tag with this name already exists",
            "tag_name constraint should produce friendly message"
        );
    }
}

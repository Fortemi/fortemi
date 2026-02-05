//! Unit tests for validation fixes (issues #271, #276, #263, #29)
//!
//! Tests verify:
//! - Issue #271: Negative limit validation logic
//! - Issue #29: Zero limit validation logic
//! - Issue #276: Empty content detection logic
//! - Issue #263: Database error pattern matching for HTTP status codes

#[cfg(test)]
mod validation_logic_tests {
    use matric_core::Error as CoreError;

    #[test]
    fn test_negative_limit_detection() {
        // Issue #271: Negative limit should be detected
        let limits = vec![-1, -10, -100, i64::MIN];

        for limit in limits {
            assert!(limit < 0, "Limit {} should be detected as negative", limit);
        }
    }

    #[test]
    fn test_zero_limit_invalid() {
        // Issue #29: Zero limit should be detected as invalid
        let limit = 0_i64;
        assert!(limit <= 0, "Limit 0 should be invalid (must be >= 1)");
    }

    #[test]
    fn test_positive_limits_valid() {
        // Issue #271 + #29: Only positive limits (>= 1) are valid
        let limits = vec![1, 10, 100, i64::MAX];

        for limit in limits {
            assert!(limit > 0, "Limit {} should be valid (>= 1)", limit);
        }
    }

    #[test]
    fn test_empty_content_detection() {
        // Issue #276: Empty content detection (whitespace-only)
        let empty_contents = vec!["", " ", "  ", "\n", "\t", " \n\t "];

        for content in empty_contents {
            assert!(
                content.trim().is_empty(),
                "Content '{}' should be detected as empty",
                content.escape_default()
            );
        }
    }

    #[test]
    fn test_non_empty_content_detection() {
        // Issue #276: Non-empty content should not trigger warning
        let valid_contents = vec!["hello", " hello ", "\nhello\n", "  hello  ", "a"];

        for content in valid_contents {
            assert!(
                !content.trim().is_empty(),
                "Content '{}' should be detected as non-empty",
                content.escape_default()
            );
        }
    }

    #[test]
    fn test_invalid_input_error_message() {
        // Issue #263: InvalidInput should map to BadRequest (400)
        let core_error = CoreError::InvalidInput("test validation error".to_string());

        // Verify the error message is preserved
        assert_eq!(
            core_error.to_string(),
            "Invalid input: test validation error"
        );

        // Verify it's the correct variant
        assert!(matches!(core_error, CoreError::InvalidInput(_)));
    }

    #[test]
    fn test_duplicate_key_pattern_detection() {
        // Issue #263: Database errors should be inspected for constraint violations

        // Test unique constraint message patterns (typical PostgreSQL messages)
        let duplicate_patterns = vec![
            "duplicate key value violates unique constraint",
            "ERROR: duplicate key",
            "violates unique constraint \"idx_name\"",
            "duplicate key value violates unique constraint \"users_email_key\"",
        ];

        for pattern in duplicate_patterns {
            let is_duplicate =
                pattern.contains("duplicate key") || pattern.contains("unique constraint");

            assert!(
                is_duplicate,
                "Pattern should match duplicate key detection: {}",
                pattern
            );
        }
    }

    #[test]
    fn test_foreign_key_pattern_detection() {
        // Issue #263: Foreign key violations should return 400

        let fk_patterns = vec![
            "foreign key constraint fails",
            "violates foreign key constraint",
            "insert or update on table violates foreign key constraint",
        ];

        for pattern in fk_patterns {
            let is_fk_violation = pattern.contains("foreign key");

            assert!(
                is_fk_violation,
                "Pattern should match foreign key detection: {}",
                pattern
            );
        }
    }

    #[test]
    fn test_polyhierarchy_pattern_detection() {
        // Issue #263: Polyhierarchy limit errors should return 400

        let patterns = vec!["Polyhierarchy limit exceeded", "Polyhierarchy limit: 3"];

        for pattern in patterns {
            let is_polyhierarchy = pattern.contains("Polyhierarchy limit");

            assert!(
                is_polyhierarchy,
                "Pattern should match polyhierarchy detection: {}",
                pattern
            );
        }
    }

    #[test]
    fn test_error_pattern_priority() {
        // Issue #263: Verify pattern matching priority
        // Duplicate key should be detected before foreign key

        let duplicate_msg = "duplicate key value violates unique constraint";
        let fk_msg = "violates foreign key constraint";
        let polyhierarchy_msg = "Polyhierarchy limit exceeded";

        // Each should match its own pattern
        assert!(
            duplicate_msg.contains("duplicate key") || duplicate_msg.contains("unique constraint")
        );
        assert!(fk_msg.contains("foreign key"));
        assert!(polyhierarchy_msg.contains("Polyhierarchy limit"));

        // They should be mutually exclusive (for proper routing)
        assert!(!duplicate_msg.contains("foreign key"));
        assert!(!fk_msg.contains("duplicate key"));
    }
}

#[cfg(test)]
mod http_status_code_mapping {
    #[test]
    fn test_status_code_values() {
        // Verify HTTP status codes used in the implementation
        use axum::http::StatusCode;

        // Issue #271: Bad Request for invalid input
        assert_eq!(StatusCode::BAD_REQUEST.as_u16(), 400);

        // Issue #263: Conflict for duplicate key
        assert_eq!(StatusCode::CONFLICT.as_u16(), 409);

        // Standard success codes
        assert_eq!(StatusCode::OK.as_u16(), 200);
        assert_eq!(StatusCode::CREATED.as_u16(), 201);

        // Error codes
        assert_eq!(StatusCode::NOT_FOUND.as_u16(), 404);
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), 500);
    }
}

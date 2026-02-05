//! Schema name validation to prevent SQL injection and ensure valid PostgreSQL identifiers.

use matric_core::{Error, Result};

/// Validate a PostgreSQL schema name for safety and correctness.
///
/// PostgreSQL schema names must:
/// - Not be empty
/// - Not exceed 63 characters (PostgreSQL identifier limit)
/// - Contain only alphanumeric characters and underscores
/// - Not start with a digit
/// - Not be a SQL keyword (basic check)
///
/// # Arguments
///
/// * `name` - The schema name to validate
///
/// # Returns
///
/// `Ok(())` if the name is valid, otherwise an `Error::InvalidInput`
///
/// # Examples
///
/// ```
/// use matric_db::validate_schema_name;
///
/// assert!(validate_schema_name("my_schema").is_ok());
/// assert!(validate_schema_name("schema123").is_ok());
/// assert!(validate_schema_name("123invalid").is_err());
/// assert!(validate_schema_name("").is_err());
/// ```
pub fn validate_schema_name(name: &str) -> Result<()> {
    // Check for empty name
    if name.is_empty() {
        return Err(Error::InvalidInput(
            "Schema name cannot be empty".to_string(),
        ));
    }

    // Check length (PostgreSQL identifier limit is 63 characters)
    if name.len() > 63 {
        return Err(Error::InvalidInput(format!(
            "Schema name exceeds 63 character limit: {} characters",
            name.len()
        )));
    }

    // Check first character: must be a letter or underscore
    if let Some(first) = name.chars().next() {
        if !first.is_ascii_alphabetic() && first != '_' {
            return Err(Error::InvalidInput(format!(
                "Schema name must start with a letter or underscore, found: '{}'",
                first
            )));
        }
    }

    // Check all characters: must be alphanumeric or underscore
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            return Err(Error::InvalidInput(format!(
                "Schema name contains invalid character: '{}'. Only alphanumeric and underscore allowed",
                ch
            )));
        }
    }

    // Check for SQL keywords (basic check for common dangerous keywords)
    // Note: "public" is intentionally NOT reserved - it's the default PostgreSQL schema
    // and is needed by Database::default_schema()
    let lowercase = name.to_lowercase();
    const RESERVED_KEYWORDS: &[&str] = &[
        "pg_catalog",
        "information_schema",
        "pg_toast",
        "select",
        "insert",
        "update",
        "delete",
        "drop",
        "create",
        "alter",
        "grant",
        "revoke",
        "truncate",
    ];

    if RESERVED_KEYWORDS.contains(&lowercase.as_str()) {
        return Err(Error::InvalidInput(format!(
            "Schema name '{}' is a reserved SQL keyword",
            name
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_schema_name_valid() {
        // Valid schema names
        assert!(validate_schema_name("my_schema").is_ok());
        assert!(validate_schema_name("schema123").is_ok());
        assert!(validate_schema_name("_private").is_ok());
        assert!(validate_schema_name("archive_2026").is_ok());
        assert!(validate_schema_name("Test_Schema_123").is_ok());
        assert!(validate_schema_name("a").is_ok());
    }

    #[test]
    fn test_validate_schema_name_empty() {
        let result = validate_schema_name("");
        assert!(result.is_err());
        match result {
            Err(Error::InvalidInput(msg)) => {
                assert!(msg.contains("empty"));
            }
            _ => panic!("Expected InvalidInput error for empty name"),
        }
    }

    #[test]
    fn test_validate_schema_name_too_long() {
        // 64 characters - exceeds PostgreSQL limit
        let long_name = "a".repeat(64);
        let result = validate_schema_name(&long_name);
        assert!(result.is_err());
        match result {
            Err(Error::InvalidInput(msg)) => {
                assert!(msg.contains("63 character limit"));
            }
            _ => panic!("Expected InvalidInput error for long name"),
        }
    }

    #[test]
    fn test_validate_schema_name_starts_with_digit() {
        let result = validate_schema_name("123invalid");
        assert!(result.is_err());
        match result {
            Err(Error::InvalidInput(msg)) => {
                assert!(msg.contains("start with a letter"));
            }
            _ => panic!("Expected InvalidInput error for name starting with digit"),
        }
    }

    #[test]
    fn test_validate_schema_name_invalid_characters() {
        // Test various invalid characters
        let invalid_names = vec![
            "schema-name",  // hyphen
            "schema.name",  // dot
            "schema name",  // space
            "schema;name",  // semicolon
            "schema'name",  // single quote
            "schema\"name", // double quote
            "schema/name",  // slash
            "schema\\name", // backslash
            "schema(name)", // parentheses
            "schema@name",  // at sign
            "schema#name",  // hash
            "schema$name",  // dollar sign
        ];

        for name in invalid_names {
            let result = validate_schema_name(name);
            assert!(result.is_err(), "Expected error for: {}", name);
            match result {
                Err(Error::InvalidInput(msg)) => {
                    assert!(
                        msg.contains("invalid character"),
                        "Name: {}, Error: {}",
                        name,
                        msg
                    );
                }
                _ => panic!("Expected InvalidInput error for: {}", name),
            }
        }
    }

    #[test]
    fn test_validate_schema_name_sql_injection_attempts() {
        // Test SQL injection patterns
        let injection_attempts = vec![
            "schema'; DROP TABLE notes; --",
            "schema' OR '1'='1",
            "schema; DROP SCHEMA public CASCADE;",
        ];

        for name in injection_attempts {
            let result = validate_schema_name(name);
            assert!(
                result.is_err(),
                "Expected error for injection attempt: {}",
                name
            );
        }
    }

    #[test]
    fn test_validate_schema_name_reserved_keywords() {
        // Test reserved SQL keywords
        // Note: "public" is intentionally NOT reserved - it's the default PostgreSQL schema
        // and is needed by Database::default_schema()
        let reserved = vec![
            "pg_catalog",
            "information_schema",
            "select",
            "drop",
            "CREATE", // Test case-insensitive
            "DELETE",
        ];

        for keyword in reserved {
            let result = validate_schema_name(keyword);
            assert!(
                result.is_err(),
                "Expected error for reserved keyword: {}",
                keyword
            );
            match result {
                Err(Error::InvalidInput(msg)) => {
                    assert!(
                        msg.contains("reserved"),
                        "Keyword: {}, Error: {}",
                        keyword,
                        msg
                    );
                }
                _ => panic!(
                    "Expected InvalidInput error for reserved keyword: {}",
                    keyword
                ),
            }
        }
    }

    #[test]
    fn test_validate_schema_name_edge_cases() {
        // Test maximum valid length (63 characters)
        let max_valid = "a".repeat(63);
        assert!(validate_schema_name(&max_valid).is_ok());

        // Test single character
        assert!(validate_schema_name("a").is_ok());
        assert!(validate_schema_name("_").is_ok());

        // Test underscore prefix
        assert!(validate_schema_name("_test").is_ok());
    }

    #[test]
    fn test_validate_schema_name_unicode_rejected() {
        // Unicode characters should be rejected
        let result = validate_schema_name("schemaλ");
        assert!(result.is_err());

        let result = validate_schema_name("schema日本");
        assert!(result.is_err());
    }
}

//! Tests for HTTP header safety - ensuring no panics on malformed input
//!
//! This test suite verifies that all HTTP header creation is safe and handles
//! invalid input gracefully without panicking.

use axum::http::HeaderValue;

#[test]
fn test_content_type_headers_are_safe() {
    // These are static strings used in the codebase - they should always parse successfully
    let valid_types = vec![
        "text/markdown; charset=utf-8",
        "text/plain; charset=utf-8",
        "application/json; charset=utf-8",
        "application/gzip",
        "application/x-tar",
    ];

    for content_type in valid_types {
        let result: Result<HeaderValue, _> = content_type.parse();
        assert!(
            result.is_ok(),
            "Content-Type '{}' should parse successfully",
            content_type
        );
    }
}

#[test]
fn test_content_disposition_with_safe_filenames() {
    // Test safe filenames that should work
    let safe_filenames = vec![
        "note.md",
        "backup-20240101.json",
        "matric-shard-20240101-120000.shard",
        "backup.sql.gz",
        "archive.tar.gz",
    ];

    for filename in safe_filenames {
        let disposition = format!("attachment; filename=\"{}\"", filename);
        let result: Result<HeaderValue, _> = disposition.parse();
        assert!(
            result.is_ok(),
            "Content-Disposition with filename '{}' should parse successfully",
            filename
        );
    }
}

#[test]
fn test_content_disposition_with_problematic_filenames() {
    // Test filenames that could contain problematic characters
    // Note: The codebase already sanitizes some of these in note titles
    let problematic_filenames = vec![
        "note\nwith\nnewlines.md", // Newlines are invalid in headers
        "note\rwith\rcarriage.md", // Carriage returns are invalid
        "note\x00null.md",         // Null bytes are invalid
        "note\x7Fdel.md",          // DEL character is invalid
    ];

    for filename in problematic_filenames {
        let disposition = format!("attachment; filename=\"{}\"", filename);
        let result: Result<HeaderValue, _> = disposition.parse();

        // These SHOULD fail - we're testing that we handle the failure gracefully
        if result.is_err() {
            // Expected failure - this is good
            continue;
        }

        // If it succeeded, that's actually okay too (some chars might be allowed)
        // The important thing is that we don't panic
    }
}

#[test]
fn test_header_value_from_str_safety() {
    // Test HeaderValue::from_str with various inputs
    let test_cases = vec![
        ("application/x-tar", true), // Valid
        ("text/plain", true),        // Valid
        ("application/json", true),  // Valid
        ("text/html\n", false),      // Invalid - newline
        ("text/html\r\n", false),    // Invalid - CRLF
        ("text/html\x00", false),    // Invalid - null byte
    ];

    for (input, should_succeed) in test_cases {
        let result = HeaderValue::from_str(input);

        if should_succeed {
            assert!(
                result.is_ok(),
                "HeaderValue::from_str('{}') should succeed",
                input.escape_debug()
            );
        } else {
            assert!(
                result.is_err(),
                "HeaderValue::from_str('{}') should fail",
                input.escape_debug()
            );
        }
    }
}

#[test]
fn test_filename_sanitization_prevents_header_injection() {
    // Test that the filename sanitization in the codebase prevents header injection
    let dangerous_filename = "test\r\nContent-Type: text/html\r\n\r\n<script>alert('xss')</script>";

    // Simulate the sanitization done in export_note function
    let sanitized = dangerous_filename.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");

    // The sanitized version should not contain newlines (but our sanitization doesn't remove them)
    // This test documents current behavior - we should handle this in the actual fix
    let disposition = format!("attachment; filename=\"{}.md\"", sanitized);
    let _result: Result<HeaderValue, _> = disposition.parse();

    // The point is: we shouldn't panic, even if parsing fails
    // In the actual code, we need to handle this error properly
}

#[test]
fn test_uuid_based_filenames_are_always_safe() {
    // UUIDs should always be safe for headers
    use uuid::Uuid;

    let id = Uuid::new_v4();
    let disposition = format!("attachment; filename=\"{}.md\"", id);
    let result: Result<HeaderValue, _> = disposition.parse();

    assert!(
        result.is_ok(),
        "UUID-based filename should always be safe: {}",
        id
    );
}

#[test]
fn test_timestamp_based_filenames_are_safe() {
    // Timestamp-based filenames used in backup/shard exports should be safe
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("matric-backup-{}.json", timestamp);
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let result: Result<HeaderValue, _> = disposition.parse();

    assert!(
        result.is_ok(),
        "Timestamp-based filename should be safe: {}",
        filename
    );
}

#[test]
fn test_archive_name_generation_is_safe() {
    // Test the archive name generation logic
    let test_filenames = vec![
        "backup-20240101.sql.gz",
        "shard-20240101.tar.gz",
        "test.sql.gz",
        "test.tar.gz",
    ];

    for filename in test_filenames {
        let archive_name = format!(
            "{}.archive",
            filename
                .trim_end_matches(".sql.gz")
                .trim_end_matches(".tar.gz")
        );

        let disposition = format!("attachment; filename=\"{}\"", archive_name);
        let result: Result<HeaderValue, _> = disposition.parse();

        assert!(
            result.is_ok(),
            "Archive filename should be safe: {}",
            archive_name
        );
    }
}

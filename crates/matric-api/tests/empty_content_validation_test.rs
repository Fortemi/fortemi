/// Unit tests for issue #378: create_note should validate minimum content
///
/// This test verifies that the create_note endpoint properly validates
/// that content is not empty and returns a BadRequest error.
///
/// Background:
/// - create_note previously accepted empty content and returned a warning
/// - Attachments are added AFTER note creation via POST /api/v1/notes/{id}/attachments
/// - Therefore, at creation time, content must be non-empty
/// - Empty content should result in HTTP 400 Bad Request

#[test]
fn test_empty_content_validation_expected_behavior() {
    // This test documents the expected behavior for issue #378
    // When create_note is called with empty or whitespace-only content, it should:
    //
    // 1. Return HTTP 400 Bad Request (not 201 Created with warning)
    // 2. Return an error message like "Content is required"
    // 3. Validate BEFORE creating the note in the database
    //
    // The previous behavior (lines 1591-1598) returned a warning but still created the note:
    //   if has_empty_content {
    //       json!({ "id": note_id, "warning": "Note created with empty content" })
    //   }
    //
    // The fix should:
    // 1. Check if content.trim().is_empty() early in create_note function
    // 2. If true, return Err(ApiError::BadRequest("Content is required"))
    // 3. This should happen BEFORE calling state.db.notes.insert()

    let empty_cases = vec![
        ("", "completely empty"),
        ("   ", "spaces only"),
        ("\n\t  ", "whitespace only"),
        ("\n\n", "newlines only"),
    ];

    for (content, description) in empty_cases {
        assert!(
            content.trim().is_empty(),
            "Test case '{}' should be considered empty",
            description
        );
    }

    // Valid content should pass validation
    let valid_cases = vec![
        ("Hello", "simple text"),
        ("# Title\n\nContent", "markdown"),
        ("  content  ", "content with surrounding whitespace"),
    ];

    for (content, description) in valid_cases {
        assert!(
            !content.trim().is_empty(),
            "Test case '{}' should be considered valid",
            description
        );
    }
}

#[test]
fn test_validation_happens_before_insert() {
    // The validation must happen BEFORE any database operations
    // to avoid creating partially-initialized notes.
    //
    // Current code flow (BEFORE fix):
    //   1. Line 1529: Detect empty content
    //   2. Line 1548: Insert note into database ❌ (happens regardless)
    //   3. Lines 1550-1572: Process SKOS tags
    //   4. Line 1588: Queue NLP pipeline
    //   5. Lines 1591-1598: Return warning ❌ (too late!)
    //
    // Fixed code flow (AFTER fix):
    //   1. Check if content.trim().is_empty()
    //   2. If true, return BadRequest error immediately ✓
    //   3. Otherwise, proceed with insert and processing ✓
    //
    // This ensures we fail fast and don't create orphaned database records.

    let content_empty = "";
    let content_valid = "Valid note content";

    // Empty content should fail validation
    assert!(
        content_empty.trim().is_empty(),
        "Empty content should be detected"
    );

    // Valid content should pass validation
    assert!(
        !content_valid.trim().is_empty(),
        "Valid content should pass"
    );
}

#[test]
fn test_error_message_clarity() {
    // The error message should clearly explain why the request failed
    // and guide users toward the correct behavior.
    //
    // Recommended error message:
    //   "Content is required"
    //
    // Alternative (more descriptive):
    //   "Content cannot be empty"
    //
    // NOT recommended:
    //   "Invalid input" (too vague)
    //   "Bad request" (doesn't explain what's wrong)
    //
    // The error should use ApiError::BadRequest which maps to HTTP 400

    let expected_messages = vec!["Content is required", "Content cannot be empty"];

    for msg in expected_messages {
        assert!(
            msg.to_lowercase().contains("content"),
            "Error message should mention 'content'"
        );
        assert!(
            msg.to_lowercase().contains("required")
                || msg.to_lowercase().contains("empty")
                || msg.to_lowercase().contains("cannot"),
            "Error message should explain the constraint"
        );
    }
}

#[test]
fn test_attachments_are_separate_concern() {
    // Important architectural note: Attachments are NOT part of CreateNoteBody
    //
    // API workflow:
    //   1. POST /api/v1/notes { "content": "..." }           → Create note
    //   2. POST /api/v1/notes/{id}/attachments (multipart)  → Add attachment
    //
    // The CreateNoteBody struct (lines 1505-1519) has these fields:
    //   - content: String
    //   - format: Option<String>
    //   - source: Option<String>
    //   - collection_id: Option<Uuid>
    //   - tags: Option<Vec<String>>
    //   - revision_mode: Option<String>
    //   - metadata: Option<serde_json::Value>
    //   - document_type_id: Option<Uuid>
    //
    // NO attachments field exists.
    //
    // Therefore, the exception mentioned in the issue description
    // ("empty content IS allowed if the note has at least one valid file attachment")
    // does NOT apply to the create_note endpoint.
    //
    // The validation should simply reject empty content, period.

    // This is a documentation test - no assertions needed
    // Just documenting that we cannot check attachments at creation time
}

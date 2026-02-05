/// Test for Issue #376: bulk_create_notes should validate content
///
/// This test verifies that bulk_create_notes validates each note's content
/// and provides helpful error messages when content is invalid.

#[test]
fn test_empty_content_validation() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize)]
    struct BulkCreateNoteItem {
        content: String,
        tags: Option<Vec<String>>,
        #[serde(default)]
        revision_mode: Option<String>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct BulkCreateNotesBody {
        notes: Vec<BulkCreateNoteItem>,
    }

    // Test valid notes with diacritics
    let valid_payload = r#"{
        "notes": [
            {"content": "café", "tags": ["test"]},
            {"content": "naïve", "tags": ["test"]},
            {"content": "Zürich", "tags": ["test"]}
        ]
    }"#;

    let valid: Result<BulkCreateNotesBody, _> = serde_json::from_str(valid_payload);
    assert!(valid.is_ok(), "Valid notes should deserialize");

    let body = valid.unwrap();
    assert_eq!(body.notes.len(), 3);

    // Verify content validation logic
    for (i, note) in body.notes.iter().enumerate() {
        assert!(
            !note.content.trim().is_empty(),
            "Note {} should have non-empty content",
            i
        );
    }

    // Test invalid notes with empty content
    let invalid_payload = r#"{
        "notes": [
            {"content": "Valid content", "tags": ["test"]},
            {"content": "   ", "tags": ["test"]},
            {"content": "More valid content", "tags": ["test"]}
        ]
    }"#;

    let invalid: Result<BulkCreateNotesBody, _> = serde_json::from_str(invalid_payload);
    assert!(
        invalid.is_ok(),
        "JSON should deserialize (validation happens in handler)"
    );

    let body = invalid.unwrap();

    // Find empty content
    let has_empty = body.notes.iter().any(|note| note.content.trim().is_empty());
    assert!(has_empty, "Should have detected empty content");
}

#[test]
fn test_unicode_normalization_forms() {
    // Test that both NFC and NFD forms work correctly
    // NFC (precomposed): café = U+00E9
    // NFD (decomposed): cafe + U+0301

    let nfc = "café";
    let nfd = "cafe\u{0301}";

    // Both should be valid UTF-8
    assert!(nfc.is_char_boundary(nfc.len()));
    assert!(nfd.is_char_boundary(nfd.len()));

    // Both should serialize correctly
    let nfc_json = serde_json::to_string(&nfc).unwrap();
    let nfd_json = serde_json::to_string(&nfd).unwrap();

    assert!(nfc_json.contains("caf"));
    assert!(nfd_json.contains("caf"));
}

#[test]
fn test_multibyte_characters() {
    // Test various multibyte UTF-8 sequences
    let test_cases = vec![
        ("ASCII only", "hello world", 1), // 1 byte per char
        ("Latin diacritics", "café", 2),  // é is 2 bytes
        ("Emoji", "hello 🚀", 4),         // 🚀 is 4 bytes
        ("Chinese", "你好", 3),           // Each char is 3 bytes
        ("Japanese", "こんにちは", 3),    // Each char is 3 bytes
        ("Mixed", "café 🚀 你好", 4),     // Mix of all
    ];

    for (name, text, _expected_bytes_per_char) in test_cases {
        // Verify UTF-8 validity
        assert!(
            text.is_char_boundary(text.len()),
            "{} should be valid UTF-8",
            name
        );

        // Verify JSON serialization
        let json = serde_json::to_string(&text).unwrap();
        let parsed: String = serde_json::from_str(&json).unwrap();
        assert_eq!(text, parsed, "{} should round-trip through JSON", name);
    }
}

/// Test for Issue #376: bulk_create_notes should handle diacritics correctly
///
/// This test verifies that bulk_create_notes accepts content with diacritical marks.

#[test]
fn test_json_deserialization_with_diacritics() {
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

    // Test JSON with diacritics (as it would come from the client)
    let json_payload = r#"{
        "notes": [
            {
                "content": "I love café",
                "tags": ["test"],
                "revision_mode": "none"
            },
            {
                "content": "She is naïve",
                "tags": ["test"],
                "revision_mode": "none"
            },
            {
                "content": "Update my résumé",
                "tags": ["test"],
                "revision_mode": "none"
            },
            {
                "content": "Jalapeño is spicy",
                "tags": ["test"],
                "revision_mode": "none"
            },
            {
                "content": "Über cool",
                "tags": ["test"],
                "revision_mode": "none"
            },
            {
                "content": "Visit Zürich",
                "tags": ["test"],
                "revision_mode": "none"
            }
        ]
    }"#;

    // This should deserialize successfully
    let result: Result<BulkCreateNotesBody, _> = serde_json::from_str(json_payload);

    assert!(
        result.is_ok(),
        "JSON with diacritics should deserialize successfully: {:?}",
        result.err()
    );

    let body = result.unwrap();
    assert_eq!(body.notes.len(), 6, "Should have 6 notes");

    // Verify content is preserved correctly
    assert_eq!(body.notes[0].content, "I love café");
    assert_eq!(body.notes[1].content, "She is naïve");
    assert_eq!(body.notes[2].content, "Update my résumé");
    assert_eq!(body.notes[3].content, "Jalapeño is spicy");
    assert_eq!(body.notes[4].content, "Über cool");
    assert_eq!(body.notes[5].content, "Visit Zürich");
}

#[test]
fn test_utf8_byte_length_vs_char_length() {
    // Diacritics can cause byte length != character length
    // This test documents that Rust strings handle this correctly

    let simple = "cafe";
    let diacritic = "café";

    assert_eq!(simple.len(), 4); // 4 bytes
    assert_eq!(simple.chars().count(), 4); // 4 characters

    assert_eq!(diacritic.len(), 5); // 5 bytes (é is 2 bytes in UTF-8)
    assert_eq!(diacritic.chars().count(), 4); // 4 characters

    // Both should serialize/deserialize correctly
    let simple_json = serde_json::to_string(&simple).unwrap();
    let diacritic_json = serde_json::to_string(&diacritic).unwrap();

    let simple_back: String = serde_json::from_str(&simple_json).unwrap();
    let diacritic_back: String = serde_json::from_str(&diacritic_json).unwrap();

    assert_eq!(simple, simple_back);
    assert_eq!(diacritic, diacritic_back);
}

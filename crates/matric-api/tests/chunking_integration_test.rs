//! Integration tests for automatic document chunking in note creation flow.

use serde_json::json;

/// Test response format for create note with chunking metadata
#[derive(Debug, serde::Deserialize)]
struct CreateNoteResponse {
    #[allow(dead_code)]
    id: uuid::Uuid,
    #[serde(default)]
    is_chunked: bool,
    chunk_count: Option<usize>,
    chunk_ids: Option<Vec<uuid::Uuid>>,
}

#[test]
fn test_create_note_response_deserialization_normal() {
    let json_response = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "is_chunked": false
    });

    let response: CreateNoteResponse = serde_json::from_value(json_response).unwrap();
    assert!(!response.is_chunked);
    assert!(response.chunk_count.is_none());
    assert!(response.chunk_ids.is_none());
}

#[test]
fn test_create_note_response_deserialization_chunked() {
    let json_response = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "is_chunked": true,
        "chunk_count": 3,
        "chunk_ids": [
            "550e8400-e29b-41d4-a716-446655440001",
            "550e8400-e29b-41d4-a716-446655440002",
            "550e8400-e29b-41d4-a716-446655440003"
        ]
    });

    let response: CreateNoteResponse = serde_json::from_value(json_response).unwrap();
    assert!(response.is_chunked);
    assert_eq!(response.chunk_count, Some(3));
    assert_eq!(response.chunk_ids.as_ref().unwrap().len(), 3);
}

#[test]
fn test_create_note_response_backward_compatibility() {
    // Old response format without chunking fields should still work
    let json_response = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000"
    });

    let response: CreateNoteResponse = serde_json::from_value(json_response).unwrap();
    assert!(!response.is_chunked); // default value
    assert!(response.chunk_count.is_none());
    assert!(response.chunk_ids.is_none());
}

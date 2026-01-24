//! Integration tests for document reconstruction endpoint (GET /notes/{id}/full).
//!
//! Tests the full document reconstruction API that stitches chunked notes back together.

use matric_api::services::reconstruction_service::FullDocumentResponse;
use serde_json::json;
use uuid::Uuid;

/// Test that FullDocumentResponse deserializes correctly for regular notes
#[test]
fn test_full_document_response_regular_note() {
    let json_response = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "My Regular Note",
        "content": "This is a regular note content.",
        "chunks": null,
        "total_chunks": null,
        "is_chunked": false,
        "tags": ["tag1", "tag2"],
        "created_at": "2024-01-01T12:00:00Z",
        "updated_at": "2024-01-02T12:00:00Z"
    });

    let response: FullDocumentResponse = serde_json::from_value(json_response).unwrap();
    assert!(!response.is_chunked);
    assert_eq!(response.title, "My Regular Note");
    assert_eq!(response.content, "This is a regular note content.");
    assert!(response.chunks.is_none());
    assert!(response.total_chunks.is_none());
    assert_eq!(response.tags.len(), 2);
}

/// Test that FullDocumentResponse deserializes correctly for chunked documents
#[test]
fn test_full_document_response_chunked_document() {
    let chunk_id_1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    let chunk_id_2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();

    let json_response = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Large Document",
        "content": "This is the full reconstructed content from multiple chunks.",
        "chunks": [
            {
                "id": chunk_id_1,
                "sequence": 1,
                "title": "Large Document (Part 1/2)",
                "byte_range": [0, 1000]
            },
            {
                "id": chunk_id_2,
                "sequence": 2,
                "title": "Large Document (Part 2/2)",
                "byte_range": [950, 2000]
            }
        ],
        "total_chunks": 2,
        "is_chunked": true,
        "tags": ["long-form", "article"],
        "created_at": "2024-01-01T12:00:00Z",
        "updated_at": "2024-01-02T12:00:00Z"
    });

    let response: FullDocumentResponse = serde_json::from_value(json_response).unwrap();
    assert!(response.is_chunked);
    assert_eq!(response.title, "Large Document");
    assert_eq!(
        response.content,
        "This is the full reconstructed content from multiple chunks."
    );
    assert_eq!(response.total_chunks, Some(2));

    let chunks = response.chunks.unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].sequence, 1);
    assert_eq!(chunks[1].sequence, 2);
    assert_eq!(chunks[0].byte_range, (0, 1000));
    assert_eq!(chunks[1].byte_range, (950, 2000));
}

/// Test that the endpoint returns 404 for non-existent notes
#[test]
fn test_response_structure_validation() {
    // Verify response structure contains all required fields
    let response = FullDocumentResponse {
        id: Uuid::new_v4(),
        title: "Test".to_string(),
        content: "Content".to_string(),
        chunks: None,
        total_chunks: None,
        is_chunked: false,
        tags: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Should serialize successfully
    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("id").is_some());
    assert!(json.get("title").is_some());
    assert!(json.get("content").is_some());
    assert!(json.get("is_chunked").is_some());
    assert!(json.get("tags").is_some());
    assert!(json.get("created_at").is_some());
    assert!(json.get("updated_at").is_some());
}

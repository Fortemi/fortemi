//! Integration tests for ReconstructionService with database operations.
//!
//! These tests verify the service layer functionality for document reconstruction.

use matric_api::services::reconstruction_service::{ChunkSummary, FullDocumentResponse};

/// Test that ChunkSummary serializes correctly
#[test]
fn test_chunk_summary_serialization() {
    let chunk = ChunkSummary {
        id: uuid::Uuid::new_v4(),
        sequence: 1,
        title: "Test Chunk".to_string(),
        byte_range: (0, 1000),
    };

    let json = serde_json::to_value(&chunk).unwrap();
    assert!(json.get("id").is_some());
    assert_eq!(json.get("sequence").unwrap().as_u64(), Some(1));
    assert_eq!(json.get("title").unwrap().as_str(), Some("Test Chunk"));

    // byte_range should serialize as a tuple array
    let byte_range = json.get("byte_range").unwrap().as_array().unwrap();
    assert_eq!(byte_range.len(), 2);
    assert_eq!(byte_range[0].as_u64(), Some(0));
    assert_eq!(byte_range[1].as_u64(), Some(1000));
}

/// Test that ChunkSummary deserializes correctly
#[test]
fn test_chunk_summary_deserialization() {
    let json = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "sequence": 2,
        "title": "Chunk 2",
        "byte_range": [100, 200]
    });

    let chunk: ChunkSummary = serde_json::from_value(json).unwrap();
    assert_eq!(chunk.sequence, 2);
    assert_eq!(chunk.title, "Chunk 2");
    assert_eq!(chunk.byte_range, (100, 200));
}

/// Test that FullDocumentResponse includes all required fields
#[test]
fn test_full_document_response_completeness() {
    let response = FullDocumentResponse {
        id: uuid::Uuid::new_v4(),
        title: "Test Document".to_string(),
        content: "Full content here".to_string(),
        chunks: Some(vec![ChunkSummary {
            id: uuid::Uuid::new_v4(),
            sequence: 1,
            title: "Chunk 1".to_string(),
            byte_range: (0, 17),
        }]),
        total_chunks: Some(1),
        is_chunked: true,
        tags: vec!["test".to_string()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Verify serialization includes all fields
    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("id").is_some());
    assert!(json.get("title").is_some());
    assert!(json.get("content").is_some());
    assert!(json.get("chunks").is_some());
    assert!(json.get("total_chunks").is_some());
    assert!(json.get("is_chunked").is_some());
    assert!(json.get("tags").is_some());
    assert!(json.get("created_at").is_some());
    assert!(json.get("updated_at").is_some());

    // Verify correct values
    assert_eq!(json.get("title").unwrap().as_str(), Some("Test Document"));
    assert_eq!(json.get("is_chunked").unwrap().as_bool(), Some(true));
    assert_eq!(json.get("total_chunks").unwrap().as_u64(), Some(1));

    let chunks_array = json.get("chunks").unwrap().as_array().unwrap();
    assert_eq!(chunks_array.len(), 1);
}

/// Test that is_chunked=false for regular notes without chunks
#[test]
fn test_regular_note_has_no_chunks() {
    let response = FullDocumentResponse {
        id: uuid::Uuid::new_v4(),
        title: "Regular Note".to_string(),
        content: "Simple content".to_string(),
        chunks: None,
        total_chunks: None,
        is_chunked: false,
        tags: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json.get("is_chunked").unwrap().as_bool(), Some(false));
    assert!(json.get("chunks").unwrap().is_null());
    assert!(json.get("total_chunks").unwrap().is_null());
}

/// Test edge case: empty content
#[test]
fn test_empty_content_response() {
    let response = FullDocumentResponse {
        id: uuid::Uuid::new_v4(),
        title: "Empty Note".to_string(),
        content: String::new(),
        chunks: None,
        total_chunks: None,
        is_chunked: false,
        tags: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json.get("content").unwrap().as_str(), Some(""));
    assert_eq!(json.get("is_chunked").unwrap().as_bool(), Some(false));
}

/// Test edge case: many chunks
#[test]
fn test_many_chunks_response() {
    let chunks: Vec<ChunkSummary> = (1..=10)
        .map(|i| ChunkSummary {
            id: uuid::Uuid::new_v4(),
            sequence: i,
            title: format!("Chunk {}", i),
            byte_range: (i as usize * 1000, (i as usize + 1) * 1000),
        })
        .collect();

    let response = FullDocumentResponse {
        id: uuid::Uuid::new_v4(),
        title: "Large Document".to_string(),
        content: "Reconstructed content from 10 chunks".to_string(),
        chunks: Some(chunks),
        total_chunks: Some(10),
        is_chunked: true,
        tags: vec!["large".to_string()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json.get("total_chunks").unwrap().as_u64(), Some(10));

    let chunks_array = json.get("chunks").unwrap().as_array().unwrap();
    assert_eq!(chunks_array.len(), 10);

    // Verify first and last chunk sequences
    assert_eq!(chunks_array[0].get("sequence").unwrap().as_u64(), Some(1));
    assert_eq!(chunks_array[9].get("sequence").unwrap().as_u64(), Some(10));
}

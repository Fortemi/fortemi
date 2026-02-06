/// Test to verify that all chunking default values are aligned across the codebase.
///
/// This test ensures that:
/// - EmbeddingConfig::default() chunk values match ChunkerConfig::default()
/// - default_chunk_size() and default_chunk_overlap() match the same values
///
/// The canonical values are set in ChunkerConfig::default() (1000/100/100).
use matric_core::models::EmbeddingConfig;

#[test]
fn test_embedding_config_default_chunk_values() {
    let embedding_config = EmbeddingConfig::default();

    // Verify chunk_size matches ChunkerConfig::default() max_chunk_size (1000)
    assert_eq!(
        embedding_config.chunk_size, 1000,
        "EmbeddingConfig::default() chunk_size should be 1000"
    );

    // Verify chunk_overlap matches ChunkerConfig::default() overlap (100)
    assert_eq!(
        embedding_config.chunk_overlap, 100,
        "EmbeddingConfig::default() chunk_overlap should be 100"
    );

    // Verify other defaults are unchanged
    assert_eq!(
        embedding_config.model, "nomic-embed-text",
        "EmbeddingConfig::default() model should be nomic-embed-text"
    );
    assert_eq!(
        embedding_config.dimension, 768,
        "EmbeddingConfig::default() dimension should be 768"
    );
}

#[test]
fn test_create_document_type_request_defaults_via_json() {
    // Test CreateDocumentTypeRequest defaults via JSON deserialization
    // which will trigger the serde default functions
    let json = r#"{
        "name": "test",
        "display_name": "Test",
        "category": "prose"
    }"#;

    let doc_type_request: matric_core::models::CreateDocumentTypeRequest =
        serde_json::from_str(json).expect("Failed to deserialize");

    // Verify chunk_size_default matches ChunkerConfig::default() max_chunk_size (1000)
    assert_eq!(
        doc_type_request.chunk_size_default, 1000,
        "CreateDocumentTypeRequest chunk_size_default should be 1000"
    );

    // Verify chunk_overlap_default matches ChunkerConfig::default() overlap (100)
    assert_eq!(
        doc_type_request.chunk_overlap_default, 100,
        "CreateDocumentTypeRequest chunk_overlap_default should be 100"
    );
}

#[test]
fn test_all_chunking_defaults_aligned() {
    // This test verifies that all three sources of chunk defaults are aligned:
    // 1. EmbeddingConfig::default()
    // 2. CreateDocumentTypeRequest defaults (via default_chunk_size/default_chunk_overlap)
    // 3. ChunkerConfig::default() (in matric-db, assumed to be 1000/100/100)

    let embedding_config = EmbeddingConfig::default();

    let json = r#"{
        "name": "test",
        "display_name": "Test",
        "category": "prose"
    }"#;

    let doc_type_request: matric_core::models::CreateDocumentTypeRequest =
        serde_json::from_str(json).expect("Failed to deserialize");

    // All chunk sizes should be 1000
    assert_eq!(
        embedding_config.chunk_size, doc_type_request.chunk_size_default as usize,
        "EmbeddingConfig and CreateDocumentTypeRequest chunk_size should match"
    );

    // All chunk overlaps should be 100
    assert_eq!(
        embedding_config.chunk_overlap, doc_type_request.chunk_overlap_default as usize,
        "EmbeddingConfig and CreateDocumentTypeRequest chunk_overlap should match"
    );
}

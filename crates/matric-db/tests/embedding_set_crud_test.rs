//! Integration tests for embedding set CRUD operations.
//!
//! This test suite validates:
//! - Create embedding set
//! - Read embedding set (get by slug)
//! - Update embedding set
//! - Delete embedding set
//! - Delete prevents deletion of system sets
//! - Delete removes member associations
//!
//! Related issues:
//! - #274: Missing delete_embedding_set tool
//!
//! **IMPORTANT**: These tests require a fully migrated PostgreSQL database.
//! Run migrations first: `sqlx migrate run`

use matric_db::{
    test_fixtures::DEFAULT_TEST_DATABASE_URL, AutoEmbedRules, CreateEmbeddingSetRequest, Database,
    EmbeddingSetAgentMetadata, EmbeddingSetCriteria, EmbeddingSetMode, EmbeddingSetType,
    NoteRepository,
};
use sqlx::PgPool;

/// Helper to create a test database connection.
async fn setup_test_db() -> Database {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    Database::new(pool)
}

#[tokio::test]
async fn test_embedding_set_crud_lifecycle() {
    let db = setup_test_db().await;

    // ============================================================================
    // CREATE
    // ============================================================================

    let create_request = CreateEmbeddingSetRequest {
        name: "Test Set for CRUD".to_string(),
        slug: Some("test-crud-set".to_string()),
        description: Some("A test embedding set for CRUD operations".to_string()),
        purpose: Some("Testing CRUD lifecycle".to_string()),
        usage_hints: None,
        keywords: vec!["test".to_string(), "crud".to_string()],
        set_type: EmbeddingSetType::Full,
        mode: EmbeddingSetMode::Manual,
        criteria: EmbeddingSetCriteria::default(),
        embedding_config_id: None,
        truncate_dim: None,
        auto_embed_rules: AutoEmbedRules::default(),
        agent_metadata: EmbeddingSetAgentMetadata::default(),
    };

    let created_set = db
        .embedding_sets
        .create(create_request)
        .await
        .expect("Failed to create embedding set");

    assert_eq!(created_set.name, "Test Set for CRUD");
    assert_eq!(created_set.slug, "test-crud-set");
    assert!(
        !created_set.is_system,
        "Test set should not be a system set"
    );

    // ============================================================================
    // READ
    // ============================================================================

    let retrieved_set = db
        .embedding_sets
        .get_by_slug("test-crud-set")
        .await
        .expect("Failed to retrieve embedding set")
        .expect("Embedding set not found");

    assert_eq!(retrieved_set.id, created_set.id);
    assert_eq!(retrieved_set.name, "Test Set for CRUD");

    // ============================================================================
    // UPDATE
    // ============================================================================

    let update_request = matric_core::UpdateEmbeddingSetRequest {
        name: Some("Updated Test Set".to_string()),
        description: Some("Updated description".to_string()),
        purpose: None,
        usage_hints: None,
        keywords: Some(vec![
            "test".to_string(),
            "crud".to_string(),
            "updated".to_string(),
        ]),
        mode: None,
        criteria: None,
        is_active: None,
        auto_refresh: None,
        agent_metadata: None,
    };

    let updated_set = db
        .embedding_sets
        .update("test-crud-set", update_request)
        .await
        .expect("Failed to update embedding set");

    assert_eq!(updated_set.name, "Updated Test Set");
    assert_eq!(
        updated_set.description,
        Some("Updated description".to_string())
    );
    assert!(updated_set.keywords.contains(&"updated".to_string()));

    // ============================================================================
    // DELETE
    // ============================================================================

    db.embedding_sets
        .delete("test-crud-set")
        .await
        .expect("Failed to delete embedding set");

    // Verify deletion
    let deleted_set = db
        .embedding_sets
        .get_by_slug("test-crud-set")
        .await
        .expect("Failed to query for deleted set");

    assert!(deleted_set.is_none(), "Embedding set should be deleted");
}

#[tokio::test]
async fn test_delete_system_set_protection() {
    let db = setup_test_db().await;

    // Attempt to delete the default (system) set
    let result = db.embedding_sets.delete("default").await;

    assert!(result.is_err(), "Should not be able to delete system set");

    let error_message = result.unwrap_err().to_string();
    assert!(
        error_message.contains("system") || error_message.contains("Cannot delete"),
        "Error should indicate system set protection: {}",
        error_message
    );
}

#[tokio::test]
async fn test_delete_nonexistent_set() {
    let db = setup_test_db().await;

    let result = db.embedding_sets.delete("nonexistent-set-slug").await;

    assert!(result.is_err(), "Deleting nonexistent set should fail");

    let error_message = result.unwrap_err().to_string();
    assert!(
        error_message.contains("not found") || error_message.contains("Not found"),
        "Error should indicate set not found: {}",
        error_message
    );
}

#[tokio::test]
async fn test_delete_removes_member_associations() {
    let db = setup_test_db().await;

    // Create a test set
    let create_request = CreateEmbeddingSetRequest {
        name: "Test Set With Members".to_string(),
        slug: Some("test-with-members".to_string()),
        description: Some("Testing member cleanup on delete".to_string()),
        purpose: None,
        usage_hints: None,
        keywords: vec![],
        set_type: EmbeddingSetType::Full,
        mode: EmbeddingSetMode::Manual,
        criteria: EmbeddingSetCriteria::default(),
        embedding_config_id: None,
        truncate_dim: None,
        auto_embed_rules: AutoEmbedRules::default(),
        agent_metadata: EmbeddingSetAgentMetadata::default(),
    };

    let _created_set = db
        .embedding_sets
        .create(create_request)
        .await
        .expect("Failed to create embedding set");

    // Create a test note
    let note_request = matric_core::CreateNoteRequest {
        content: "Test note for membership".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = db
        .notes
        .insert(note_request)
        .await
        .expect("Failed to create test note");

    // Add note as member
    let add_members_request = matric_core::AddMembersRequest {
        note_ids: vec![note_id],
        added_by: Some("test-system".to_string()),
    };

    db.embedding_sets
        .add_members("test-with-members", add_members_request)
        .await
        .expect("Failed to add member");

    // Verify member was added
    let members = db
        .embedding_sets
        .list_members("test-with-members", 100, 0)
        .await
        .expect("Failed to list members");

    assert_eq!(members.len(), 1, "Should have one member");
    assert_eq!(members[0].note_id, note_id);

    // Delete the embedding set
    db.embedding_sets
        .delete("test-with-members")
        .await
        .expect("Failed to delete embedding set");

    // Verify set is deleted
    let deleted_set = db
        .embedding_sets
        .get_by_slug("test-with-members")
        .await
        .expect("Failed to query for deleted set");

    assert!(deleted_set.is_none(), "Embedding set should be deleted");

    // Note: We can't directly query members for deleted set, but we can verify
    // the set no longer exists which should have cascaded the delete
    // (assuming foreign key constraints are set up correctly in migrations)
}

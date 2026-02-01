//! Integration test to verify embedding_set_id is properly set when storing embeddings.
//!
//! This test addresses issue #353 where embeddings were being stored without
//! the embedding_set_id field being set, causing them to be orphaned.

use matric_core::{CreateNoteRequest, EmbeddingRepository, NoteRepository};
use matric_db::{PgEmbeddingRepository, PgNoteRepository};
use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper to get database connection from environment.
async fn get_test_pool() -> PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Test that embeddings are stored with proper embedding_set_id.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_embeddings_have_embedding_set_id() {
    let pool = get_test_pool().await;
    let note_repo = PgNoteRepository::new(pool.clone());
    let embedding_repo = PgEmbeddingRepository::new(pool.clone());

    // Create a test note
    let note_id = note_repo
        .insert(CreateNoteRequest {
            collection_id: None,
            format: "markdown".to_string(),
            source: "test".to_string(),
            content: "Test content for embedding".to_string(),
            tags: None,
            metadata: None,
        })
        .await
        .expect("Failed to create test note");

    // Create test embeddings
    let test_vector = Vector::from(vec![0.1_f32; 768]);
    let chunks = vec![
        ("Test chunk 1".to_string(), test_vector.clone()),
        ("Test chunk 2".to_string(), test_vector.clone()),
    ];

    // Store embeddings
    embedding_repo
        .store(note_id, chunks, "test-model")
        .await
        .expect("Failed to store embeddings");

    // Verify embeddings have embedding_set_id set
    let result: Vec<(Uuid, Option<Uuid>)> =
        sqlx::query_as("SELECT id, embedding_set_id FROM embedding WHERE note_id = $1")
            .bind(note_id)
            .fetch_all(&pool)
            .await
            .expect("Failed to query embeddings");

    // Verify we have embeddings
    assert_eq!(result.len(), 2, "Should have stored 2 embeddings");

    // Verify all embeddings have embedding_set_id set
    for (embedding_id, embedding_set_id) in result {
        assert!(
            embedding_set_id.is_some(),
            "Embedding {} should have embedding_set_id set",
            embedding_id
        );
    }

    // Verify the embedding_set_id is the default set
    let default_set_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default' LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to query default embedding set");

    if let Some(default_id) = default_set_id {
        // Verify all embeddings use the default set
        let embedding_set_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT embedding_set_id FROM embedding WHERE note_id = $1",
        )
        .bind(note_id)
        .fetch_all(&pool)
        .await
        .expect("Failed to query embedding set IDs");

        assert_eq!(
            embedding_set_ids.len(),
            1,
            "All embeddings should use the same set"
        );
        assert_eq!(
            embedding_set_ids[0], default_id,
            "Embeddings should use the default set"
        );
    }

    // Cleanup
    note_repo
        .hard_delete(note_id)
        .await
        .expect("Failed to cleanup test note");
}

/// Test that embeddings can be found by embedding_set_id.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_find_embeddings_by_set() {
    let pool = get_test_pool().await;
    let note_repo = PgNoteRepository::new(pool.clone());
    let embedding_repo = PgEmbeddingRepository::new(pool.clone());

    // Create a test note
    let note_id = note_repo
        .insert(CreateNoteRequest {
            collection_id: None,
            format: "markdown".to_string(),
            source: "test".to_string(),
            content: "Test content for set-based search".to_string(),
            tags: None,
            metadata: None,
        })
        .await
        .expect("Failed to create test note");

    // Create test embeddings
    let test_vector = Vector::from(vec![0.5_f32; 768]);
    let chunks = vec![("Test chunk for set search".to_string(), test_vector.clone())];

    // Store embeddings
    embedding_repo
        .store(note_id, chunks, "test-model")
        .await
        .expect("Failed to store embeddings");

    // Get the default set ID
    let default_set_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to get default embedding set");

    // Search within the embedding set
    let query_vec = Vector::from(vec![0.5_f32; 768]);
    let results = embedding_repo
        .find_similar_in_set(&query_vec, default_set_id, 10, true)
        .await
        .expect("Failed to search in embedding set");

    // Verify we can find the note in the set
    let found = results.iter().any(|hit| hit.note_id == note_id);
    assert!(
        found,
        "Should find the note when searching within its embedding set"
    );

    // Cleanup
    note_repo
        .hard_delete(note_id)
        .await
        .expect("Failed to cleanup test note");
}

/// Test that updating embeddings maintains embedding_set_id.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_update_embeddings_preserves_set_id() {
    let pool = get_test_pool().await;
    let note_repo = PgNoteRepository::new(pool.clone());
    let embedding_repo = PgEmbeddingRepository::new(pool.clone());

    // Create a test note
    let note_id = note_repo
        .insert(CreateNoteRequest {
            collection_id: None,
            format: "markdown".to_string(),
            source: "test".to_string(),
            content: "Initial content".to_string(),
            tags: None,
            metadata: None,
        })
        .await
        .expect("Failed to create test note");

    // Store initial embeddings
    let test_vector1 = Vector::from(vec![0.1_f32; 768]);
    let chunks1 = vec![("Initial chunk".to_string(), test_vector1)];

    embedding_repo
        .store(note_id, chunks1, "test-model")
        .await
        .expect("Failed to store initial embeddings");

    // Get the initial embedding_set_id
    let initial_set_id: Uuid =
        sqlx::query_scalar("SELECT embedding_set_id FROM embedding WHERE note_id = $1 LIMIT 1")
            .bind(note_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to get initial embedding_set_id");

    // Update embeddings (store overwrites existing ones)
    let test_vector2 = Vector::from(vec![0.2_f32; 768]);
    let chunks2 = vec![
        ("Updated chunk 1".to_string(), test_vector2.clone()),
        ("Updated chunk 2".to_string(), test_vector2),
    ];

    embedding_repo
        .store(note_id, chunks2, "test-model")
        .await
        .expect("Failed to update embeddings");

    // Verify new embeddings have the same embedding_set_id
    let updated_set_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT embedding_set_id FROM embedding WHERE note_id = $1")
            .bind(note_id)
            .fetch_all(&pool)
            .await
            .expect("Failed to query updated embedding_set_ids");

    assert_eq!(
        updated_set_ids.len(),
        1,
        "All embeddings should use the same set after update"
    );
    assert_eq!(
        updated_set_ids[0], initial_set_id,
        "Embedding set ID should be preserved during update"
    );

    // Cleanup
    note_repo
        .hard_delete(note_id)
        .await
        .expect("Failed to cleanup test note");
}

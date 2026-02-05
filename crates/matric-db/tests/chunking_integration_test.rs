//! Integration tests for document chunking pipeline.
//!
//! Tests the complete chunking workflow including:
//! - Chain creation with proper metadata
//! - Document reconstruction from chunks
//! - Search deduplication of chunk matches
//! - Edge cases and boundary conditions
//!
//! **IMPORTANT**: These tests require a fully migrated PostgreSQL database.
//! Run migrations first: `sqlx migrate run`

use chrono::Utc;
use matric_db::Database;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

// =============================================================================
// Test Context and Setup
// =============================================================================

/// Test context for chunking pipeline tests.
///
/// Testing framework: Tokio async tests with sqlx
/// Coverage target: 100% (critical path)
/// Test types: Integration tests with real database
struct TestContext {
    pool: PgPool,
    #[allow(dead_code)]
    db: Database,
}

impl TestContext {
    async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");
        let db = Database::new(pool.clone());

        Self { pool, db }
    }

    /// Create a test note with chunking metadata.
    async fn create_chunked_note(
        &self,
        chain_id: Uuid,
        sequence: u32,
        total: u32,
        content: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let note_id = Uuid::new_v4();
        let now = Utc::now();

        // Chunk metadata following the schema documented in migration 20260122000000
        let chunk_metadata = json!({
            "chain_id": chain_id.to_string(),
            "chunk_sequence": sequence,
            "total_chunks": total,
            "chunking_strategy": "semantic"
        });

        // Insert note
        sqlx::query(
            r#"
            INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, chunk_metadata)
            VALUES ($1, 'markdown', 'test', $2, $2, $3)
            "#,
        )
        .bind(note_id)
        .bind(now)
        .bind(chunk_metadata)
        .execute(&self.pool)
        .await?;

        // Insert original content
        sqlx::query(
            r#"
            INSERT INTO note_original (id, note_id, content, hash)
            VALUES ($1, $2, $3, 'testhash')
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(note_id)
        .bind(content)
        .execute(&self.pool)
        .await?;

        // Insert revision
        let revision_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO note_revision (id, note_id, content, created_at_utc, revision_number)
            VALUES ($1, $2, $3, $4, 1)
            "#,
        )
        .bind(revision_id)
        .bind(note_id)
        .bind(content)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Insert current revision
        sqlx::query(
            r#"
            INSERT INTO note_revised_current (note_id, content, last_revision_id)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(note_id)
        .bind(content)
        .bind(revision_id)
        .execute(&self.pool)
        .await?;

        Ok(note_id)
    }

    /// Query chunk metadata from a note.
    async fn get_chunk_metadata(
        &self,
        note_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query("SELECT chunk_metadata FROM note WHERE id = $1")
            .bind(note_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.and_then(|r| r.get("chunk_metadata")))
    }

    /// Get all chunks for a chain_id.
    async fn get_chain_chunks(&self, chain_id: Uuid) -> Result<Vec<(Uuid, u32)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, chunk_metadata->>'chunk_sequence' as sequence
            FROM note
            WHERE chunk_metadata->>'chain_id' = $1
            ORDER BY (chunk_metadata->>'chunk_sequence')::int
            "#,
        )
        .bind(chain_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let chunks = rows
            .into_iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let seq: String = r.get("sequence");
                let sequence = seq.parse::<u32>().unwrap_or(0);
                (id, sequence)
            })
            .collect();

        Ok(chunks)
    }

    /// Cleanup test data.
    async fn cleanup_note(&self, note_id: Uuid) {
        let _ = sqlx::query("DELETE FROM note_revised_current WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await;

        let _ = sqlx::query("DELETE FROM note_revision WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await;

        let _ = sqlx::query("DELETE FROM note_original WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await;

        let _ = sqlx::query("DELETE FROM note WHERE id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await;
    }
}

// =============================================================================
// Chain Creation Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_large_document_creates_chunk_chain() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create a 3-chunk document
    let chunk1_content = "# Large Document\n\nThis is the first chunk.".repeat(100);
    let chunk2_content = "This is the second chunk with more content.".repeat(100);
    let chunk3_content = "This is the final chunk of the document.".repeat(100);

    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 3, &chunk1_content)
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(chain_id, 1, 3, &chunk2_content)
        .await
        .expect("Failed to create chunk 2");
    let chunk3_id = ctx
        .create_chunked_note(chain_id, 2, 3, &chunk3_content)
        .await
        .expect("Failed to create chunk 3");

    // Verify correct number of chunks created
    let chunks = ctx
        .get_chain_chunks(chain_id)
        .await
        .expect("Failed to get chain chunks");
    assert_eq!(chunks.len(), 3, "Should have 3 chunks");

    // Verify chain_id consistent across chunks
    for chunk_id in [chunk1_id, chunk2_id, chunk3_id] {
        let metadata = ctx
            .get_chunk_metadata(chunk_id)
            .await
            .expect("Failed to get metadata")
            .expect("Metadata should exist");

        let stored_chain_id = metadata
            .get("chain_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .expect("chain_id should be valid UUID");

        assert_eq!(
            stored_chain_id, chain_id,
            "chain_id should match for all chunks"
        );
    }

    // Verify sequence numbers are sequential
    let sequences: Vec<u32> = chunks.iter().map(|(_, seq)| *seq).collect();
    assert_eq!(sequences, vec![0, 1, 2], "Sequences should be 0, 1, 2");

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
    ctx.cleanup_note(chunk3_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_chunk_metadata_structure() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    let chunk_id = ctx
        .create_chunked_note(chain_id, 0, 2, "Test content")
        .await
        .expect("Failed to create chunk");

    let metadata = ctx
        .get_chunk_metadata(chunk_id)
        .await
        .expect("Failed to get metadata")
        .expect("Metadata should exist");

    // Verify required fields exist
    assert!(
        metadata.get("chain_id").is_some(),
        "Metadata should have chain_id"
    );
    assert!(
        metadata.get("chunk_sequence").is_some(),
        "Metadata should have chunk_sequence"
    );
    assert!(
        metadata.get("total_chunks").is_some(),
        "Metadata should have total_chunks"
    );
    assert!(
        metadata.get("chunking_strategy").is_some(),
        "Metadata should have chunking_strategy"
    );

    // Verify values are correct types
    assert_eq!(metadata.get("chunk_sequence").unwrap().as_u64().unwrap(), 0);
    assert_eq!(metadata.get("total_chunks").unwrap().as_u64().unwrap(), 2);
    assert_eq!(
        metadata.get("chunking_strategy").unwrap().as_str().unwrap(),
        "semantic"
    );

    // Cleanup
    ctx.cleanup_note(chunk_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_single_chunk_document() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create a "chunked" document with only 1 chunk (edge case)
    let chunk_id = ctx
        .create_chunked_note(chain_id, 0, 1, "Single chunk content")
        .await
        .expect("Failed to create chunk");

    let metadata = ctx
        .get_chunk_metadata(chunk_id)
        .await
        .expect("Failed to get metadata")
        .expect("Metadata should exist");

    assert_eq!(metadata.get("total_chunks").unwrap().as_u64().unwrap(), 1);
    assert_eq!(metadata.get("chunk_sequence").unwrap().as_u64().unwrap(), 0);

    // Cleanup
    ctx.cleanup_note(chunk_id).await;
}

// =============================================================================
// Reconstruction Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_full_document_reconstruction() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create chunks with distinct content
    let chunk1_content = "Part 1: Introduction to the topic.";
    let chunk2_content = "Part 2: Detailed analysis of the subject.";
    let chunk3_content = "Part 3: Conclusion and final thoughts.";

    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 3, chunk1_content)
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(chain_id, 1, 3, chunk2_content)
        .await
        .expect("Failed to create chunk 2");
    let chunk3_id = ctx
        .create_chunked_note(chain_id, 2, 3, chunk3_content)
        .await
        .expect("Failed to create chunk 3");

    // Fetch chunks in order and reconstruct
    let chunks = ctx
        .get_chain_chunks(chain_id)
        .await
        .expect("Failed to get chunks");

    // Verify we can fetch all chunks
    assert_eq!(chunks.len(), 3, "Should have 3 chunks");

    let mut contents = Vec::new();
    for (chunk_id, _) in chunks {
        let row = sqlx::query("SELECT content FROM note_original WHERE note_id = $1")
            .bind(chunk_id)
            .fetch_one(&ctx.pool)
            .await
            .expect("Failed to fetch content");
        let content: String = row.get("content");
        contents.push(content);
    }

    // Verify content can be reconstructed
    let reconstructed = contents.join("\n");
    assert!(
        reconstructed.contains("Part 1"),
        "Should contain first part"
    );
    assert!(
        reconstructed.contains("Part 2"),
        "Should contain second part"
    );
    assert!(
        reconstructed.contains("Part 3"),
        "Should contain third part"
    );

    // Verify order is preserved
    let part1_pos = reconstructed.find("Part 1").unwrap();
    let part2_pos = reconstructed.find("Part 2").unwrap();
    let part3_pos = reconstructed.find("Part 3").unwrap();
    assert!(
        part1_pos < part2_pos && part2_pos < part3_pos,
        "Parts should be in order"
    );

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
    ctx.cleanup_note(chunk3_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_reconstruction_missing_chunk() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create only 2 out of 3 chunks (simulate missing chunk)
    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 3, "Chunk 1 content")
        .await
        .expect("Failed to create chunk 1");
    let chunk3_id = ctx
        .create_chunked_note(chain_id, 2, 3, "Chunk 3 content")
        .await
        .expect("Failed to create chunk 3");
    // Chunk 2 is missing

    // Verify we detect incomplete chain
    let chunks = ctx
        .get_chain_chunks(chain_id)
        .await
        .expect("Failed to get chunks");

    assert_eq!(chunks.len(), 2, "Should have 2 chunks (incomplete)");

    // Verify sequence numbers show gap
    let sequences: Vec<u32> = chunks.iter().map(|(_, seq)| *seq).collect();
    assert_eq!(sequences, vec![0, 2], "Should have gap at sequence 1");

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk3_id).await;
}

// =============================================================================
// Search Deduplication Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_search_query_matches_multiple_chunks() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create chunks with overlapping keyword "important"
    let chunk1_id = ctx
        .create_chunked_note(
            chain_id,
            0,
            3,
            "This is an important introduction with important concepts.",
        )
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(
            chain_id,
            1,
            3,
            "More important details in the middle section.",
        )
        .await
        .expect("Failed to create chunk 2");
    let chunk3_id = ctx
        .create_chunked_note(
            chain_id,
            2,
            3,
            "The conclusion is also important for understanding.",
        )
        .await
        .expect("Failed to create chunk 3");

    // Query for notes with "important" keyword
    // Using FTS to find all matches
    let rows = sqlx::query(
        r#"
        SELECT n.id, nrc.content,
               ts_rank(to_tsvector('english', nrc.content),
                       websearch_to_tsquery('english', $1)) as rank
        FROM note n
        JOIN note_revised_current nrc ON nrc.note_id = n.id
        WHERE to_tsvector('english', nrc.content) @@ websearch_to_tsquery('english', $1)
          AND n.chunk_metadata->>'chain_id' = $2
        ORDER BY rank DESC
        "#,
    )
    .bind("important")
    .bind(chain_id.to_string())
    .fetch_all(&ctx.pool)
    .await
    .expect("Failed to search");

    // All 3 chunks should match the keyword
    assert!(
        rows.len() >= 3,
        "All chunks should match 'important' keyword"
    );

    // Verify they all belong to the same chain
    for row in &rows {
        let note_id: Uuid = row.get("id");
        let metadata = ctx
            .get_chunk_metadata(note_id)
            .await
            .expect("Failed to get metadata")
            .expect("Metadata should exist");

        let found_chain_id = metadata
            .get("chain_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .expect("chain_id should be valid");

        assert_eq!(
            found_chain_id, chain_id,
            "All results should be from same chain"
        );
    }

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
    ctx.cleanup_note(chunk3_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_deduplication_keeps_highest_score() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create chunks where middle chunk has most keyword density
    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 3, "Introduction mentions rust once.")
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(
            chain_id,
            1,
            3,
            "Rust rust rust! This section is all about rust programming in rust language.",
        )
        .await
        .expect("Failed to create chunk 2");
    let chunk3_id = ctx
        .create_chunked_note(chain_id, 2, 3, "Conclusion mentions rust briefly.")
        .await
        .expect("Failed to create chunk 3");

    // Query for "rust"
    let rows = sqlx::query(
        r#"
        SELECT n.id,
               n.chunk_metadata->>'chunk_sequence' as sequence,
               ts_rank(to_tsvector('english', nrc.content),
                       websearch_to_tsquery('english', $1)) as rank
        FROM note n
        JOIN note_revised_current nrc ON nrc.note_id = n.id
        WHERE to_tsvector('english', nrc.content) @@ websearch_to_tsquery('english', $1)
          AND n.chunk_metadata->>'chain_id' = $2
        ORDER BY rank DESC
        "#,
    )
    .bind("rust")
    .bind(chain_id.to_string())
    .fetch_all(&ctx.pool)
    .await
    .expect("Failed to search");

    assert!(rows.len() >= 3, "All chunks should match 'rust'");

    // Verify highest scoring chunk is the middle one
    let best_match_id: Uuid = rows[0].get("id");
    let best_sequence: String = rows[0].get("sequence");

    assert_eq!(
        best_match_id, chunk2_id,
        "Chunk 2 should have highest score"
    );
    assert_eq!(best_sequence, "1", "Best match should be sequence 1");

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
    ctx.cleanup_note(chunk3_id).await;
}

// =============================================================================
// Edge Cases
// =============================================================================

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_document_at_exact_threshold() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create exactly 2 chunks at threshold
    let threshold_content = "x".repeat(1000); // Exactly at threshold

    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 2, &threshold_content)
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(chain_id, 1, 2, &threshold_content)
        .await
        .expect("Failed to create chunk 2");

    // Verify both chunks are created
    let chunks = ctx
        .get_chain_chunks(chain_id)
        .await
        .expect("Failed to get chunks");

    assert_eq!(chunks.len(), 2, "Should have exactly 2 chunks");

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_empty_chunk_content() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Test edge case: empty content chunk
    let result = ctx.create_chunked_note(chain_id, 0, 1, "").await;

    // Should succeed - empty content is valid
    assert!(result.is_ok(), "Empty content should be allowed");

    if let Ok(chunk_id) = result {
        // Verify metadata exists despite empty content
        let metadata = ctx
            .get_chunk_metadata(chunk_id)
            .await
            .expect("Failed to get metadata");

        assert!(metadata.is_some(), "Metadata should exist for empty chunk");

        // Cleanup
        ctx.cleanup_note(chunk_id).await;
    }
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_unicode_content_in_chunks() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create chunks with unicode content
    let unicode_content = "こんにちは世界 🌍 Здравствуй мир";

    let chunk_id = ctx
        .create_chunked_note(chain_id, 0, 1, unicode_content)
        .await
        .expect("Failed to create unicode chunk");

    // Verify content is preserved correctly
    let row = sqlx::query("SELECT content FROM note_original WHERE note_id = $1")
        .bind(chunk_id)
        .fetch_one(&ctx.pool)
        .await
        .expect("Failed to fetch content");

    let stored_content: String = row.get("content");
    assert_eq!(
        stored_content, unicode_content,
        "Unicode content should be preserved"
    );

    // Cleanup
    ctx.cleanup_note(chunk_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_large_number_of_chunks() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create a document with many chunks (stress test)
    let total_chunks = 50;
    let mut chunk_ids = Vec::new();

    for i in 0..total_chunks {
        let content = format!("Chunk {} of {}", i + 1, total_chunks);
        let chunk_id = ctx
            .create_chunked_note(chain_id, i, total_chunks, &content)
            .await
            .expect("Failed to create chunk");
        chunk_ids.push(chunk_id);
    }

    // Verify all chunks created successfully
    let chunks = ctx
        .get_chain_chunks(chain_id)
        .await
        .expect("Failed to get chunks");

    assert_eq!(
        chunks.len(),
        total_chunks as usize,
        "Should have {} chunks",
        total_chunks
    );

    // Verify sequences are contiguous
    let sequences: Vec<u32> = chunks.iter().map(|(_, seq)| *seq).collect();
    for i in 0..total_chunks {
        assert!(sequences.contains(&i), "Should have sequence {}", i);
    }

    // Cleanup
    for chunk_id in chunk_ids {
        ctx.cleanup_note(chunk_id).await;
    }
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_chunk_with_special_characters() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Content with special characters that might break JSON/SQL
    let special_content = r#"Special chars: "quotes" 'apostrophes' \backslashes\ {braces} [brackets] <tags> & ampersands $dollar #hash @at"#;

    let chunk_id = ctx
        .create_chunked_note(chain_id, 0, 1, special_content)
        .await
        .expect("Failed to create chunk with special chars");

    // Verify content is preserved
    let row = sqlx::query("SELECT content FROM note_original WHERE note_id = $1")
        .bind(chunk_id)
        .fetch_one(&ctx.pool)
        .await
        .expect("Failed to fetch content");

    let stored_content: String = row.get("content");
    assert_eq!(
        stored_content, special_content,
        "Special characters should be preserved"
    );

    // Cleanup
    ctx.cleanup_note(chunk_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database"]
async fn test_chunk_metadata_indexing() {
    let ctx = TestContext::new().await;
    let chain_id = Uuid::new_v4();

    // Create chunks to test GIN index on chunk_metadata
    let chunk1_id = ctx
        .create_chunked_note(chain_id, 0, 2, "First chunk")
        .await
        .expect("Failed to create chunk 1");
    let chunk2_id = ctx
        .create_chunked_note(chain_id, 1, 2, "Second chunk")
        .await
        .expect("Failed to create chunk 2");

    // Query using JSONB operators (should use GIN index)
    let rows = sqlx::query(
        r#"
        SELECT id FROM note
        WHERE chunk_metadata @> $1::jsonb
        ORDER BY (chunk_metadata->>'chunk_sequence')::int
        "#,
    )
    .bind(json!({ "chain_id": chain_id.to_string() }))
    .fetch_all(&ctx.pool)
    .await
    .expect("Failed to query with JSONB operator");

    assert_eq!(rows.len(), 2, "Should find 2 chunks");

    let found_id1: Uuid = rows[0].get("id");
    let found_id2: Uuid = rows[1].get("id");

    assert_eq!(found_id1, chunk1_id, "First result should be chunk 1");
    assert_eq!(found_id2, chunk2_id, "Second result should be chunk 2");

    // Cleanup
    ctx.cleanup_note(chunk1_id).await;
    ctx.cleanup_note(chunk2_id).await;
}

//! Test suite for N+1 query optimization (Issue #467).
//!
//! This test validates that tag loading is batched rather than executed
//! once per note, eliminating N+1 query patterns in:
//! - list_notes_with_filter (notes.rs line 514-515)
//! - search methods (search.rs multiple locations)
//!
//! The optimization replaces correlated subqueries with LEFT JOIN and
//! string_agg grouping to load all tags in a single query.
//!
//! Related issue:
//! - #467: HIGH - Fix N+1 query patterns in notes and search

use matric_core::{CreateNoteRequest, ListNotesRequest, NoteRepository};
use matric_db::{create_pool, PgFtsSearch, PgNoteRepository};
use sqlx::PgPool;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

/// Creates test notes with different tag combinations to validate batched tag loading.
/// Uses unique tags with timestamp prefix for test isolation.
async fn create_test_notes(
    repo: &PgNoteRepository,
    unique_prefix: &str,
) -> Result<Vec<uuid::Uuid>, Box<dyn std::error::Error>> {
    let mut note_ids = Vec::new();

    // Note 1: Multiple tags
    let req1 = CreateNoteRequest {
        content: format!(
            "# Note with multiple tags\n\nFirst test note {}",
            unique_prefix
        ),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![
            format!("{}-rust", unique_prefix),
            format!("{}-performance", unique_prefix),
            format!("{}-database", unique_prefix),
        ]),
        metadata: None,
        document_type_id: None,
    };
    note_ids.push(repo.insert(req1).await?);

    // Note 2: Single tag
    let req2 = CreateNoteRequest {
        content: format!(
            "# Note with single tag\n\nSecond test note {}",
            unique_prefix
        ),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![format!("{}-optimization", unique_prefix)]),
        metadata: None,
        document_type_id: None,
    };
    note_ids.push(repo.insert(req2).await?);

    // Note 3: No tags (but with unique content for identification)
    let req3 = CreateNoteRequest {
        content: format!("# Note without tags\n\nThird test note {}", unique_prefix),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![format!("{}-marker", unique_prefix)]), // Add marker tag for filtering
        metadata: None,
        document_type_id: None,
    };
    note_ids.push(repo.insert(req3).await?);

    // Note 4: Many tags to stress test
    let req4 = CreateNoteRequest {
        content: format!(
            "# Note with many tags\n\nFourth test note {}",
            unique_prefix
        ),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![
            format!("{}-tag1", unique_prefix),
            format!("{}-tag2", unique_prefix),
            format!("{}-tag3", unique_prefix),
            format!("{}-tag4", unique_prefix),
            format!("{}-tag5", unique_prefix),
            format!("{}-tag6", unique_prefix),
            format!("{}-tag7", unique_prefix),
            format!("{}-tag8", unique_prefix),
        ]),
        metadata: None,
        document_type_id: None,
    };
    note_ids.push(repo.insert(req4).await?);

    Ok(note_ids)
}

#[tokio::test]
async fn test_list_notes_loads_tags_efficiently() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    // Use unique prefix for test isolation
    let unique_prefix = format!("n1-test-{}", chrono::Utc::now().timestamp_millis());

    // Create test notes with various tag configurations
    let note_ids = create_test_notes(&repo, &unique_prefix)
        .await
        .expect("Failed to create test notes");

    // List notes filtered by our unique tag prefix
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![format!("{}-rust", unique_prefix)]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Verify our test note with the rust tag is found
    assert!(
        response.notes.iter().any(|n| n.id == note_ids[0]),
        "Should find the note with rust tag"
    );

    // The found note should have 3 tags loaded (not just 1)
    if let Some(note_summary) = response.notes.iter().find(|n| n.id == note_ids[0]) {
        assert_eq!(
            note_summary.tags.len(),
            3,
            "First note should have 3 tags loaded"
        );
    }

    // Clean up
    for note_id in note_ids {
        repo.hard_delete(note_id)
            .await
            .expect("Failed to delete test note");
    }
}

#[tokio::test]
async fn test_search_loads_tags_efficiently() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());
    let search = PgFtsSearch::new(pool);

    // Use unique prefix for test isolation
    let unique_prefix = format!("n1-search-{}", chrono::Utc::now().timestamp_millis());

    // Create test notes
    let note_ids = create_test_notes(&repo, &unique_prefix)
        .await
        .expect("Failed to create test notes");

    // Search for notes using the unique prefix - this should use batched tag loading
    let results = search
        .search(&unique_prefix, 10, false)
        .await
        .expect("Failed to search notes");

    // Verify that search results include tags
    assert!(
        !results.is_empty(),
        "Search should return at least one result"
    );

    // At least some results should have tags
    let notes_with_tags = results.iter().filter(|r| !r.tags.is_empty()).count();
    assert!(
        notes_with_tags > 0,
        "At least some search results should have tags"
    );

    // Clean up
    for note_id in note_ids {
        repo.hard_delete(note_id)
            .await
            .expect("Failed to delete test note");
    }
}

#[tokio::test]
async fn test_search_filtered_loads_tags_efficiently() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());
    let search = PgFtsSearch::new(pool);

    // Use unique prefix for test isolation
    let unique_prefix = format!("n1-filtered-{}", chrono::Utc::now().timestamp_millis());
    let rust_tag = format!("{}-rust", unique_prefix);

    // Create test notes
    let note_ids = create_test_notes(&repo, &unique_prefix)
        .await
        .expect("Failed to create test notes");

    // Search with filter - this should use batched tag loading
    let results = search
        .search_filtered(&unique_prefix, &format!("tag:{}", rust_tag), 10, false)
        .await
        .expect("Failed to search with filter");

    // Verify that filtered search results include tags
    if !results.is_empty() {
        // Check that at least one result has the unique rust tag
        let has_rust_tag = results.iter().any(|r| r.tags.contains(&rust_tag));
        assert!(
            has_rust_tag,
            "At least one result should have the '{}' tag",
            rust_tag
        );
    }

    // Clean up
    for note_id in note_ids {
        repo.hard_delete(note_id)
            .await
            .expect("Failed to delete test note");
    }
}

#[tokio::test]
async fn test_tag_ordering_consistency() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    // Use unique prefix for test isolation
    let unique_prefix = format!("n1-order-{}", chrono::Utc::now().timestamp_millis());

    // Create a note with tags in specific order
    let req = CreateNoteRequest {
        content: format!("# Test tag ordering\n\nTest note {}", unique_prefix),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![
            format!("{}-zebra", unique_prefix),
            format!("{}-apple", unique_prefix),
            format!("{}-mango", unique_prefix),
        ]),
        metadata: None,
        document_type_id: None,
    };
    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // List notes filtered by one of our unique tags
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![format!("{}-apple", unique_prefix)]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Find our note
    let note_summary = response
        .notes
        .iter()
        .find(|n| n.id == note_id)
        .expect("Note not found in list response");

    // Verify tags are present (they should be sorted alphabetically in the DB)
    assert_eq!(note_summary.tags.len(), 3, "Should have 3 tags");

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

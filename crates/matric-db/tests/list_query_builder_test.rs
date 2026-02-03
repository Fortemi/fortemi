//! Test suite for NoteQueryBuilder and helper functions.
//!
//! This test validates the refactoring of the high-complexity list() function
//! by testing the extracted query building logic independently.
//!
//! Related issues:
//! - #468: Refactor high-complexity list() function (CC=28)

use matric_core::{CreateNoteRequest, ListNotesRequest, NoteRepository};
use matric_db::{create_pool, PgNoteRepository};
use sqlx::PgPool;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

#[tokio::test]
async fn test_list_with_no_filters() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    // Create a test note
    let req = CreateNoteRequest {
        content: "# Test Note\n\nSimple content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test-no-filter".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // List with no filters
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find at least our note
    assert!(
        response.total > 0,
        "Should have at least one note in database"
    );
    assert!(
        !response.notes.is_empty(),
        "Should return at least one note"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_with_tag_filter() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    let unique_tag = format!("test-tag-filter-{}", chrono::Utc::now().timestamp());

    // Create a test note with specific tag
    let req = CreateNoteRequest {
        content: "# Test Note\n\nTagged content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // List with tag filter
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find exactly our note
    assert!(response.total >= 1, "Should have at least one tagged note");
    assert!(
        response.notes.iter().any(|n| n.id == note_id),
        "Should find the created note"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_with_multiple_tag_filters() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    let tag1 = format!("test-multi-tag-1-{}", chrono::Utc::now().timestamp());
    let tag2 = format!("test-multi-tag-2-{}", chrono::Utc::now().timestamp());

    // Create a test note with multiple tags
    let req = CreateNoteRequest {
        content: "# Test Note\n\nMulti-tagged content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![tag1.clone(), tag2.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // List with multiple tag filters
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![tag1.clone(), tag2.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find our note
    assert!(
        response.notes.iter().any(|n| n.id == note_id),
        "Should find note with multiple tags"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_with_date_filters() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    let unique_tag = format!("test-date-filter-{}", chrono::Utc::now().timestamp());

    // Create a test note
    let req = CreateNoteRequest {
        content: "# Test Note\n\nDate filtered content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // List with date filters
    let now = chrono::Utc::now();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let one_hour_later = now + chrono::Duration::hours(1);

    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: Some(one_hour_ago),
        created_before: Some(one_hour_later),
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find our note within the date range
    assert!(
        response.notes.iter().any(|n| n.id == note_id),
        "Should find note within date range"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_with_starred_filter() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    let unique_tag = format!("test-starred-{}", chrono::Utc::now().timestamp());

    // Create a test note
    let req = CreateNoteRequest {
        content: "# Starred Note\n\nStarred content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Star the note
    sqlx::query("UPDATE note SET starred = true WHERE id = $1")
        .bind(note_id)
        .execute(&pool)
        .await
        .expect("Failed to star note");

    // List with starred filter
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: Some("starred".to_string()),
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find our starred note
    let found_note = response.notes.iter().find(|n| n.id == note_id);
    assert!(found_note.is_some(), "Should find starred note");
    assert!(found_note.unwrap().starred, "Note should be starred");

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_with_archived_filter() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    let unique_tag = format!("test-archived-{}", chrono::Utc::now().timestamp());

    // Create a test note
    let req = CreateNoteRequest {
        content: "# Archived Note\n\nArchived content.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Archive the note
    sqlx::query("UPDATE note SET archived = true WHERE id = $1")
        .bind(note_id)
        .execute(&pool)
        .await
        .expect("Failed to archive note");

    // List with archived filter
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: Some("archived".to_string()),
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Should find our archived note
    let found_note = response.notes.iter().find(|n| n.id == note_id);
    assert!(found_note.is_some(), "Should find archived note");
    assert!(found_note.unwrap().archived, "Note should be archived");

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_list_sort_by_updated_at() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    let unique_tag = format!("test-sort-{}", chrono::Utc::now().timestamp());

    // Create two notes
    let req1 = CreateNoteRequest {
        content: "# Note 1\n\nFirst note.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id1 = repo.insert(req1).await.expect("Failed to insert note 1");

    // Wait a bit to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req2 = CreateNoteRequest {
        content: "# Note 2\n\nSecond note.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id2 = repo.insert(req2).await.expect("Failed to insert note 2");

    // Update note 1 to make it more recent
    sqlx::query("UPDATE note SET updated_at_utc = NOW() WHERE id = $1")
        .bind(note_id1)
        .execute(&pool)
        .await
        .expect("Failed to update note");

    // List sorted by updated_at descending
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("updated_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Find both notes
    let note1_pos = response.notes.iter().position(|n| n.id == note_id1);
    let note2_pos = response.notes.iter().position(|n| n.id == note_id2);

    assert!(note1_pos.is_some(), "Should find note 1");
    assert!(note2_pos.is_some(), "Should find note 2");

    // Note 1 should come before note 2 (more recent update)
    assert!(
        note1_pos < note2_pos,
        "Note 1 should appear before note 2 when sorted by updated_at desc"
    );

    // Clean up
    repo.hard_delete(note_id1)
        .await
        .expect("Failed to delete note 1");
    repo.hard_delete(note_id2)
        .await
        .expect("Failed to delete note 2");
}

#[tokio::test]
async fn test_list_pagination() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    let unique_tag = format!("test-pagination-{}", chrono::Utc::now().timestamp());

    // Create three notes
    let mut note_ids = Vec::new();
    for i in 1..=3 {
        let req = CreateNoteRequest {
            content: format!("# Note {}\n\nContent {}.", i, i),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: Some(vec![unique_tag.clone()]),
            metadata: None,
            document_type_id: None,
        };

        let note_id = repo
            .insert(req)
            .await
            .unwrap_or_else(|_| panic!("Failed to insert note {}", i));
        note_ids.push(note_id);

        // Small delay to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Get first page (limit 2)
    let list_req = ListNotesRequest {
        limit: Some(2),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    assert_eq!(response.notes.len(), 2, "Should return 2 notes");
    assert!(response.total >= 3, "Total should be at least 3");

    // Get second page (offset 2)
    let list_req = ListNotesRequest {
        limit: Some(2),
        offset: Some(2),
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: Some(vec![unique_tag.clone()]),
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    assert!(
        !response.notes.is_empty(),
        "Should return at least 1 note on second page"
    );

    // Clean up
    for note_id in note_ids {
        repo.hard_delete(note_id)
            .await
            .expect("Failed to delete test note");
    }
}

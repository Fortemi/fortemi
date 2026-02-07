//! Integration tests for SKOS/Concepts endpoints at the database layer.
//!
//! Tests verify:
//! - SKOS concept scheme lifecycle (create, get, update, delete, list)
//! - SKOS concept lifecycle (create, get, update, delete)
//! - Concept search functionality
//! - Concept relations (broader/narrower)
//! - Governance statistics
//! - Error handling for invalid operations
//!
//! Pattern: `#[tokio::test]` with Database::connect(), UUID-based isolation.

use matric_core::{
    CreateConceptRequest, CreateConceptSchemeRequest, SearchConceptsRequest, TagStatus,
    UpdateConceptRequest, UpdateConceptSchemeRequest,
};
use matric_db::{
    Database, SkosConceptRepository, SkosConceptSchemeRepository, SkosGovernanceRepository,
};
use uuid::Uuid;

/// Helper to create a test database connection.
async fn setup_test_db() -> Database {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    Database::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Helper to create a test concept scheme with unique identifier.
async fn create_test_scheme(db: &Database, suffix: &str) -> Uuid {
    let request = CreateConceptSchemeRequest {
        notation: format!("test-scheme-{}", suffix),
        title: format!("Test Scheme {}", suffix),
        uri: Some(format!("http://test.example.org/scheme/{}", suffix)),
        description: Some("Test concept scheme".to_string()),
        creator: Some("Test Suite".to_string()),
        publisher: None,
        rights: None,
        version: Some("1.0".to_string()),
    };

    db.skos
        .create_scheme(request)
        .await
        .expect("Failed to create concept scheme")
}

/// Helper to create a test concept in a scheme.
async fn create_test_concept(db: &Database, scheme_id: Uuid, label: &str) -> Uuid {
    let request = CreateConceptRequest {
        scheme_id,
        notation: None,
        pref_label: label.to_string(),
        language: "en".to_string(),
        status: TagStatus::Approved,
        facet_type: None,
        facet_source: None,
        facet_domain: None,
        facet_scope: None,
        definition: Some(format!("Definition for {}", label)),
        scope_note: None,
        broader_ids: vec![],
        related_ids: vec![],
        alt_labels: vec![],
    };

    db.skos
        .create_concept(request)
        .await
        .expect("Failed to create concept")
}

// =============================================================================
// SKOS Concept Scheme Tests
// =============================================================================

#[tokio::test]
async fn test_concept_scheme_lifecycle() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // 1. Create concept scheme
    let scheme_id = create_test_scheme(&db, &unique_id).await;
    assert_ne!(scheme_id, Uuid::nil(), "Scheme ID should not be nil");

    // 2. Get concept scheme
    let retrieved = db
        .skos
        .get_scheme(scheme_id)
        .await
        .expect("Failed to get scheme")
        .expect("Scheme not found");

    assert_eq!(retrieved.notation, format!("test-scheme-{}", unique_id));
    assert_eq!(retrieved.title, format!("Test Scheme {}", unique_id));
    assert_eq!(
        retrieved.uri,
        Some(format!("http://test.example.org/scheme/{}", unique_id))
    );

    // 3. Update concept scheme
    let update_request = UpdateConceptSchemeRequest {
        title: Some("Updated Test Scheme".to_string()),
        description: Some("Updated description".to_string()),
        creator: None,
        publisher: Some("Test Publisher".to_string()),
        rights: Some("CC-BY-4.0".to_string()),
        version: Some("2.0".to_string()),
        is_active: Some(true),
    };

    db.skos
        .update_scheme(scheme_id, update_request)
        .await
        .expect("Failed to update scheme");

    let updated = db
        .skos
        .get_scheme(scheme_id)
        .await
        .expect("Failed to get updated scheme")
        .expect("Scheme not found");

    assert_eq!(updated.title, "Updated Test Scheme");
    assert_eq!(updated.description, Some("Updated description".to_string()));
    assert_eq!(updated.publisher, Some("Test Publisher".to_string()));
    assert_eq!(updated.rights, Some("CC-BY-4.0".to_string()));

    // 4. List concept schemes (include inactive)
    let schemes = db
        .skos
        .list_schemes(true)
        .await
        .expect("Failed to list schemes");

    assert!(
        schemes.iter().any(|s| s.id == scheme_id),
        "Created scheme should be in list"
    );

    // 5. Delete concept scheme (cleanup)
    db.skos
        .delete_scheme(scheme_id)
        .await
        .expect("Failed to delete scheme");

    let deleted = db
        .skos
        .get_scheme(scheme_id)
        .await
        .expect("Query should succeed");

    assert!(deleted.is_none(), "Scheme should be deleted");
}

#[tokio::test]
async fn test_list_concept_schemes_filtering() {
    let db = setup_test_db().await;
    let unique_id1 = Uuid::new_v4().to_string();
    let unique_id2 = Uuid::new_v4().to_string();

    // Create two schemes
    let scheme_id1 = create_test_scheme(&db, &unique_id1).await;
    let scheme_id2 = create_test_scheme(&db, &unique_id2).await;

    // List all schemes (including inactive)
    let all_schemes = db
        .skos
        .list_schemes(true)
        .await
        .expect("Failed to list schemes");

    assert!(all_schemes.len() >= 2, "Should have at least 2 schemes");
    assert!(all_schemes.iter().any(|s| s.id == scheme_id1));
    assert!(all_schemes.iter().any(|s| s.id == scheme_id2));

    // List only active schemes
    let active_schemes = db
        .skos
        .list_schemes(false)
        .await
        .expect("Failed to list active schemes");

    assert!(active_schemes.iter().all(|s| s.is_active));

    // Cleanup
    db.skos.delete_scheme(scheme_id1).await.expect("cleanup");
    db.skos.delete_scheme(scheme_id2).await.expect("cleanup");
}

// =============================================================================
// SKOS Concept Tests
// =============================================================================

#[tokio::test]
async fn test_concept_lifecycle() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // Create a scheme
    let scheme_id = create_test_scheme(&db, &unique_id).await;

    // 1. Create concept
    let concept_id = create_test_concept(&db, scheme_id, "Test Concept").await;
    assert_ne!(concept_id, Uuid::nil(), "Concept ID should not be nil");

    // 2. Get concept
    let retrieved = db
        .skos
        .get_concept(concept_id)
        .await
        .expect("Failed to get concept")
        .expect("Concept not found");

    assert_eq!(retrieved.primary_scheme_id, scheme_id);
    assert!(
        retrieved.notation.is_some(),
        "Notation should be auto-generated"
    );

    // 3. Get full concept (with relations)
    let full = db
        .skos
        .get_concept_full(concept_id)
        .await
        .expect("Failed to get full concept")
        .expect("Concept not found");

    assert!(full.broader.is_empty());
    assert!(full.narrower.is_empty());
    assert!(full.related.is_empty());

    // 4. Update concept (change status to Deprecated)
    let update_request = UpdateConceptRequest {
        notation: None,
        status: Some(TagStatus::Deprecated),
        deprecation_reason: Some("Testing deprecation".to_string()),
        replaced_by_id: None,
        facet_type: None,
        facet_source: None,
        facet_domain: None,
        facet_scope: None,
    };

    db.skos
        .update_concept(concept_id, update_request)
        .await
        .expect("Failed to update concept");

    let updated = db
        .skos
        .get_concept(concept_id)
        .await
        .expect("Failed to get updated concept")
        .expect("Concept not found");

    assert_eq!(updated.status, TagStatus::Deprecated);

    // 5. Delete concept
    db.skos
        .delete_concept(concept_id)
        .await
        .expect("Failed to delete concept");

    let deleted = db
        .skos
        .get_concept(concept_id)
        .await
        .expect("Query should succeed");

    assert!(deleted.is_none(), "Concept should be deleted");

    // Cleanup scheme
    db.skos.delete_scheme(scheme_id).await.expect("cleanup");
}

#[tokio::test]
async fn test_concept_relations() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // Create a scheme
    let scheme_id = create_test_scheme(&db, &unique_id).await;

    // Create parent concept
    let parent_id = create_test_concept(&db, scheme_id, "Parent Concept").await;

    // Create child concept with broader relation
    let child_request = CreateConceptRequest {
        scheme_id,
        notation: None,
        pref_label: "Child Concept".to_string(),
        language: "en".to_string(),
        status: TagStatus::Approved,
        facet_type: None,
        facet_source: None,
        facet_domain: None,
        facet_scope: None,
        definition: Some("Child concept".to_string()),
        scope_note: None,
        broader_ids: vec![parent_id],
        related_ids: vec![],
        alt_labels: vec![],
    };

    let child_id = db
        .skos
        .create_concept(child_request)
        .await
        .expect("Failed to create child concept");

    // Verify parent has narrower relation
    let parent_full = db
        .skos
        .get_concept_full(parent_id)
        .await
        .expect("Failed to get parent")
        .expect("Parent not found");

    assert!(
        parent_full.narrower.iter().any(|c| c.id == child_id),
        "Parent should have child in narrower"
    );

    // Verify child has broader relation
    let child_full = db
        .skos
        .get_concept_full(child_id)
        .await
        .expect("Failed to get child")
        .expect("Child not found");

    assert!(
        child_full.broader.iter().any(|c| c.id == parent_id),
        "Child should have parent in broader"
    );

    // Cleanup
    db.skos.delete_concept(child_id).await.expect("cleanup");
    db.skos.delete_concept(parent_id).await.expect("cleanup");
    db.skos.delete_scheme(scheme_id).await.expect("cleanup");
}

// =============================================================================
// SKOS Concept Search Tests
// =============================================================================

#[tokio::test]
async fn test_search_concepts_by_query() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // Create a scheme with concepts
    let scheme_id = create_test_scheme(&db, &unique_id).await;
    let concept1 =
        create_test_concept(&db, scheme_id, &format!("Rust Programming {}", unique_id)).await;
    let concept2 =
        create_test_concept(&db, scheme_id, &format!("Rust Language {}", unique_id)).await;
    let concept3 =
        create_test_concept(&db, scheme_id, &format!("Python Programming {}", unique_id)).await;

    // Search for "Rust"
    let request = SearchConceptsRequest {
        query: Some(format!("Rust {}", unique_id)),
        scheme_id: Some(scheme_id),
        status: None,
        ..Default::default()
    };

    let results = db
        .skos
        .search_concepts(request)
        .await
        .expect("Failed to search concepts");

    assert!(
        results.concepts.len() >= 2,
        "Should find at least 2 concepts matching 'Rust' (got {})",
        results.concepts.len()
    );

    // Cleanup - delete concepts before scheme
    db.skos.delete_concept(concept1).await.expect("cleanup");
    db.skos.delete_concept(concept2).await.expect("cleanup");
    db.skos.delete_concept(concept3).await.expect("cleanup");
    db.skos.delete_scheme(scheme_id).await.expect("cleanup");
}

// =============================================================================
// SKOS Governance Tests
// =============================================================================

#[tokio::test]
async fn test_governance_stats() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // Create a scheme with some concepts
    let scheme_id = create_test_scheme(&db, &unique_id).await;
    let concept1 = create_test_concept(&db, scheme_id, "Gov Test Concept 1").await;
    let concept2 = create_test_concept(&db, scheme_id, "Gov Test Concept 2").await;

    // Get governance stats for the scheme
    let stats = db
        .skos
        .get_governance_stats(scheme_id)
        .await
        .expect("Failed to get governance stats");

    // Verify structure
    assert_eq!(stats.scheme_id, scheme_id);
    assert!(stats.total_concepts >= 2, "Should have at least 2 concepts");

    // Cleanup - delete concepts before scheme
    db.skos.delete_concept(concept1).await.expect("cleanup");
    db.skos.delete_concept(concept2).await.expect("cleanup");
    db.skos.delete_scheme(scheme_id).await.expect("cleanup");
}

#[tokio::test]
async fn test_all_governance_stats() {
    let db = setup_test_db().await;

    // Get all governance stats (across all schemes)
    let all_stats = db
        .skos
        .get_all_governance_stats()
        .await
        .expect("Failed to get all governance stats");

    // Should return a list (may be empty if no schemes exist)
    // Just verify the call succeeded - any result is valid
    let _ = all_stats.len();
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[tokio::test]
async fn test_get_nonexistent_concept_scheme() {
    let db = setup_test_db().await;

    let nonexistent_id = Uuid::new_v4();
    let result = db
        .skos
        .get_scheme(nonexistent_id)
        .await
        .expect("Query should succeed");

    assert!(
        result.is_none(),
        "Should return None for nonexistent scheme"
    );
}

#[tokio::test]
async fn test_get_nonexistent_concept() {
    let db = setup_test_db().await;

    let nonexistent_id = Uuid::new_v4();
    let result = db
        .skos
        .get_concept(nonexistent_id)
        .await
        .expect("Query should succeed");

    assert!(
        result.is_none(),
        "Should return None for nonexistent concept"
    );
}

#[tokio::test]
async fn test_create_concept_in_nonexistent_scheme() {
    let db = setup_test_db().await;

    let nonexistent_scheme_id = Uuid::new_v4();
    let request = CreateConceptRequest {
        scheme_id: nonexistent_scheme_id,
        notation: None,
        pref_label: "Test".to_string(),
        language: "en".to_string(),
        status: TagStatus::Candidate,
        facet_type: None,
        facet_source: None,
        facet_domain: None,
        facet_scope: None,
        definition: None,
        scope_note: None,
        broader_ids: vec![],
        related_ids: vec![],
        alt_labels: vec![],
    };

    let result = db.skos.create_concept(request).await;

    // Should fail with foreign key violation or similar
    assert!(
        result.is_err(),
        "Should fail to create concept in nonexistent scheme"
    );
}

#[tokio::test]
async fn test_delete_scheme_with_concepts_fails() {
    let db = setup_test_db().await;
    let unique_id = Uuid::new_v4().to_string();

    // Create a scheme with a concept
    let scheme_id = create_test_scheme(&db, &unique_id).await;
    let concept_id = create_test_concept(&db, scheme_id, "Blocking Concept").await;

    // Attempt to delete scheme should fail (has concepts)
    let result = db.skos.delete_scheme(scheme_id).await;
    assert!(
        result.is_err(),
        "Should fail to delete scheme with concepts"
    );

    // Cleanup: delete concept first, then scheme
    db.skos.delete_concept(concept_id).await.expect("cleanup");
    db.skos.delete_scheme(scheme_id).await.expect("cleanup");
}

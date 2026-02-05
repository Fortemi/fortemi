use matric_core::TagInput;
use matric_db::{
    create_pool, test_fixtures::DEFAULT_TEST_DATABASE_URL, PgSkosRepository, SkosConceptRepository,
    SkosConceptSchemeRepository, SkosLabelRepository, SkosTagResolutionRepository,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Create a test database connection pool.
///
/// Uses DATABASE_URL environment variable if set, otherwise defaults to
/// the local test database on port 15432 (as documented in quick-start-testing.md).
async fn setup_test_pool() -> PgPool {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

/// Test that tags with different casing (e.g., "CATS" vs "cats")
/// resolve to the same concept to prevent duplicates.
#[tokio::test]
async fn test_tag_case_insensitive_resolution() {
    let pool = setup_test_pool().await;
    let skos = PgSkosRepository::new(pool);

    // Create a scheme for testing with unique notation
    let unique_notation = format!("test-scheme-{}", Uuid::new_v4());
    let scheme_id = skos
        .create_scheme(matric_core::CreateConceptSchemeRequest {
            notation: unique_notation.clone(),
            title: "Test Scheme".to_string(),
            uri: None,
            description: Some("Test scheme for case sensitivity".to_string()),
            creator: None,
            publisher: None,
            rights: None,
            version: None,
        })
        .await
        .expect("Failed to create scheme");

    // Resolve a tag with uppercase path components
    let tag1 = skos
        .resolve_or_create_tag(&TagInput {
            scheme: unique_notation.clone(),
            path: vec!["CATS".to_string(), "RAGDOLL".to_string()],
            notation: None,
        })
        .await
        .expect("Failed to resolve CATS/RAGDOLL");

    // Resolve the same tag with lowercase path components
    let tag2 = skos
        .resolve_or_create_tag(&TagInput {
            scheme: unique_notation.clone(),
            path: vec!["cats".to_string(), "ragdoll".to_string()],
            notation: None,
        })
        .await
        .expect("Failed to resolve cats/ragdoll");

    // Resolve the same tag with mixed case path components
    let tag3 = skos
        .resolve_or_create_tag(&TagInput {
            scheme: unique_notation.clone(),
            path: vec!["Cats".to_string(), "Ragdoll".to_string()],
            notation: None,
        })
        .await
        .expect("Failed to resolve Cats/Ragdoll");

    // All three should resolve to the same concept
    assert_eq!(
        tag1.concept_id, tag2.concept_id,
        "CATS/RAGDOLL and cats/ragdoll should resolve to the same concept, but got different IDs: {} vs {}",
        tag1.concept_id, tag2.concept_id
    );
    assert_eq!(
        tag1.concept_id, tag3.concept_id,
        "CATS/RAGDOLL and Cats/Ragdoll should resolve to the same concept, but got different IDs: {} vs {}",
        tag1.concept_id, tag3.concept_id
    );

    // Verify the notation is lowercase
    let concept = skos
        .get_concept(tag1.concept_id)
        .await
        .expect("Failed to get concept")
        .expect("Concept should exist");

    assert_eq!(
        concept.notation,
        Some("cats/ragdoll".to_string()),
        "Notation should be normalized to lowercase"
    );

    // Verify the preferred label is also lowercase (the fix)
    let labels = skos
        .get_labels(tag1.concept_id)
        .await
        .expect("Failed to get labels");

    let pref_labels: Vec<_> = labels
        .iter()
        .filter(|l| l.label_type == matric_core::SkosLabelType::PrefLabel)
        .collect();

    assert!(
        !pref_labels.is_empty(),
        "Should have at least one preferred label"
    );

    for label in &pref_labels {
        assert_eq!(
            label.value.to_lowercase(),
            label.value,
            "Preferred label '{}' should be normalized to lowercase",
            label.value
        );
    }

    // Verify only one parent concept was created
    let parent_concept = skos
        .get_concept_by_notation(scheme_id, "cats")
        .await
        .expect("Failed to get parent concept");

    assert!(
        parent_concept.is_some(),
        "Parent concept 'cats' should exist"
    );

    // Check parent's label too
    if let Some(parent) = parent_concept {
        let parent_labels = skos
            .get_labels(parent.id)
            .await
            .expect("Failed to get parent labels");

        let parent_pref_labels: Vec<_> = parent_labels
            .iter()
            .filter(|l| l.label_type == matric_core::SkosLabelType::PrefLabel)
            .collect();

        for label in &parent_pref_labels {
            assert_eq!(
                label.value.to_lowercase(),
                label.value,
                "Parent preferred label '{}' should be normalized to lowercase",
                label.value
            );
        }
    }
}

/// Test that different case variants during creation don't create duplicates
#[tokio::test]
async fn test_no_duplicates_with_mixed_case() {
    let pool = setup_test_pool().await;
    let skos = PgSkosRepository::new(pool);

    // Create a scheme with unique notation
    let unique_notation = format!("test-scheme-{}", Uuid::new_v4());
    let _scheme_id = skos
        .create_scheme(matric_core::CreateConceptSchemeRequest {
            notation: unique_notation.clone(),
            title: "Test Scheme".to_string(),
            uri: None,
            description: Some("Test scheme for duplicate prevention".to_string()),
            creator: None,
            publisher: None,
            rights: None,
            version: None,
        })
        .await
        .expect("Failed to create scheme");

    // Create tags with different casing in rapid succession
    let tags = vec![
        TagInput {
            scheme: unique_notation.clone(),
            path: vec!["TECH".to_string(), "RUST".to_string()],
            notation: None,
        },
        TagInput {
            scheme: unique_notation.clone(),
            path: vec!["tech".to_string(), "rust".to_string()],
            notation: None,
        },
        TagInput {
            scheme: unique_notation.clone(),
            path: vec!["Tech".to_string(), "Rust".to_string()],
            notation: None,
        },
        TagInput {
            scheme: unique_notation.clone(),
            path: vec!["TeCh".to_string(), "RuSt".to_string()],
            notation: None,
        },
    ];

    let mut resolved_tags = Vec::new();
    for tag_input in tags {
        let resolved = skos
            .resolve_or_create_tag(&tag_input)
            .await
            .expect("Failed to resolve tag");
        resolved_tags.push(resolved);
    }

    // All should resolve to the same concept ID
    let first_id = resolved_tags[0].concept_id;
    for (i, tag) in resolved_tags.iter().enumerate() {
        assert_eq!(
            tag.concept_id, first_id,
            "Tag variant {} should resolve to the same concept, but got different ID: {} vs {}",
            i, tag.concept_id, first_id
        );
    }

    // Verify the final concept has lowercase notation and pref_label
    let concept = skos
        .get_concept(first_id)
        .await
        .expect("Failed to get concept")
        .expect("Concept should exist");

    assert_eq!(
        concept.notation,
        Some("tech/rust".to_string()),
        "Notation should be lowercase"
    );

    // Verify preferred label is lowercase
    let labels = skos
        .get_labels(first_id)
        .await
        .expect("Failed to get labels");

    let pref_labels: Vec<_> = labels
        .iter()
        .filter(|l| l.label_type == matric_core::SkosLabelType::PrefLabel)
        .collect();

    assert!(
        !pref_labels.is_empty(),
        "Should have at least one preferred label"
    );

    for label in &pref_labels {
        assert_eq!(
            label.value.to_lowercase(),
            label.value,
            "Preferred label '{}' should be lowercase",
            label.value
        );
    }
}

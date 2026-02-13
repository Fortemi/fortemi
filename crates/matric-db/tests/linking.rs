//! Tests for automatic linking functionality.
//!
//! This test suite validates:
//! - Link-001: Similar notes create automatic links (>70% similarity)
//! - Link-002: Bidirectional link consistency
//! - Link-003: Link updates on content change
//! - Link-004: Link threshold behavior
//! - Link-005: Link score calculation
//! - Link-006: Link metadata preservation
//! - Link-007: Link deletion cascades
//! - Link-008: Content-type-aware thresholds (code uses 0.85)
//!
//! Related issues:
//! - #347: Embedding similarity thresholds need calibration
//! - #355: Verify automatic linking functionality

use uuid::Uuid;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default similarity threshold for automatic linking (prose/general, 70%)
const LINK_SIMILARITY_THRESHOLD: f32 = matric_core::defaults::SEMANTIC_LINK_THRESHOLD;

/// Stricter similarity threshold for code-category notes (85%)
const CODE_SIMILARITY_THRESHOLD: f32 = matric_core::defaults::SEMANTIC_LINK_THRESHOLD_CODE;

/// High similarity score (should always create link regardless of category)
const HIGH_SIMILARITY: f32 = 0.90;

/// Medium similarity score (above prose threshold but below code threshold)
#[allow(dead_code)]
const MEDIUM_SIMILARITY: f32 = 0.75;

/// Low similarity score (below all thresholds)
#[allow(dead_code)]
const LOW_SIMILARITY: f32 = 0.60;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MockNote {
    id: Uuid,
    title: String,
    content: String,
    embedding: Vec<f32>,
}

#[derive(Debug, Clone)]
struct MockLink {
    id: Uuid,
    from_note_id: Uuid,
    to_note_id: Uuid,
    kind: String,
    score: f32,
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

/// Generate a normalized random-like vector from a seed.
fn generate_embedding(seed: u64, dimension: usize) -> Vec<f32> {
    let mut vec = vec![0.0; dimension];
    let mut state = seed;

    for val in vec.iter_mut().take(dimension) {
        // Simple LCG for deterministic randomness
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        *val = ((state % 1000) as f32) / 1000.0 - 0.5;
    }

    // Normalize
    let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        vec.iter_mut().for_each(|x| *x /= magnitude);
    }

    vec
}

/// Generate a similar embedding by adding small noise.
fn generate_similar_embedding(base: &[f32], similarity_target: f32) -> Vec<f32> {
    let dimension = base.len();
    let noise_magnitude = (1.0 - similarity_target).sqrt();

    let noise = generate_embedding(42, dimension);

    let mut result: Vec<f32> = base
        .iter()
        .zip(noise.iter())
        .map(|(b, n)| b + n * noise_magnitude)
        .collect();

    // Normalize
    let magnitude: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        result.iter_mut().for_each(|x| *x /= magnitude);
    }

    result
}

/// Check if a similarity score should create a link for prose/general content.
fn should_create_link(similarity: f32) -> bool {
    similarity >= LINK_SIMILARITY_THRESHOLD
}

/// Check if a similarity score should create a link for code content.
fn should_create_link_code(similarity: f32) -> bool {
    similarity >= CODE_SIMILARITY_THRESHOLD
}

// ============================================================================
// UNIT TESTS - Similarity Calculation
// ============================================================================

#[test]
fn test_cosine_similarity_identical_vectors() {
    let vec_a = vec![1.0, 0.0, 0.0];
    let vec_b = vec![1.0, 0.0, 0.0];

    let similarity = cosine_similarity(&vec_a, &vec_b);
    assert!(
        (similarity - 1.0).abs() < 0.001,
        "Identical vectors should have similarity 1.0"
    );
}

#[test]
fn test_cosine_similarity_orthogonal_vectors() {
    let vec_a = vec![1.0, 0.0, 0.0];
    let vec_b = vec![0.0, 1.0, 0.0];

    let similarity = cosine_similarity(&vec_a, &vec_b);
    assert!(
        (similarity - 0.0).abs() < 0.001,
        "Orthogonal vectors should have similarity 0.0"
    );
}

#[test]
fn test_cosine_similarity_opposite_vectors() {
    let vec_a = vec![1.0, 0.0, 0.0];
    let vec_b = vec![-1.0, 0.0, 0.0];

    let similarity = cosine_similarity(&vec_a, &vec_b);
    assert!(
        (similarity + 1.0).abs() < 0.001,
        "Opposite vectors should have similarity -1.0"
    );
}

#[test]
fn test_cosine_similarity_partial() {
    let vec_a = vec![1.0, 1.0, 0.0];
    let vec_b = vec![1.0, 0.0, 0.0];

    let similarity = cosine_similarity(&vec_a, &vec_b);
    // cos(45°) ≈ 0.707
    assert!((similarity - 0.707).abs() < 0.01);
}

#[test]
fn test_cosine_similarity_zero_vector() {
    let vec_a = vec![1.0, 0.0, 0.0];
    let vec_b = vec![0.0, 0.0, 0.0];

    let similarity = cosine_similarity(&vec_a, &vec_b);
    assert_eq!(similarity, 0.0, "Zero vector should have similarity 0.0");
}

#[test]
#[should_panic(expected = "Vectors must have same dimension")]
fn test_cosine_similarity_different_dimensions() {
    let vec_a = vec![1.0, 0.0];
    let vec_b = vec![1.0, 0.0, 0.0];

    cosine_similarity(&vec_a, &vec_b);
}

#[test]
fn test_generate_embedding_deterministic() {
    let embedding1 = generate_embedding(42, 128);
    let embedding2 = generate_embedding(42, 128);

    assert_eq!(
        embedding1, embedding2,
        "Same seed should produce same embedding"
    );
}

#[test]
fn test_generate_embedding_normalized() {
    let embedding = generate_embedding(42, 128);
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    assert!(
        (magnitude - 1.0).abs() < 0.01,
        "Embedding should be normalized"
    );
}

#[test]
fn test_generate_embedding_different_seeds() {
    let embedding1 = generate_embedding(42, 128);
    let embedding2 = generate_embedding(43, 128);

    assert_ne!(
        embedding1, embedding2,
        "Different seeds should produce different embeddings"
    );

    // Should have low similarity
    let similarity = cosine_similarity(&embedding1, &embedding2);
    assert!(
        similarity.abs() < 0.5,
        "Random embeddings should have low similarity"
    );
}

#[test]
fn test_generate_similar_embedding_high_similarity() {
    let base = generate_embedding(42, 128);
    let similar = generate_similar_embedding(&base, 0.95);

    let actual_similarity = cosine_similarity(&base, &similar);
    assert!(
        actual_similarity > 0.9,
        "Generated embedding should have high similarity"
    );
}

#[test]
fn test_generate_similar_embedding_medium_similarity() {
    let base = generate_embedding(42, 128);
    let similar = generate_similar_embedding(&base, 0.75);

    let actual_similarity = cosine_similarity(&base, &similar);
    assert!(
        actual_similarity > 0.5,
        "Generated embedding should have medium similarity"
    );
}

// ============================================================================
// UNIT TESTS - Link Threshold Logic
// ============================================================================

#[test]
fn test_should_create_link_above_threshold() {
    assert!(should_create_link(0.71));
    assert!(should_create_link(0.80));
    assert!(should_create_link(0.90));
    assert!(should_create_link(1.0));
}

#[test]
fn test_should_create_link_at_threshold() {
    assert!(should_create_link(LINK_SIMILARITY_THRESHOLD));
}

#[test]
fn test_should_create_link_below_threshold() {
    assert!(!should_create_link(0.69));
    assert!(!should_create_link(0.50));
    assert!(!should_create_link(0.0));
}

#[test]
fn test_threshold_value() {
    assert_eq!(
        LINK_SIMILARITY_THRESHOLD, 0.70,
        "Default threshold should be 70% as per spec"
    );
}

#[test]
fn test_code_threshold_value() {
    assert_eq!(
        CODE_SIMILARITY_THRESHOLD, 0.85,
        "Code threshold should be 85% (stricter than default)"
    );
}

// ============================================================================
// UNIT TESTS - Content-Type-Aware Thresholds (Link-008)
// ============================================================================

#[test]
fn test_code_notes_not_linked_at_default_threshold() {
    // Issue #347: Code embeddings cluster tightly — a similarity of 0.75
    // between Rust and Python code should NOT create a link.
    let similarity = 0.75;
    assert!(
        should_create_link(similarity),
        "Prose at 0.75 SHOULD create link (above 0.70 default)"
    );
    assert!(
        !should_create_link_code(similarity),
        "Code at 0.75 should NOT create link (below 0.85 code threshold)"
    );
}

#[test]
fn test_code_notes_linked_above_code_threshold() {
    // Very similar code (same algorithm in same language) should link
    let similarity = 0.88;
    assert!(
        should_create_link_code(similarity),
        "Code at 0.88 SHOULD create link (above 0.85 code threshold)"
    );
}

#[test]
fn test_threshold_for_category_matches_constants() {
    use matric_core::defaults::semantic_link_threshold_for;
    use matric_core::models::DocumentCategory;

    assert_eq!(
        semantic_link_threshold_for(DocumentCategory::Code),
        CODE_SIMILARITY_THRESHOLD
    );
    assert_eq!(
        semantic_link_threshold_for(DocumentCategory::Prose),
        LINK_SIMILARITY_THRESHOLD
    );
}

#[test]
fn test_code_threshold_boundary_between_thresholds() {
    // Scores in the 0.70-0.85 gap: linked for prose, NOT linked for code
    for score_x100 in 70..85 {
        let score = score_x100 as f32 / 100.0;
        assert!(
            should_create_link(score),
            "Prose at {score} should create link"
        );
        assert!(
            !should_create_link_code(score),
            "Code at {score} should NOT create link"
        );
    }
}

#[test]
fn test_high_similarity_links_regardless_of_category() {
    // Scores above 0.85 should always create links
    assert!(should_create_link(HIGH_SIMILARITY));
    assert!(should_create_link_code(HIGH_SIMILARITY));
}

// ============================================================================
// UNIT TESTS - Link Operations
// ============================================================================

#[test]
fn test_mock_link_creation() {
    let note_a = Uuid::new_v4();
    let note_b = Uuid::new_v4();

    let link = MockLink {
        id: Uuid::new_v4(),
        from_note_id: note_a,
        to_note_id: note_b,
        kind: "similar".to_string(),
        score: HIGH_SIMILARITY,
    };

    assert_eq!(link.from_note_id, note_a);
    assert_eq!(link.to_note_id, note_b);
    assert_eq!(link.kind, "similar");
    assert_eq!(link.score, HIGH_SIMILARITY);
}

#[test]
fn test_bidirectional_link_consistency() {
    let note_a = Uuid::new_v4();
    let note_b = Uuid::new_v4();

    let link_a_to_b = MockLink {
        id: Uuid::new_v4(),
        from_note_id: note_a,
        to_note_id: note_b,
        kind: "similar".to_string(),
        score: HIGH_SIMILARITY,
    };

    let link_b_to_a = MockLink {
        id: Uuid::new_v4(),
        from_note_id: note_b,
        to_note_id: note_a,
        kind: "similar".to_string(),
        score: HIGH_SIMILARITY,
    };

    // Bidirectional links should have same score and kind
    assert_eq!(link_a_to_b.score, link_b_to_a.score);
    assert_eq!(link_a_to_b.kind, link_b_to_a.kind);

    // But different IDs
    assert_ne!(link_a_to_b.id, link_b_to_a.id);

    // Verify reciprocal relationship
    assert_eq!(link_a_to_b.from_note_id, link_b_to_a.to_note_id);
    assert_eq!(link_a_to_b.to_note_id, link_b_to_a.from_note_id);
}

#[test]
fn test_link_score_range() {
    // Link scores should be in [0.0, 1.0] range
    let note_a = Uuid::new_v4();
    let note_b = Uuid::new_v4();

    for score in [0.0, 0.5, 0.7, 0.9, 1.0] {
        let link = MockLink {
            id: Uuid::new_v4(),
            from_note_id: note_a,
            to_note_id: note_b,
            kind: "similar".to_string(),
            score,
        };

        assert!(
            link.score >= 0.0 && link.score <= 1.0,
            "Link score must be in [0.0, 1.0]"
        );
    }
}

// ============================================================================
// INTEGRATION TESTS (Database required)
// ============================================================================

/// Link-001: Test that similar notes create automatic links above threshold.
#[test]
fn test_similar_notes_create_links() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create note A with content about quantum computing
    // - Generate embedding for note A
    // - Create note B with similar content (similarity > 0.70)
    // - Generate embedding for note B
    //
    // Test:
    // - Trigger automatic linking (or check if background job ran)
    // - Query links for note A
    //
    // Assert:
    // - Link exists from A to B
    // - Link exists from B to A (bidirectional)
    // - Link kind = "similar"
    // - Link score >= 0.70
}

/// Link-002: Test bidirectional link consistency.
#[test]
fn test_bidirectional_link_consistency_integration() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create two similar notes A and B
    // - Trigger automatic linking
    //
    // Test:
    // - Query outgoing links from A
    // - Query incoming links to A
    //
    // Assert:
    // - Link from A to B exists in A's outgoing links
    // - Link from B to A exists in A's incoming links
    // - Both links have same score
    // - Both links have same kind
    // - Link IDs are different (not the same link object)
}

/// Link-003: Test link updates when content changes.
#[test]
fn test_link_updates_on_content_change() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create notes A and B with high similarity (0.90)
    // - Verify link exists with score ~0.90
    // - Update note B's content to be dissimilar (similarity drops to 0.50)
    // - Regenerate embedding for B
    // - Trigger link recalculation
    //
    // Assert:
    // - Old link is deleted (similarity now below threshold)
    // - No link exists between A and B anymore
}

/// Link-004: Test link threshold boundary behavior.
#[test]
fn test_link_threshold_boundary() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create note A
    // - Create note B with similarity exactly 0.70 (at threshold)
    // - Create note C with similarity 0.69 (below threshold)
    // - Create note D with similarity 0.71 (above threshold)
    //
    // Assert:
    // - Link exists between A and B (at threshold)
    // - Link exists between A and D (above threshold)
    // - NO link exists between A and C (below threshold)
}

/// Link-005: Test link score calculation accuracy.
#[test]
fn test_link_score_calculation() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create notes with known embeddings
    // - Calculate expected cosine similarity manually
    //
    // Test:
    // - Trigger automatic linking
    // - Query link score
    //
    // Assert:
    // - Link score matches expected cosine similarity (within tolerance)
}

/// Link-006: Test link metadata preservation.
#[test]
fn test_link_metadata_preservation() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create links with custom metadata (e.g., link creation timestamp, link type)
    //
    // Test:
    // - Query link
    //
    // Assert:
    // - Metadata fields are preserved
    // - Created timestamp exists
    // - Link kind is correct
}

/// Link-007: Test link deletion cascades when note is deleted.
#[test]
fn test_link_deletion_cascade() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create notes A, B, C
    // - Create links: A<->B, A<->C, B<->C
    // - Verify 6 total links exist (bidirectional)
    //
    // Test:
    // - Delete note A
    //
    // Assert:
    // - Links A<->B are deleted (both directions)
    // - Links A<->C are deleted (both directions)
    // - Links B<->C remain (not affected)
    // - Total links = 2 (B<->C only)
}

/// Test link creation with multiple similar notes.
#[test]
fn test_multiple_similar_notes_linking() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create 5 notes on same topic (all > 0.70 similarity)
    //
    // Assert:
    // - Each note has 4 outgoing links
    // - Each note has 4 incoming links
    // - Total unique links = 5 * 4 = 20 (bidirectional)
    // - All links have score > 0.70
}

/// Test link creation with no similar notes.
#[test]
fn test_no_similar_notes_no_links() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create note about quantum computing
    // - Create note about cooking recipes (dissimilar)
    //
    // Assert:
    // - No links created (similarity < 0.70)
}

/// Test link update maintains bidirectionality.
#[test]
fn test_link_update_bidirectionality() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create notes A and B with similarity 0.90
    // - Verify links exist
    // - Update note A slightly (similarity drops to 0.80 but still above threshold)
    //
    // Assert:
    // - Links still exist (above threshold)
    // - Both links updated with new score
    // - Link A->B score == Link B->A score
}

/// Test link performance with large number of notes.
#[test]
fn test_link_creation_performance() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create 1000 notes
    // - Trigger automatic linking for all notes
    //
    // Measure:
    // - Time to create all links
    // - Memory usage
    //
    // Assert:
    // - Link creation completes within reasonable time (< 10 seconds)
    // - No memory leaks or excessive memory usage
}

/// Test concurrent link operations.
#[test]
fn test_concurrent_link_operations() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create base note A
    //
    // Test:
    // - Concurrently create 10 notes B1..B10 similar to A
    // - All trigger link creation simultaneously
    //
    // Assert:
    // - All links created successfully
    // - No duplicate links
    // - No race conditions or deadlocks
}

/// Test link query ordering by score.
#[test]
fn test_link_query_ordering() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create note A
    // - Create note B (similarity 0.95)
    // - Create note C (similarity 0.80)
    // - Create note D (similarity 0.72)
    //
    // Test:
    // - Query outgoing links from A
    //
    // Assert:
    // - Links ordered by score descending: B, C, D
    // - Highest similarity link appears first
}

/// Test link pagination.
#[test]
fn test_link_pagination() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create note with 100 similar notes
    //
    // Test:
    // - Query links with limit=10, offset=0
    // - Query links with limit=10, offset=10
    //
    // Assert:
    // - First page has 10 links
    // - Second page has 10 different links
    // - No overlap between pages
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

/// Test link creation with invalid note IDs.
#[test]
fn test_link_creation_invalid_note_id() {
    // TODO: Implement once database test infrastructure is available
    //
    // Test:
    // - Attempt to create link with non-existent note ID
    //
    // Assert:
    // - Returns error (foreign key constraint violation)
    // - No link created
}

/// Test link creation with self-reference.
#[test]
fn test_link_creation_self_reference() {
    // TODO: Implement once database test infrastructure is available
    //
    // Test:
    // - Attempt to create link from note A to itself
    //
    // Assert:
    // - Should be prevented or ignored
    // - No self-referencing links in database
}

/// Test duplicate link prevention.
#[test]
fn test_duplicate_link_prevention() {
    // TODO: Implement once database test infrastructure is available
    //
    // Setup:
    // - Create link from A to B
    //
    // Test:
    // - Attempt to create same link again (A to B with same kind)
    //
    // Assert:
    // - Duplicate insert is ignored or handled gracefully
    // - Only one link exists
}

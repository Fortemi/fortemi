//! Maximal Marginal Relevance (MMR) re-ranking for search result diversity.
//!
//! Implements the MMR algorithm from Carbonell & Goldstein (1998) to balance
//! relevance and diversity in search results.
//!
//! MMR = argmax_{d ∈ R\S} [λ · Sim(d, q) - (1-λ) · max_{d' ∈ S} Sim(d, d')]
//!
//! where:
//!   R = candidate result set
//!   S = already selected results
//!   q = query
//!   λ = 1 - diversity (0.0 = max diversity, 1.0 = pure relevance)

use std::collections::HashMap;

use pgvector::Vector;
use tracing::debug;
use uuid::Uuid;

use matric_core::SearchHit;

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Apply MMR re-ranking to search results.
///
/// # Arguments
///
/// * `candidates` - Post-RRF search results with their embedding vectors
/// * `query_vec` - The query embedding vector
/// * `diversity` - Diversity weight (0.0 = pure relevance, 1.0 = max diversity)
/// * `limit` - Maximum number of results to return
///
/// # Returns
///
/// Re-ranked search results balancing relevance and diversity.
/// Results without vectors are appended at the end in their original order.
pub fn mmr_rerank(
    candidates: Vec<SearchHit>,
    vectors: &HashMap<Uuid, Vector>,
    _query_vec: &Vector,
    diversity: f32,
    limit: usize,
) -> Vec<SearchHit> {
    if candidates.is_empty() || limit == 0 {
        return Vec::new();
    }

    let diversity = diversity.clamp(0.0, 1.0);
    let lambda = 1.0 - diversity;

    // If diversity is 0, return original ranking (pure relevance)
    if diversity == 0.0 {
        let mut results = candidates;
        results.truncate(limit);
        return results;
    }

    // Split candidates into those with vectors (can do MMR) and those without
    let mut with_vectors: Vec<(SearchHit, &[f32])> = Vec::new();
    let mut without_vectors: Vec<SearchHit> = Vec::new();

    for hit in candidates {
        if let Some(vec) = vectors.get(&hit.note_id) {
            with_vectors.push((hit, vec.as_slice()));
        } else {
            without_vectors.push(hit);
        }
    }

    // If no vectors available, return original ranking
    if with_vectors.is_empty() {
        let mut results = without_vectors;
        results.truncate(limit);
        return results;
    }

    // Normalize relevance scores to [0, 1] for MMR computation
    let max_score = with_vectors
        .iter()
        .map(|(h, _)| h.score)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_score = with_vectors
        .iter()
        .map(|(h, _)| h.score)
        .fold(f32::INFINITY, f32::min);
    let score_range = max_score - min_score;

    let mut selected: Vec<SearchHit> = Vec::with_capacity(limit);
    let mut selected_vecs: Vec<&[f32]> = Vec::with_capacity(limit);
    let mut remaining: Vec<(SearchHit, &[f32])> = with_vectors;

    while selected.len() < limit && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f32::NEG_INFINITY;

        for (i, (hit, vec)) in remaining.iter().enumerate() {
            // Relevance component: normalized RRF score
            let relevance = if score_range > 0.0 {
                (hit.score - min_score) / score_range
            } else {
                1.0 // All scores equal
            };

            // Diversity component: max similarity to already selected
            let max_sim_to_selected = if selected_vecs.is_empty() {
                // First selection: use similarity to query as diversity baseline
                // This ensures the most relevant result is selected first
                0.0
            } else {
                selected_vecs
                    .iter()
                    .map(|sv| cosine_similarity(vec, sv))
                    .fold(f32::NEG_INFINITY, f32::max)
            };

            // MMR score: balance relevance and diversity
            let mmr_score = lambda * relevance - (1.0 - lambda) * max_sim_to_selected;

            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = i;
            }
        }

        let (hit, vec) = remaining.swap_remove(best_idx);
        selected_vecs.push(vec);
        selected.push(hit);
    }

    // Append results without vectors at the end (if we still have capacity)
    let remaining_capacity = limit.saturating_sub(selected.len());
    if remaining_capacity > 0 {
        selected.extend(without_vectors.into_iter().take(remaining_capacity));
    }

    debug!(
        diversity = %diversity,
        lambda = %lambda,
        selected_count = selected.len(),
        "MMR re-ranking complete"
    );

    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hit(id: Uuid, score: f32) -> SearchHit {
        SearchHit {
            note_id: id,
            score,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }
    }

    fn make_vector(values: &[f32]) -> Vector {
        Vector::from(values.to_vec())
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = [1.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = [1.0, 0.0, 0.0];
        let b = [-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = [1.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_mmr_empty_candidates() {
        let vectors = HashMap::new();
        let query = make_vector(&[1.0, 0.0, 0.0]);
        let results = mmr_rerank(Vec::new(), &vectors, &query, 0.5, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_mmr_zero_limit() {
        let id = Uuid::new_v4();
        let vectors = HashMap::new();
        let query = make_vector(&[1.0, 0.0, 0.0]);
        let results = mmr_rerank(vec![make_hit(id, 1.0)], &vectors, &query, 0.5, 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_mmr_zero_diversity_preserves_order() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let candidates = vec![make_hit(id1, 0.9), make_hit(id2, 0.7), make_hit(id3, 0.5)];
        let vectors = HashMap::new();
        let query = make_vector(&[1.0, 0.0, 0.0]);

        let results = mmr_rerank(candidates, &vectors, &query, 0.0, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].note_id, id1);
        assert_eq!(results[1].note_id, id2);
        assert_eq!(results[2].note_id, id3);
    }

    #[test]
    fn test_mmr_high_diversity_promotes_dissimilar() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // id1 and id2 have very similar vectors, id3 is different
        let mut vectors = HashMap::new();
        vectors.insert(id1, make_vector(&[1.0, 0.0, 0.0]));
        vectors.insert(id2, make_vector(&[0.99, 0.1, 0.0])); // very similar to id1
        vectors.insert(id3, make_vector(&[0.0, 1.0, 0.0])); // orthogonal

        let query = make_vector(&[1.0, 0.0, 0.0]);

        // Pure relevance: id1 (0.9) > id2 (0.8) > id3 (0.7)
        let candidates = vec![make_hit(id1, 0.9), make_hit(id2, 0.8), make_hit(id3, 0.7)];

        // With high diversity, id3 should be promoted over id2
        let results = mmr_rerank(candidates, &vectors, &query, 0.8, 3);
        assert_eq!(results.len(), 3);
        // First result should still be the most relevant
        assert_eq!(results[0].note_id, id1);
        // Second should be the diverse one (id3), not the similar one (id2)
        assert_eq!(results[1].note_id, id3);
        assert_eq!(results[2].note_id, id2);
    }

    #[test]
    fn test_mmr_respects_limit() {
        let ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
        let mut vectors = HashMap::new();
        let candidates: Vec<SearchHit> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                vectors.insert(*id, make_vector(&[i as f32, 0.0, 0.0]));
                make_hit(*id, 1.0 - (i as f32 * 0.1))
            })
            .collect();

        let query = make_vector(&[1.0, 0.0, 0.0]);
        let results = mmr_rerank(candidates, &vectors, &query, 0.5, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_mmr_candidates_without_vectors_appended() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4(); // no vector
        let id3 = Uuid::new_v4();

        let mut vectors = HashMap::new();
        vectors.insert(id1, make_vector(&[1.0, 0.0, 0.0]));
        vectors.insert(id3, make_vector(&[0.0, 1.0, 0.0]));
        // id2 has no vector

        let candidates = vec![make_hit(id1, 0.9), make_hit(id2, 0.8), make_hit(id3, 0.7)];

        let query = make_vector(&[1.0, 0.0, 0.0]);
        let results = mmr_rerank(candidates, &vectors, &query, 0.5, 10);
        assert_eq!(results.len(), 3);

        // id2 should be last (no vector, appended after MMR-ranked results)
        assert_eq!(results[2].note_id, id2);
    }

    #[test]
    fn test_mmr_single_candidate() {
        let id = Uuid::new_v4();
        let mut vectors = HashMap::new();
        vectors.insert(id, make_vector(&[1.0, 0.0, 0.0]));

        let query = make_vector(&[1.0, 0.0, 0.0]);
        let results = mmr_rerank(vec![make_hit(id, 0.9)], &vectors, &query, 0.5, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, id);
    }

    #[test]
    fn test_mmr_all_identical_vectors() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let mut vectors = HashMap::new();
        vectors.insert(id1, make_vector(&[1.0, 0.0, 0.0]));
        vectors.insert(id2, make_vector(&[1.0, 0.0, 0.0]));
        vectors.insert(id3, make_vector(&[1.0, 0.0, 0.0]));

        let candidates = vec![make_hit(id1, 0.9), make_hit(id2, 0.7), make_hit(id3, 0.5)];

        let query = make_vector(&[1.0, 0.0, 0.0]);
        // With identical vectors, diversity penalty is the same for all, so
        // relevance ordering should be preserved
        let results = mmr_rerank(candidates, &vectors, &query, 0.5, 3);
        assert_eq!(results.len(), 3);
    }
}

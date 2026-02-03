//! Relative Score Fusion (RSF) for combining search results.
//!
//! RSF normalizes actual similarity scores to [0,1] via min-max scaling,
//! then combines with weighted sum. Unlike RRF which only uses rank position,
//! RSF preserves score magnitude — top results with large score gaps maintain
//! that distinction.
//!
//! Weaviate made RSF their default fusion in v1.24 (2024) after measuring
//! +6% recall on the FIQA benchmark compared to RRF.
//!
//! Reference: Weaviate Blog (2024), "Hybrid search fusion algorithms"

use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

use matric_core::SearchHit;

/// Metadata preserved from search results during RSF fusion.
struct HitMetadata {
    snippet: Option<String>,
    title: Option<String>,
    tags: Vec<String>,
}

/// Fuse multiple scored lists using Relative Score Fusion.
///
/// Each input list has scores normalized to [0,1] via min-max scaling,
/// then combined with weighted sum. Weights must sum to 1.0.
///
/// # Arguments
/// * `scored_lists` - Scored result lists (order matches weights)
/// * `weights` - Weight per list (should sum to 1.0)
/// * `limit` - Maximum results to return
pub fn rsf_fuse(
    scored_lists: Vec<Vec<SearchHit>>,
    weights: &[f32],
    limit: usize,
) -> Vec<SearchHit> {
    if scored_lists.is_empty() {
        return Vec::new();
    }

    let scored_list_count = scored_lists.len();
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    let mut metadata: HashMap<Uuid, HitMetadata> = HashMap::new();

    for (list_idx, list) in scored_lists.into_iter().enumerate() {
        let weight = weights.get(list_idx).copied().unwrap_or(1.0);

        // Normalize scores in this list to [0,1] via min-max
        let normalized = normalize_min_max(list);

        for hit in normalized {
            *scores.entry(hit.note_id).or_insert(0.0) += hit.score * weight;

            metadata.entry(hit.note_id).or_insert(HitMetadata {
                snippet: hit.snippet,
                title: hit.title,
                tags: hit.tags,
            });
        }
    }

    if scores.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<SearchHit> = scores
        .into_iter()
        .map(|(note_id, score)| {
            let meta = metadata.remove(&note_id).unwrap_or(HitMetadata {
                snippet: None,
                title: None,
                tags: Vec::new(),
            });
            SearchHit {
                note_id,
                score: score.min(1.0),
                snippet: meta.snippet,
                title: meta.title,
                tags: meta.tags,
                embedding_status: None,
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    debug!(
        input_lists = scored_list_count,
        result_count = results.len(),
        "RSF fusion complete"
    );

    results
}

/// Normalize scores to [0,1] range using min-max scaling.
///
/// If all scores are equal (range = 0), all normalized scores become 1.0.
fn normalize_min_max(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    if hits.is_empty() {
        return hits;
    }

    let min = hits.iter().map(|h| h.score).fold(f32::INFINITY, f32::min);
    let max = hits
        .iter()
        .map(|h| h.score)
        .fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;

    if range == 0.0 {
        // All scores equal — normalize to 1.0
        return hits
            .into_iter()
            .map(|mut h| {
                h.score = 1.0;
                h
            })
            .collect();
    }

    hits.into_iter()
        .map(|mut h| {
            h.score = (h.score - min) / range;
            h
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: Uuid, score: f32) -> SearchHit {
        SearchHit {
            note_id: id,
            score,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }
    }

    #[test]
    fn test_rsf_empty_lists() {
        let result = rsf_fuse(vec![], &[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rsf_single_list() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let list = vec![hit(id1, 0.9), hit(id2, 0.3)];

        let result = rsf_fuse(vec![list], &[1.0], 10);
        assert_eq!(result.len(), 2);
        // id1 should score 1.0 (max), id2 should score 0.0 (min) after normalization
        assert_eq!(result[0].note_id, id1);
        assert!((result[0].score - 1.0).abs() < 0.01);
        assert_eq!(result[1].note_id, id2);
        assert!(result[1].score.abs() < 0.01);
    }

    #[test]
    fn test_rsf_two_lists_overlapping() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let list1 = vec![hit(id1, 0.9), hit(id2, 0.5), hit(id3, 0.1)];
        let list2 = vec![hit(id2, 0.8), hit(id3, 0.6), hit(id1, 0.2)];

        let result = rsf_fuse(vec![list1, list2], &[0.5, 0.5], 10);
        assert_eq!(result.len(), 3);

        // id2 appears high in both lists, should score well
        // id1 is top of list1 but low in list2
        // id3 is low in list1 but mid in list2
        // All should be present with fused scores
        let scores: HashMap<Uuid, f32> = result.iter().map(|h| (h.note_id, h.score)).collect();
        assert!(scores.contains_key(&id1));
        assert!(scores.contains_key(&id2));
        assert!(scores.contains_key(&id3));
    }

    #[test]
    fn test_rsf_disjoint_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let list1 = vec![hit(id1, 0.9)];
        let list2 = vec![hit(id2, 0.8)];

        let result = rsf_fuse(vec![list1, list2], &[0.5, 0.5], 10);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_rsf_respects_limit() {
        let ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
        let list: Vec<SearchHit> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| hit(*id, 1.0 - (i as f32 * 0.1)))
            .collect();

        let result = rsf_fuse(vec![list], &[1.0], 3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_rsf_equal_scores_normalize_to_one() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let list = vec![hit(id1, 0.5), hit(id2, 0.5)];

        let result = rsf_fuse(vec![list], &[1.0], 10);
        assert_eq!(result.len(), 2);
        // All equal scores → normalize to 1.0
        assert!((result[0].score - 1.0).abs() < 0.01);
        assert!((result[1].score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rsf_asymmetric_weights() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // id1 is top in FTS (weight 0.7), id2 is top in semantic (weight 0.3)
        let fts = vec![hit(id1, 0.9), hit(id2, 0.1)];
        let sem = vec![hit(id2, 0.9), hit(id1, 0.1)];

        let result = rsf_fuse(vec![fts, sem], &[0.7, 0.3], 10);
        // id1 should score higher due to FTS weight advantage
        assert_eq!(result[0].note_id, id1);
    }

    #[test]
    fn test_normalize_min_max_empty() {
        let result = normalize_min_max(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_normalize_min_max_single() {
        let id = Uuid::new_v4();
        let result = normalize_min_max(vec![hit(id, 0.5)]);
        assert_eq!(result.len(), 1);
        // Single item → score 1.0 (range=0 case)
        assert!((result[0].score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_min_max_range() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let result = normalize_min_max(vec![hit(id1, 1.0), hit(id2, 0.5), hit(id3, 0.0)]);

        let scores: HashMap<Uuid, f32> = result.iter().map(|h| (h.note_id, h.score)).collect();
        assert!((scores[&id1] - 1.0).abs() < 0.01);
        assert!((scores[&id2] - 0.5).abs() < 0.01);
        assert!(scores[&id3].abs() < 0.01);
    }

    #[test]
    fn test_rsf_preserves_metadata() {
        let id = Uuid::new_v4();
        let list = vec![SearchHit {
            note_id: id,
            score: 0.9,
            snippet: Some("test snippet".to_string()),
            title: Some("Test Title".to_string()),
            tags: vec!["tag1".to_string()],
            embedding_status: None,
        }];

        let result = rsf_fuse(vec![list], &[1.0], 10);
        assert_eq!(result[0].snippet, Some("test snippet".to_string()));
        assert_eq!(result[0].title, Some("Test Title".to_string()));
        assert_eq!(result[0].tags, vec!["tag1".to_string()]);
    }
}

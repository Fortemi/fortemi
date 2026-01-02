//! Reciprocal Rank Fusion (RRF) for combining search results.

use std::collections::HashMap;
use uuid::Uuid;

use matric_core::SearchHit;

/// RRF constant (typically 60)
pub const RRF_K: f32 = 60.0;

/// Fuse multiple ranked lists using Reciprocal Rank Fusion.
///
/// Each input is a list of (note_id, score) pairs, ranked by score descending.
/// The output combines all lists using RRF scoring.
pub fn rrf_fuse(ranked_lists: Vec<Vec<SearchHit>>, limit: usize) -> Vec<SearchHit> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    let mut snippets: HashMap<Uuid, Option<String>> = HashMap::new();

    for list in ranked_lists {
        for (rank, hit) in list.into_iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + (rank as f32) + 1.0);
            *scores.entry(hit.note_id).or_insert(0.0) += rrf_score;

            // Keep the first non-empty snippet we find
            snippets.entry(hit.note_id).or_insert(hit.snippet);
        }
    }

    // Sort by RRF score descending
    let mut results: Vec<SearchHit> = scores
        .into_iter()
        .map(|(note_id, score)| SearchHit {
            note_id,
            score,
            snippet: snippets.remove(&note_id).flatten(),
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fuse_single_list() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let list = vec![
            SearchHit { note_id: id1, score: 0.9, snippet: Some("first".to_string()) },
            SearchHit { note_id: id2, score: 0.8, snippet: Some("second".to_string()) },
        ];

        let results = rrf_fuse(vec![list], 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].note_id, id1);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_rrf_fuse_multiple_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // List 1: id1 rank 0, id2 rank 1
        let list1 = vec![
            SearchHit { note_id: id1, score: 0.9, snippet: None },
            SearchHit { note_id: id2, score: 0.8, snippet: None },
        ];

        // List 2: id2 rank 0, id3 rank 1, id1 rank 2
        let list2 = vec![
            SearchHit { note_id: id2, score: 0.95, snippet: None },
            SearchHit { note_id: id3, score: 0.85, snippet: None },
            SearchHit { note_id: id1, score: 0.75, snippet: None },
        ];

        let results = rrf_fuse(vec![list1, list2], 10);

        // id2 should rank highest (rank 0 in list2, rank 1 in list1)
        // Both id1 and id2 appear in both lists
        assert_eq!(results.len(), 3);
        assert!(results.iter().any(|h| h.note_id == id1));
        assert!(results.iter().any(|h| h.note_id == id2));
        assert!(results.iter().any(|h| h.note_id == id3));
    }

    #[test]
    fn test_rrf_fuse_respects_limit() {
        let hits: Vec<SearchHit> = (0..100)
            .map(|i| SearchHit {
                note_id: Uuid::new_v4(),
                score: 1.0 - (i as f32 * 0.01),
                snippet: None,
            })
            .collect();

        let results = rrf_fuse(vec![hits], 10);
        assert_eq!(results.len(), 10);
    }
}

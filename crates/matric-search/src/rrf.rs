//! Reciprocal Rank Fusion (RRF) for combining search results.

use std::collections::HashMap;
use uuid::Uuid;

use matric_core::SearchHit;

/// RRF constant (typically 60)
pub const RRF_K: f32 = 60.0;

/// Metadata preserved from search results during RRF fusion.
struct HitMetadata {
    snippet: Option<String>,
    title: Option<String>,
    tags: Vec<String>,
}

/// Fuse multiple ranked lists using Reciprocal Rank Fusion.
///
/// Each input is a list of (note_id, score) pairs, ranked by score descending.
/// The output combines all lists using RRF scoring, normalized to 0.0-1.0 range.
pub fn rrf_fuse(ranked_lists: Vec<Vec<SearchHit>>, limit: usize) -> Vec<SearchHit> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    let mut metadata: HashMap<Uuid, HitMetadata> = HashMap::new();

    let num_lists = ranked_lists.len();

    for list in ranked_lists {
        for (rank, hit) in list.into_iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + (rank as f32) + 1.0);
            *scores.entry(hit.note_id).or_insert(0.0) += rrf_score;

            // Keep the first non-empty metadata we find
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

    // Calculate the maximum possible RRF score for normalization
    // Max score is achieved when a document is rank 0 in all lists
    let max_possible_score = num_lists as f32 / (RRF_K + 1.0);

    // Sort by RRF score descending
    let mut results: Vec<SearchHit> = scores
        .into_iter()
        .map(|(note_id, score)| {
            // Normalize score to 0.0-1.0 range
            let normalized_score = if max_possible_score > 0.0 {
                (score / max_possible_score).min(1.0)
            } else {
                0.0
            };

            let meta = metadata.remove(&note_id).unwrap_or(HitMetadata {
                snippet: None,
                title: None,
                tags: Vec::new(),
            });
            SearchHit {
                note_id,
                score: normalized_score,
                snippet: meta.snippet,
                title: meta.title,
                tags: meta.tags,
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
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
            SearchHit {
                note_id: id1,
                score: 0.9,
                snippet: Some("first".to_string()),
                title: Some("First Note".to_string()),
                tags: vec!["tag1".to_string()],
            },
            SearchHit {
                note_id: id2,
                score: 0.8,
                snippet: Some("second".to_string()),
                title: Some("Second Note".to_string()),
                tags: vec!["tag2".to_string()],
            },
        ];

        let results = rrf_fuse(vec![list], 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].note_id, id1);
        assert!(results[0].score > results[1].score);
        // First result should have normalized score of 1.0 (rank 0 in only list)
        assert!((results[0].score - 1.0).abs() < 0.001, "First result should be ~1.0, got {}", results[0].score);
        // Verify metadata is preserved
        assert_eq!(results[0].title, Some("First Note".to_string()));
        assert_eq!(results[0].tags, vec!["tag1".to_string()]);
    }

    #[test]
    fn test_rrf_fuse_multiple_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // List 1: id1 rank 0, id2 rank 1
        let list1 = vec![
            SearchHit {
                note_id: id1,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
            SearchHit {
                note_id: id2,
                score: 0.8,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
        ];

        // List 2: id2 rank 0, id3 rank 1, id1 rank 2
        let list2 = vec![
            SearchHit {
                note_id: id2,
                score: 0.95,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
            SearchHit {
                note_id: id3,
                score: 0.85,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
            SearchHit {
                note_id: id1,
                score: 0.75,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
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
                title: None,
                tags: Vec::new(),
            })
            .collect();

        let results = rrf_fuse(vec![hits], 10);
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_rrf_scores_normalized_to_0_1_range() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Document appears at rank 0 in both lists (maximum score)
        let list1 = vec![
            SearchHit { note_id: id1, score: 1.0, snippet: None, title: None, tags: Vec::new() },
            SearchHit { note_id: id2, score: 0.5, snippet: None, title: None, tags: Vec::new() },
        ];
        let list2 = vec![
            SearchHit { note_id: id1, score: 1.0, snippet: None, title: None, tags: Vec::new() },
        ];

        let results = rrf_fuse(vec![list1, list2], 10);

        // id1 is rank 0 in both lists, should have score = 1.0
        let id1_result = results.iter().find(|h| h.note_id == id1).unwrap();
        assert!((id1_result.score - 1.0).abs() < 0.001, "Top result should be ~1.0, got {}", id1_result.score);

        // All scores should be in 0.0-1.0 range
        for result in &results {
            assert!(result.score >= 0.0 && result.score <= 1.0,
                "Score {} out of range", result.score);
        }
    }
}

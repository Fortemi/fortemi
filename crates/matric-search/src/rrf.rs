//! Reciprocal Rank Fusion (RRF) for combining search results.

use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

use matric_core::SearchHit;

/// RRF constant. K=20 emphasizes top-ranked results more strongly than the
/// original K=60 default. Validated by Elasticsearch's BEIR grid search (2024)
/// which found K=20 optimal across diverse retrieval benchmarks.
/// Lower K is particularly suited for small-to-medium corpora where precision
/// matters more than deep recall.
///
/// Reference: Cormack et al. (2009), Elasticsearch BEIR analysis (2024)
pub const RRF_K: f32 = 20.0;

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
        input_lists = num_lists,
        rrf_k = RRF_K,
        result_count = results.len(),
        "RRF fusion complete"
    );

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
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.8,
                snippet: Some("second".to_string()),
                title: Some("Second Note".to_string()),
                tags: vec!["tag2".to_string()],
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list], 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].note_id, id1);
        assert!(results[0].score > results[1].score);
        // First result should have normalized score of 1.0 (rank 0 in only list)
        assert!(
            (results[0].score - 1.0).abs() < 0.001,
            "First result should be ~1.0, got {}",
            results[0].score
        );
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
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.8,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
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
                embedding_status: None,
            },
            SearchHit {
                note_id: id3,
                score: 0.85,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id1,
                score: 0.75,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
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
                embedding_status: None,
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
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];
        let list2 = vec![SearchHit {
            note_id: id1,
            score: 1.0,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }];

        let results = rrf_fuse(vec![list1, list2], 10);

        // id1 is rank 0 in both lists, should have score = 1.0
        let id1_result = results.iter().find(|h| h.note_id == id1).unwrap();
        assert!(
            (id1_result.score - 1.0).abs() < 0.001,
            "Top result should be ~1.0, got {}",
            id1_result.score
        );

        // All scores should be in 0.0-1.0 range
        for result in &results {
            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "Score {} out of range",
                result.score
            );
        }
    }

    // ========== NEW COMPREHENSIVE TESTS ==========

    #[test]
    fn test_rrf_fuse_empty_lists() {
        let results = rrf_fuse(vec![], 10);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rrf_fuse_empty_list_input() {
        let empty_list: Vec<SearchHit> = vec![];
        let results = rrf_fuse(vec![empty_list], 10);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rrf_fuse_multiple_empty_lists() {
        let empty1: Vec<SearchHit> = vec![];
        let empty2: Vec<SearchHit> = vec![];
        let empty3: Vec<SearchHit> = vec![];
        let results = rrf_fuse(vec![empty1, empty2, empty3], 10);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rrf_fuse_single_result() {
        let id1 = Uuid::new_v4();
        let list = vec![SearchHit {
            note_id: id1,
            score: 0.9,
            snippet: Some("single".to_string()),
            title: Some("Single Note".to_string()),
            tags: vec!["tag1".to_string()],
            embedding_status: None,
        }];

        let results = rrf_fuse(vec![list], 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, id1);
        assert!((results[0].score - 1.0).abs() < 0.001);
        assert_eq!(results[0].snippet, Some("single".to_string()));
        assert_eq!(results[0].title, Some("Single Note".to_string()));
    }

    #[test]
    fn test_rrf_fuse_limit_zero() {
        let id1 = Uuid::new_v4();
        let list = vec![SearchHit {
            note_id: id1,
            score: 1.0,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }];

        let results = rrf_fuse(vec![list], 0);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rrf_fuse_limit_one() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let list = vec![
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list], 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, id1);
    }

    #[test]
    fn test_rrf_fuse_large_result_set() {
        let hits: Vec<SearchHit> = (0..1000)
            .map(|i| SearchHit {
                note_id: Uuid::new_v4(),
                score: 1.0 - (i as f32 * 0.001),
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            })
            .collect();

        let results = rrf_fuse(vec![hits], 100);
        assert_eq!(results.len(), 100);

        // Verify results are sorted by score descending
        for i in 0..results.len() - 1 {
            assert!(results[i].score >= results[i + 1].score);
        }
    }

    #[test]
    fn test_rrf_fuse_metadata_preservation() {
        let id1 = Uuid::new_v4();
        let _id2 = Uuid::new_v4();

        let list1 = vec![SearchHit {
            note_id: id1,
            score: 0.9,
            snippet: Some("snippet1".to_string()),
            title: Some("Title1".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            embedding_status: None,
        }];

        let list2 = vec![SearchHit {
            note_id: id1,
            score: 0.8,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }];

        let results = rrf_fuse(vec![list1, list2], 10);

        // First list's metadata should be preserved
        assert_eq!(results[0].snippet, Some("snippet1".to_string()));
        assert_eq!(results[0].title, Some("Title1".to_string()));
        assert_eq!(results[0].tags.len(), 2);
    }

    #[test]
    fn test_rrf_fuse_metadata_from_first_occurrence() {
        let id1 = Uuid::new_v4();

        // First list has no metadata
        let list1 = vec![SearchHit {
            note_id: id1,
            score: 0.9,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }];

        // Second list has metadata
        let list2 = vec![SearchHit {
            note_id: id1,
            score: 0.8,
            snippet: Some("snippet2".to_string()),
            title: Some("Title2".to_string()),
            tags: vec!["tag2".to_string()],
            embedding_status: None,
        }];

        let results = rrf_fuse(vec![list1, list2], 10);

        // First occurrence's metadata (empty) should be preserved
        assert_eq!(results[0].snippet, None);
        assert_eq!(results[0].title, None);
        assert_eq!(results[0].tags.len(), 0);
    }

    #[test]
    fn test_rrf_fuse_score_calculation_single_list() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let list = vec![
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list], 10);

        // For a single list:
        // id1: rank 0 -> RRF score = 1/(60+0+1) = 1/61
        // id2: rank 1 -> RRF score = 1/(60+1+1) = 1/62
        // Max possible = 1/61 (since num_lists = 1)
        // id1 normalized = (1/61) / (1/61) = 1.0
        assert!((results[0].score - 1.0).abs() < 0.001);

        // id2 normalized should be less than 1.0
        assert!(results[1].score < 1.0);
        assert!(results[1].score > 0.0);
    }

    #[test]
    fn test_rrf_fuse_score_calculation_multiple_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // id1 appears at rank 0 in both lists
        let list1 = vec![SearchHit {
            note_id: id1,
            score: 1.0,
            snippet: None,
            title: None,
            tags: Vec::new(),
            embedding_status: None,
        }];

        let list2 = vec![
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list1, list2], 10);

        // id1: appears at rank 0 in 2 lists -> 2 * (1/61)
        // Max possible = 2 * (1/61) = 2/61 (num_lists = 2)
        // id1 normalized = (2/61) / (2/61) = 1.0
        let id1_result = results.iter().find(|h| h.note_id == id1).unwrap();
        assert!((id1_result.score - 1.0).abs() < 0.001);

        // id2: appears at rank 1 in 1 list -> 1/62
        // id2 normalized = (1/62) / (2/61) < 1.0
        let id2_result = results.iter().find(|h| h.note_id == id2).unwrap();
        assert!(id2_result.score < 1.0);
    }

    #[test]
    fn test_rrf_fuse_sorted_output() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let list1 = vec![
            SearchHit {
                note_id: id3,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id1,
                score: 0.8,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list1], 10);

        // Results should be sorted by score descending
        for i in 0..results.len() - 1 {
            assert!(
                results[i].score >= results[i + 1].score,
                "Results not sorted: {} < {}",
                results[i].score,
                results[i + 1].score
            );
        }
    }

    #[test]
    fn test_rrf_fuse_disjoint_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let id4 = Uuid::new_v4();

        // Completely different IDs in each list
        let list1 = vec![
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let list2 = vec![
            SearchHit {
                note_id: id3,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id4,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list1, list2], 10);

        // All 4 IDs should be present
        assert_eq!(results.len(), 4);
        assert!(results.iter().any(|h| h.note_id == id1));
        assert!(results.iter().any(|h| h.note_id == id2));
        assert!(results.iter().any(|h| h.note_id == id3));
        assert!(results.iter().any(|h| h.note_id == id4));
    }

    #[test]
    fn test_rrf_fuse_overlapping_lists() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // id1 and id2 in both lists, id3 only in list2
        let list1 = vec![
            SearchHit {
                note_id: id1,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id2,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let list2 = vec![
            SearchHit {
                note_id: id2,
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id1,
                score: 0.9,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
            SearchHit {
                note_id: id3,
                score: 0.8,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
            },
        ];

        let results = rrf_fuse(vec![list1, list2], 10);

        assert_eq!(results.len(), 3);

        // id1 and id2 should have higher scores than id3 (they appear in both lists)
        let id1_score = results.iter().find(|h| h.note_id == id1).unwrap().score;
        let id2_score = results.iter().find(|h| h.note_id == id2).unwrap().score;
        let id3_score = results.iter().find(|h| h.note_id == id3).unwrap().score;

        assert!(id1_score > id3_score);
        assert!(id2_score > id3_score);
    }

    #[test]
    fn test_rrf_constant_value() {
        // Verify RRF_K has the optimized value (K=20 per BEIR benchmarks)
        assert_eq!(RRF_K, 20.0);
    }
}

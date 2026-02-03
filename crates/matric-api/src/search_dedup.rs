//! Search deduplication integration for API endpoints.

use matric_core::SearchHit;
use matric_search::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit};

/// Apply deduplication to search results based on query parameters.
///
/// # Arguments
///
/// * `results` - Raw search results from the search engine
/// * `deduplicate` - Whether to deduplicate chunks (default: true)
/// * `expand` - Whether to expand chains (default: false)
///
/// # Returns
///
/// Deduplicated and enhanced search results
pub fn apply_search_deduplication(
    results: Vec<SearchHit>,
    deduplicate: Option<bool>,
    expand: Option<bool>,
) -> Vec<EnhancedSearchHit> {
    let config = DeduplicationConfig {
        deduplicate_chains: deduplicate.unwrap_or(true),
        expand_chains: expand.unwrap_or(false),
    };

    deduplicate_search_results(results, &config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_hit(note_id: Uuid, score: f32) -> SearchHit {
        SearchHit {
            note_id,
            score,
            snippet: Some("test".to_string()),
            title: Some("Test (Part 1/2)".to_string()),
            tags: vec![],
        }
    }

    #[test]
    fn test_apply_deduplication_default() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.9),
            create_test_hit(note_id, 0.7),
        ];

        let deduplicated = apply_search_deduplication(results, None, None);

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].hit.score, 0.9);
        assert!(deduplicated[0].chain_info.is_some());
    }

    #[test]
    fn test_apply_deduplication_disabled() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.9),
            create_test_hit(note_id, 0.7),
        ];

        let deduplicated = apply_search_deduplication(results, Some(false), None);

        assert_eq!(deduplicated.len(), 2);
        assert!(deduplicated.iter().all(|h| h.chain_info.is_none()));
    }

    #[test]
    fn test_apply_deduplication_explicit_enabled() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.9),
            create_test_hit(note_id, 0.7),
        ];

        let deduplicated = apply_search_deduplication(results, Some(true), Some(false));

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].hit.score, 0.9);
    }

    #[test]
    fn test_apply_deduplication_empty_results() {
        let results: Vec<SearchHit> = vec![];
        let deduplicated = apply_search_deduplication(results, None, None);

        assert_eq!(deduplicated.len(), 0);
    }
}

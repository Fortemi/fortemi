//! Search result deduplication for chunked documents.
//!
//! When documents are chunked for embedding, multiple chunks from the same
//! document can appear in search results. This module provides deduplication
//! logic to show only the best-scoring chunk per document.

use matric_core::SearchHit;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Information about a document chain (chunked document).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainSearchInfo {
    /// The note ID that acts as the chain identifier
    pub chain_id: Uuid,
    /// Original title of the note (not "Part N/M")
    pub original_title: String,
    /// How many chunks matched the search
    pub chunks_matched: usize,
    /// The sequence number of the best-scoring chunk
    pub best_chunk_sequence: u32,
    /// Total number of chunks in this document
    pub total_chunks: u32,
}

/// Extended search result with optional chain information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedSearchHit {
    /// The core search hit data
    #[serde(flatten)]
    pub hit: SearchHit,
    /// Chain information if this result is from a chunked document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_info: Option<ChainSearchInfo>,
}

/// Configuration for search result deduplication.
#[derive(Debug, Clone)]
pub struct DeduplicationConfig {
    /// Whether to deduplicate chunks from the same document (default: true)
    pub deduplicate_chains: bool,
    /// Whether to expand chains to include full document content (default: false)
    /// When true, the returned content includes all chunks concatenated
    pub expand_chains: bool,
}

impl Default for DeduplicationConfig {
    fn default() -> Self {
        Self {
            deduplicate_chains: true,
            expand_chains: false,
        }
    }
}

/// Deduplicate search results by grouping chunks from the same note.
///
/// This function:
/// 1. Groups results by note_id (chain_id)
/// 2. For each group, keeps only the best-scoring chunk
/// 3. Adds ChainSearchInfo with metadata about matched chunks
/// 4. Re-sorts by score after deduplication
///
/// # Arguments
///
/// * `results` - Raw search results potentially containing multiple chunks per note
/// * `config` - Deduplication configuration
///
/// # Returns
///
/// Deduplicated results with chain information attached
pub fn deduplicate_search_results(
    results: Vec<SearchHit>,
    config: &DeduplicationConfig,
) -> Vec<EnhancedSearchHit> {
    if !config.deduplicate_chains {
        // No deduplication - just wrap results
        return results
            .into_iter()
            .map(|hit| EnhancedSearchHit {
                hit,
                chain_info: None,
            })
            .collect();
    }

    // Group results by note_id (chain_id)
    let mut chains: HashMap<Uuid, Vec<SearchHit>> = HashMap::new();
    for hit in results {
        chains.entry(hit.note_id).or_default().push(hit);
    }

    // Process each chain
    let mut deduplicated: Vec<EnhancedSearchHit> = chains
        .into_iter()
        .map(|(chain_id, mut hits)| {
            // Sort hits by score descending to find best chunk
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let chunks_matched = hits.len();
            let best_hit = hits.into_iter().next().unwrap(); // Safe: at least one hit per chain

            // Extract original title (remove "Part N/M" suffix if present)
            let original_title = best_hit
                .title
                .as_ref()
                .map(|t| extract_original_title(t))
                .unwrap_or_else(|| format!("Note {}", chain_id));

            EnhancedSearchHit {
                hit: best_hit.clone(),
                chain_info: Some(ChainSearchInfo {
                    chain_id,
                    original_title,
                    chunks_matched,
                    best_chunk_sequence: 0, // TODO: Extract from chunk_index
                    total_chunks: chunks_matched as u32, // Conservative estimate
                }),
            }
        })
        .collect();

    // Re-sort by score after deduplication
    deduplicated.sort_by(|a, b| {
        b.hit
            .score
            .partial_cmp(&a.hit.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    deduplicated
}

/// Extract the original title from a potentially suffixed title.
///
/// Removes patterns like " (Part 1/3)" or " - Part 2 of 5" from titles.
fn extract_original_title(title: &str) -> String {
    // First trim the input
    let title = title.trim();

    // Remove common chunk suffixes
    let patterns = [
        regex::Regex::new(r"\s*\(Part\s+\d+/\d+\)\s*$").unwrap(),
        regex::Regex::new(r"\s*-\s*Part\s+\d+\s+of\s+\d+\s*$").unwrap(),
        regex::Regex::new(r"\s*\[\d+/\d+\]\s*$").unwrap(),
    ];

    let mut result = title.to_string();
    for pattern in &patterns {
        if let Some(m) = pattern.find(&result) {
            result = result[..m.start()].to_string();
            break;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_hit(note_id: Uuid, score: f32, title: Option<&str>) -> SearchHit {
        SearchHit {
            note_id,
            score,
            snippet: Some("test snippet".to_string()),
            title: title.map(|s| s.to_string()),
            tags: vec![],
            embedding_status: None,
        }
    }

    #[test]
    fn test_no_deduplication_when_disabled() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.9, Some("Title (Part 1/3)")),
            create_test_hit(note_id, 0.8, Some("Title (Part 2/3)")),
            create_test_hit(note_id, 0.7, Some("Title (Part 3/3)")),
        ];

        let config = DeduplicationConfig {
            deduplicate_chains: false,
            expand_chains: false,
        };

        let deduplicated = deduplicate_search_results(results.clone(), &config);

        assert_eq!(deduplicated.len(), 3);
        assert!(deduplicated.iter().all(|h| h.chain_info.is_none()));
    }

    #[test]
    fn test_deduplication_keeps_best_score() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.7, Some("Title (Part 1/3)")),
            create_test_hit(note_id, 0.9, Some("Title (Part 2/3)")),
            create_test_hit(note_id, 0.8, Some("Title (Part 3/3)")),
        ];

        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].hit.score, 0.9);
        assert!(deduplicated[0].chain_info.is_some());
    }

    #[test]
    fn test_deduplication_with_multiple_notes() {
        let note1 = Uuid::new_v4();
        let note2 = Uuid::new_v4();
        let note3 = Uuid::new_v4();

        let results = vec![
            create_test_hit(note1, 0.95, Some("Note 1 (Part 1/2)")),
            create_test_hit(note1, 0.85, Some("Note 1 (Part 2/2)")),
            create_test_hit(note2, 0.90, Some("Note 2 (Part 1/3)")),
            create_test_hit(note2, 0.88, Some("Note 2 (Part 2/3)")),
            create_test_hit(note2, 0.82, Some("Note 2 (Part 3/3)")),
            create_test_hit(note3, 0.70, Some("Note 3")),
        ];

        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        // Should have 3 results (one per note)
        assert_eq!(deduplicated.len(), 3);

        // Check scores are sorted descending
        assert_eq!(deduplicated[0].hit.score, 0.95); // note1
        assert_eq!(deduplicated[1].hit.score, 0.90); // note2
        assert_eq!(deduplicated[2].hit.score, 0.70); // note3

        // Check chain info
        assert_eq!(
            deduplicated[0].chain_info.as_ref().unwrap().chunks_matched,
            2
        );
        assert_eq!(
            deduplicated[1].chain_info.as_ref().unwrap().chunks_matched,
            3
        );
        assert_eq!(
            deduplicated[2].chain_info.as_ref().unwrap().chunks_matched,
            1
        );
    }

    #[test]
    fn test_chain_info_metadata() {
        let note_id = Uuid::new_v4();
        let results = vec![
            create_test_hit(note_id, 0.8, Some("My Document (Part 1/3)")),
            create_test_hit(note_id, 0.9, Some("My Document (Part 2/3)")),
            create_test_hit(note_id, 0.7, Some("My Document (Part 3/3)")),
        ];

        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        assert_eq!(deduplicated.len(), 1);

        let chain_info = deduplicated[0].chain_info.as_ref().unwrap();
        assert_eq!(chain_info.chain_id, note_id);
        assert_eq!(chain_info.original_title, "My Document");
        assert_eq!(chain_info.chunks_matched, 3);
    }

    #[test]
    fn test_extract_original_title_with_part_suffix() {
        assert_eq!(
            extract_original_title("My Document (Part 1/3)"),
            "My Document"
        );
        assert_eq!(
            extract_original_title("My Document - Part 2 of 5"),
            "My Document"
        );
        assert_eq!(extract_original_title("My Document [3/10]"), "My Document");
    }

    #[test]
    fn test_extract_original_title_without_suffix() {
        assert_eq!(extract_original_title("My Document"), "My Document");
        assert_eq!(
            extract_original_title("Document with (parentheses) in middle"),
            "Document with (parentheses) in middle"
        );
    }

    #[test]
    fn test_extract_original_title_edge_cases() {
        assert_eq!(extract_original_title(""), "");
        assert_eq!(
            extract_original_title("   Title with spaces   (Part 1/2)  "),
            "Title with spaces"
        );
    }

    #[test]
    fn test_deduplication_preserves_all_metadata() {
        let note_id = Uuid::new_v4();
        let results = vec![
            SearchHit {
                note_id,
                score: 0.9,
                snippet: Some("Best snippet".to_string()),
                title: Some("Title (Part 1/2)".to_string()),
                tags: vec!["tag1".to_string(), "tag2".to_string()],
                embedding_status: None,
            },
            SearchHit {
                note_id,
                score: 0.7,
                snippet: Some("Other snippet".to_string()),
                title: Some("Title (Part 2/2)".to_string()),
                tags: vec!["tag1".to_string(), "tag2".to_string()],
                embedding_status: None,
            },
        ];

        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(
            deduplicated[0].hit.snippet,
            Some("Best snippet".to_string())
        );
        assert_eq!(deduplicated[0].hit.tags.len(), 2);
    }

    #[test]
    fn test_deduplication_config_default() {
        let config = DeduplicationConfig::default();
        assert!(config.deduplicate_chains);
        assert!(!config.expand_chains);
    }

    #[test]
    fn test_enhanced_search_hit_serialization() {
        let note_id = Uuid::new_v4();
        let hit = EnhancedSearchHit {
            hit: create_test_hit(note_id, 0.9, Some("Test")),
            chain_info: Some(ChainSearchInfo {
                chain_id: note_id,
                original_title: "Test".to_string(),
                chunks_matched: 2,
                best_chunk_sequence: 1,
                total_chunks: 3,
            }),
        };

        let json = serde_json::to_string(&hit).unwrap();
        let deserialized: EnhancedSearchHit = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.hit.note_id, note_id);
        assert_eq!(deserialized.chain_info.as_ref().unwrap().chunks_matched, 2);
    }

    #[test]
    fn test_chain_search_info_serialization() {
        let chain_id = Uuid::new_v4();
        let info = ChainSearchInfo {
            chain_id,
            original_title: "Test Document".to_string(),
            chunks_matched: 5,
            best_chunk_sequence: 2,
            total_chunks: 10,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ChainSearchInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.chain_id, chain_id);
        assert_eq!(deserialized.original_title, "Test Document");
        assert_eq!(deserialized.chunks_matched, 5);
        assert_eq!(deserialized.best_chunk_sequence, 2);
        assert_eq!(deserialized.total_chunks, 10);
    }

    #[test]
    fn test_empty_results() {
        let results: Vec<SearchHit> = vec![];
        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        assert_eq!(deduplicated.len(), 0);
    }

    #[test]
    fn test_single_result_no_duplication() {
        let note_id = Uuid::new_v4();
        let results = vec![create_test_hit(note_id, 0.9, Some("Single Note"))];

        let config = DeduplicationConfig::default();
        let deduplicated = deduplicate_search_results(results, &config);

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(
            deduplicated[0].chain_info.as_ref().unwrap().chunks_matched,
            1
        );
    }
}

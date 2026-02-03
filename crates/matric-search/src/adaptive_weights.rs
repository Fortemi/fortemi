//! Adaptive FTS/semantic weight selection based on query characteristics.
//!
//! Selects optimal FTS vs. semantic weights depending on query type.
//! Research basis: Elasticsearch BEIR benchmarks (2024), Pinecone hybrid search guide (2024).
//!
//! Equal 0.5/0.5 weighting is a safe default but suboptimal across query types.
//! Short keyword queries benefit from FTS emphasis; long conceptual queries
//! benefit from semantic emphasis.

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::adaptive_rrf::QueryCharacteristics;

/// FTS and semantic weight pair, always summing to 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FusionWeights {
    /// Weight for full-text search (BM25) results
    pub fts: f32,
    /// Weight for semantic (dense retrieval) results
    pub semantic: f32,
}

impl Default for FusionWeights {
    fn default() -> Self {
        Self {
            fts: 0.5,
            semantic: 0.5,
        }
    }
}

/// Configuration for adaptive weight selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveWeightConfig {
    /// Whether adaptive weight selection is enabled
    pub enabled: bool,
    /// Weights for exact match / UUID / reference code queries
    pub exact_match_weights: FusionWeights,
    /// Weights for short keyword queries (1-2 tokens)
    pub keyword_weights: FusionWeights,
    /// Weights for balanced natural language queries (3-5 tokens)
    pub balanced_weights: FusionWeights,
    /// Weights for long conceptual queries (6+ tokens)
    pub conceptual_weights: FusionWeights,
    /// Weights for quoted phrase queries
    pub quoted_weights: FusionWeights,
}

impl Default for AdaptiveWeightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            exact_match_weights: FusionWeights {
                fts: 0.8,
                semantic: 0.2,
            },
            keyword_weights: FusionWeights {
                fts: 0.6,
                semantic: 0.4,
            },
            balanced_weights: FusionWeights {
                fts: 0.5,
                semantic: 0.5,
            },
            conceptual_weights: FusionWeights {
                fts: 0.35,
                semantic: 0.65,
            },
            quoted_weights: FusionWeights {
                fts: 0.7,
                semantic: 0.3,
            },
        }
    }
}

/// Selects FTS/semantic weights based on query characteristics.
///
/// # Weight Selection Strategy
///
/// | Query Type | FTS | Semantic | Rationale |
/// |------------|-----|----------|-----------|
/// | Quoted phrases | 0.7 | 0.3 | Lexical precision matters |
/// | Keywords (1-2 tokens) | 0.6 | 0.4 | FTS handles keywords well |
/// | Natural language (3-5) | 0.5 | 0.5 | Balanced |
/// | Conceptual (6+ tokens) | 0.35 | 0.65 | Semantic captures intent |
pub fn select_weights(
    config: &AdaptiveWeightConfig,
    query: &QueryCharacteristics,
) -> FusionWeights {
    if !config.enabled {
        return config.balanced_weights;
    }

    // Quoted queries get precision-focused weights
    if query.has_quotes {
        return config.quoted_weights;
    }

    // Select by query length
    let weights = match query.token_count {
        0 => config.balanced_weights,
        1..=2 => config.keyword_weights,
        3..=5 => config.balanced_weights,
        _ => config.conceptual_weights,
    };

    debug!(
        fts_weight = weights.fts,
        semantic_weight = weights.semantic,
        token_count = query.token_count,
        "Adaptive weights selected"
    );

    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_weights() {
        let w = FusionWeights::default();
        assert_eq!(w.fts, 0.5);
        assert_eq!(w.semantic, 0.5);
    }

    #[test]
    fn test_default_config() {
        let config = AdaptiveWeightConfig::default();
        assert!(config.enabled);
        assert_eq!(config.keyword_weights.fts, 0.6);
        assert_eq!(config.conceptual_weights.semantic, 0.65);
    }

    #[test]
    fn test_select_weights_disabled() {
        let config = AdaptiveWeightConfig {
            enabled: false,
            ..Default::default()
        };
        let query = QueryCharacteristics::analyze("rust");
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.5);
        assert_eq!(w.semantic, 0.5);
    }

    #[test]
    fn test_select_weights_keyword() {
        let config = AdaptiveWeightConfig::default();
        let query = QueryCharacteristics::analyze("rust");
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.6);
        assert_eq!(w.semantic, 0.4);
    }

    #[test]
    fn test_select_weights_two_keywords() {
        let config = AdaptiveWeightConfig::default();
        let query = QueryCharacteristics::analyze("rust async");
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.6);
        assert_eq!(w.semantic, 0.4);
    }

    #[test]
    fn test_select_weights_balanced() {
        let config = AdaptiveWeightConfig::default();
        let query = QueryCharacteristics::analyze("rust async programming guide");
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.5);
        assert_eq!(w.semantic, 0.5);
    }

    #[test]
    fn test_select_weights_conceptual() {
        let config = AdaptiveWeightConfig::default();
        let query =
            QueryCharacteristics::analyze("how do I implement semantic search in rust programming");
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.35);
        assert_eq!(w.semantic, 0.65);
    }

    #[test]
    fn test_select_weights_quoted() {
        let config = AdaptiveWeightConfig::default();
        let query = QueryCharacteristics::analyze(r#""machine learning" algorithms"#);
        let w = select_weights(&config, &query);
        assert_eq!(w.fts, 0.7);
        assert_eq!(w.semantic, 0.3);
    }

    #[test]
    fn test_select_weights_empty_query() {
        let config = AdaptiveWeightConfig::default();
        let query = QueryCharacteristics::analyze("");
        let w = select_weights(&config, &query);
        // Empty query → balanced
        assert_eq!(w.fts, 0.5);
        assert_eq!(w.semantic, 0.5);
    }

    #[test]
    fn test_config_serialization() {
        let config = AdaptiveWeightConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AdaptiveWeightConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.enabled);
        assert_eq!(deserialized.keyword_weights.fts, 0.6);
    }

    #[test]
    fn test_weights_serialization() {
        let w = FusionWeights {
            fts: 0.7,
            semantic: 0.3,
        };
        let json = serde_json::to_string(&w).unwrap();
        let deserialized: FusionWeights = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.fts, 0.7);
        assert_eq!(deserialized.semantic, 0.3);
    }
}

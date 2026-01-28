//! Adaptive Reciprocal Rank Fusion (RRF) parameter tuning.
//!
//! Adjusts the RRF k parameter based on query characteristics
//! and result distribution for improved fusion quality.
//!
//! Reference: REF-027 - Cormack et al. "Reciprocal Rank Fusion"

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Query characteristics extracted from user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCharacteristics {
    /// Number of tokens in the query
    pub token_count: usize,
    /// Whether query contains quoted phrases
    pub has_quotes: bool,
    /// Average token length
    pub avg_token_length: f32,
    /// Whether this appears to be a keyword query (vs natural language)
    pub is_keyword_query: bool,
}

impl QueryCharacteristics {
    /// Analyzes a query string to extract characteristics.
    pub fn analyze(query: &str) -> Self {
        let has_quotes = query.contains('"') || query.contains('\'');

        // Simple whitespace tokenization
        let tokens: Vec<&str> = query.split_whitespace().collect();
        let token_count = tokens.len();

        let avg_token_length = if token_count > 0 {
            let total_len: usize = tokens.iter().map(|t| t.len()).sum();
            total_len as f32 / token_count as f32
        } else {
            0.0
        };

        // Heuristic: keyword queries tend to have shorter tokens and fewer tokens
        // Natural language queries have longer average tokens and more tokens
        let is_keyword_query = token_count <= 3 && avg_token_length < 6.0;

        Self {
            token_count,
            has_quotes,
            avg_token_length,
            is_keyword_query,
        }
    }
}

/// Configuration for adaptive RRF parameter selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveRrfConfig {
    /// Whether adaptive k selection is enabled
    pub adaptive_enabled: bool,
    /// Default k value when adaptive is disabled or as baseline
    pub default_k: u32,
    /// Minimum allowed k value
    pub min_k: u32,
    /// Maximum allowed k value
    pub max_k: u32,
}

impl Default for AdaptiveRrfConfig {
    fn default() -> Self {
        Self {
            adaptive_enabled: true,
            default_k: 20,
            min_k: 8,
            max_k: 40,
        }
    }
}

/// Selects appropriate RRF k parameter based on query characteristics.
///
/// # Algorithm
/// - Short queries (<=2 tokens): k *= 0.7 (tighter fusion)
/// - Long queries (>=6 tokens): k *= 1.3 (looser fusion)
/// - Quoted queries: k *= 0.6 (precision focus)
/// - Result is clamped to [min_k, max_k]
pub fn select_k(config: &AdaptiveRrfConfig, query: &QueryCharacteristics) -> u32 {
    if !config.adaptive_enabled {
        return config.default_k;
    }

    let mut k = config.default_k as f32;

    // Short queries benefit from tighter fusion
    if query.token_count <= 2 {
        k *= 0.7;
    }

    // Long queries benefit from looser fusion
    if query.token_count >= 6 {
        k *= 1.3;
    }

    // Quoted queries indicate precision requirements
    if query.has_quotes {
        k *= 0.6;
    }

    // Clamp to configured bounds
    let k = k.round() as u32;
    let k = k.clamp(config.min_k, config.max_k);

    debug!(
        selected_k = k,
        token_count = query.token_count,
        has_quotes = query.has_quotes,
        is_keyword = query.is_keyword_query,
        "Adaptive RRF k selected"
    );

    k
}

/// Computes RRF score for a given rank and k parameter.
///
/// # Formula
/// score = 1.0 / (k + rank)
///
/// # Arguments
/// * `rank` - 1-indexed rank position
/// * `k` - RRF k parameter
pub fn rrf_score(rank: usize, k: u32) -> f64 {
    1.0 / ((k as usize + rank) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AdaptiveRrfConfig::default();
        assert!(config.adaptive_enabled);
        assert_eq!(config.default_k, 20);
        assert_eq!(config.min_k, 8);
        assert_eq!(config.max_k, 40);
    }

    #[test]
    fn test_query_analysis_short() {
        let chars = QueryCharacteristics::analyze("rust");
        assert_eq!(chars.token_count, 1);
        assert!(!chars.has_quotes);
        assert!(chars.is_keyword_query);
    }

    #[test]
    fn test_query_analysis_long() {
        let chars = QueryCharacteristics::analyze("how do I implement semantic search in rust");
        assert_eq!(chars.token_count, 8);
        assert!(!chars.has_quotes);
        assert!(!chars.is_keyword_query);
    }

    #[test]
    fn test_query_analysis_quoted() {
        let chars = QueryCharacteristics::analyze(r#""machine learning" algorithms"#);
        assert!(chars.has_quotes);
    }

    #[test]
    fn test_query_analysis_avg_token_length() {
        let chars = QueryCharacteristics::analyze("cat dog");
        assert_eq!(chars.token_count, 2);
        assert_eq!(chars.avg_token_length, 3.0); // (3 + 3) / 2
    }

    #[test]
    fn test_query_analysis_empty() {
        let chars = QueryCharacteristics::analyze("");
        assert_eq!(chars.token_count, 0);
        assert_eq!(chars.avg_token_length, 0.0);
    }

    #[test]
    fn test_select_k_disabled() {
        let config = AdaptiveRrfConfig {
            adaptive_enabled: false,
            default_k: 20,
            min_k: 8,
            max_k: 40,
        };
        let chars = QueryCharacteristics::analyze("any query");
        assert_eq!(select_k(&config, &chars), 20);
    }

    #[test]
    fn test_select_k_short_query() {
        let config = AdaptiveRrfConfig::default();
        let chars = QueryCharacteristics::analyze("rust");
        let k = select_k(&config, &chars);
        // 20 * 0.7 = 14
        assert_eq!(k, 14);
    }

    #[test]
    fn test_select_k_long_query() {
        let config = AdaptiveRrfConfig::default();
        let chars =
            QueryCharacteristics::analyze("how to implement semantic search in rust programming");
        let k = select_k(&config, &chars);
        // 20 * 1.3 = 26
        assert_eq!(k, 26);
    }

    #[test]
    fn test_select_k_quoted_query() {
        let config = AdaptiveRrfConfig::default();
        let chars =
            QueryCharacteristics::analyze(r#""machine learning" "neural networks" research"#);
        let k = select_k(&config, &chars);
        // 3 tokens (not short, not long), has quotes: 20 * 0.6 = 12
        assert_eq!(k, 12);
    }

    #[test]
    fn test_select_k_short_quoted() {
        let config = AdaptiveRrfConfig::default();
        let chars = QueryCharacteristics {
            token_count: 2,
            has_quotes: true,
            avg_token_length: 5.0,
            is_keyword_query: true,
        };
        let k = select_k(&config, &chars);
        // 20 * 0.7 * 0.6 = 8.4 -> 8
        assert_eq!(k, 8);
    }

    #[test]
    fn test_select_k_clamping_min() {
        let config = AdaptiveRrfConfig {
            adaptive_enabled: true,
            default_k: 15,
            min_k: 10,
            max_k: 40,
        };
        let chars = QueryCharacteristics {
            token_count: 1,
            has_quotes: true,
            avg_token_length: 3.0,
            is_keyword_query: true,
        };
        let k = select_k(&config, &chars);
        // 15 * 0.7 * 0.6 = 6.3, clamped to 10
        assert_eq!(k, 10);
    }

    #[test]
    fn test_select_k_clamping_max() {
        let config = AdaptiveRrfConfig {
            adaptive_enabled: true,
            default_k: 35,
            min_k: 8,
            max_k: 40,
        };
        let chars = QueryCharacteristics {
            token_count: 8,
            has_quotes: false,
            avg_token_length: 7.0,
            is_keyword_query: false,
        };
        let k = select_k(&config, &chars);
        // 35 * 1.3 = 45.5, clamped to 40
        assert_eq!(k, 40);
    }

    #[test]
    fn test_rrf_score_rank_1() {
        let score = rrf_score(1, 60);
        assert!((score - 1.0 / 61.0).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_score_rank_10() {
        let score = rrf_score(10, 60);
        assert!((score - 1.0 / 70.0).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_score_different_k() {
        let score1 = rrf_score(5, 20);
        let score2 = rrf_score(5, 100);
        // score1 = 1/25, score2 = 1/105
        assert!(score1 > score2); // Lower k gives higher scores
    }

    #[test]
    fn test_rrf_score_monotonic_decrease() {
        let k = 60;
        let score1 = rrf_score(1, k);
        let score2 = rrf_score(2, k);
        let score3 = rrf_score(10, k);
        assert!(score1 > score2);
        assert!(score2 > score3);
    }

    #[test]
    fn test_config_serialization() {
        let config = AdaptiveRrfConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AdaptiveRrfConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.default_k, 20);
        assert_eq!(deserialized.min_k, 8);
        assert_eq!(deserialized.max_k, 40);
    }

    #[test]
    fn test_query_characteristics_serialization() {
        let chars = QueryCharacteristics::analyze("test query");
        let json = serde_json::to_string(&chars).unwrap();
        let deserialized: QueryCharacteristics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.token_count, chars.token_count);
        assert_eq!(deserialized.has_quotes, chars.has_quotes);
    }
}

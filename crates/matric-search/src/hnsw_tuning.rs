//! Dynamic HNSW ef_search parameter tuning.
//!
//! Adjusts ef_search based on recall targets and corpus size
//! for optimal precision/latency trade-offs.
//!
//! Reference: REF-031 - Malkov & Yashunin "HNSW"

use serde::{Deserialize, Serialize};

/// Recall target levels for HNSW search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecallTarget {
    /// Fast search with moderate recall (~85%)
    Fast,
    /// Balanced recall/latency (~92%)
    Balanced,
    /// High recall (~96%)
    High,
    /// Exhaustive search for maximum recall (~99%)
    Exhaustive,
}

impl RecallTarget {
    /// Returns the base ef_search value for this recall target.
    pub fn base_ef(&self) -> u32 {
        match self {
            RecallTarget::Fast => 20,
            RecallTarget::Balanced => 40,
            RecallTarget::High => 100,
            RecallTarget::Exhaustive => 200,
        }
    }
}

/// Configuration for HNSW ef_search tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswTuningConfig {
    /// Default recall target
    pub default_target: RecallTarget,
    /// Scaling factor for corpus size adjustment
    pub corpus_scale_factor: f32,
    /// Minimum ef_search value
    pub min_ef: u32,
    /// Maximum ef_search value
    pub max_ef: u32,
}

impl Default for HnswTuningConfig {
    fn default() -> Self {
        Self {
            default_target: RecallTarget::Balanced,
            corpus_scale_factor: 1.0,
            min_ef: 10,
            max_ef: 500,
        }
    }
}

/// Computes optimal ef_search parameter based on recall target and corpus size.
///
/// # Algorithm
/// ef = base_ef * max(1.0, log2(corpus_size / 10000) * scale_factor)
/// Result is clamped to [min_ef, max_ef]
///
/// # Arguments
/// * `target` - Desired recall target
/// * `corpus_size` - Number of vectors in the index
/// * `config` - Tuning configuration
pub fn compute_ef(target: &RecallTarget, corpus_size: usize, config: &HnswTuningConfig) -> u32 {
    let base = target.base_ef() as f32;

    // Scale ef based on corpus size
    let size_ratio = corpus_size as f32 / 10000.0;
    let scale = if size_ratio > 1.0 {
        size_ratio.log2() * config.corpus_scale_factor
    } else {
        0.0
    };

    let ef = base * (1.0 + scale).max(1.0);
    let ef = ef.round() as u32;

    // Clamp to configured bounds
    ef.clamp(config.min_ef, config.max_ef)
}

/// Estimates recall rate for a given ef_search value.
///
/// # Formula
/// recall ≈ 1.0 - 1.0 / (1.0 + ef / 20.0)
///
/// This is a heuristic model based on empirical HNSW behavior.
pub fn estimated_recall(ef: u32) -> f32 {
    1.0 - 1.0 / (1.0 + (ef as f32 / 20.0))
}

/// Estimates search latency in milliseconds for given ef and corpus size.
///
/// # Formula
/// latency ≈ (ef / 40) * sqrt(corpus_size / 10000) * 4.0
///
/// This is a rough model for estimation purposes.
///
/// # Arguments
/// * `ef` - ef_search parameter
/// * `corpus_size` - Number of vectors in index
pub fn estimated_latency_ms(ef: u32, corpus_size: usize) -> f32 {
    let ef_factor = ef as f32 / 40.0;
    let size_factor = (corpus_size as f32 / 10000.0).sqrt();
    ef_factor * size_factor * 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recall_target_base_ef() {
        assert_eq!(RecallTarget::Fast.base_ef(), 20);
        assert_eq!(RecallTarget::Balanced.base_ef(), 40);
        assert_eq!(RecallTarget::High.base_ef(), 100);
        assert_eq!(RecallTarget::Exhaustive.base_ef(), 200);
    }

    #[test]
    fn test_default_config() {
        let config = HnswTuningConfig::default();
        assert_eq!(config.default_target, RecallTarget::Balanced);
        assert_eq!(config.corpus_scale_factor, 1.0);
        assert_eq!(config.min_ef, 10);
        assert_eq!(config.max_ef, 500);
    }

    #[test]
    fn test_compute_ef_small_corpus() {
        let config = HnswTuningConfig::default();
        let ef = compute_ef(&RecallTarget::Balanced, 5000, &config);
        // 5000 < 10000, so no scaling: base_ef = 40
        assert_eq!(ef, 40);
    }

    #[test]
    fn test_compute_ef_medium_corpus() {
        let config = HnswTuningConfig::default();
        let ef = compute_ef(&RecallTarget::Balanced, 40000, &config);
        // 40000/10000 = 4, log2(4) = 2, 40 * (1 + 2) = 120
        assert_eq!(ef, 120);
    }

    #[test]
    fn test_compute_ef_large_corpus() {
        let config = HnswTuningConfig::default();
        let ef = compute_ef(&RecallTarget::Balanced, 160000, &config);
        // 160000/10000 = 16, log2(16) = 4, 40 * (1 + 4) = 200
        assert_eq!(ef, 200);
    }

    #[test]
    fn test_compute_ef_fast_target() {
        let config = HnswTuningConfig::default();
        let ef = compute_ef(&RecallTarget::Fast, 40000, &config);
        // base_ef = 20, 20 * (1 + 2) = 60
        assert_eq!(ef, 60);
    }

    #[test]
    fn test_compute_ef_exhaustive_target() {
        let config = HnswTuningConfig::default();
        let ef = compute_ef(&RecallTarget::Exhaustive, 40000, &config);
        // base_ef = 200, 200 * (1 + 2) = 600, clamped to max_ef = 500
        assert_eq!(ef, 500);
    }

    #[test]
    fn test_compute_ef_clamping_min() {
        let config = HnswTuningConfig {
            min_ef: 30,
            max_ef: 500,
            ..Default::default()
        };
        let ef = compute_ef(&RecallTarget::Fast, 1000, &config);
        // Would be 20, but clamped to 30
        assert_eq!(ef, 30);
    }

    #[test]
    fn test_compute_ef_clamping_max() {
        let config = HnswTuningConfig {
            min_ef: 10,
            max_ef: 150,
            ..Default::default()
        };
        let ef = compute_ef(&RecallTarget::High, 100000, &config);
        // Would be > 150, clamped to max
        assert_eq!(ef, 150);
    }

    #[test]
    fn test_compute_ef_custom_scale_factor() {
        let config = HnswTuningConfig {
            corpus_scale_factor: 0.5,
            ..Default::default()
        };
        let ef = compute_ef(&RecallTarget::Balanced, 40000, &config);
        // log2(4) = 2, 2 * 0.5 = 1, 40 * (1 + 1) = 80
        assert_eq!(ef, 80);
    }

    #[test]
    fn test_estimated_recall_low_ef() {
        let recall = estimated_recall(10);
        // 1 - 1/(1 + 0.5) = 1 - 1/1.5 = 0.333...
        assert!((recall - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_estimated_recall_medium_ef() {
        let recall = estimated_recall(40);
        // 1 - 1/(1 + 2) = 1 - 1/3 = 0.666...
        assert!((recall - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_estimated_recall_high_ef() {
        let recall = estimated_recall(200);
        // 1 - 1/(1 + 10) = 1 - 1/11 = 0.909...
        assert!((recall - 0.909).abs() < 0.01);
    }

    #[test]
    fn test_estimated_recall_monotonic() {
        let recall1 = estimated_recall(10);
        let recall2 = estimated_recall(40);
        let recall3 = estimated_recall(200);
        assert!(recall1 < recall2);
        assert!(recall2 < recall3);
    }

    #[test]
    fn test_estimated_latency_baseline() {
        let latency = estimated_latency_ms(40, 10000);
        // (40/40) * sqrt(1) * 4 = 1 * 1 * 4 = 4.0
        assert_eq!(latency, 4.0);
    }

    #[test]
    fn test_estimated_latency_scales_with_ef() {
        let latency1 = estimated_latency_ms(20, 10000);
        let latency2 = estimated_latency_ms(40, 10000);
        // Should double when ef doubles
        assert!((latency2 - 2.0 * latency1).abs() < 0.01);
    }

    #[test]
    fn test_estimated_latency_scales_with_corpus() {
        let latency1 = estimated_latency_ms(40, 10000);
        let latency2 = estimated_latency_ms(40, 40000);
        // sqrt(4) = 2, so should double
        assert!((latency2 - 2.0 * latency1).abs() < 0.01);
    }

    #[test]
    fn test_estimated_latency_large_corpus() {
        let latency = estimated_latency_ms(100, 100000);
        // (100/40) * sqrt(10) * 4 ≈ 2.5 * 3.162 * 4 ≈ 31.62
        assert!((latency - 31.62).abs() < 0.1);
    }

    #[test]
    fn test_config_serialization() {
        let config = HnswTuningConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: HnswTuningConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.default_target, RecallTarget::Balanced);
        assert_eq!(deserialized.min_ef, 10);
        assert_eq!(deserialized.max_ef, 500);
    }

    #[test]
    fn test_recall_target_serialization() {
        let target = RecallTarget::High;
        let json = serde_json::to_string(&target).unwrap();
        let deserialized: RecallTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, RecallTarget::High);
    }

    #[test]
    fn test_ef_recall_tradeoff() {
        // Verify that higher ef leads to higher recall and latency
        let ef_values = [20, 40, 100, 200];
        let corpus = 50000;

        for i in 0..ef_values.len() - 1 {
            let ef1 = ef_values[i];
            let ef2 = ef_values[i + 1];

            let recall1 = estimated_recall(ef1);
            let recall2 = estimated_recall(ef2);
            assert!(recall2 > recall1, "Higher ef should give higher recall");

            let latency1 = estimated_latency_ms(ef1, corpus);
            let latency2 = estimated_latency_ms(ef2, corpus);
            assert!(latency2 > latency1, "Higher ef should give higher latency");
        }
    }
}

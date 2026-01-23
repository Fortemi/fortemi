//! Latency tracking and context optimization for knowledge management.
//!
//! This module provides:
//! - Latency tracking and statistics for operations
//! - Context window optimization per operation type
//! - Adaptive context sizing based on hardware and load
//!
//! # Latency Targets
//!
//! | Operation | Target P95 | Max Acceptable |
//! |-----------|-----------|----------------|
//! | Title Generation | 500ms | 1000ms |
//! | AI Revision | 3000ms | 5000ms |
//! | Embedding | 200ms | 500ms |
//! | Semantic Linking | 1000ms | 2000ms |

use crate::hardware::HardwareTier;
use crate::selector::KmOperation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

/// Latency statistics for an operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Median latency (P50) in milliseconds.
    pub p50_ms: u64,
    /// 95th percentile latency in milliseconds.
    pub p95_ms: u64,
    /// 99th percentile latency in milliseconds.
    pub p99_ms: u64,
    /// Maximum observed latency in milliseconds.
    pub max_ms: u64,
    /// Number of samples.
    pub samples: usize,
}

impl LatencyStats {
    /// Create stats from a sorted list of latencies.
    pub fn from_samples(mut latencies: Vec<u64>) -> Self {
        if latencies.is_empty() {
            return Self::default();
        }

        latencies.sort_unstable();
        let n = latencies.len();

        let p50_idx = n / 2;
        let p95_idx = (n as f64 * 0.95) as usize;
        let p99_idx = (n as f64 * 0.99) as usize;

        Self {
            p50_ms: latencies[p50_idx.min(n - 1)],
            p95_ms: latencies[p95_idx.min(n - 1)],
            p99_ms: latencies[p99_idx.min(n - 1)],
            max_ms: *latencies.last().unwrap_or(&0),
            samples: n,
        }
    }

    /// Check if latency is degraded for an operation.
    pub fn is_degraded(&self, operation: KmOperation) -> bool {
        let target = operation.max_latency_ms();
        self.p95_ms > target
    }
}

/// Latency tracker for all operations.
pub struct LatencyTracker {
    /// Latency samples per operation.
    samples: RwLock<HashMap<KmOperation, Vec<u64>>>,
    /// Maximum samples to keep per operation.
    max_samples: usize,
    /// Total requests tracked.
    total_requests: AtomicU64,
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl LatencyTracker {
    /// Create a new latency tracker.
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: RwLock::new(HashMap::new()),
            max_samples,
            total_requests: AtomicU64::new(0),
        }
    }

    /// Record a latency sample.
    pub fn record(&self, operation: KmOperation, duration: Duration) {
        let ms = duration.as_millis() as u64;
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut samples) = self.samples.write() {
            let entry = samples.entry(operation).or_insert_with(Vec::new);
            entry.push(ms);

            // Keep only the most recent samples
            if entry.len() > self.max_samples {
                entry.remove(0);
            }
        }
    }

    /// Get statistics for an operation.
    pub fn stats(&self, operation: KmOperation) -> LatencyStats {
        if let Ok(samples) = self.samples.read() {
            if let Some(latencies) = samples.get(&operation) {
                return LatencyStats::from_samples(latencies.clone());
            }
        }
        LatencyStats::default()
    }

    /// Get statistics for all operations.
    pub fn all_stats(&self) -> HashMap<KmOperation, LatencyStats> {
        let mut result = HashMap::new();

        if let Ok(samples) = self.samples.read() {
            for (op, latencies) in samples.iter() {
                result.insert(*op, LatencyStats::from_samples(latencies.clone()));
            }
        }

        result
    }

    /// Get total requests tracked.
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// Check if any operation is degraded.
    pub fn any_degraded(&self) -> bool {
        if let Ok(samples) = self.samples.read() {
            for (op, latencies) in samples.iter() {
                let stats = LatencyStats::from_samples(latencies.clone());
                if stats.is_degraded(*op) {
                    return true;
                }
            }
        }
        false
    }

    /// Get optimization suggestions based on current latency.
    pub fn suggest_optimizations(&self) -> Vec<LatencyOptimization> {
        let mut suggestions = Vec::new();

        for (op, stats) in self.all_stats() {
            if stats.is_degraded(op) {
                suggestions.push(LatencyOptimization {
                    operation: op,
                    current_p95_ms: stats.p95_ms,
                    target_p95_ms: op.max_latency_ms(),
                    suggestion: self.optimization_for(op, stats.p95_ms),
                });
            }
        }

        suggestions
    }

    fn optimization_for(&self, op: KmOperation, current_p95: u64) -> String {
        match op {
            KmOperation::TitleGeneration => {
                if current_p95 > 1000 {
                    "Consider using a faster model (llama3.1:8b) for title generation".to_string()
                } else {
                    "Reduce context window size for title prompts".to_string()
                }
            }
            KmOperation::AiRevision => {
                if current_p95 > 5000 {
                    "Consider using a smaller model or reducing related context included"
                        .to_string()
                } else {
                    "Limit related notes context to top 3 most relevant".to_string()
                }
            }
            KmOperation::Embedding => {
                if current_p95 > 500 {
                    "Enable batch embedding to reduce per-request overhead".to_string()
                } else {
                    "Increase embedding batch size".to_string()
                }
            }
            KmOperation::SemanticLinking => {
                "Reduce similarity threshold to compute fewer comparisons".to_string()
            }
            KmOperation::ContextGeneration => {
                "Cache generated context for frequently accessed notes".to_string()
            }
        }
    }

    /// Clear all samples.
    pub fn reset(&self) {
        if let Ok(mut samples) = self.samples.write() {
            samples.clear();
        }
        self.total_requests.store(0, Ordering::Relaxed);
    }
}

/// A latency optimization suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyOptimization {
    /// Operation needing optimization.
    pub operation: KmOperation,
    /// Current P95 latency.
    pub current_p95_ms: u64,
    /// Target P95 latency.
    pub target_p95_ms: u64,
    /// Suggested optimization.
    pub suggestion: String,
}

/// Context window configuration for an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Optimal context size for this operation.
    pub optimal_context: usize,
    /// Maximum context before chunking is required.
    pub max_context: usize,
    /// Chunking strategy if max is exceeded.
    pub chunking: ChunkingStrategy,
}

/// Strategy for handling content that exceeds context window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkingStrategy {
    /// Truncate with a summary of omitted content.
    TruncateWithSummary,
    /// Split by document sections.
    SplitBySection,
    /// Split into semantic chunks.
    SemanticChunks,
    /// Sliding window approach.
    SlidingWindow,
}

/// Context optimizer for knowledge management operations.
#[derive(Debug, Clone)]
pub struct ContextOptimizer {
    /// Context configurations by operation.
    configs: HashMap<KmOperation, ContextConfig>,
    /// Current scale factor (for adaptive sizing).
    scale_factor: f32,
}

impl Default for ContextOptimizer {
    fn default() -> Self {
        let mut configs = HashMap::new();

        configs.insert(
            KmOperation::TitleGeneration,
            ContextConfig {
                optimal_context: 2048,
                max_context: 4096,
                chunking: ChunkingStrategy::TruncateWithSummary,
            },
        );

        configs.insert(
            KmOperation::AiRevision,
            ContextConfig {
                optimal_context: 8192,
                max_context: 16384,
                chunking: ChunkingStrategy::SplitBySection,
            },
        );

        configs.insert(
            KmOperation::Embedding,
            ContextConfig {
                optimal_context: 512,
                max_context: 2048,
                chunking: ChunkingStrategy::SemanticChunks,
            },
        );

        configs.insert(
            KmOperation::SemanticLinking,
            ContextConfig {
                optimal_context: 1024,
                max_context: 4096,
                chunking: ChunkingStrategy::TruncateWithSummary,
            },
        );

        configs.insert(
            KmOperation::ContextGeneration,
            ContextConfig {
                optimal_context: 4096,
                max_context: 8192,
                chunking: ChunkingStrategy::SplitBySection,
            },
        );

        Self {
            configs,
            scale_factor: 1.0,
        }
    }
}

impl ContextOptimizer {
    /// Create optimizer with default configurations.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get context configuration for an operation.
    pub fn config_for(&self, operation: KmOperation) -> ContextConfig {
        self.configs
            .get(&operation)
            .cloned()
            .map(|mut c| {
                c.optimal_context = (c.optimal_context as f32 * self.scale_factor) as usize;
                c.max_context = (c.max_context as f32 * self.scale_factor) as usize;
                c
            })
            .unwrap_or(ContextConfig {
                optimal_context: 4096,
                max_context: 8192,
                chunking: ChunkingStrategy::TruncateWithSummary,
            })
    }

    /// Adjust context sizes for hardware tier.
    pub fn adjust_for_tier(&mut self, tier: HardwareTier) {
        self.scale_factor = match tier {
            HardwareTier::Budget => 0.5,
            HardwareTier::Mainstream => 1.0,
            HardwareTier::Performance => 1.5,
            HardwareTier::Professional => 2.0,
        };
    }

    /// Adjust context sizes for current load.
    pub fn adjust_for_load(&mut self, queue_depth: usize) {
        if queue_depth > 10 {
            self.scale_factor *= 0.75;
        } else if queue_depth < 3 {
            self.scale_factor = self.scale_factor.min(1.5);
        }
    }

    /// Get optimal token limit for an operation.
    pub fn optimal_tokens(&self, operation: KmOperation) -> usize {
        self.config_for(operation).optimal_context
    }

    /// Get maximum token limit for an operation.
    pub fn max_tokens(&self, operation: KmOperation) -> usize {
        self.config_for(operation).max_context
    }

    /// Get recommended max_tokens parameter for generation.
    pub fn recommended_max_output(&self, operation: KmOperation) -> usize {
        match operation {
            KmOperation::TitleGeneration => 50,
            KmOperation::AiRevision => 2000,
            KmOperation::Embedding => 0, // No output for embedding
            KmOperation::SemanticLinking => 200,
            KmOperation::ContextGeneration => 500,
        }
    }
}

/// Batch embedding service for efficient embedding generation.
pub struct BatchEmbeddingConfig {
    /// Maximum batch size.
    pub max_batch_size: usize,
    /// Flush timeout in milliseconds.
    pub flush_timeout_ms: u64,
}

impl Default for BatchEmbeddingConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 32,
            flush_timeout_ms: 100,
        }
    }
}

impl BatchEmbeddingConfig {
    /// Create config optimized for hardware tier.
    pub fn for_tier(tier: HardwareTier) -> Self {
        match tier {
            HardwareTier::Budget => Self {
                max_batch_size: 8,
                flush_timeout_ms: 200,
            },
            HardwareTier::Mainstream => Self {
                max_batch_size: 16,
                flush_timeout_ms: 100,
            },
            HardwareTier::Performance => Self {
                max_batch_size: 32,
                flush_timeout_ms: 50,
            },
            HardwareTier::Professional => Self {
                max_batch_size: 64,
                flush_timeout_ms: 25,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_stats_from_samples() {
        let samples = vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
        let stats = LatencyStats::from_samples(samples);

        assert_eq!(stats.samples, 10);
        assert_eq!(stats.p50_ms, 600); // median of 10 items at index 5
        assert_eq!(stats.max_ms, 1000);
    }

    #[test]
    fn test_latency_stats_empty() {
        let stats = LatencyStats::from_samples(vec![]);
        assert_eq!(stats.samples, 0);
        assert_eq!(stats.p50_ms, 0);
    }

    #[test]
    fn test_latency_stats_is_degraded() {
        let stats = LatencyStats {
            p50_ms: 400,
            p95_ms: 600,
            p99_ms: 800,
            max_ms: 1000,
            samples: 100,
        };

        // Title generation target is 500ms, so 600ms p95 is degraded
        assert!(stats.is_degraded(KmOperation::TitleGeneration));

        // AI revision target is 3000ms, so 600ms is fine
        assert!(!stats.is_degraded(KmOperation::AiRevision));
    }

    #[test]
    fn test_latency_tracker_record() {
        let tracker = LatencyTracker::new(100);

        tracker.record(KmOperation::TitleGeneration, Duration::from_millis(100));
        tracker.record(KmOperation::TitleGeneration, Duration::from_millis(200));
        tracker.record(KmOperation::TitleGeneration, Duration::from_millis(300));

        let stats = tracker.stats(KmOperation::TitleGeneration);
        assert_eq!(stats.samples, 3);
        assert_eq!(tracker.total_requests(), 3);
    }

    #[test]
    fn test_latency_tracker_max_samples() {
        let tracker = LatencyTracker::new(5);

        for i in 0..10 {
            tracker.record(KmOperation::Embedding, Duration::from_millis(i * 100));
        }

        let stats = tracker.stats(KmOperation::Embedding);
        assert_eq!(stats.samples, 5);
    }

    #[test]
    fn test_context_optimizer_default() {
        let optimizer = ContextOptimizer::default();

        let title_config = optimizer.config_for(KmOperation::TitleGeneration);
        assert_eq!(title_config.optimal_context, 2048);
        assert_eq!(title_config.max_context, 4096);

        let embed_config = optimizer.config_for(KmOperation::Embedding);
        assert_eq!(embed_config.optimal_context, 512);
    }

    #[test]
    fn test_context_optimizer_tier_adjustment() {
        let mut optimizer = ContextOptimizer::new();

        // Budget tier scales down
        optimizer.adjust_for_tier(HardwareTier::Budget);
        let config = optimizer.config_for(KmOperation::TitleGeneration);
        assert_eq!(config.optimal_context, 1024); // 2048 * 0.5

        // Performance tier scales up
        optimizer.adjust_for_tier(HardwareTier::Performance);
        let config = optimizer.config_for(KmOperation::TitleGeneration);
        assert_eq!(config.optimal_context, 3072); // 2048 * 1.5
    }

    #[test]
    fn test_recommended_max_output() {
        let optimizer = ContextOptimizer::new();

        assert_eq!(
            optimizer.recommended_max_output(KmOperation::TitleGeneration),
            50
        );
        assert_eq!(
            optimizer.recommended_max_output(KmOperation::AiRevision),
            2000
        );
    }

    #[test]
    fn test_batch_embedding_config_for_tier() {
        let budget = BatchEmbeddingConfig::for_tier(HardwareTier::Budget);
        assert_eq!(budget.max_batch_size, 8);

        let pro = BatchEmbeddingConfig::for_tier(HardwareTier::Professional);
        assert_eq!(pro.max_batch_size, 64);
    }

    #[test]
    fn test_latency_tracker_suggest_optimizations() {
        let tracker = LatencyTracker::new(100);

        // Add slow title generation samples
        for _ in 0..10 {
            tracker.record(KmOperation::TitleGeneration, Duration::from_millis(800));
        }

        let suggestions = tracker.suggest_optimizations();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].operation, KmOperation::TitleGeneration);
    }
}

//! Hardware configuration and context budget management.
//!
//! This module provides VRAM-aware context limit configuration for LLM operations,
//! ensuring safe memory utilization based on available GPU resources.

use serde::{Deserialize, Serialize};

/// Hardware configuration for LLM operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    pub vram_gb: u32,
    pub context_budget: ContextBudget,
}

/// Context budget configuration for managing token allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Fraction of max context to use (default: 0.85 for safety margin)
    pub utilization_factor: f32,
    /// Tokens reserved for system/template overhead (default: 512)
    pub reserved_tokens: usize,
    /// Minimum chunk size for processing (default: 256)
    pub min_chunk_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            utilization_factor: 0.85,
            reserved_tokens: 512,
            min_chunk_tokens: 256,
        }
    }
}

impl HardwareConfig {
    /// Create a new hardware configuration for the given VRAM capacity.
    pub fn new(vram_gb: u32) -> Self {
        Self {
            vram_gb,
            context_budget: ContextBudget::default(),
        }
    }

    /// Create a hardware configuration with custom context budget.
    pub fn with_budget(vram_gb: u32, context_budget: ContextBudget) -> Self {
        Self {
            vram_gb,
            context_budget,
        }
    }

    /// Get the safe context limit accounting for utilization factor and reserved tokens.
    ///
    /// This applies the utilization factor to the base context limit and subtracts
    /// reserved tokens to ensure safe operation with headroom for system overhead.
    pub fn get_safe_context_limit(&self) -> usize {
        let base_limit = self.vram_to_context_mapping(self.vram_gb);
        let after_utilization =
            (base_limit as f32 * self.context_budget.utilization_factor) as usize;
        after_utilization.saturating_sub(self.context_budget.reserved_tokens)
    }

    /// Map VRAM capacity to maximum context window size.
    ///
    /// Based on empirical VRAM requirements for different context sizes:
    /// - 6GB  → 8,192 tokens
    /// - 8GB  → 16,384 tokens
    /// - 11GB → 32,768 tokens
    /// - 12GB → 32,768 tokens
    /// - 16GB → 65,536 tokens
    /// - 24GB → 131,072 tokens
    fn vram_to_context_mapping(&self, vram_gb: u32) -> usize {
        match vram_gb {
            0..=6 => 8_192,
            7..=8 => 16_384,
            9..=12 => 32_768,
            13..=16 => 65_536,
            _ => 131_072, // 17GB+
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // ContextBudget Tests
    // =============================================================================

    #[test]
    fn test_context_budget_default() {
        let budget = ContextBudget::default();
        assert_eq!(budget.utilization_factor, 0.85);
        assert_eq!(budget.reserved_tokens, 512);
        assert_eq!(budget.min_chunk_tokens, 256);
    }

    #[test]
    fn test_context_budget_serialization() {
        let budget = ContextBudget {
            utilization_factor: 0.9,
            reserved_tokens: 1024,
            min_chunk_tokens: 512,
        };

        let json = serde_json::to_string(&budget).unwrap();
        let parsed: ContextBudget = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.utilization_factor, 0.9);
        assert_eq!(parsed.reserved_tokens, 1024);
        assert_eq!(parsed.min_chunk_tokens, 512);
    }

    // =============================================================================
    // HardwareConfig Construction Tests
    // =============================================================================

    #[test]
    fn test_hardware_config_new() {
        let config = HardwareConfig::new(8);
        assert_eq!(config.vram_gb, 8);
        assert_eq!(config.context_budget.utilization_factor, 0.85);
        assert_eq!(config.context_budget.reserved_tokens, 512);
    }

    #[test]
    fn test_hardware_config_with_custom_budget() {
        let custom_budget = ContextBudget {
            utilization_factor: 0.9,
            reserved_tokens: 1024,
            min_chunk_tokens: 512,
        };

        let config = HardwareConfig::with_budget(16, custom_budget.clone());
        assert_eq!(config.vram_gb, 16);
        assert_eq!(config.context_budget.utilization_factor, 0.9);
        assert_eq!(config.context_budget.reserved_tokens, 1024);
    }

    #[test]
    fn test_hardware_config_serialization() {
        let config = HardwareConfig::new(12);
        let json = serde_json::to_string(&config).unwrap();
        let parsed: HardwareConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.vram_gb, 12);
        assert_eq!(parsed.context_budget.utilization_factor, 0.85);
    }

    // =============================================================================
    // VRAM to Context Mapping Tests (All Required Tiers)
    // =============================================================================

    #[test]
    fn test_vram_mapping_6gb() {
        let config = HardwareConfig::new(6);
        assert_eq!(config.vram_to_context_mapping(6), 8_192);
    }

    #[test]
    fn test_vram_mapping_8gb() {
        let config = HardwareConfig::new(8);
        assert_eq!(config.vram_to_context_mapping(8), 16_384);
    }

    #[test]
    fn test_vram_mapping_11gb() {
        let config = HardwareConfig::new(11);
        assert_eq!(config.vram_to_context_mapping(11), 32_768);
    }

    #[test]
    fn test_vram_mapping_12gb() {
        let config = HardwareConfig::new(12);
        assert_eq!(config.vram_to_context_mapping(12), 32_768);
    }

    #[test]
    fn test_vram_mapping_16gb() {
        let config = HardwareConfig::new(16);
        assert_eq!(config.vram_to_context_mapping(16), 65_536);
    }

    #[test]
    fn test_vram_mapping_24gb() {
        let config = HardwareConfig::new(24);
        assert_eq!(config.vram_to_context_mapping(24), 131_072);
    }

    // =============================================================================
    // Edge Cases and Boundary Tests
    // =============================================================================

    #[test]
    fn test_vram_mapping_below_6gb() {
        let config = HardwareConfig::new(4);
        assert_eq!(config.vram_to_context_mapping(4), 8_192);
    }

    #[test]
    fn test_vram_mapping_7gb_boundary() {
        let config = HardwareConfig::new(7);
        assert_eq!(config.vram_to_context_mapping(7), 16_384);
    }

    #[test]
    fn test_vram_mapping_13gb_boundary() {
        let config = HardwareConfig::new(13);
        assert_eq!(config.vram_to_context_mapping(13), 65_536);
    }

    #[test]
    fn test_vram_mapping_above_24gb() {
        let config = HardwareConfig::new(32);
        assert_eq!(config.vram_to_context_mapping(32), 131_072);
    }

    #[test]
    fn test_vram_mapping_zero() {
        let config = HardwareConfig::new(0);
        assert_eq!(config.vram_to_context_mapping(0), 8_192);
    }

    // =============================================================================
    // Safe Context Limit Tests
    // =============================================================================

    #[test]
    fn test_safe_context_limit_6gb_default() {
        let config = HardwareConfig::new(6);
        // 8192 * 0.85 = 6963.2 → 6963, then 6963 - 512 = 6451
        assert_eq!(config.get_safe_context_limit(), 6451);
    }

    #[test]
    fn test_safe_context_limit_8gb_default() {
        let config = HardwareConfig::new(8);
        // 16384 * 0.85 = 13926.4 → 13926, then 13926 - 512 = 13414
        assert_eq!(config.get_safe_context_limit(), 13414);
    }

    #[test]
    fn test_safe_context_limit_11gb_default() {
        let config = HardwareConfig::new(11);
        // 32768 * 0.85 = 27852.8 → 27852, then 27852 - 512 = 27340
        assert_eq!(config.get_safe_context_limit(), 27340);
    }

    #[test]
    fn test_safe_context_limit_12gb_default() {
        let config = HardwareConfig::new(12);
        // 32768 * 0.85 = 27852.8 → 27852, then 27852 - 512 = 27340
        assert_eq!(config.get_safe_context_limit(), 27340);
    }

    #[test]
    fn test_safe_context_limit_16gb_default() {
        let config = HardwareConfig::new(16);
        // 65536 * 0.85 = 55705.6 → 55705, then 55705 - 512 = 55193
        assert_eq!(config.get_safe_context_limit(), 55193);
    }

    #[test]
    fn test_safe_context_limit_24gb_default() {
        let config = HardwareConfig::new(24);
        // 131072 * 0.85 = 111411.2 → 111411, then 111411 - 512 = 110899
        assert_eq!(config.get_safe_context_limit(), 110899);
    }

    #[test]
    fn test_safe_context_limit_custom_utilization() {
        let custom_budget = ContextBudget {
            utilization_factor: 0.9,
            reserved_tokens: 1024,
            min_chunk_tokens: 256,
        };
        let config = HardwareConfig::with_budget(16, custom_budget);
        // 65536 * 0.9 = 58982.4 → 58982, then 58982 - 1024 = 57958
        assert_eq!(config.get_safe_context_limit(), 57958);
    }

    #[test]
    fn test_safe_context_limit_high_utilization() {
        let custom_budget = ContextBudget {
            utilization_factor: 0.95,
            reserved_tokens: 0,
            min_chunk_tokens: 256,
        };
        let config = HardwareConfig::with_budget(8, custom_budget);
        // 16384 * 0.95 = 15564.8 → 15564, then 15564 - 0 = 15564
        assert_eq!(config.get_safe_context_limit(), 15564);
    }

    #[test]
    fn test_safe_context_limit_saturating_sub() {
        // Test that reserved_tokens doesn't cause underflow
        let custom_budget = ContextBudget {
            utilization_factor: 0.1, // Very low utilization
            reserved_tokens: 10_000, // High reservation
            min_chunk_tokens: 256,
        };
        let config = HardwareConfig::with_budget(6, custom_budget);
        // 8192 * 0.1 = 819.2 → 819, then saturating_sub(10000) = 0
        assert_eq!(config.get_safe_context_limit(), 0);
    }

    // =============================================================================
    // Integration Tests
    // =============================================================================

    #[test]
    fn test_all_vram_tiers_produce_valid_limits() {
        let vram_tiers = vec![6, 8, 11, 12, 16, 24];

        for vram in vram_tiers {
            let config = HardwareConfig::new(vram);
            let limit = config.get_safe_context_limit();

            // Safe limit should always be positive and less than base mapping
            assert!(limit > 0, "VRAM {}GB produced zero limit", vram);
            assert!(
                limit < config.vram_to_context_mapping(vram),
                "VRAM {}GB safe limit >= base limit",
                vram
            );
        }
    }

    #[test]
    fn test_context_limits_increase_with_vram() {
        let configs: Vec<_> = vec![6, 8, 11, 16, 24]
            .into_iter()
            .map(HardwareConfig::new)
            .collect();

        for i in 1..configs.len() {
            let prev_limit = configs[i - 1].get_safe_context_limit();
            let curr_limit = configs[i].get_safe_context_limit();

            assert!(
                curr_limit > prev_limit,
                "Context limit should increase with VRAM: {}GB={} vs {}GB={}",
                configs[i - 1].vram_gb,
                prev_limit,
                configs[i].vram_gb,
                curr_limit
            );
        }
    }

    #[test]
    fn test_min_chunk_tokens_accessible() {
        let config = HardwareConfig::new(8);
        assert_eq!(config.context_budget.min_chunk_tokens, 256);
    }
}

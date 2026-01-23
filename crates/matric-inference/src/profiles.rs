//! Model performance profiles and registry.
//!
//! This module provides model performance profiles based on empirical testing
//! documented in `/docs/research/consolidated_model_data.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Thinking model type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingType {
    /// Model uses explicit `<think>...</think>` tags for reasoning.
    ExplicitTags,
    /// Model uses verbose step-by-step reasoning without explicit tags.
    VerboseReasoning,
    /// Model uses pattern-based reasoning (e.g., "Step N:", "Let me think").
    PatternBased,
    /// Model does not exhibit thinking behavior.
    None,
    /// Model was not tested for thinking capabilities.
    NotTested,
}

/// Performance profile for an Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Model name as used in Ollama.
    pub name: String,
    /// Native context window size in tokens.
    pub native_context: usize,
    /// Maximum output tokens the model can generate.
    pub max_output: usize,
    /// Output speed in tokens per second.
    pub speed_tok_s: f32,
    /// Thinking/reasoning model type.
    pub thinking_type: ThinkingType,
    /// Whether to use raw mode (required for some thinking models).
    pub use_raw_mode: bool,
    /// Model family (llama, qwen2, etc.).
    pub family: String,
    /// Model size (e.g., "8.0B", "14.8B").
    pub size: String,
}

impl ModelProfile {
    /// Returns true if this is a thinking/reasoning model.
    pub fn is_thinking_model(&self) -> bool {
        !matches!(
            self.thinking_type,
            ThinkingType::None | ThinkingType::NotTested
        )
    }

    /// Returns true if this model is fast (>150 tok/s).
    pub fn is_fast(&self) -> bool {
        self.speed_tok_s > 150.0
    }

    /// Returns true if this model has large context (>90K tokens).
    pub fn has_large_context(&self) -> bool {
        self.native_context > 90_000
    }

    /// Returns true if this model can generate large outputs (>4K tokens).
    pub fn has_large_output(&self) -> bool {
        self.max_output > 4_000
    }

    /// Get recommended input context size (90% of native context).
    pub fn recommended_input(&self) -> usize {
        (self.native_context as f32 * 0.9) as usize
    }
}

/// Task requirements for model selection.
#[derive(Debug, Clone, Default)]
pub struct TaskRequirements {
    /// Minimum context window size needed.
    pub min_context: Option<usize>,
    /// Minimum output tokens needed.
    pub min_output: Option<usize>,
    /// Minimum speed in tok/s.
    pub min_speed: Option<f32>,
    /// Whether thinking/reasoning is required.
    pub requires_thinking: bool,
    /// Preferred model family.
    pub preferred_family: Option<String>,
}

/// Registry of model performance profiles.
pub struct ModelRegistry {
    profiles: HashMap<String, ModelProfile>,
}

impl ModelRegistry {
    /// Create a new registry with all profiled models.
    pub fn new() -> Self {
        let mut profiles = HashMap::new();

        // Tier 1: Large Context Models (98K+)
        profiles.insert(
            "gpt-oss:20b".to_string(),
            ModelProfile {
                name: "gpt-oss:20b".to_string(),
                native_context: 98_376,
                max_output: 7_611,
                speed_tok_s: 179.0,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "gptoss".to_string(),
                size: "20.9B".to_string(),
            },
        );

        profiles.insert(
            "llama3.1:8b".to_string(),
            ModelProfile {
                name: "llama3.1:8b".to_string(),
                native_context: 98_319,
                max_output: 1_024,
                speed_tok_s: 29.0,
                thinking_type: ThinkingType::None,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "8.0B".to_string(),
            },
        );

        profiles.insert(
            "deepseek-r1:14b".to_string(),
            ModelProfile {
                name: "deepseek-r1:14b".to_string(),
                native_context: 98_312,
                max_output: 2_824,
                speed_tok_s: 9.2,
                thinking_type: ThinkingType::ExplicitTags,
                use_raw_mode: true, // CRITICAL: Required for thinking tags
                family: "qwen2".to_string(),
                size: "14.8B".to_string(),
            },
        );

        profiles.insert(
            "llama3.2:latest".to_string(),
            ModelProfile {
                name: "llama3.2:latest".to_string(),
                native_context: 98_334,
                max_output: 2_900,
                speed_tok_s: 274.9,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "3.2B".to_string(),
            },
        );

        profiles.insert(
            "granite4:3b".to_string(),
            ModelProfile {
                name: "granite4:3b".to_string(),
                native_context: 98_339,
                max_output: 1_277,
                speed_tok_s: 244.3,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "granite".to_string(),
                size: "3.4B".to_string(),
            },
        );

        profiles.insert(
            "hf.co/DevQuasar/FreedomIntelligence.HuatuoGPT-o1-8B-GGUF:Q4_K_M".to_string(),
            ModelProfile {
                name: "hf.co/DevQuasar/FreedomIntelligence.HuatuoGPT-o1-8B-GGUF:Q4_K_M".to_string(),
                native_context: 98_319,
                max_output: 8_192,
                speed_tok_s: 27.7,
                thinking_type: ThinkingType::VerboseReasoning,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "8.03B".to_string(),
            },
        );

        profiles.insert(
            "phi3:mini".to_string(),
            ModelProfile {
                name: "phi3:mini".to_string(),
                native_context: 98_318,
                max_output: 1_024,
                speed_tok_s: 21.5,
                thinking_type: ThinkingType::VerboseReasoning,
                use_raw_mode: false,
                family: "phi3".to_string(),
                size: "3.8B".to_string(),
            },
        );

        // Tier 2: Medium Context Models (32K-41K)
        profiles.insert(
            "qwen2.5-coder:1.5b".to_string(),
            ModelProfile {
                name: "qwen2.5-coder:1.5b".to_string(),
                native_context: 32_768,
                max_output: 1_024,
                speed_tok_s: 373.1,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "1.5B".to_string(),
            },
        );

        profiles.insert(
            "qwen2.5-coder:7b".to_string(),
            ModelProfile {
                name: "qwen2.5-coder:7b".to_string(),
                native_context: 32_768,
                max_output: 4_096,
                speed_tok_s: 160.7,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "7.6B".to_string(),
            },
        );

        profiles.insert(
            "qwen2.5-coder:14b".to_string(),
            ModelProfile {
                name: "qwen2.5-coder:14b".to_string(),
                native_context: 32_768,
                max_output: 1_003,
                speed_tok_s: 88.5,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "14.8B".to_string(),
            },
        );

        profiles.insert(
            "qwen2.5:7b".to_string(),
            ModelProfile {
                name: "qwen2.5:7b".to_string(),
                native_context: 32_768,
                max_output: 1_673,
                speed_tok_s: 161.7,
                thinking_type: ThinkingType::None,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "7.6B".to_string(),
            },
        );

        profiles.insert(
            "qwen2.5:14b".to_string(),
            ModelProfile {
                name: "qwen2.5:14b".to_string(),
                native_context: 32_768,
                max_output: 2_048,
                speed_tok_s: 88.4,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "14.8B".to_string(),
            },
        );

        profiles.insert(
            "qwen2.5:32b".to_string(),
            ModelProfile {
                name: "qwen2.5:32b".to_string(),
                native_context: 32_768,
                max_output: 2_048,
                speed_tok_s: 6.5,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen2".to_string(),
                size: "32.8B".to_string(),
            },
        );

        profiles.insert(
            "qwen3:8b".to_string(),
            ModelProfile {
                name: "qwen3:8b".to_string(),
                native_context: 40_960,
                max_output: 4_096,
                speed_tok_s: 144.3,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "qwen3".to_string(),
                size: "8.2B".to_string(),
            },
        );

        profiles.insert(
            "mistral:latest".to_string(),
            ModelProfile {
                name: "mistral:latest".to_string(),
                native_context: 32_768,
                max_output: 1_024,
                speed_tok_s: 174.3,
                thinking_type: ThinkingType::None,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "7.2B".to_string(),
            },
        );

        profiles.insert(
            "codestral:latest".to_string(),
            ModelProfile {
                name: "codestral:latest".to_string(),
                native_context: 32_768,
                max_output: 2_048,
                speed_tok_s: 16.0,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "22.2B".to_string(),
            },
        );

        profiles.insert(
            "exaone-deep:7.8b".to_string(),
            ModelProfile {
                name: "exaone-deep:7.8b".to_string(),
                native_context: 32_768,
                max_output: 3_464,
                speed_tok_s: 165.3,
                thinking_type: ThinkingType::PatternBased,
                use_raw_mode: false,
                family: "exaone".to_string(),
                size: "7.8B".to_string(),
            },
        );

        // Tier 3: Standard Context Models (8K-16K)
        profiles.insert(
            "deepseek-coder-v2:16b".to_string(),
            ModelProfile {
                name: "deepseek-coder-v2:16b".to_string(),
                native_context: 16_397,
                max_output: 1_350,
                speed_tok_s: 241.8,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "deepseek2".to_string(),
                size: "15.7B".to_string(),
            },
        );

        profiles.insert(
            "gemma2:9b".to_string(),
            ModelProfile {
                name: "gemma2:9b".to_string(),
                native_context: 8_192,
                max_output: 434,
                speed_tok_s: 115.9,
                thinking_type: ThinkingType::None,
                use_raw_mode: false,
                family: "gemma2".to_string(),
                size: "9.2B".to_string(),
            },
        );

        profiles.insert(
            "command-r7b:latest".to_string(),
            ModelProfile {
                name: "command-r7b:latest".to_string(),
                native_context: 8_192,
                max_output: 2_048,
                speed_tok_s: 133.8,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "cohere2".to_string(),
                size: "8.0B".to_string(),
            },
        );

        profiles.insert(
            "smollm2:1.7b".to_string(),
            ModelProfile {
                name: "smollm2:1.7b".to_string(),
                native_context: 8_192,
                max_output: 1_219,
                speed_tok_s: 336.5,
                thinking_type: ThinkingType::NotTested,
                use_raw_mode: false,
                family: "llama".to_string(),
                size: "1.7B".to_string(),
            },
        );

        Self { profiles }
    }

    /// Get a model profile by name.
    pub fn get(&self, model_name: &str) -> Option<&ModelProfile> {
        self.profiles.get(model_name)
    }

    /// Get all available model names.
    pub fn model_names(&self) -> Vec<&str> {
        self.profiles.keys().map(|s| s.as_str()).collect()
    }

    /// Find models matching task requirements.
    pub fn find_matching(&self, requirements: &TaskRequirements) -> Vec<&ModelProfile> {
        self.profiles
            .values()
            .filter(|profile| {
                // Check context requirement
                if let Some(min_ctx) = requirements.min_context {
                    if profile.native_context < min_ctx {
                        return false;
                    }
                }

                // Check output requirement
                if let Some(min_out) = requirements.min_output {
                    if profile.max_output < min_out {
                        return false;
                    }
                }

                // Check speed requirement
                if let Some(min_spd) = requirements.min_speed {
                    if profile.speed_tok_s < min_spd {
                        return false;
                    }
                }

                // Check thinking requirement
                if requirements.requires_thinking && !profile.is_thinking_model() {
                    return false;
                }

                // Check family preference
                if let Some(ref fam) = requirements.preferred_family {
                    if &profile.family != fam {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Get the best model for general inference.
    pub fn get_best_general(&self) -> Option<&ModelProfile> {
        self.get("gpt-oss:20b")
    }

    /// Get the best model for fast queries.
    pub fn get_best_fast(&self) -> Option<&ModelProfile> {
        self.get("qwen2.5-coder:1.5b")
    }

    /// Get the best model for code generation.
    pub fn get_best_code(&self) -> Option<&ModelProfile> {
        self.get("qwen2.5-coder:7b")
    }

    /// Get the best model for reasoning/thinking tasks.
    pub fn get_best_reasoning(&self) -> Option<&ModelProfile> {
        self.get("deepseek-r1:14b")
    }

    /// Get the best model for long documents.
    pub fn get_best_long_context(&self) -> Option<&ModelProfile> {
        self.get("llama3.1:8b")
    }

    /// Get all thinking models.
    pub fn get_thinking_models(&self) -> Vec<&ModelProfile> {
        self.profiles
            .values()
            .filter(|p| p.is_thinking_model())
            .collect()
    }

    /// Get the total number of profiled models.
    pub fn count(&self) -> usize {
        self.profiles.len()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // ModelProfile Tests
    // ==========================================================================

    #[test]
    fn test_model_profile_creation() {
        let profile = ModelProfile {
            name: "test-model".to_string(),
            native_context: 32_768,
            max_output: 2_048,
            speed_tok_s: 150.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert_eq!(profile.name, "test-model");
        assert_eq!(profile.native_context, 32_768);
    }

    #[test]
    fn test_is_thinking_model() {
        let thinking = ModelProfile {
            name: "thinking".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::ExplicitTags,
            use_raw_mode: true,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        let non_thinking = ModelProfile {
            name: "non-thinking".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert!(thinking.is_thinking_model());
        assert!(!non_thinking.is_thinking_model());
    }

    #[test]
    fn test_is_fast() {
        let fast = ModelProfile {
            name: "fast".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 200.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        let slow = ModelProfile {
            name: "slow".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert!(fast.is_fast());
        assert!(!slow.is_fast());
    }

    #[test]
    fn test_has_large_context() {
        let large = ModelProfile {
            name: "large".to_string(),
            native_context: 98_000,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        let small = ModelProfile {
            name: "small".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert!(large.has_large_context());
        assert!(!small.has_large_context());
    }

    #[test]
    fn test_has_large_output() {
        let large = ModelProfile {
            name: "large".to_string(),
            native_context: 8_192,
            max_output: 8_192,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        let small = ModelProfile {
            name: "small".to_string(),
            native_context: 8_192,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert!(large.has_large_output());
        assert!(!small.has_large_output());
    }

    #[test]
    fn test_recommended_input() {
        let profile = ModelProfile {
            name: "test".to_string(),
            native_context: 10_000,
            max_output: 1_024,
            speed_tok_s: 50.0,
            thinking_type: ThinkingType::None,
            use_raw_mode: false,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        assert_eq!(profile.recommended_input(), 9_000);
    }

    // ==========================================================================
    // ThinkingType Tests
    // ==========================================================================

    #[test]
    fn test_thinking_type_equality() {
        assert_eq!(ThinkingType::ExplicitTags, ThinkingType::ExplicitTags);
        assert_ne!(ThinkingType::ExplicitTags, ThinkingType::None);
    }

    #[test]
    fn test_thinking_type_serialization() {
        let explicit = ThinkingType::ExplicitTags;
        let json = serde_json::to_string(&explicit).unwrap();
        assert_eq!(json, r#""explicit_tags""#);

        let deserialized: ThinkingType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ThinkingType::ExplicitTags);
    }

    // ==========================================================================
    // ModelRegistry Tests
    // ==========================================================================

    #[test]
    fn test_registry_creation() {
        let registry = ModelRegistry::new();
        assert!(registry.count() > 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = ModelRegistry::default();
        assert!(registry.count() > 0);
    }

    #[test]
    fn test_registry_get_existing_model() {
        let registry = ModelRegistry::new();
        let profile = registry.get("gpt-oss:20b");
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().name, "gpt-oss:20b");
    }

    #[test]
    fn test_registry_get_nonexistent_model() {
        let registry = ModelRegistry::new();
        let profile = registry.get("nonexistent-model");
        assert!(profile.is_none());
    }

    #[test]
    fn test_registry_model_names() {
        let registry = ModelRegistry::new();
        let names = registry.model_names();
        assert!(!names.is_empty());
        assert!(names.contains(&"gpt-oss:20b"));
    }

    #[test]
    fn test_get_best_general() {
        let registry = ModelRegistry::new();
        let best = registry.get_best_general();
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "gpt-oss:20b");
    }

    #[test]
    fn test_get_best_fast() {
        let registry = ModelRegistry::new();
        let best = registry.get_best_fast();
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "qwen2.5-coder:1.5b");
        assert!(best.unwrap().is_fast());
    }

    #[test]
    fn test_get_best_code() {
        let registry = ModelRegistry::new();
        let best = registry.get_best_code();
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_get_best_reasoning() {
        let registry = ModelRegistry::new();
        let best = registry.get_best_reasoning();
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "deepseek-r1:14b");
        assert!(best.unwrap().is_thinking_model());
        assert!(best.unwrap().use_raw_mode);
    }

    #[test]
    fn test_get_best_long_context() {
        let registry = ModelRegistry::new();
        let best = registry.get_best_long_context();
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "llama3.1:8b");
        assert!(best.unwrap().has_large_context());
    }

    #[test]
    fn test_get_thinking_models() {
        let registry = ModelRegistry::new();
        let thinking = registry.get_thinking_models();
        assert!(!thinking.is_empty());

        // All returned models should be thinking models
        for model in &thinking {
            assert!(model.is_thinking_model());
        }

        // Check specific known thinking models
        let names: Vec<&str> = thinking.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"deepseek-r1:14b"));
        assert!(names.contains(&"phi3:mini"));
    }

    #[test]
    fn test_registry_contains_key_models() {
        let registry = ModelRegistry::new();

        // Check Tier 1 models
        assert!(registry.get("gpt-oss:20b").is_some());
        assert!(registry.get("llama3.1:8b").is_some());
        assert!(registry.get("deepseek-r1:14b").is_some());

        // Check Tier 2 models
        assert!(registry.get("qwen2.5-coder:1.5b").is_some());
        assert!(registry.get("qwen2.5-coder:7b").is_some());
        assert!(registry.get("mistral:latest").is_some());

        // Check Tier 3 models
        assert!(registry.get("deepseek-coder-v2:16b").is_some());
        assert!(registry.get("gemma2:9b").is_some());
    }

    // ==========================================================================
    // TaskRequirements Tests
    // ==========================================================================

    #[test]
    fn test_task_requirements_default() {
        let req = TaskRequirements::default();
        assert!(req.min_context.is_none());
        assert!(req.min_output.is_none());
        assert!(req.min_speed.is_none());
        assert!(!req.requires_thinking);
        assert!(req.preferred_family.is_none());
    }

    #[test]
    fn test_find_matching_no_requirements() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements::default();
        let matches = registry.find_matching(&req);

        // Should return all models when no requirements
        assert_eq!(matches.len(), registry.count());
    }

    #[test]
    fn test_find_matching_min_context() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            min_context: Some(90_000),
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should have large context
        for model in &matches {
            assert!(model.native_context >= 90_000);
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_min_output() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            min_output: Some(4_000),
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should have large output
        for model in &matches {
            assert!(model.max_output >= 4_000);
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_min_speed() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            min_speed: Some(150.0),
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should be fast
        for model in &matches {
            assert!(model.speed_tok_s >= 150.0);
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_requires_thinking() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            requires_thinking: true,
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should be thinking models
        for model in &matches {
            assert!(model.is_thinking_model());
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_preferred_family() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            preferred_family: Some("qwen2".to_string()),
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should be qwen2 family
        for model in &matches {
            assert_eq!(model.family, "qwen2");
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_multiple_requirements() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            min_context: Some(30_000),
            min_speed: Some(100.0),
            requires_thinking: false,
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // All matches should meet all criteria
        for model in &matches {
            assert!(model.native_context >= 30_000);
            assert!(model.speed_tok_s >= 100.0);
        }

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_matching_impossible_requirements() {
        let registry = ModelRegistry::new();
        let req = TaskRequirements {
            min_speed: Some(1000.0), // Impossibly fast
            ..Default::default()
        };
        let matches = registry.find_matching(&req);

        // Should return empty when requirements can't be met
        assert!(matches.is_empty());
    }

    // ==========================================================================
    // Integration Tests
    // ==========================================================================

    #[test]
    fn test_deepseek_r1_raw_mode_requirement() {
        let registry = ModelRegistry::new();
        let deepseek = registry.get("deepseek-r1:14b").unwrap();

        // Critical: deepseek-r1 MUST use raw mode
        assert!(deepseek.use_raw_mode);
        assert_eq!(deepseek.thinking_type, ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_model_profile_completeness() {
        let registry = ModelRegistry::new();

        // All models should have valid data
        for (name, profile) in &registry.profiles {
            assert_eq!(profile.name, *name);
            assert!(profile.native_context > 0);
            assert!(profile.speed_tok_s >= 0.0);
            assert!(!profile.family.is_empty());
            assert!(!profile.size.is_empty());
        }
    }

    #[test]
    fn test_registry_count() {
        let registry = ModelRegistry::new();
        // We've added 20+ models
        assert!(registry.count() >= 20);
    }

    #[test]
    fn test_fastest_model_is_really_fast() {
        let registry = ModelRegistry::new();
        let fastest = registry.get_best_fast().unwrap();

        // qwen2.5-coder:1.5b should be ultra fast (>300 tok/s)
        assert!(fastest.speed_tok_s > 300.0);
    }

    #[test]
    fn test_largest_context_model() {
        let registry = ModelRegistry::new();
        let largest = registry.get_best_general().unwrap();

        // gpt-oss:20b has the largest context in our set
        assert!(largest.native_context > 98_000);
    }

    #[test]
    fn test_model_profile_serialization() {
        let profile = ModelProfile {
            name: "test".to_string(),
            native_context: 32_768,
            max_output: 2_048,
            speed_tok_s: 150.0,
            thinking_type: ThinkingType::ExplicitTags,
            use_raw_mode: true,
            family: "test".to_string(),
            size: "8B".to_string(),
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: ModelProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, profile.name);
        assert_eq!(deserialized.native_context, profile.native_context);
        assert_eq!(deserialized.thinking_type, profile.thinking_type);
    }
}

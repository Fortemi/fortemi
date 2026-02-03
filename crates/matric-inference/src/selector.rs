//! Task-based model selection for knowledge management operations.
//!
//! This module provides intelligent model selection based on the specific
//! knowledge management task being performed. Different tasks have different
//! requirements:
//!
//! - **Title Generation**: Needs format compliance, conciseness, speed
//! - **AI Revision**: Needs semantic understanding, content enhancement
//! - **Embedding**: Needs vector quality, dimension consistency
//! - **Semantic Linking**: Needs semantic understanding, format compliance

use crate::capabilities::{known_model_capabilities, Capability, ModelCapabilities, QualityTier};
use crate::hardware::{tier_model_recommendations, HardwareTier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Knowledge management operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KmOperation {
    /// Generate a concise title for a note.
    TitleGeneration,
    /// Enhance and structure note content.
    AiRevision,
    /// Generate vector embeddings for search.
    Embedding,
    /// Find semantically related notes.
    SemanticLinking,
    /// Generate context summaries for related notes.
    ContextGeneration,
}

impl KmOperation {
    /// Get required capabilities for this operation.
    pub fn required_capabilities(&self) -> Vec<Capability> {
        match self {
            KmOperation::TitleGeneration => {
                vec![Capability::TitleGeneration, Capability::FormatCompliance]
            }
            KmOperation::AiRevision => vec![
                Capability::ContentRevision,
                Capability::SemanticUnderstanding,
            ],
            KmOperation::Embedding => vec![Capability::Embedding],
            KmOperation::SemanticLinking => vec![
                Capability::SemanticUnderstanding,
                Capability::FormatCompliance,
            ],
            KmOperation::ContextGeneration => vec![
                Capability::SemanticUnderstanding,
                Capability::ContentRevision,
            ],
        }
    }

    /// Get minimum acceptable quality tier for this operation.
    pub fn min_quality_tier(&self) -> QualityTier {
        match self {
            KmOperation::TitleGeneration => QualityTier::Good, // 80%+ required
            KmOperation::AiRevision => QualityTier::Good,
            KmOperation::Embedding => QualityTier::Excellent, // Embeddings need high quality
            KmOperation::SemanticLinking => QualityTier::Good,
            KmOperation::ContextGeneration => QualityTier::Basic, // Can be lower quality
        }
    }

    /// Get maximum acceptable latency in milliseconds.
    pub fn max_latency_ms(&self) -> u64 {
        match self {
            KmOperation::TitleGeneration => 500,  // Fast for interactive use
            KmOperation::AiRevision => 3000,      // Can be slower
            KmOperation::Embedding => 200,        // Must be fast for batch
            KmOperation::SemanticLinking => 1000, // Moderate
            KmOperation::ContextGeneration => 2000, // Can be slower
        }
    }

    /// Whether this operation prefers speed over quality.
    pub fn prefers_speed(&self) -> bool {
        matches!(self, KmOperation::TitleGeneration | KmOperation::Embedding)
    }
}

/// Model selection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    /// Selected model name.
    pub model: String,
    /// Operation this selection is for.
    pub operation: KmOperation,
    /// Quality tier of the selected model for this operation.
    pub quality_tier: QualityTier,
    /// Expected latency in milliseconds.
    pub expected_latency_ms: Option<u64>,
    /// Why this model was selected.
    pub rationale: String,
    /// Alternative models that could be used.
    pub alternatives: Vec<String>,
}

/// Model selector for knowledge management tasks.
#[derive(Debug, Clone)]
pub struct ModelSelector {
    /// Available model capabilities.
    model_capabilities: HashMap<String, ModelCapabilities>,
    /// Hardware tier constraint.
    hardware_tier: HardwareTier,
    /// Prefer speed over quality.
    prefer_speed: bool,
}

impl ModelSelector {
    /// Create a new model selector with known models.
    pub fn new(hardware_tier: HardwareTier) -> Self {
        let mut model_capabilities = HashMap::new();

        // Load known model capabilities
        let known_models = [
            "nomic-embed-text",
            "mxbai-embed-large",
            "qwen2.5:14b",
            "qwen2.5:7b",
            "gpt-oss:20b",
            "llama3.1:8b",
            "deepseek-r1:14b",
            "deepseek-coder-v2:16b",
            "command-r7b:latest",
            "mistral:latest",
            "hermes3:8b",
            "cogito:8b",
        ];

        for model in known_models {
            if let Some(caps) = known_model_capabilities(model) {
                model_capabilities.insert(model.to_string(), caps);
            }
        }

        Self {
            model_capabilities,
            hardware_tier,
            prefer_speed: false,
        }
    }

    /// Set speed preference.
    pub fn with_speed_preference(mut self, prefer_speed: bool) -> Self {
        self.prefer_speed = prefer_speed;
        self
    }

    /// Add a model with its capabilities.
    pub fn add_model(&mut self, capabilities: ModelCapabilities) {
        self.model_capabilities
            .insert(capabilities.model_name.clone(), capabilities);
    }

    /// Select the best model for an operation.
    pub fn select(&self, operation: KmOperation) -> Option<ModelSelection> {
        let required_caps = operation.required_capabilities();
        let min_tier = operation.min_quality_tier();
        let max_latency = operation.max_latency_ms();

        // For embedding operations, only consider embedding models
        let is_embedding_op = operation == KmOperation::Embedding;

        // Filter models that meet requirements
        let mut candidates: Vec<(&String, &ModelCapabilities)> = self
            .model_capabilities
            .iter()
            .filter(|(_, caps)| {
                // For embedding ops, must be an embedding model
                if is_embedding_op && !caps.is_embedding_model {
                    return false;
                }
                // For non-embedding ops, skip embedding models
                if !is_embedding_op && caps.is_embedding_model {
                    return false;
                }
                // Check all required capabilities
                required_caps
                    .iter()
                    .all(|cap| caps.tier_for(*cap) >= min_tier)
            })
            .collect();

        if candidates.is_empty() {
            // Fall back to tier recommendations
            return self.fallback_selection(operation);
        }

        // Sort by quality (descending) then by latency (ascending)
        candidates.sort_by(|(_, a), (_, b)| {
            // Get primary capability for the operation
            let primary_cap = required_caps.first().unwrap();
            let a_tier = a.tier_for(*primary_cap);
            let b_tier = b.tier_for(*primary_cap);

            // First compare by tier (higher is better)
            match b_tier.cmp(&a_tier) {
                std::cmp::Ordering::Equal => {
                    // Then by latency (lower is better)
                    let a_latency = a
                        .get_rating(Capability::FastInference)
                        .and_then(|r| r.latency_p95_ms)
                        .unwrap_or(u64::MAX);
                    let b_latency = b
                        .get_rating(Capability::FastInference)
                        .and_then(|r| r.latency_p95_ms)
                        .unwrap_or(u64::MAX);
                    a_latency.cmp(&b_latency)
                }
                other => other,
            }
        });

        // If preferring speed, filter by latency first
        if self.prefer_speed || operation.prefers_speed() {
            candidates.retain(|(_, caps)| {
                caps.get_rating(Capability::FastInference)
                    .and_then(|r| r.latency_p95_ms)
                    .map(|l| l <= max_latency)
                    .unwrap_or(false)
            });

            // Re-sort by latency for speed preference
            candidates.sort_by(|(_, a), (_, b)| {
                let a_latency = a
                    .get_rating(Capability::FastInference)
                    .and_then(|r| r.latency_p95_ms)
                    .unwrap_or(u64::MAX);
                let b_latency = b
                    .get_rating(Capability::FastInference)
                    .and_then(|r| r.latency_p95_ms)
                    .unwrap_or(u64::MAX);
                a_latency.cmp(&b_latency)
            });
        }

        // Select best candidate
        let (model_name, caps) = candidates.first()?;
        let primary_cap = required_caps.first().unwrap();

        let alternatives: Vec<String> = candidates
            .iter()
            .skip(1)
            .take(3)
            .map(|(name, _)| (*name).clone())
            .collect();

        Some(ModelSelection {
            model: (*model_name).clone(),
            operation,
            quality_tier: caps.tier_for(*primary_cap),
            expected_latency_ms: caps
                .get_rating(Capability::FastInference)
                .and_then(|r| r.latency_p95_ms),
            rationale: format!(
                "{:?} quality for {:?}",
                caps.tier_for(*primary_cap),
                operation
            ),
            alternatives,
        })
    }

    /// Fallback selection based on hardware tier recommendations.
    fn fallback_selection(&self, operation: KmOperation) -> Option<ModelSelection> {
        let recommendations = tier_model_recommendations(self.hardware_tier);

        // Find appropriate recommendation for operation
        let role = match operation {
            KmOperation::Embedding => "embedding",
            _ => "generation",
        };

        let rec = recommendations.iter().find(|r| r.role == role)?;

        Some(ModelSelection {
            model: rec.model.clone(),
            operation,
            quality_tier: QualityTier::Good, // Assumed
            expected_latency_ms: None,
            rationale: format!("Tier recommendation: {}", rec.rationale),
            alternatives: recommendations
                .iter()
                .filter(|r| r.role == role && r.model != rec.model)
                .take(2)
                .map(|r| r.model.clone())
                .collect(),
        })
    }

    /// Select models for all common operations.
    pub fn select_all(&self) -> HashMap<KmOperation, ModelSelection> {
        let mut selections = HashMap::new();

        for op in [
            KmOperation::TitleGeneration,
            KmOperation::AiRevision,
            KmOperation::Embedding,
            KmOperation::SemanticLinking,
            KmOperation::ContextGeneration,
        ] {
            if let Some(selection) = self.select(op) {
                selections.insert(op, selection);
            }
        }

        selections
    }

    /// Get a recommended configuration for matric-memory.
    pub fn recommended_config(&self) -> RecommendedConfig {
        let embedding = self.select(KmOperation::Embedding);
        let generation = self.select(KmOperation::AiRevision);
        let fast_generation = {
            let mut selector = self.clone();
            selector.prefer_speed = true;
            selector.select(KmOperation::TitleGeneration)
        };

        RecommendedConfig {
            embedding_model: embedding
                .map(|s| s.model)
                .unwrap_or_else(|| "nomic-embed-text".to_string()),
            generation_model: generation
                .map(|s| s.model)
                .unwrap_or_else(|| "qwen2.5:14b".to_string()),
            fast_generation_model: fast_generation.map(|s| s.model),
            hardware_tier: self.hardware_tier,
        }
    }
}

/// Recommended model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedConfig {
    /// Model for embeddings.
    pub embedding_model: String,
    /// Model for generation (revision, context).
    pub generation_model: String,
    /// Optional fast model for titles.
    pub fast_generation_model: Option<String>,
    /// Hardware tier these recommendations are for.
    pub hardware_tier: HardwareTier,
}

impl RecommendedConfig {
    /// Generate environment variable configuration.
    pub fn to_env_config(&self) -> String {
        let mut config = format!(
            "MATRIC_EMBED_MODEL={}\nMATRIC_GEN_MODEL={}",
            self.embedding_model, self.generation_model
        );

        if let Some(ref fast) = self.fast_generation_model {
            config.push_str(&format!("\nMATRIC_FAST_GEN_MODEL={}", fast));
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_km_operation_required_capabilities() {
        let title_caps = KmOperation::TitleGeneration.required_capabilities();
        assert!(title_caps.contains(&Capability::TitleGeneration));
        assert!(title_caps.contains(&Capability::FormatCompliance));

        let embedding_caps = KmOperation::Embedding.required_capabilities();
        assert!(embedding_caps.contains(&Capability::Embedding));
    }

    #[test]
    fn test_km_operation_latency() {
        assert!(
            KmOperation::TitleGeneration.max_latency_ms()
                < KmOperation::AiRevision.max_latency_ms()
        );
        assert!(
            KmOperation::Embedding.max_latency_ms() < KmOperation::TitleGeneration.max_latency_ms()
        );
    }

    #[test]
    fn test_model_selector_creation() {
        let selector = ModelSelector::new(HardwareTier::Mainstream);
        assert!(!selector.model_capabilities.is_empty());
    }

    #[test]
    fn test_select_for_title_generation() {
        let selector = ModelSelector::new(HardwareTier::Mainstream);
        let selection = selector.select(KmOperation::TitleGeneration);

        assert!(selection.is_some());
        let sel = selection.unwrap();
        assert_eq!(sel.operation, KmOperation::TitleGeneration);
        assert!(sel.quality_tier >= QualityTier::Good);
    }

    #[test]
    fn test_select_for_embedding() {
        let selector = ModelSelector::new(HardwareTier::Mainstream);
        let selection = selector.select(KmOperation::Embedding);

        assert!(selection.is_some(), "No embedding model selected");
        let sel = selection.unwrap();
        assert!(
            sel.model.contains("embed"),
            "Selected model '{}' should contain 'embed'",
            sel.model
        );
        assert_eq!(sel.operation, KmOperation::Embedding);
    }

    #[test]
    fn test_select_all() {
        let selector = ModelSelector::new(HardwareTier::Performance);
        let selections = selector.select_all();

        assert!(!selections.is_empty());
        assert!(selections.contains_key(&KmOperation::TitleGeneration));
        assert!(selections.contains_key(&KmOperation::Embedding));
    }

    #[test]
    fn test_recommended_config() {
        let selector = ModelSelector::new(HardwareTier::Mainstream);
        let config = selector.recommended_config();

        assert!(!config.embedding_model.is_empty());
        assert!(!config.generation_model.is_empty());
        assert_eq!(config.hardware_tier, HardwareTier::Mainstream);
    }

    #[test]
    fn test_speed_preference() {
        let fast_selector =
            ModelSelector::new(HardwareTier::Mainstream).with_speed_preference(true);
        let quality_selector =
            ModelSelector::new(HardwareTier::Mainstream).with_speed_preference(false);

        let fast_sel = fast_selector.select(KmOperation::TitleGeneration);
        let quality_sel = quality_selector.select(KmOperation::TitleGeneration);

        // Both should return something
        assert!(fast_sel.is_some());
        assert!(quality_sel.is_some());

        // Fast selection should prefer lower latency
        if let (Some(fast), Some(quality)) = (fast_sel, quality_sel) {
            if let (Some(fast_lat), Some(qual_lat)) =
                (fast.expected_latency_ms, quality.expected_latency_ms)
            {
                assert!(fast_lat <= qual_lat);
            }
        }
    }

    #[test]
    fn test_config_to_env() {
        let config = RecommendedConfig {
            embedding_model: "nomic-embed-text".to_string(),
            generation_model: "qwen2.5:14b".to_string(),
            fast_generation_model: Some("llama3.1:8b".to_string()),
            hardware_tier: HardwareTier::Mainstream,
        };

        let env = config.to_env_config();
        assert!(env.contains("MATRIC_EMBED_MODEL=nomic-embed-text"));
        assert!(env.contains("MATRIC_GEN_MODEL=qwen2.5:14b"));
        assert!(env.contains("MATRIC_FAST_GEN_MODEL=llama3.1:8b"));
    }
}

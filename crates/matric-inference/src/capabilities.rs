//! Model capability flags for knowledge management tasks.
//!
//! This module defines capabilities that models may have for knowledge management
//! operations in matric-memory. Unlike general-purpose LLM capabilities, these
//! are specific to the tasks matric-memory performs:
//!
//! - **Embedding**: Generating vector embeddings for semantic search
//! - **Title Generation**: Creating concise, descriptive titles for notes
//! - **Content Revision**: Enhancing and structuring note content
//! - **Semantic Understanding**: Understanding meaning for linking and search
//! - **Format Compliance**: Following output format instructions reliably
//! - **Fast Inference**: Low-latency responses for interactive use
//! - **Long Context**: Handling large documents without truncation

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Knowledge management capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Model can generate embeddings (embedding models only).
    Embedding,
    /// Model produces high-quality titles (concise, descriptive, accurate).
    TitleGeneration,
    /// Model can enhance and structure content while preserving meaning.
    ContentRevision,
    /// Model understands semantic relationships for linking.
    SemanticUnderstanding,
    /// Model reliably follows output format instructions.
    FormatCompliance,
    /// Model responds quickly (<500ms p95 for short prompts).
    FastInference,
    /// Model can handle large documents (>8K tokens).
    LongContext,
}

impl Capability {
    /// All available capabilities.
    pub fn all() -> &'static [Capability] {
        &[
            Capability::Embedding,
            Capability::TitleGeneration,
            Capability::ContentRevision,
            Capability::SemanticUnderstanding,
            Capability::FormatCompliance,
            Capability::FastInference,
            Capability::LongContext,
        ]
    }

    /// Capabilities required for title generation task.
    pub fn for_title_generation() -> HashSet<Capability> {
        [Capability::TitleGeneration, Capability::FormatCompliance]
            .into_iter()
            .collect()
    }

    /// Capabilities required for AI revision task.
    pub fn for_ai_revision() -> HashSet<Capability> {
        [
            Capability::ContentRevision,
            Capability::SemanticUnderstanding,
        ]
        .into_iter()
        .collect()
    }

    /// Capabilities required for embedding task.
    pub fn for_embedding() -> HashSet<Capability> {
        [Capability::Embedding].into_iter().collect()
    }

    /// Capabilities required for semantic linking task.
    pub fn for_semantic_linking() -> HashSet<Capability> {
        [
            Capability::SemanticUnderstanding,
            Capability::FormatCompliance,
        ]
        .into_iter()
        .collect()
    }
}

/// Quality tier for a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityTier {
    /// Not suitable for this capability.
    Unsuitable,
    /// Basic quality (70-79%).
    Basic,
    /// Good quality (80-89%).
    Good,
    /// Excellent quality (90-94%).
    Excellent,
    /// Best-in-class quality (95%+).
    Elite,
}

impl QualityTier {
    /// Convert a percentage score to a quality tier.
    pub fn from_score(score: f32) -> Self {
        match score {
            s if s >= 95.0 => QualityTier::Elite,
            s if s >= 90.0 => QualityTier::Excellent,
            s if s >= 80.0 => QualityTier::Good,
            s if s >= 70.0 => QualityTier::Basic,
            _ => QualityTier::Unsuitable,
        }
    }

    /// Get the minimum score for this tier.
    pub fn min_score(&self) -> f32 {
        match self {
            QualityTier::Elite => 95.0,
            QualityTier::Excellent => 90.0,
            QualityTier::Good => 80.0,
            QualityTier::Basic => 70.0,
            QualityTier::Unsuitable => 0.0,
        }
    }

    /// Human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            QualityTier::Elite => "Best-in-class (95%+)",
            QualityTier::Excellent => "Excellent (90-94%)",
            QualityTier::Good => "Good (80-89%)",
            QualityTier::Basic => "Basic (70-79%)",
            QualityTier::Unsuitable => "Not recommended (<70%)",
        }
    }
}

/// Capability rating for a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRating {
    /// The capability being rated.
    pub capability: Capability,
    /// Quality tier for this capability.
    pub tier: QualityTier,
    /// Raw score (0-100) if available from evaluation.
    pub score: Option<f32>,
    /// P95 latency in milliseconds (for latency-sensitive capabilities).
    pub latency_p95_ms: Option<u64>,
    /// Notes about this capability (e.g., "requires raw mode").
    pub notes: Option<String>,
}

impl CapabilityRating {
    /// Create a new capability rating.
    pub fn new(capability: Capability, tier: QualityTier) -> Self {
        Self {
            capability,
            tier,
            score: None,
            latency_p95_ms: None,
            notes: None,
        }
    }

    /// Create from an evaluation score.
    pub fn from_score(capability: Capability, score: f32) -> Self {
        Self {
            capability,
            tier: QualityTier::from_score(score),
            score: Some(score),
            latency_p95_ms: None,
            notes: None,
        }
    }

    /// Add latency information.
    pub fn with_latency(mut self, latency_p95_ms: u64) -> Self {
        self.latency_p95_ms = Some(latency_p95_ms);
        self
    }

    /// Add notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Check if this capability meets a minimum tier requirement.
    pub fn meets_tier(&self, min_tier: QualityTier) -> bool {
        self.tier >= min_tier
    }
}

/// Collection of capability ratings for a model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Model name.
    pub model_name: String,
    /// Capability ratings.
    pub ratings: Vec<CapabilityRating>,
    /// Whether this model is an embedding model.
    pub is_embedding_model: bool,
    /// Last evaluation timestamp (ISO 8601).
    pub evaluated_at: Option<String>,
}

impl ModelCapabilities {
    /// Create new capabilities for a model.
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            ratings: Vec::new(),
            is_embedding_model: false,
            evaluated_at: None,
        }
    }

    /// Add a capability rating.
    pub fn add_rating(&mut self, rating: CapabilityRating) {
        // Remove existing rating for same capability
        self.ratings.retain(|r| r.capability != rating.capability);
        self.ratings.push(rating);
    }

    /// Get rating for a specific capability.
    pub fn get_rating(&self, capability: Capability) -> Option<&CapabilityRating> {
        self.ratings.iter().find(|r| r.capability == capability)
    }

    /// Get the quality tier for a capability.
    pub fn tier_for(&self, capability: Capability) -> QualityTier {
        self.get_rating(capability)
            .map(|r| r.tier)
            .unwrap_or(QualityTier::Unsuitable)
    }

    /// Check if model has all required capabilities at minimum tier.
    pub fn has_capabilities(&self, required: &HashSet<Capability>, min_tier: QualityTier) -> bool {
        required.iter().all(|cap| self.tier_for(*cap) >= min_tier)
    }

    /// Get capabilities that meet a minimum tier.
    pub fn capabilities_at_tier(&self, min_tier: QualityTier) -> Vec<Capability> {
        self.ratings
            .iter()
            .filter(|r| r.tier >= min_tier)
            .map(|r| r.capability)
            .collect()
    }

    /// Get the best capability tier this model has.
    pub fn best_tier(&self) -> QualityTier {
        self.ratings
            .iter()
            .map(|r| r.tier)
            .max()
            .unwrap_or(QualityTier::Unsuitable)
    }

    /// Check if model is suitable for title generation.
    pub fn is_good_for_titles(&self) -> bool {
        self.tier_for(Capability::TitleGeneration) >= QualityTier::Good
            && self.tier_for(Capability::FormatCompliance) >= QualityTier::Good
    }

    /// Check if model is suitable for AI revision.
    pub fn is_good_for_revision(&self) -> bool {
        self.tier_for(Capability::ContentRevision) >= QualityTier::Good
    }

    /// Check if model is fast enough for interactive use.
    pub fn is_fast_enough(&self, max_latency_ms: u64) -> bool {
        self.get_rating(Capability::FastInference)
            .and_then(|r| r.latency_p95_ms)
            .map(|l| l <= max_latency_ms)
            .unwrap_or(false)
    }
}

/// Known model capabilities based on evaluation data.
///
/// This provides default capabilities for well-known models based on
/// evaluation results documented in `/docs/research/`.
pub fn known_model_capabilities(model_name: &str) -> Option<ModelCapabilities> {
    let mut caps = ModelCapabilities::new(model_name);

    match model_name {
        // Embedding models
        "nomic-embed-text" | "mxbai-embed-large" => {
            caps.is_embedding_model = true;
            caps.add_rating(
                CapabilityRating::from_score(Capability::Embedding, 90.0).with_latency(50),
            );
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                90.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 95.0).with_latency(50),
            );
        }

        // Best quality: qwen2.5:14b
        "qwen2.5:14b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                93.8,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                91.2,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                100.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                92.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 85.0).with_latency(492),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 85.0));
        }

        // Production stable: gpt-oss:20b
        "gpt-oss:20b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                91.1,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                87.3,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                100.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                90.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 70.0).with_latency(1800),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 95.0));
        }

        // Best value: qwen2.5:7b
        "qwen2.5:7b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                88.9,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                84.2,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                100.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                85.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 90.0).with_latency(375),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 85.0));
        }

        // Fastest: llama3.1:8b
        "llama3.1:8b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                85.4,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                83.4,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                90.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                82.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 95.0).with_latency(258),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 95.0));
        }

        // DeepSeek-R1: Too slow for interactive use
        "deepseek-r1:14b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                85.4,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                83.5,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                90.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                85.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 30.0)
                    .with_latency(6600)
                    .with_notes("25x slower due to reasoning overhead"),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 95.0));
        }

        // DeepSeek-Coder: Not recommended for text generation
        "deepseek-coder-v2:16b" => {
            caps.add_rating(
                CapabilityRating::from_score(Capability::TitleGeneration, 66.1)
                    .with_notes("Optimized for code, not text generation"),
            );
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                83.7,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                25.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 90.0).with_latency(319),
            );
        }

        // Command-R7B: Good balance
        "command-r7b:latest" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                86.1,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                80.1,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                90.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                82.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 88.0).with_latency(400),
            );
        }

        // Mistral: Basic quality
        "mistral:latest" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                66.1,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                81.6,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                70.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 92.0).with_latency(360),
            );
        }

        // Hermes3: Good semantic understanding
        "hermes3:8b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                78.2,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                86.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                85.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                80.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 70.0).with_latency(1800),
            );
        }

        // Granite4: Fast extraction model (3B, 244 tok/s, 98K context)
        "granite4:3b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                78.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                75.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                85.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 95.0).with_latency(180),
            );
            caps.add_rating(CapabilityRating::from_score(Capability::LongContext, 98.0));
        }

        // Cogito: Good all-around
        "cogito:8b" => {
            caps.add_rating(CapabilityRating::from_score(
                Capability::TitleGeneration,
                85.2,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::SemanticUnderstanding,
                81.1,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::FormatCompliance,
                85.0,
            ));
            caps.add_rating(CapabilityRating::from_score(
                Capability::ContentRevision,
                82.0,
            ));
            caps.add_rating(
                CapabilityRating::from_score(Capability::FastInference, 85.0).with_latency(450),
            );
        }

        _ => return None,
    }

    Some(caps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_tier_from_score() {
        assert_eq!(QualityTier::from_score(96.0), QualityTier::Elite);
        assert_eq!(QualityTier::from_score(92.0), QualityTier::Excellent);
        assert_eq!(QualityTier::from_score(85.0), QualityTier::Good);
        assert_eq!(QualityTier::from_score(75.0), QualityTier::Basic);
        assert_eq!(QualityTier::from_score(50.0), QualityTier::Unsuitable);
    }

    #[test]
    fn test_capability_rating() {
        let rating = CapabilityRating::from_score(Capability::TitleGeneration, 93.8)
            .with_latency(492)
            .with_notes("Best model for titles");

        assert_eq!(rating.tier, QualityTier::Excellent);
        assert_eq!(rating.score, Some(93.8));
        assert_eq!(rating.latency_p95_ms, Some(492));
        assert!(rating.notes.is_some());
    }

    #[test]
    fn test_model_capabilities() {
        let mut caps = ModelCapabilities::new("test-model");
        caps.add_rating(CapabilityRating::from_score(
            Capability::TitleGeneration,
            90.0,
        ));
        caps.add_rating(CapabilityRating::from_score(
            Capability::FormatCompliance,
            95.0,
        ));

        assert_eq!(
            caps.tier_for(Capability::TitleGeneration),
            QualityTier::Excellent
        );
        assert_eq!(
            caps.tier_for(Capability::FormatCompliance),
            QualityTier::Elite
        );
        assert_eq!(
            caps.tier_for(Capability::ContentRevision),
            QualityTier::Unsuitable
        );
    }

    #[test]
    fn test_has_capabilities() {
        let mut caps = ModelCapabilities::new("test-model");
        caps.add_rating(CapabilityRating::from_score(
            Capability::TitleGeneration,
            90.0,
        ));
        caps.add_rating(CapabilityRating::from_score(
            Capability::FormatCompliance,
            85.0,
        ));

        let required = Capability::for_title_generation();
        assert!(caps.has_capabilities(&required, QualityTier::Good));
        assert!(!caps.has_capabilities(&required, QualityTier::Elite));
    }

    #[test]
    fn test_known_model_capabilities() {
        let qwen = known_model_capabilities("qwen2.5:14b").unwrap();
        assert!(qwen.is_good_for_titles());
        assert!(qwen.is_good_for_revision());

        let deepseek_coder = known_model_capabilities("deepseek-coder-v2:16b").unwrap();
        assert!(!deepseek_coder.is_good_for_titles());

        let unknown = known_model_capabilities("unknown-model");
        assert!(unknown.is_none());
    }

    #[test]
    fn test_is_fast_enough() {
        let llama = known_model_capabilities("llama3.1:8b").unwrap();
        assert!(llama.is_fast_enough(500));

        let deepseek_r1 = known_model_capabilities("deepseek-r1:14b").unwrap();
        assert!(!deepseek_r1.is_fast_enough(500));
        assert!(deepseek_r1.is_fast_enough(10000));
    }

    #[test]
    fn test_capability_for_task() {
        let title_caps = Capability::for_title_generation();
        assert!(title_caps.contains(&Capability::TitleGeneration));
        assert!(title_caps.contains(&Capability::FormatCompliance));
        assert!(!title_caps.contains(&Capability::Embedding));

        let revision_caps = Capability::for_ai_revision();
        assert!(revision_caps.contains(&Capability::ContentRevision));
        assert!(revision_caps.contains(&Capability::SemanticUnderstanding));
    }

    #[test]
    fn test_embedding_model() {
        let nomic = known_model_capabilities("nomic-embed-text").unwrap();
        assert!(nomic.is_embedding_model);
        assert_eq!(
            nomic.tier_for(Capability::Embedding),
            QualityTier::Excellent
        );
    }
}

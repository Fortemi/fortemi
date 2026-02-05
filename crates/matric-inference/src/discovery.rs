//! Model discovery and recommendation for matric-memory.
//!
//! This module provides automatic detection of available Ollama models
//! and generates optimal configuration recommendations based on:
//! - Available models on the system
//! - Hardware capabilities
//! - Known model performance data
//!
//! # Example
//!
//! ```rust,no_run
//! use matric_inference::discovery::ModelDiscovery;
//!
//! #[tokio::main]
//! async fn main() {
//!     let discovery = ModelDiscovery::new("http://localhost:11434");
//!     let available = discovery.discover_models().await.unwrap();
//!     let recommendation = discovery.recommend_config(&available).await;
//!     println!("{}", recommendation.summary());
//! }
//! ```

use crate::capabilities::{known_model_capabilities, Capability, ModelCapabilities, QualityTier};
use crate::hardware::{HardwareTier, OllamaSettings, SystemCapabilities};
use crate::selector::ModelSelector;
use serde::{Deserialize, Serialize};

/// Discovered Ollama model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// Model name (e.g., "qwen2.5:14b").
    pub name: String,
    /// Model size in bytes.
    pub size: u64,
    /// Model family (if known).
    pub family: Option<String>,
    /// Parameter count (if known).
    pub parameter_size: Option<String>,
    /// Quantization level (if known).
    pub quantization: Option<String>,
    /// Whether this model has known capabilities.
    pub has_known_capabilities: bool,
    /// Capabilities (if known).
    pub capabilities: Option<ModelCapabilities>,
}

impl DiscoveredModel {
    /// Check if this is likely an embedding model.
    pub fn is_likely_embedding(&self) -> bool {
        self.name.contains("embed")
            || self.name.contains("nomic")
            || self.name.contains("mxbai")
            || self.name.contains("bge")
    }

    /// Get model size in human-readable format.
    pub fn size_human(&self) -> String {
        let gb = self.size as f64 / (1024.0 * 1024.0 * 1024.0);
        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            let mb = self.size as f64 / (1024.0 * 1024.0);
            format!("{:.0} MB", mb)
        }
    }
}

/// Result of model discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    /// All discovered models.
    pub models: Vec<DiscoveredModel>,
    /// Models suitable for embedding.
    pub embedding_models: Vec<String>,
    /// Models suitable for generation.
    pub generation_models: Vec<String>,
    /// Models with known high-quality capabilities.
    pub recommended_models: Vec<String>,
    /// Discovery timestamp.
    pub discovered_at: String,
}

impl DiscoveryResult {
    /// Get a summary of the discovery.
    pub fn summary(&self) -> String {
        format!(
            "Discovered {} models: {} embedding, {} generation, {} recommended",
            self.models.len(),
            self.embedding_models.len(),
            self.generation_models.len(),
            self.recommended_models.len()
        )
    }

    /// Check if any models were discovered.
    pub fn has_models(&self) -> bool {
        !self.models.is_empty()
    }

    /// Get embedding model names.
    pub fn get_embedding_models(&self) -> &[String] {
        &self.embedding_models
    }

    /// Get generation model names.
    pub fn get_generation_models(&self) -> &[String] {
        &self.generation_models
    }
}

/// System configuration recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigRecommendation {
    /// Recommended embedding model.
    pub embedding_model: String,
    /// Recommended generation model.
    pub generation_model: String,
    /// Optional fast generation model.
    pub fast_generation_model: Option<String>,
    /// Hardware tier.
    pub hardware_tier: HardwareTier,
    /// Ollama settings.
    pub ollama_settings: OllamaSettings,
    /// Quality expectations.
    pub expected_quality: QualityExpectation,
    /// Confidence in this recommendation (0-100).
    pub confidence: u8,
    /// Explanation of the recommendation.
    pub rationale: String,
    /// Alternative configurations.
    pub alternatives: Vec<AlternativeConfig>,
    /// Comparison to cloud providers.
    pub cloud_comparison: Option<CloudComparisonNote>,
}

impl ConfigRecommendation {
    /// Generate environment configuration.
    pub fn to_env_config(&self) -> String {
        let mut config = format!(
            "# Matric Memory Model Configuration\n\
            # Hardware Tier: {:?}\n\
            # Expected Quality: {}\n\n\
            MATRIC_EMBED_MODEL={}\n\
            MATRIC_GEN_MODEL={}\n",
            self.hardware_tier,
            self.expected_quality.summary(),
            self.embedding_model,
            self.generation_model
        );

        if let Some(ref fast) = self.fast_generation_model {
            config.push_str(&format!("MATRIC_FAST_GEN_MODEL={}\n", fast));
        }

        config.push_str("\n# Ollama Settings\n");
        config.push_str(&self.ollama_settings.to_env_exports());

        config
    }

    /// Get a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "Recommendation ({:?} tier, {}% confidence):\n\
            - Embedding: {}\n\
            - Generation: {}\n\
            - Expected Title Quality: {:.0}%-{:.0}%\n\
            - Expected Latency: {}ms-{}ms\n\n\
            {}",
            self.hardware_tier,
            self.confidence,
            self.embedding_model,
            self.generation_model,
            self.expected_quality.title_quality_range.0,
            self.expected_quality.title_quality_range.1,
            self.expected_quality.latency_range_ms.0,
            self.expected_quality.latency_range_ms.1,
            self.rationale
        )
    }
}

/// Quality expectations for a configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityExpectation {
    /// Title generation quality range.
    pub title_quality_range: (f32, f32),
    /// Semantic accuracy range.
    pub semantic_accuracy_range: (f32, f32),
    /// Latency range in ms.
    pub latency_range_ms: (u64, u64),
}

impl QualityExpectation {
    /// Get a summary string.
    pub fn summary(&self) -> String {
        format!(
            "Title: {:.0}%-{:.0}%, Semantic: {:.0}%-{:.0}%, Latency: {}ms-{}ms",
            self.title_quality_range.0,
            self.title_quality_range.1,
            self.semantic_accuracy_range.0,
            self.semantic_accuracy_range.1,
            self.latency_range_ms.0,
            self.latency_range_ms.1
        )
    }
}

/// Alternative configuration option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeConfig {
    /// Description of this alternative.
    pub description: String,
    /// Embedding model.
    pub embedding_model: String,
    /// Generation model.
    pub generation_model: String,
    /// Tradeoff explanation.
    pub tradeoff: String,
}

/// Cloud provider comparison note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudComparisonNote {
    /// Equivalent cloud tier.
    pub equivalent_to: String,
    /// Estimated monthly cost if using cloud.
    pub estimated_cloud_cost: String,
    /// Notes about the comparison.
    pub notes: String,
}

/// Model discovery service.
pub struct ModelDiscovery {
    /// Ollama base URL.
    ollama_url: String,
    /// HTTP client.
    client: reqwest::Client,
}

impl ModelDiscovery {
    /// Create a new discovery service.
    pub fn new(ollama_url: impl Into<String>) -> Self {
        Self {
            ollama_url: ollama_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create from environment.
    pub fn from_env() -> Self {
        let url =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        Self::new(url)
    }

    /// Discover available Ollama models.
    pub async fn discover_models(&self) -> Result<DiscoveryResult, DiscoveryError> {
        let url = format!("{}/api/tags", self.ollama_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| DiscoveryError::ConnectionFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DiscoveryError::ApiError(format!(
                "Ollama API returned {}",
                response.status()
            )));
        }

        let tags_response: OllamaTagsResponse = response
            .json()
            .await
            .map_err(|e| DiscoveryError::ParseError(e.to_string()))?;

        let mut models = Vec::new();
        let mut embedding_models = Vec::new();
        let mut generation_models = Vec::new();
        let mut recommended_models = Vec::new();

        for model in tags_response.models {
            let has_known_caps = known_model_capabilities(&model.name).is_some();
            let capabilities = known_model_capabilities(&model.name);

            let discovered = DiscoveredModel {
                name: model.name.clone(),
                size: model.size,
                family: model.details.as_ref().map(|d| d.family.clone()),
                parameter_size: model.details.as_ref().map(|d| d.parameter_size.clone()),
                quantization: model.details.as_ref().map(|d| d.quantization_level.clone()),
                has_known_capabilities: has_known_caps,
                capabilities,
            };

            if discovered.is_likely_embedding() {
                embedding_models.push(model.name.clone());
            } else {
                generation_models.push(model.name.clone());
            }

            if has_known_caps {
                recommended_models.push(model.name.clone());
            }

            models.push(discovered);
        }

        Ok(DiscoveryResult {
            models,
            embedding_models,
            generation_models,
            recommended_models,
            discovered_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Generate a configuration recommendation.
    pub async fn recommend_config(&self, discovered: &DiscoveryResult) -> ConfigRecommendation {
        // Detect hardware
        let hw = SystemCapabilities::detect();
        let tier = hw.detected_tier;

        // Create selector with discovered models
        let mut selector = ModelSelector::new(tier);

        // Add discovered models that have known capabilities
        for model in &discovered.models {
            if let Some(ref caps) = model.capabilities {
                selector.add_model(caps.clone());
            }
        }

        // Get recommended config from selector
        let config = selector.recommended_config();

        // Determine confidence based on how many known models we found
        let known_count = discovered.recommended_models.len();
        let confidence = match known_count {
            0 => 30,     // Low confidence - using defaults
            1..=2 => 60, // Medium confidence
            3..=5 => 80, // Good confidence
            _ => 95,     // High confidence
        };

        // Build quality expectations
        let expected_quality =
            self.quality_for_models(&config.embedding_model, &config.generation_model, tier);

        // Build alternatives
        let alternatives = self.build_alternatives(discovered, tier);

        // Build cloud comparison
        let cloud_comparison = self.build_cloud_comparison(tier);

        ConfigRecommendation {
            embedding_model: config.embedding_model,
            generation_model: config.generation_model,
            fast_generation_model: config.fast_generation_model,
            hardware_tier: tier,
            ollama_settings: OllamaSettings::for_tier(tier),
            expected_quality,
            confidence,
            rationale: self.build_rationale(discovered, tier, confidence),
            alternatives,
            cloud_comparison: Some(cloud_comparison),
        }
    }

    fn quality_for_models(
        &self,
        _embedding_model: &str,
        generation_model: &str,
        tier: HardwareTier,
    ) -> QualityExpectation {
        // Try to get from known capabilities
        if let Some(caps) = known_model_capabilities(generation_model) {
            let title_tier = caps.tier_for(Capability::TitleGeneration);
            let semantic_tier = caps.tier_for(Capability::SemanticUnderstanding);

            let title_range = match title_tier {
                QualityTier::Elite => (95.0, 99.0),
                QualityTier::Excellent => (90.0, 94.0),
                QualityTier::Good => (80.0, 89.0),
                QualityTier::Basic => (70.0, 79.0),
                QualityTier::Unsuitable => (50.0, 69.0),
            };

            let semantic_range = match semantic_tier {
                QualityTier::Elite => (95.0, 99.0),
                QualityTier::Excellent => (90.0, 94.0),
                QualityTier::Good => (80.0, 89.0),
                QualityTier::Basic => (70.0, 79.0),
                QualityTier::Unsuitable => (50.0, 69.0),
            };

            let latency = caps
                .get_rating(Capability::FastInference)
                .and_then(|r| r.latency_p95_ms)
                .map(|l| (l / 2, l * 2))
                .unwrap_or((300, 1000));

            return QualityExpectation {
                title_quality_range: title_range,
                semantic_accuracy_range: semantic_range,
                latency_range_ms: latency,
            };
        }

        // Fall back to tier-based expectations
        use crate::hardware::tier_quality_expectations;
        let tier_exp = tier_quality_expectations(tier);

        QualityExpectation {
            title_quality_range: tier_exp.title_quality_range,
            semantic_accuracy_range: tier_exp.semantic_accuracy_range,
            latency_range_ms: tier_exp.latency_range_ms,
        }
    }

    fn build_alternatives(
        &self,
        discovered: &DiscoveryResult,
        _tier: HardwareTier,
    ) -> Vec<AlternativeConfig> {
        let mut alternatives = Vec::new();

        // Find alternative embedding models
        let alt_embed: Vec<_> = discovered
            .embedding_models
            .iter()
            .filter(|m| known_model_capabilities(m).is_some())
            .take(2)
            .collect();

        // Find alternative generation models
        let alt_gen: Vec<_> = discovered
            .generation_models
            .iter()
            .filter(|m| known_model_capabilities(m).is_some())
            .take(3)
            .collect();

        // Speed-focused alternative
        if let Some(fast_gen) = alt_gen.iter().find(|m| {
            known_model_capabilities(m)
                .map(|c| c.tier_for(Capability::FastInference) >= QualityTier::Excellent)
                .unwrap_or(false)
        }) {
            alternatives.push(AlternativeConfig {
                description: "Speed-focused configuration".to_string(),
                embedding_model: alt_embed
                    .first()
                    .map(|s| (*s).clone())
                    .unwrap_or_else(|| "nomic-embed-text".to_string()),
                generation_model: (*fast_gen).clone(),
                tradeoff: "Lower latency at the cost of slightly reduced quality".to_string(),
            });
        }

        // Quality-focused alternative
        if let Some(quality_gen) = alt_gen.iter().find(|m| {
            known_model_capabilities(m)
                .map(|c| c.tier_for(Capability::TitleGeneration) >= QualityTier::Excellent)
                .unwrap_or(false)
        }) {
            alternatives.push(AlternativeConfig {
                description: "Quality-focused configuration".to_string(),
                embedding_model: alt_embed
                    .first()
                    .map(|s| (*s).clone())
                    .unwrap_or_else(|| "mxbai-embed-large".to_string()),
                generation_model: (*quality_gen).clone(),
                tradeoff: "Higher quality at the cost of increased latency".to_string(),
            });
        }

        alternatives
    }

    fn build_cloud_comparison(&self, tier: HardwareTier) -> CloudComparisonNote {
        match tier {
            HardwareTier::Budget => CloudComparisonNote {
                equivalent_to: "Below Claude Haiku".to_string(),
                estimated_cloud_cost: "$5-20/month for similar usage".to_string(),
                notes: "Consider upgrading GPU for better local performance, or using Groq API for speed".to_string(),
            },
            HardwareTier::Mainstream => CloudComparisonNote {
                equivalent_to: "Between Claude Haiku and Sonnet".to_string(),
                estimated_cloud_cost: "$20-50/month for similar usage".to_string(),
                notes: "Good local value. Cloud APIs offer higher quality at higher cost".to_string(),
            },
            HardwareTier::Performance => CloudComparisonNote {
                equivalent_to: "Comparable to Claude Sonnet".to_string(),
                estimated_cloud_cost: "$50-100/month for similar usage".to_string(),
                notes: "Excellent local performance. Cloud APIs mainly useful for burst capacity".to_string(),
            },
            HardwareTier::Professional => CloudComparisonNote {
                equivalent_to: "Comparable to GPT-4o or Claude Opus".to_string(),
                estimated_cloud_cost: "$100-500/month for similar usage".to_string(),
                notes: "Best-in-class local performance. Cloud rarely needed except for specialized tasks".to_string(),
            },
        }
    }

    fn build_rationale(
        &self,
        discovered: &DiscoveryResult,
        tier: HardwareTier,
        confidence: u8,
    ) -> String {
        let mut rationale = format!(
            "Based on {} discovered models and {:?} hardware tier.\n\n",
            discovered.models.len(),
            tier
        );

        if confidence >= 80 {
            rationale.push_str("High confidence: Multiple known high-quality models available.\n");
        } else if confidence >= 60 {
            rationale.push_str("Medium confidence: Some known models available, recommendations based on eval data.\n");
        } else {
            rationale.push_str("Low confidence: Few known models detected. Consider downloading qwen2.5:14b for better quality.\n");
        }

        if discovered.embedding_models.is_empty() {
            rationale.push_str(
                "\nWarning: No embedding models detected. Run: ollama pull nomic-embed-text\n",
            );
        }

        rationale
    }
}

/// Errors that can occur during discovery.
#[derive(Debug, Clone)]
pub enum DiscoveryError {
    /// Failed to connect to Ollama.
    ConnectionFailed(String),
    /// API returned an error.
    ApiError(String),
    /// Failed to parse response.
    ParseError(String),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            DiscoveryError::ApiError(e) => write!(f, "API error: {}", e),
            DiscoveryError::ParseError(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for DiscoveryError {}

// Ollama API response types
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    size: u64,
    details: Option<OllamaModelDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelDetails {
    family: String,
    parameter_size: String,
    quantization_level: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_model_is_likely_embedding() {
        let embed = DiscoveredModel {
            name: "nomic-embed-text".to_string(),
            size: 1_000_000,
            family: None,
            parameter_size: None,
            quantization: None,
            has_known_capabilities: true,
            capabilities: None,
        };
        assert!(embed.is_likely_embedding());

        let gen = DiscoveredModel {
            name: "qwen2.5:14b".to_string(),
            size: 10_000_000_000,
            family: Some("qwen2".to_string()),
            parameter_size: Some("14B".to_string()),
            quantization: None,
            has_known_capabilities: true,
            capabilities: None,
        };
        assert!(!gen.is_likely_embedding());
    }

    #[test]
    fn test_size_human() {
        let small = DiscoveredModel {
            name: "test".to_string(),
            size: 500 * 1024 * 1024, // 500 MB
            family: None,
            parameter_size: None,
            quantization: None,
            has_known_capabilities: false,
            capabilities: None,
        };
        assert!(small.size_human().contains("MB"));

        let large = DiscoveredModel {
            name: "test".to_string(),
            size: 10 * 1024 * 1024 * 1024, // 10 GB
            family: None,
            parameter_size: None,
            quantization: None,
            has_known_capabilities: false,
            capabilities: None,
        };
        assert!(large.size_human().contains("GB"));
    }

    #[test]
    fn test_discovery_result_summary() {
        let result = DiscoveryResult {
            models: vec![],
            embedding_models: vec!["nomic-embed-text".to_string()],
            generation_models: vec!["qwen2.5:14b".to_string(), "llama3.1:8b".to_string()],
            recommended_models: vec!["qwen2.5:14b".to_string()],
            discovered_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let summary = result.summary();
        assert!(summary.contains("1 embedding"));
        assert!(summary.contains("2 generation"));
    }

    #[test]
    fn test_config_recommendation_to_env() {
        let config = ConfigRecommendation {
            embedding_model: "nomic-embed-text".to_string(),
            generation_model: "qwen2.5:14b".to_string(),
            fast_generation_model: Some("llama3.1:8b".to_string()),
            hardware_tier: HardwareTier::Mainstream,
            ollama_settings: OllamaSettings::for_tier(HardwareTier::Mainstream),
            expected_quality: QualityExpectation {
                title_quality_range: (88.0, 94.0),
                semantic_accuracy_range: (85.0, 91.0),
                latency_range_ms: (300, 600),
            },
            confidence: 80,
            rationale: "Test rationale".to_string(),
            alternatives: vec![],
            cloud_comparison: None,
        };

        let env = config.to_env_config();
        assert!(env.contains("MATRIC_EMBED_MODEL=nomic-embed-text"));
        assert!(env.contains("MATRIC_GEN_MODEL=qwen2.5:14b"));
        assert!(env.contains("MATRIC_FAST_GEN_MODEL=llama3.1:8b"));
    }

    #[test]
    fn test_model_discovery_from_env() {
        std::env::set_var("OLLAMA_HOST", "http://test:11434");
        let discovery = ModelDiscovery::from_env();
        assert_eq!(discovery.ollama_url, "http://test:11434");
        std::env::remove_var("OLLAMA_HOST");
    }
}

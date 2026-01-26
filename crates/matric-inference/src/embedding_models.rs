//! Embedding model configuration and registry.
//!
//! Provides known embedding model profiles with support for:
//! - Asymmetric embeddings (E5 models: "query:" / "passage:" prefixes)
//! - Symmetric embeddings (nomic-embed-text, all-MiniLM, etc.)
//! - Model dimension and capability metadata
//!
//! # E5 Prefix Requirement
//!
//! E5 models (Wang et al., 2022) require task-specific prefixes for optimal performance:
//! - `"query: "` for search queries
//! - `"passage: "` for document passages being indexed
//!
//! Without prefixes, retrieval quality drops ~6.7% on average.
//!
//! Reference: REF-050 - Wang, L., et al. (2022). "Text Embeddings by
//! Weakly-Supervised Contrastive Pre-training." arXiv.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedding type: whether the model uses different encodings for queries vs passages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingSymmetry {
    /// Same encoding for queries and passages (e.g., nomic-embed-text)
    Symmetric,
    /// Different prefixes for queries and passages (e.g., E5, BGE)
    Asymmetric,
}

/// Known embedding model profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelProfile {
    /// Model name as used in Ollama (e.g., "e5-base-v2", "nomic-embed-text")
    pub name: String,
    /// Output vector dimension
    pub dimension: usize,
    /// Whether the model uses symmetric or asymmetric embeddings
    pub symmetry: EmbeddingSymmetry,
    /// Prefix to prepend to query text (for asymmetric models)
    pub query_prefix: Option<String>,
    /// Prefix to prepend to passage/document text (for asymmetric models)
    pub passage_prefix: Option<String>,
    /// Maximum input tokens
    pub max_tokens: usize,
    /// Model family (e5, nomic, bge, etc.)
    pub family: String,
    /// Brief description
    pub description: String,
}

impl EmbeddingModelProfile {
    /// Returns true if this model requires asymmetric prefixes.
    pub fn is_asymmetric(&self) -> bool {
        self.symmetry == EmbeddingSymmetry::Asymmetric
    }

    /// Apply the appropriate prefix for a query string.
    pub fn prefix_query(&self, text: &str) -> String {
        match &self.query_prefix {
            Some(prefix) => format!("{}{}", prefix, text),
            None => text.to_string(),
        }
    }

    /// Apply the appropriate prefix for a passage/document string.
    pub fn prefix_passage(&self, text: &str) -> String {
        match &self.passage_prefix {
            Some(prefix) => format!("{}{}", prefix, text),
            None => text.to_string(),
        }
    }

    /// Apply prefixes to a batch of query strings.
    pub fn prefix_queries(&self, texts: &[String]) -> Vec<String> {
        texts.iter().map(|t| self.prefix_query(t)).collect()
    }

    /// Apply prefixes to a batch of passage strings.
    pub fn prefix_passages(&self, texts: &[String]) -> Vec<String> {
        texts.iter().map(|t| self.prefix_passage(t)).collect()
    }
}

/// Registry of known embedding models.
pub struct EmbeddingModelRegistry {
    models: HashMap<String, EmbeddingModelProfile>,
}

impl EmbeddingModelRegistry {
    /// Create a new registry with all known embedding models.
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // =================================================================
        // E5 Family (Asymmetric - requires query:/passage: prefixes)
        // =================================================================

        models.insert(
            "e5-small-v2".to_string(),
            EmbeddingModelProfile {
                name: "e5-small-v2".to_string(),
                dimension: 384,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some("query: ".to_string()),
                passage_prefix: Some("passage: ".to_string()),
                max_tokens: 512,
                family: "e5".to_string(),
                description: "E5-small-v2: Fast asymmetric embeddings (384d)".to_string(),
            },
        );

        models.insert(
            "e5-base-v2".to_string(),
            EmbeddingModelProfile {
                name: "e5-base-v2".to_string(),
                dimension: 768,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some("query: ".to_string()),
                passage_prefix: Some("passage: ".to_string()),
                max_tokens: 512,
                family: "e5".to_string(),
                description: "E5-base-v2: Balanced asymmetric embeddings (768d)".to_string(),
            },
        );

        models.insert(
            "e5-large-v2".to_string(),
            EmbeddingModelProfile {
                name: "e5-large-v2".to_string(),
                dimension: 1024,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some("query: ".to_string()),
                passage_prefix: Some("passage: ".to_string()),
                max_tokens: 512,
                family: "e5".to_string(),
                description: "E5-large-v2: High-quality asymmetric embeddings (1024d)".to_string(),
            },
        );

        // Multilingual E5
        models.insert(
            "multilingual-e5-base".to_string(),
            EmbeddingModelProfile {
                name: "multilingual-e5-base".to_string(),
                dimension: 768,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some("query: ".to_string()),
                passage_prefix: Some("passage: ".to_string()),
                max_tokens: 512,
                family: "e5".to_string(),
                description: "Multilingual E5-base: 100+ languages (768d)".to_string(),
            },
        );

        // =================================================================
        // Nomic Family (Symmetric - no prefix needed)
        // =================================================================

        models.insert(
            "nomic-embed-text".to_string(),
            EmbeddingModelProfile {
                name: "nomic-embed-text".to_string(),
                dimension: 768,
                symmetry: EmbeddingSymmetry::Symmetric,
                query_prefix: None,
                passage_prefix: None,
                max_tokens: 8192,
                family: "nomic".to_string(),
                description: "Nomic Embed Text: Long-context symmetric embeddings (768d)"
                    .to_string(),
            },
        );

        // =================================================================
        // BGE Family (Asymmetric - uses "Represent this sentence:" prefix)
        // =================================================================

        models.insert(
            "bge-base-en-v1.5".to_string(),
            EmbeddingModelProfile {
                name: "bge-base-en-v1.5".to_string(),
                dimension: 768,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some(
                    "Represent this sentence for searching relevant passages: ".to_string(),
                ),
                passage_prefix: None, // BGE only prefixes queries
                max_tokens: 512,
                family: "bge".to_string(),
                description: "BGE-base: BAAI general embedding (768d)".to_string(),
            },
        );

        models.insert(
            "bge-large-en-v1.5".to_string(),
            EmbeddingModelProfile {
                name: "bge-large-en-v1.5".to_string(),
                dimension: 1024,
                symmetry: EmbeddingSymmetry::Asymmetric,
                query_prefix: Some(
                    "Represent this sentence for searching relevant passages: ".to_string(),
                ),
                passage_prefix: None,
                max_tokens: 512,
                family: "bge".to_string(),
                description: "BGE-large: High-quality BAAI embedding (1024d)".to_string(),
            },
        );

        // =================================================================
        // MxBAI Family (Symmetric)
        // =================================================================

        models.insert(
            "mxbai-embed-large".to_string(),
            EmbeddingModelProfile {
                name: "mxbai-embed-large".to_string(),
                dimension: 1024,
                symmetry: EmbeddingSymmetry::Symmetric,
                query_prefix: None,
                passage_prefix: None,
                max_tokens: 512,
                family: "mxbai".to_string(),
                description: "MxBAI Embed Large: High-quality symmetric embeddings (1024d)"
                    .to_string(),
            },
        );

        // =================================================================
        // all-MiniLM Family (Symmetric)
        // =================================================================

        models.insert(
            "all-minilm".to_string(),
            EmbeddingModelProfile {
                name: "all-minilm".to_string(),
                dimension: 384,
                symmetry: EmbeddingSymmetry::Symmetric,
                query_prefix: None,
                passage_prefix: None,
                max_tokens: 256,
                family: "minilm".to_string(),
                description: "all-MiniLM: Fast lightweight embeddings (384d)".to_string(),
            },
        );

        Self { models }
    }

    /// Get a model profile by name.
    pub fn get(&self, model_name: &str) -> Option<&EmbeddingModelProfile> {
        self.models.get(model_name)
    }

    /// Get all known model names.
    pub fn model_names(&self) -> Vec<&str> {
        self.models.keys().map(|s| s.as_str()).collect()
    }

    /// Get all E5 models.
    pub fn e5_models(&self) -> Vec<&EmbeddingModelProfile> {
        self.models.values().filter(|m| m.family == "e5").collect()
    }

    /// Get all asymmetric models.
    pub fn asymmetric_models(&self) -> Vec<&EmbeddingModelProfile> {
        self.models.values().filter(|m| m.is_asymmetric()).collect()
    }

    /// Get the total number of known models.
    pub fn count(&self) -> usize {
        self.models.len()
    }

    /// Look up a model, falling back to a default symmetric profile for unknown models.
    pub fn get_or_default(&self, model_name: &str) -> EmbeddingModelProfile {
        match self.get(model_name) {
            Some(profile) => profile.clone(),
            None => EmbeddingModelProfile {
                name: model_name.to_string(),
                dimension: 768, // safe default
                symmetry: EmbeddingSymmetry::Symmetric,
                query_prefix: None,
                passage_prefix: None,
                max_tokens: 512,
                family: "unknown".to_string(),
                description: format!("Unknown model: {} (assuming symmetric 768d)", model_name),
            },
        }
    }
}

impl Default for EmbeddingModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // EmbeddingModelProfile Tests
    // =========================================================================

    #[test]
    fn test_e5_is_asymmetric() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        assert!(e5.is_asymmetric());
        assert_eq!(e5.symmetry, EmbeddingSymmetry::Asymmetric);
    }

    #[test]
    fn test_nomic_is_symmetric() {
        let registry = EmbeddingModelRegistry::new();
        let nomic = registry.get("nomic-embed-text").unwrap();
        assert!(!nomic.is_asymmetric());
        assert_eq!(nomic.symmetry, EmbeddingSymmetry::Symmetric);
    }

    #[test]
    fn test_e5_query_prefix() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        assert_eq!(e5.prefix_query("What is Rust?"), "query: What is Rust?");
    }

    #[test]
    fn test_e5_passage_prefix() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        assert_eq!(
            e5.prefix_passage("Rust is a systems programming language."),
            "passage: Rust is a systems programming language."
        );
    }

    #[test]
    fn test_symmetric_model_no_prefix() {
        let registry = EmbeddingModelRegistry::new();
        let nomic = registry.get("nomic-embed-text").unwrap();
        assert_eq!(nomic.prefix_query("What is Rust?"), "What is Rust?");
        assert_eq!(
            nomic.prefix_passage("Rust is a language."),
            "Rust is a language."
        );
    }

    #[test]
    fn test_prefix_queries_batch() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        let queries = vec!["Q1".to_string(), "Q2".to_string()];
        let prefixed = e5.prefix_queries(&queries);
        assert_eq!(prefixed, vec!["query: Q1", "query: Q2"]);
    }

    #[test]
    fn test_prefix_passages_batch() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        let passages = vec!["P1".to_string(), "P2".to_string()];
        let prefixed = e5.prefix_passages(&passages);
        assert_eq!(prefixed, vec!["passage: P1", "passage: P2"]);
    }

    #[test]
    fn test_bge_only_prefixes_queries() {
        let registry = EmbeddingModelRegistry::new();
        let bge = registry.get("bge-base-en-v1.5").unwrap();
        assert!(bge.is_asymmetric());
        // BGE prefixes queries but not passages
        assert!(bge.query_prefix.is_some());
        assert!(bge.passage_prefix.is_none());
        assert_eq!(bge.prefix_passage("Some passage."), "Some passage.");
    }

    // =========================================================================
    // EmbeddingModelRegistry Tests
    // =========================================================================

    #[test]
    fn test_registry_creation() {
        let registry = EmbeddingModelRegistry::new();
        assert!(registry.count() >= 9); // At minimum the 9 models we defined
    }

    #[test]
    fn test_registry_default() {
        let registry = EmbeddingModelRegistry::default();
        assert!(registry.count() >= 9);
    }

    #[test]
    fn test_registry_get_e5_models() {
        let registry = EmbeddingModelRegistry::new();
        let e5_models = registry.e5_models();
        assert!(e5_models.len() >= 4); // small, base, large, multilingual
        for model in e5_models {
            assert_eq!(model.family, "e5");
            assert!(model.is_asymmetric());
        }
    }

    #[test]
    fn test_registry_get_asymmetric_models() {
        let registry = EmbeddingModelRegistry::new();
        let asymmetric = registry.asymmetric_models();
        assert!(!asymmetric.is_empty());
        for model in asymmetric {
            assert!(model.is_asymmetric());
        }
    }

    #[test]
    fn test_registry_get_or_default_known() {
        let registry = EmbeddingModelRegistry::new();
        let profile = registry.get_or_default("e5-base-v2");
        assert_eq!(profile.name, "e5-base-v2");
        assert_eq!(profile.dimension, 768);
        assert!(profile.is_asymmetric());
    }

    #[test]
    fn test_registry_get_or_default_unknown() {
        let registry = EmbeddingModelRegistry::new();
        let profile = registry.get_or_default("unknown-model-xyz");
        assert_eq!(profile.name, "unknown-model-xyz");
        assert_eq!(profile.dimension, 768);
        assert!(!profile.is_asymmetric()); // Default to symmetric
    }

    #[test]
    fn test_model_names() {
        let registry = EmbeddingModelRegistry::new();
        let names = registry.model_names();
        assert!(names.contains(&"e5-base-v2"));
        assert!(names.contains(&"nomic-embed-text"));
        assert!(names.contains(&"mxbai-embed-large"));
    }

    // =========================================================================
    // E5 Model Dimension Tests
    // =========================================================================

    #[test]
    fn test_e5_small_dimension() {
        let registry = EmbeddingModelRegistry::new();
        let model = registry.get("e5-small-v2").unwrap();
        assert_eq!(model.dimension, 384);
    }

    #[test]
    fn test_e5_base_dimension() {
        let registry = EmbeddingModelRegistry::new();
        let model = registry.get("e5-base-v2").unwrap();
        assert_eq!(model.dimension, 768);
    }

    #[test]
    fn test_e5_large_dimension() {
        let registry = EmbeddingModelRegistry::new();
        let model = registry.get("e5-large-v2").unwrap();
        assert_eq!(model.dimension, 1024);
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_symmetry_serialization() {
        let sym = EmbeddingSymmetry::Symmetric;
        let json = serde_json::to_string(&sym).unwrap();
        assert_eq!(json, "\"symmetric\"");

        let asym = EmbeddingSymmetry::Asymmetric;
        let json = serde_json::to_string(&asym).unwrap();
        assert_eq!(json, "\"asymmetric\"");
    }

    #[test]
    fn test_model_profile_serialization() {
        let registry = EmbeddingModelRegistry::new();
        let e5 = registry.get("e5-base-v2").unwrap();
        let json = serde_json::to_string(e5).unwrap();
        let parsed: EmbeddingModelProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "e5-base-v2");
        assert_eq!(parsed.dimension, 768);
        assert!(parsed.is_asymmetric());
        assert_eq!(parsed.query_prefix, Some("query: ".to_string()));
    }
}

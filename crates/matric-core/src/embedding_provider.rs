//! Embedding provider types and configuration for dynamic embedding generation.

use crate::models::DocumentComposition;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Embedding provider for generating embeddings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    /// Local Ollama instance (default)
    #[default]
    Ollama,
    /// OpenAI API
    OpenAI,
    /// Voyage AI
    Voyage,
    /// Cohere API
    Cohere,
    /// Custom HTTP endpoint
    Custom,
}

impl std::fmt::Display for EmbeddingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Voyage => write!(f, "voyage"),
            Self::Cohere => write!(f, "cohere"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl std::str::FromStr for EmbeddingProvider {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAI),
            "voyage" => Ok(Self::Voyage),
            "cohere" => Ok(Self::Cohere),
            "custom" => Ok(Self::Custom),
            _ => Err(format!("Invalid embedding provider: {}", s)),
        }
    }
}

fn default_chunk_size() -> i32 {
    1000
}

fn default_chunk_overlap() -> i32 {
    100
}

/// Request to create a new embedding config.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateEmbeddingConfigRequest {
    pub name: String,
    pub description: Option<String>,
    pub model: String,
    pub dimension: i32,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: i32,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: i32,

    #[serde(default)]
    pub provider: EmbeddingProvider,
    #[serde(default)]
    pub provider_config: JsonValue,

    #[serde(default)]
    pub supports_mrl: bool,
    pub matryoshka_dims: Option<Vec<i32>>,
    pub default_truncate_dim: Option<i32>,

    #[serde(default)]
    pub content_types: Vec<String>,

    pub hnsw_m: Option<i32>,
    pub hnsw_ef_construction: Option<i32>,

    /// Document composition for this config. Defaults to title+content only.
    #[serde(default)]
    pub document_composition: DocumentComposition,
}

/// Request to update an existing embedding config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateEmbeddingConfigRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub model: Option<String>,
    pub dimension: Option<i32>,
    pub chunk_size: Option<i32>,
    pub chunk_overlap: Option<i32>,
    pub provider: Option<EmbeddingProvider>,
    pub provider_config: Option<JsonValue>,
    pub supports_mrl: Option<bool>,
    pub matryoshka_dims: Option<Vec<i32>>,
    pub default_truncate_dim: Option<i32>,
    pub content_types: Option<Vec<String>>,
    pub hnsw_m: Option<i32>,
    pub hnsw_ef_construction: Option<i32>,

    /// Document composition override. If `None`, composition is not changed.
    pub document_composition: Option<DocumentComposition>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_provider_display() {
        assert_eq!(EmbeddingProvider::Ollama.to_string(), "ollama");
        assert_eq!(EmbeddingProvider::OpenAI.to_string(), "openai");
        assert_eq!(EmbeddingProvider::Voyage.to_string(), "voyage");
        assert_eq!(EmbeddingProvider::Cohere.to_string(), "cohere");
        assert_eq!(EmbeddingProvider::Custom.to_string(), "custom");
    }

    #[test]
    fn test_embedding_provider_from_str() {
        assert_eq!(
            "ollama".parse::<EmbeddingProvider>().unwrap(),
            EmbeddingProvider::Ollama
        );
        assert_eq!(
            "OPENAI".parse::<EmbeddingProvider>().unwrap(),
            EmbeddingProvider::OpenAI
        );
        assert_eq!(
            "voyage".parse::<EmbeddingProvider>().unwrap(),
            EmbeddingProvider::Voyage
        );
        assert_eq!(
            "cohere".parse::<EmbeddingProvider>().unwrap(),
            EmbeddingProvider::Cohere
        );
        assert_eq!(
            "custom".parse::<EmbeddingProvider>().unwrap(),
            EmbeddingProvider::Custom
        );

        let result = "invalid".parse::<EmbeddingProvider>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid embedding provider"));
    }

    #[test]
    fn test_embedding_provider_default() {
        assert_eq!(EmbeddingProvider::default(), EmbeddingProvider::Ollama);
    }

    #[test]
    fn test_embedding_provider_serialization() {
        let providers = vec![
            (EmbeddingProvider::Ollama, "ollama"),
            (EmbeddingProvider::OpenAI, "openai"),
            (EmbeddingProvider::Voyage, "voyage"),
            (EmbeddingProvider::Cohere, "cohere"),
            (EmbeddingProvider::Custom, "custom"),
        ];

        for (provider, expected) in providers {
            let json = serde_json::to_string(&provider).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let deserialized: EmbeddingProvider = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, provider);
        }
    }

    #[test]
    fn test_create_embedding_config_request() {
        let request = CreateEmbeddingConfigRequest {
            name: "voyage-code-2".to_string(),
            description: Some("Voyage AI code embedding".to_string()),
            model: "voyage-code-2".to_string(),
            dimension: 1536,
            chunk_size: 512,
            chunk_overlap: 50,
            provider: EmbeddingProvider::Voyage,
            provider_config: serde_json::json!({
                "api_key_env": "VOYAGE_API_KEY",
                "base_url": "https://api.voyageai.com/v1"
            }),
            supports_mrl: false,
            matryoshka_dims: None,
            default_truncate_dim: None,
            content_types: vec!["code".to_string()],
            hnsw_m: Some(16),
            hnsw_ef_construction: Some(200),
            document_composition: DocumentComposition::default(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateEmbeddingConfigRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "voyage-code-2");
        assert_eq!(deserialized.provider, EmbeddingProvider::Voyage);
        assert_eq!(deserialized.content_types, vec!["code"]);
    }
}

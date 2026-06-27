//! Embedding provider types and configuration for dynamic embedding generation.

use crate::models::DocumentComposition;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

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
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for CreateEmbeddingConfigRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateEmbeddingConfigRequest")
            .field("name_len", &self.name.len())
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .field("model_len", &self.model.len())
            .field("dimension", &self.dimension)
            .field("chunk_size", &self.chunk_size)
            .field("chunk_overlap", &self.chunk_overlap)
            .field("provider", &self.provider)
            .field(
                "provider_config_class",
                &json_value_class(&self.provider_config),
            )
            .field(
                "provider_config_len",
                &json_serialized_len(&self.provider_config),
            )
            .field("supports_mrl", &self.supports_mrl)
            .field(
                "matryoshka_dims_count",
                &self.matryoshka_dims.as_ref().map(Vec::len),
            )
            .field("default_truncate_dim", &self.default_truncate_dim)
            .field("content_types_count", &self.content_types.len())
            .field(
                "content_type_lens",
                &self
                    .content_types
                    .iter()
                    .map(|content_type| content_type.len())
                    .collect::<Vec<_>>(),
            )
            .field("hnsw_m", &self.hnsw_m)
            .field("hnsw_ef_construction", &self.hnsw_ef_construction)
            .field("document_composition_set", &true)
            .finish()
    }
}

/// Request to update an existing embedding config.
#[derive(Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for UpdateEmbeddingConfigRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateEmbeddingConfigRequest")
            .field("name_len", &self.name.as_ref().map(String::len))
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .field("model_len", &self.model.as_ref().map(String::len))
            .field("dimension", &self.dimension)
            .field("chunk_size", &self.chunk_size)
            .field("chunk_overlap", &self.chunk_overlap)
            .field("provider", &self.provider)
            .field(
                "provider_config_class",
                &self.provider_config.as_ref().map(json_value_class),
            )
            .field(
                "provider_config_len",
                &self.provider_config.as_ref().map(json_serialized_len),
            )
            .field("supports_mrl", &self.supports_mrl)
            .field(
                "matryoshka_dims_count",
                &self.matryoshka_dims.as_ref().map(Vec::len),
            )
            .field("default_truncate_dim", &self.default_truncate_dim)
            .field(
                "content_types_count",
                &self.content_types.as_ref().map(Vec::len),
            )
            .field(
                "content_type_lens",
                &self.content_types.as_ref().map(|content_types| {
                    content_types
                        .iter()
                        .map(|content_type| content_type.len())
                        .collect::<Vec<_>>()
                }),
            )
            .field("hnsw_m", &self.hnsw_m)
            .field("hnsw_ef_construction", &self.hnsw_ef_construction)
            .field(
                "document_composition_set",
                &self.document_composition.is_some(),
            )
            .finish()
    }
}

fn json_value_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn json_serialized_len(value: &JsonValue) -> usize {
    serde_json::to_string(value)
        .map(|json| json.len())
        .unwrap_or(0)
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

    #[test]
    fn embedding_config_request_debug_redacts_provider_config_and_identifiers() {
        let request = CreateEmbeddingConfigRequest {
            name: "private-config@example.test".to_string(),
            description: Some("private description with /tmp/path".to_string()),
            model: "private-model-sk-live-secret".to_string(),
            dimension: 1536,
            chunk_size: 800,
            chunk_overlap: 80,
            provider: EmbeddingProvider::Custom,
            provider_config: serde_json::json!({
                "base_url": "https://example.test/embed?token=secret",
                "api_key": "sk-live-secret",
                "headers": { "authorization": "Bearer secret" }
            }),
            supports_mrl: true,
            matryoshka_dims: Some(vec![256, 512, 1024]),
            default_truncate_dim: Some(512),
            content_types: vec![
                "text/private-sk-live-secret".to_string(),
                "application/private@example.test".to_string(),
            ],
            hnsw_m: Some(16),
            hnsw_ef_construction: Some(200),
            document_composition: DocumentComposition::default(),
        };

        let update = UpdateEmbeddingConfigRequest {
            name: Some("updated-private@example.test".to_string()),
            description: Some("updated private description".to_string()),
            model: Some("updated-model-sk-live-secret".to_string()),
            provider_config: Some(serde_json::json!({
                "base_url": "https://example.test/updated?token=secret",
                "api_key": "sk-live-updated"
            })),
            content_types: Some(vec!["updated/private@example.test".to_string()]),
            document_composition: Some(DocumentComposition::default()),
            ..Default::default()
        };

        let debug = format!("{request:?}\n{update:?}");

        for secret in [
            "private-config@example.test",
            "private description",
            "/tmp/path",
            "private-model-sk-live-secret",
            "https://example.test/embed?token=secret",
            "https://example.test/updated?token=secret",
            "token=secret",
            "api_key",
            "sk-live-secret",
            "sk-live-updated",
            "Bearer secret",
            "text/private-sk-live-secret",
            "application/private@example.test",
            "updated-private@example.test",
            "updated-model-sk-live-secret",
            "updated/private@example.test",
        ] {
            assert!(
                !debug.contains(secret),
                "embedding config request Debug output leaked sensitive value {secret:?}: {debug}"
            );
        }

        for expected in [
            "name_len",
            "description_len",
            "model_len",
            "provider_config_class",
            "provider_config_len",
            "content_types_count",
            "content_type_lens",
            "document_composition_set",
        ] {
            assert!(
                debug.contains(expected),
                "embedding config request Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }
}

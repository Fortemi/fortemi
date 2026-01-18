//! Ollama inference backend implementation.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

use matric_core::{EmbeddingBackend, Error, GenerationBackend, InferenceBackend, Result, Vector};

/// Default Ollama endpoint.
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// Default embedding model.
pub const DEFAULT_EMBED_MODEL: &str = "nomic-embed-text";

/// Default generation model.
pub const DEFAULT_GEN_MODEL: &str = "gpt-oss:20b";

/// Default embedding dimension for nomic-embed-text.
pub const DEFAULT_DIMENSION: usize = 768;

/// Timeout for embedding requests (seconds).
pub const EMBED_TIMEOUT_SECS: u64 = 30;

/// Timeout for generation requests (seconds).
pub const GEN_TIMEOUT_SECS: u64 = 120;

/// Ollama inference backend.
pub struct OllamaBackend {
    client: Client,
    base_url: String,
    embed_model: String,
    gen_model: String,
    dimension: usize,
}

impl OllamaBackend {
    /// Create a new Ollama backend with default settings.
    pub fn new() -> Self {
        Self::with_config(
            DEFAULT_OLLAMA_URL.to_string(),
            DEFAULT_EMBED_MODEL.to_string(),
            DEFAULT_GEN_MODEL.to_string(),
            DEFAULT_DIMENSION,
        )
    }

    /// Create a new Ollama backend with custom configuration.
    pub fn with_config(
        base_url: String,
        embed_model: String,
        gen_model: String,
        dimension: usize,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(GEN_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        info!(
            "Initializing Ollama backend: url={}, embed={}, gen={}",
            base_url, embed_model, gen_model
        );

        Self {
            client,
            base_url,
            embed_model,
            gen_model,
            dimension,
        }
    }

    /// Create from environment variables.
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("OLLAMA_BASE").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
        let embed_model =
            std::env::var("OLLAMA_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_EMBED_MODEL.to_string());
        let gen_model =
            std::env::var("OLLAMA_GEN_MODEL").unwrap_or_else(|_| DEFAULT_GEN_MODEL.to_string());
        let dimension = std::env::var("OLLAMA_EMBED_DIM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DIMENSION);

        Self::with_config(base_url, embed_model, gen_model, dimension)
    }
}

impl Default for OllamaBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[async_trait]
impl EmbeddingBackend for OllamaBackend {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vector>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        debug!(
            "Embedding {} texts with model {}",
            texts.len(),
            self.embed_model
        );

        let request = EmbeddingRequest {
            model: self.embed_model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .timeout(Duration::from_secs(EMBED_TIMEOUT_SECS))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embedding(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Embedding(format!(
                "Ollama returned {}: {}",
                status, body
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| Error::Embedding(format!("Failed to parse response: {}", e)))?;

        let vectors: Vec<Vector> = result.embeddings.into_iter().map(Vector::from).collect();

        debug!("Generated {} embeddings", vectors.len());
        Ok(vectors)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.embed_model
    }
}

#[async_trait]
impl GenerationBackend for OllamaBackend {
    async fn generate(&self, prompt: &str) -> Result<String> {
        self.generate_with_system("", prompt).await
    }

    async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        debug!(
            "Generating with model {}, prompt length: {}",
            self.gen_model,
            prompt.len()
        );

        let request = GenerateRequest {
            model: self.gen_model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            system: if system.is_empty() {
                None
            } else {
                Some(system.to_string())
            },
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .timeout(Duration::from_secs(GEN_TIMEOUT_SECS))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Inference(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Inference(format!(
                "Ollama returned {}: {}",
                status, body
            )));
        }

        let result: GenerateResponse = response
            .json()
            .await
            .map_err(|e| Error::Inference(format!("Failed to parse response: {}", e)))?;

        debug!(
            "Generation complete, response length: {}",
            result.response.len()
        );
        Ok(result.response)
    }

    fn model_name(&self) -> &str {
        &self.gen_model
    }
}

#[async_trait]
impl InferenceBackend for OllamaBackend {
    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    info!("Ollama health check passed");
                    Ok(true)
                } else {
                    warn!("Ollama health check failed: {}", resp.status());
                    Ok(false)
                }
            }
            Err(e) => {
                warn!("Ollama health check error: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Constants Tests
    // ==========================================================================

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_OLLAMA_URL, "http://127.0.0.1:11434");
        assert_eq!(DEFAULT_EMBED_MODEL, "nomic-embed-text");
        assert_eq!(DEFAULT_GEN_MODEL, "gpt-oss:20b");
        assert_eq!(DEFAULT_DIMENSION, 768);
        assert_eq!(EMBED_TIMEOUT_SECS, 30);
        assert_eq!(GEN_TIMEOUT_SECS, 120);
    }

    #[test]
    fn test_default_url_is_localhost() {
        assert!(DEFAULT_OLLAMA_URL.contains("127.0.0.1"));
    }

    #[test]
    fn test_default_dimension_is_standard() {
        // 768 is standard for many embedding models
        let valid_dims = [384, 768, 1536];
        assert!(
            valid_dims.contains(&DEFAULT_DIMENSION),
            "DEFAULT_DIMENSION {} should be a standard embedding dimension",
            DEFAULT_DIMENSION
        );
    }

    // ==========================================================================
    // Backend Configuration Tests
    // ==========================================================================

    #[test]
    fn test_default_config() {
        let backend = OllamaBackend::new();
        assert_eq!(backend.base_url, DEFAULT_OLLAMA_URL);
        assert_eq!(backend.embed_model, DEFAULT_EMBED_MODEL);
        assert_eq!(backend.gen_model, DEFAULT_GEN_MODEL);
        assert_eq!(backend.dimension, DEFAULT_DIMENSION);
    }

    #[test]
    fn test_from_env_defaults() {
        // Clear env vars to test defaults
        std::env::remove_var("OLLAMA_BASE");
        std::env::remove_var("OLLAMA_EMBED_MODEL");
        std::env::remove_var("OLLAMA_GEN_MODEL");
        std::env::remove_var("OLLAMA_EMBED_DIM");

        let backend = OllamaBackend::from_env();
        assert_eq!(backend.base_url, DEFAULT_OLLAMA_URL);
        assert_eq!(backend.embed_model, DEFAULT_EMBED_MODEL);
        assert_eq!(backend.gen_model, DEFAULT_GEN_MODEL);
        assert_eq!(backend.dimension, DEFAULT_DIMENSION);
    }

    #[test]
    fn test_custom_config() {
        let backend = OllamaBackend::with_config(
            "http://custom:1234".to_string(),
            "custom-embed".to_string(),
            "custom-gen".to_string(),
            512,
        );
        assert_eq!(backend.base_url, "http://custom:1234");
        assert_eq!(backend.embed_model, "custom-embed");
        assert_eq!(backend.gen_model, "custom-gen");
        assert_eq!(backend.dimension, 512);
    }

    #[test]
    fn test_custom_config_with_https() {
        let backend = OllamaBackend::with_config(
            "https://remote-ollama.example.com".to_string(),
            "mxbai-embed-large".to_string(),
            "llama3".to_string(),
            1024,
        );
        assert_eq!(backend.base_url, "https://remote-ollama.example.com");
        assert_eq!(backend.dimension, 1024);
    }

    #[test]
    fn test_default_impl() {
        let backend = OllamaBackend::default();
        assert_eq!(backend.base_url, DEFAULT_OLLAMA_URL);
        assert_eq!(backend.embed_model, DEFAULT_EMBED_MODEL);
    }

    // ==========================================================================
    // Accessor Tests
    // ==========================================================================

    #[test]
    fn test_dimension_accessor() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "model".to_string(),
            "gen".to_string(),
            384,
        );
        assert_eq!(backend.dimension(), 384);
    }

    #[test]
    fn test_model_name_accessor() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "my-embed-model".to_string(),
            "my-gen-model".to_string(),
            768,
        );
        assert_eq!(EmbeddingBackend::model_name(&backend), "my-embed-model");
        assert_eq!(GenerationBackend::model_name(&backend), "my-gen-model");
    }

    // ==========================================================================
    // Request/Response Struct Tests
    // ==========================================================================

    #[test]
    fn test_embedding_request_serialization() {
        let request = EmbeddingRequest {
            model: "test-model".to_string(),
            input: vec!["hello".to_string(), "world".to_string()],
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test-model"));
        assert!(json.contains("hello"));
        assert!(json.contains("world"));
    }

    #[test]
    fn test_embedding_response_deserialization() {
        let json = r#"{"embeddings": [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]}"#;
        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0], vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn test_generate_request_serialization() {
        let request = GenerateRequest {
            model: "llama3".to_string(),
            prompt: "Hello".to_string(),
            stream: false,
            system: Some("Be helpful".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("llama3"));
        assert!(json.contains("Hello"));
        assert!(json.contains("Be helpful"));
    }

    #[test]
    fn test_generate_request_without_system() {
        let request = GenerateRequest {
            model: "llama3".to_string(),
            prompt: "Hello".to_string(),
            stream: false,
            system: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("system")); // Should skip serializing None
    }

    #[test]
    fn test_generate_response_deserialization() {
        let json = r#"{"response": "Hello there!"}"#;
        let response: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.response, "Hello there!");
    }

    // ==========================================================================
    // Edge Cases
    // ==========================================================================

    #[test]
    fn test_zero_dimension_config() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "model".to_string(),
            "gen".to_string(),
            0,
        );
        assert_eq!(backend.dimension(), 0);
    }

    #[test]
    fn test_large_dimension_config() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "model".to_string(),
            "gen".to_string(),
            4096,
        );
        assert_eq!(backend.dimension(), 4096);
    }

    #[test]
    fn test_empty_model_names() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "".to_string(),
            "".to_string(),
            768,
        );
        assert_eq!(backend.embed_model, "");
        assert_eq!(backend.gen_model, "");
    }

    #[test]
    fn test_special_characters_in_url() {
        let backend = OllamaBackend::with_config(
            "http://user:pass@host:1234/path?query=value".to_string(),
            "model".to_string(),
            "gen".to_string(),
            768,
        );
        assert_eq!(
            backend.base_url,
            "http://user:pass@host:1234/path?query=value"
        );
    }
}

/// Integration tests that require a live Ollama server.
/// Run with: cargo test --package matric-inference --features integration
#[cfg(all(test, feature = "integration"))]
mod integration_tests {
    use super::*;

    fn get_backend() -> OllamaBackend {
        OllamaBackend::from_env()
    }

    #[tokio::test]
    async fn test_health_check() {
        let backend = get_backend();
        let healthy = backend.health_check().await.expect("health check failed");
        assert!(healthy, "Ollama should be healthy and reachable");
    }

    #[tokio::test]
    async fn test_embed_single_text() {
        let backend = get_backend();

        let texts = vec!["Hello, this is a test sentence for embedding.".to_string()];
        let vectors = backend.embed_texts(&texts).await.expect("embedding failed");

        assert_eq!(vectors.len(), 1, "Should return one vector");
        let slice = vectors[0].as_slice();
        assert_eq!(
            slice.len(),
            backend.dimension(),
            "Vector dimension should match model dimension"
        );

        // Check vector is normalized (approximately unit length for cosine similarity)
        let magnitude: f32 = slice.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (0.9..=1.1).contains(&magnitude),
            "Vector should be approximately normalized, got {}",
            magnitude
        );
    }

    #[tokio::test]
    async fn test_embed_multiple_texts() {
        let backend = get_backend();

        let texts = vec![
            "First document about programming.".to_string(),
            "Second document about cooking.".to_string(),
            "Third document about music.".to_string(),
        ];
        let vectors = backend.embed_texts(&texts).await.expect("embedding failed");

        assert_eq!(vectors.len(), 3, "Should return three vectors");

        // All vectors should have same dimension
        for (i, v) in vectors.iter().enumerate() {
            assert_eq!(
                v.as_slice().len(),
                backend.dimension(),
                "Vector {} should have correct dimension",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_embed_empty_list() {
        let backend = get_backend();

        let texts: Vec<String> = vec![];
        let vectors = backend.embed_texts(&texts).await.expect("embedding failed");

        assert!(vectors.is_empty(), "Empty input should return empty output");
    }

    #[tokio::test]
    async fn test_semantic_similarity() {
        let backend = get_backend();

        let texts = vec![
            "The quick brown fox jumps over the lazy dog.".to_string(),
            "A fast auburn fox leaps above a sleepy canine.".to_string(), // semantically similar
            "Python is a popular programming language.".to_string(),      // semantically different
        ];
        let vectors = backend.embed_texts(&texts).await.expect("embedding failed");

        // Calculate cosine similarities
        let sim_similar = cosine_similarity(vectors[0].as_slice(), vectors[1].as_slice());
        let sim_different = cosine_similarity(vectors[0].as_slice(), vectors[2].as_slice());

        assert!(
            sim_similar > sim_different,
            "Similar sentences should have higher similarity ({}) than different ones ({})",
            sim_similar,
            sim_different
        );
    }

    #[tokio::test]
    async fn test_generate_simple() {
        let backend = get_backend();

        let response = backend
            .generate("Say 'hello' and nothing else.")
            .await
            .expect("generation failed");

        assert!(!response.is_empty(), "Response should not be empty");
        // The model should respond with something containing "hello"
        assert!(
            response.to_lowercase().contains("hello"),
            "Response should contain 'hello', got: {}",
            response
        );
    }

    #[tokio::test]
    async fn test_generate_with_system() {
        let backend = get_backend();

        let response = backend
            .generate_with_system(
                "You are a helpful assistant that only responds with single words.",
                "What is 2+2?",
            )
            .await
            .expect("generation failed");

        assert!(!response.is_empty(), "Response should not be empty");
        // Should contain "4" or "four" somewhere
        let lower = response.to_lowercase();
        assert!(
            lower.contains("4") || lower.contains("four"),
            "Response should contain the answer, got: {}",
            response
        );
    }

    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (mag_a * mag_b)
    }
}

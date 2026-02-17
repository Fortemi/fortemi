//! Ollama inference backend implementation.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, warn};

use matric_core::{EmbeddingBackend, Error, GenerationBackend, InferenceBackend, Result, Vector};

use crate::embedding_models::{EmbeddingModelProfile, EmbeddingModelRegistry};
// requires_raw_mode is tested below but no longer used in generate_internal (switched to chat API).
#[cfg(test)]
use crate::model_config::requires_raw_mode;
use crate::profiles::{ModelProfile, ModelRegistry};

/// Default Ollama endpoint.
pub const DEFAULT_OLLAMA_URL: &str = matric_core::defaults::OLLAMA_URL;

/// Default embedding model.
pub const DEFAULT_EMBED_MODEL: &str = matric_core::defaults::EMBED_MODEL;

/// Default generation model.
pub const DEFAULT_GEN_MODEL: &str = matric_core::defaults::GEN_MODEL;

/// Default embedding dimension for nomic-embed-text.
pub const DEFAULT_DIMENSION: usize = matric_core::defaults::EMBED_DIMENSION;

/// Timeout for embedding requests (seconds).
pub const EMBED_TIMEOUT_SECS: u64 = matric_core::defaults::EMBED_TIMEOUT_SECS;

/// Timeout for generation requests (seconds).
pub const GEN_TIMEOUT_SECS: u64 = matric_core::defaults::GEN_TIMEOUT_SECS;

/// Ollama inference backend.
pub struct OllamaBackend {
    client: Client,
    base_url: String,
    embed_model: String,
    gen_model: String,
    dimension: usize,
    registry: ModelRegistry,
    embed_registry: EmbeddingModelRegistry,
    embed_profile: EmbeddingModelProfile,
    embed_timeout_secs: u64,
    gen_timeout_secs: u64,
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
        let gen_timeout = std::env::var("MATRIC_GEN_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(matric_core::defaults::GEN_TIMEOUT_SECS);

        let embed_timeout = std::env::var("MATRIC_EMBED_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(matric_core::defaults::EMBED_TIMEOUT_SECS);

        let client = Client::builder()
            .timeout(Duration::from_secs(gen_timeout))
            .build()
            .expect("Failed to create HTTP client");

        info!(
            "Initializing Ollama backend: url={}, embed={}, gen={}",
            base_url, embed_model, gen_model
        );

        let embed_registry = EmbeddingModelRegistry::new();
        let embed_profile = embed_registry.get_or_default(&embed_model);

        if embed_profile.is_asymmetric() {
            info!(
                "Embedding model {} uses asymmetric prefixes (query/passage)",
                embed_model
            );
        }

        Self {
            client,
            base_url,
            embed_model,
            gen_model,
            dimension,
            registry: ModelRegistry::new(),
            embed_registry,
            embed_profile,
            embed_timeout_secs: embed_timeout,
            gen_timeout_secs: gen_timeout,
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

    /// Create from environment variables with a specific generation model override.
    pub fn from_env_with_gen_model(gen_model: String) -> Self {
        let base_url =
            std::env::var("OLLAMA_BASE").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
        let embed_model =
            std::env::var("OLLAMA_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_EMBED_MODEL.to_string());
        let dimension = std::env::var("OLLAMA_EMBED_DIM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DIMENSION);

        Self::with_config(base_url, embed_model, gen_model, dimension)
    }

    /// Create a fast generation backend for the extraction pipeline.
    ///
    /// Model resolution: `MATRIC_FAST_GEN_MODEL` env var → default `granite4:3b`.
    /// Set `MATRIC_FAST_GEN_MODEL=""` (empty) to explicitly disable.
    /// Timeout: `MATRIC_FAST_GEN_TIMEOUT_SECS` env var → default 30s.
    pub fn fast_from_env() -> Option<Self> {
        let model = match std::env::var("MATRIC_FAST_GEN_MODEL") {
            Ok(val) if val.is_empty() => return None, // Explicitly disabled
            Ok(val) => val,
            Err(_) => matric_core::defaults::FAST_GEN_MODEL.to_string(), // Default
        };

        let mut backend = Self::from_env_with_gen_model(model);

        // Override generation timeout with fast-specific value
        let fast_timeout = std::env::var("MATRIC_FAST_GEN_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(matric_core::defaults::FAST_GEN_TIMEOUT_SECS);
        backend.gen_timeout_secs = fast_timeout;

        info!(
            "Fast model backend: model={}, timeout={}s",
            backend.gen_model, backend.gen_timeout_secs
        );
        Some(backend)
    }

    /// Get the model registry.
    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    /// Get the profile for the current generation model.
    pub fn gen_model_profile(&self) -> Option<&ModelProfile> {
        self.registry.get(&self.gen_model)
    }

    /// Set the generation model to use.
    pub fn set_gen_model(&mut self, model_name: String) {
        info!(
            "Switching generation model from {} to {}",
            self.gen_model, model_name
        );
        self.gen_model = model_name;
    }

    /// Set generation model to the best model for general inference.
    pub fn use_best_general(&mut self) {
        if let Some(profile) = self.registry.get_best_general() {
            self.set_gen_model(profile.name.clone());
        }
    }

    /// Set generation model to the best model for fast queries.
    pub fn use_best_fast(&mut self) {
        if let Some(profile) = self.registry.get_best_fast() {
            self.set_gen_model(profile.name.clone());
        }
    }

    /// Set generation model to the best model for code generation.
    pub fn use_best_code(&mut self) {
        if let Some(profile) = self.registry.get_best_code() {
            self.set_gen_model(profile.name.clone());
        }
    }

    /// Set generation model to the best model for reasoning/thinking tasks.
    pub fn use_best_reasoning(&mut self) {
        if let Some(profile) = self.registry.get_best_reasoning() {
            self.set_gen_model(profile.name.clone());
        }
    }

    /// Set generation model to the best model for long documents.
    pub fn use_best_long_context(&mut self) {
        if let Some(profile) = self.registry.get_best_long_context() {
            self.set_gen_model(profile.name.clone());
        }
    }

    // ========================================================================
    // E5 / Asymmetric Embedding Support
    // ========================================================================

    /// Get the embedding model profile (includes prefix configuration).
    pub fn embed_model_profile(&self) -> &EmbeddingModelProfile {
        &self.embed_profile
    }

    /// Get the embedding model registry.
    pub fn embed_registry(&self) -> &EmbeddingModelRegistry {
        &self.embed_registry
    }

    /// Returns true if the current embedding model uses asymmetric prefixes.
    pub fn uses_asymmetric_embeddings(&self) -> bool {
        self.embed_profile.is_asymmetric()
    }

    /// Embed texts as **queries** (applies "query: " prefix for E5/asymmetric models).
    ///
    /// Use this when embedding search queries or questions.
    /// For symmetric models, this is identical to `embed_texts`.
    pub async fn embed_queries(&self, texts: &[String]) -> Result<Vec<Vector>> {
        let prefixed = self.embed_profile.prefix_queries(texts);
        self.embed_texts(&prefixed).await
    }

    /// Embed texts as **passages** (applies "passage: " prefix for E5/asymmetric models).
    ///
    /// Use this when embedding documents/notes for storage and indexing.
    /// For symmetric models, this is identical to `embed_texts`.
    pub async fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vector>> {
        let prefixed = self.embed_profile.prefix_passages(texts);
        self.embed_texts(&prefixed).await
    }

    /// Set the embedding model, updating the profile accordingly.
    pub fn set_embed_model(&mut self, model_name: String) {
        info!(
            "Switching embedding model from {} to {}",
            self.embed_model, model_name
        );
        self.embed_profile = self.embed_registry.get_or_default(&model_name);
        self.embed_model = model_name;

        if self.embed_profile.is_asymmetric() {
            info!("New embedding model uses asymmetric prefixes (query/passage)");
        }
    }

    /// Internal generation method shared by all generate variants.
    ///
    /// Uses the `/api/chat` endpoint which properly separates thinking/reasoning
    /// from the final response content. This is essential for thinking models
    /// (e.g., gpt-oss, qwen3) where `/api/generate` leaks reasoning into the response.
    async fn generate_internal(
        &self,
        system: &str,
        prompt: &str,
        format: Option<serde_json::Value>,
    ) -> Result<String> {
        let start = Instant::now();

        debug!(
            json_format = format.is_some(),
            "Starting generation via chat API"
        );

        let mut messages = Vec::new();
        if !system.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system.to_string(),
            });
        }
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let think = if format.is_some() { Some(false) } else { None };
        let request = ChatRequest {
            model: self.gen_model.clone(),
            messages,
            stream: false,
            format,
            think,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .timeout(Duration::from_secs(self.gen_timeout_secs))
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

        let result: ChatResponse = response
            .json()
            .await
            .map_err(|e| Error::Inference(format!("Failed to parse response: {}", e)))?;

        let content = result.message.content;
        let elapsed = start.elapsed().as_millis() as u64;
        debug!(
            response_len = content.len(),
            duration_ms = elapsed,
            "Generation complete"
        );
        if elapsed > 30000 {
            warn!(
                duration_ms = elapsed,
                prompt_len = prompt.len(),
                slow = true,
                "Slow generation operation"
            );
        }
        Ok(content)
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

/// Chat API message for `/api/chat`.
#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Request payload for the Ollama `/api/chat` endpoint.
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    /// Ollama format enforcement. Set to `"json"` for guaranteed valid JSON output.
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
    /// Disable thinking/reasoning for models that support it (e.g., gpt-oss, qwen3).
    /// When `false`, suppresses chain-of-thought reasoning in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
}

/// Response from the Ollama `/api/chat` endpoint.
#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[async_trait]
impl EmbeddingBackend for OllamaBackend {
    #[instrument(skip(self, texts), fields(subsystem = "inference", component = "ollama", op = "embed_texts", model = %self.embed_model, input_count = texts.len()))]
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vector>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let start = Instant::now();

        let request = EmbeddingRequest {
            model: self.embed_model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .timeout(Duration::from_secs(self.embed_timeout_secs))
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
        let elapsed = start.elapsed().as_millis() as u64;

        debug!(
            result_count = vectors.len(),
            duration_ms = elapsed,
            "Embedding complete"
        );
        if elapsed > 5000 {
            warn!(
                duration_ms = elapsed,
                input_count = texts.len(),
                slow = true,
                "Slow embedding operation"
            );
        }
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

    #[instrument(skip(self, system, prompt), fields(subsystem = "inference", component = "ollama", op = "generate", model = %self.gen_model, prompt_len = prompt.len()))]
    async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        self.generate_internal(system, prompt, None).await
    }

    async fn generate_json(&self, prompt: &str) -> Result<String> {
        self.generate_json_with_system("", prompt).await
    }

    #[instrument(skip(self, system, prompt), fields(subsystem = "inference", component = "ollama", op = "generate_json", model = %self.gen_model, prompt_len = prompt.len()))]
    async fn generate_json_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        self.generate_internal(system, prompt, Some(serde_json::json!("json")))
            .await
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
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            model: "llama3".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "Be helpful".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
            ],
            stream: false,
            format: None,
            think: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("llama3"));
        assert!(json.contains("Hello"));
        assert!(json.contains("Be helpful"));
        assert!(json.contains("\"role\":\"system\""));
        assert!(json.contains("\"role\":\"user\""));
        assert!(!json.contains("format")); // Should not serialize None
        assert!(!json.contains("think")); // Should not serialize None
    }

    #[test]
    fn test_chat_request_with_json_format() {
        let request = ChatRequest {
            model: "llama3".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Output JSON".to_string(),
            }],
            stream: false,
            format: Some(serde_json::json!("json")),
            think: Some(false),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"format\":\"json\""));
        assert!(json.contains("\"think\":false"));
    }

    #[test]
    fn test_chat_request_without_format() {
        let request = ChatRequest {
            model: "llama3".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            stream: false,
            format: None,
            think: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("format")); // Should not serialize None
        assert!(!json.contains("think")); // Should not serialize None
    }

    #[test]
    fn test_chat_response_deserialization() {
        let json = r#"{"message": {"role": "assistant", "content": "Hello there!"}, "done": true}"#;
        let response: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.message.content, "Hello there!");
        assert_eq!(response.message.role, "assistant");
    }

    // ==========================================================================
    // Raw Mode Configuration Tests
    // ==========================================================================

    #[test]
    fn test_raw_mode_applied_for_thinking_models() {
        // Test that thinking models would trigger raw mode
        // This tests the configuration logic
        assert!(requires_raw_mode("deepseek-r1:14b"));
        assert!(requires_raw_mode("Mistral-Nemo-12B-Thinking"));
    }

    #[test]
    fn test_raw_mode_not_applied_for_regular_models() {
        // Test that regular models don't trigger raw mode
        assert!(!requires_raw_mode("llama3.1:8b"));
        assert!(!requires_raw_mode("gpt-oss:20b"));
    }

    // ==========================================================================
    // Model Profile Integration Tests
    // ==========================================================================

    #[test]
    fn test_registry_accessor() {
        let backend = OllamaBackend::new();
        let registry = backend.registry();
        assert!(registry.count() > 0);
    }

    #[test]
    fn test_gen_model_profile() {
        let backend = OllamaBackend::new();
        let profile = backend.gen_model_profile();
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().name, DEFAULT_GEN_MODEL);
    }

    #[test]
    fn test_gen_model_profile_custom() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "embed".to_string(),
            "qwen2.5-coder:7b".to_string(),
            768,
        );
        let profile = backend.gen_model_profile();
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().name, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_gen_model_profile_unknown_model() {
        let backend = OllamaBackend::with_config(
            "http://test".to_string(),
            "embed".to_string(),
            "unknown-model".to_string(),
            768,
        );
        let profile = backend.gen_model_profile();
        assert!(profile.is_none());
    }

    #[test]
    fn test_set_gen_model() {
        let mut backend = OllamaBackend::new();
        assert_eq!(backend.gen_model, DEFAULT_GEN_MODEL);

        backend.set_gen_model("qwen2.5-coder:7b".to_string());
        assert_eq!(backend.gen_model, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_use_best_general() {
        let mut backend = OllamaBackend::new();
        backend.use_best_general();
        assert_eq!(backend.gen_model, "gpt-oss:20b");
    }

    #[test]
    fn test_use_best_fast() {
        let mut backend = OllamaBackend::new();
        backend.use_best_fast();
        assert_eq!(backend.gen_model, "qwen2.5-coder:1.5b");
    }

    #[test]
    fn test_use_best_code() {
        let mut backend = OllamaBackend::new();
        backend.use_best_code();
        assert_eq!(backend.gen_model, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_use_best_reasoning() {
        let mut backend = OllamaBackend::new();
        backend.use_best_reasoning();
        assert_eq!(backend.gen_model, "deepseek-r1:14b");
    }

    #[test]
    fn test_use_best_long_context() {
        let mut backend = OllamaBackend::new();
        backend.use_best_long_context();
        assert_eq!(backend.gen_model, "llama3.1:8b");
    }

    #[test]
    fn test_model_switching_preserves_profile_access() {
        let mut backend = OllamaBackend::new();

        backend.use_best_fast();
        let profile = backend.gen_model_profile();
        assert!(profile.is_some());
        assert!(profile.unwrap().is_fast());

        backend.use_best_reasoning();
        let profile = backend.gen_model_profile();
        assert!(profile.is_some());
        assert!(profile.unwrap().is_thinking_model());
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

//! OpenAI-compatible inference backend implementation.

use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, info, warn};

use matric_core::{EmbeddingBackend, Error, GenerationBackend, InferenceBackend, Result, Vector};

use super::streaming::{parse_sse_stream, StreamingGeneration, TokenStream};
use super::types::*;

/// Default OpenAI API endpoint.
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";

/// Default embedding model.
pub const DEFAULT_EMBED_MODEL: &str = "text-embedding-3-small";

/// Default generation model.
pub const DEFAULT_GEN_MODEL: &str = "gpt-4o-mini";

/// Default embedding dimension for text-embedding-3-small.
pub const DEFAULT_DIMENSION: usize = 1536;

/// Default timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Configuration for OpenAI-compatible backend.
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    /// Base URL for the API endpoint.
    pub base_url: String,
    /// API key for authentication (optional for local endpoints).
    pub api_key: Option<String>,
    /// Model to use for embeddings.
    pub embed_model: String,
    /// Model to use for generation.
    pub gen_model: String,
    /// Expected embedding dimension.
    pub embed_dimension: usize,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
    /// Skip TLS verification (for self-signed certs in local environments).
    pub skip_tls_verify: bool,
    /// HTTP-Referer header for OpenRouter.ai rankings (optional).
    pub http_referer: Option<String>,
    /// X-Title header for app name on OpenRouter.ai (optional).
    pub x_title: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OPENAI_URL.to_string(),
            api_key: None,
            embed_model: DEFAULT_EMBED_MODEL.to_string(),
            gen_model: DEFAULT_GEN_MODEL.to_string(),
            embed_dimension: DEFAULT_DIMENSION,
            timeout_seconds: DEFAULT_TIMEOUT_SECS,
            skip_tls_verify: false,
            http_referer: None,
            x_title: None,
        }
    }
}

/// OpenAI-compatible inference backend.
pub struct OpenAIBackend {
    client: Client,
    config: OpenAIConfig,
}

impl OpenAIBackend {
    /// Create a new OpenAI backend with the given configuration.
    pub fn new(config: OpenAIConfig) -> Result<Self> {
        let mut client_builder =
            Client::builder().timeout(Duration::from_secs(config.timeout_seconds));

        if config.skip_tls_verify {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder
            .build()
            .map_err(|e| Error::Inference(format!("Failed to create HTTP client: {}", e)))?;

        info!(
            "Initializing OpenAI backend: url={}, embed={}, gen={}",
            config.base_url, config.embed_model, config.gen_model
        );

        Ok(Self { client, config })
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Result<Self> {
        Self::new(OpenAIConfig::default())
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self> {
        let config = OpenAIConfig {
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_OPENAI_URL.to_string()),
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            embed_model: std::env::var("OPENAI_EMBED_MODEL")
                .unwrap_or_else(|_| DEFAULT_EMBED_MODEL.to_string()),
            gen_model: std::env::var("OPENAI_GEN_MODEL")
                .unwrap_or_else(|_| DEFAULT_GEN_MODEL.to_string()),
            embed_dimension: std::env::var("OPENAI_EMBED_DIM")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_DIMENSION),
            timeout_seconds: std::env::var("OPENAI_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_TIMEOUT_SECS),
            skip_tls_verify: std::env::var("OPENAI_SKIP_TLS_VERIFY")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            http_referer: std::env::var("OPENAI_HTTP_REFERER").ok(),
            x_title: std::env::var("OPENAI_X_TITLE").ok(),
        };

        Self::new(config)
    }

    /// Get the current configuration.
    pub fn config(&self) -> &OpenAIConfig {
        &self.config
    }

    /// Build a request with authentication if configured.
    fn build_request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), endpoint);
        let mut req = self.client.post(&url);

        if let Some(ref api_key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add OpenRouter-specific headers if configured
        if let Some(ref referer) = self.config.http_referer {
            req = req.header("HTTP-Referer", referer);
        }

        if let Some(ref title) = self.config.x_title {
            req = req.header("X-Title", title);
        }

        req.header("Content-Type", "application/json")
    }

    /// Build a GET request with authentication.
    fn build_get_request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), endpoint);
        let mut req = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        req
    }
}

#[async_trait]
impl EmbeddingBackend for OpenAIBackend {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vector>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        debug!(
            "Embedding {} texts with model {}",
            texts.len(),
            self.config.embed_model
        );

        let request = EmbeddingRequest {
            model: self.config.embed_model.clone(),
            input: texts.to_vec(),
            encoding_format: Some("float".to_string()),
        };

        let response = self
            .build_request("/embeddings")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embedding(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body: OpenAIErrorResponse = response.json().await.unwrap_or(OpenAIErrorResponse {
                error: OpenAIError {
                    message: "Unknown error".to_string(),
                    error_type: "unknown".to_string(),
                    code: None,
                },
            });
            return Err(Error::Embedding(format!(
                "OpenAI returned {}: {}",
                status, body.error.message
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| Error::Embedding(format!("Failed to parse response: {}", e)))?;

        // Sort by index to ensure correct ordering
        let mut data = result.data;
        data.sort_by_key(|d| d.index);

        let vectors: Vec<Vector> = data
            .into_iter()
            .map(|d| Vector::from(d.embedding))
            .collect();

        debug!("Generated {} embeddings", vectors.len());
        Ok(vectors)
    }

    fn dimension(&self) -> usize {
        self.config.embed_dimension
    }

    fn model_name(&self) -> &str {
        &self.config.embed_model
    }
}

#[async_trait]
impl GenerationBackend for OpenAIBackend {
    async fn generate(&self, prompt: &str) -> Result<String> {
        self.generate_with_system("", prompt).await
    }

    async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        debug!(
            "Generating with model {}, prompt length: {}",
            self.config.gen_model,
            prompt.len()
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

        let request = ChatCompletionRequest {
            model: self.config.gen_model.clone(),
            messages,
            temperature: None,
            max_tokens: None,
            stream: false,
        };

        let response = self
            .build_request("/chat/completions")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Inference(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body: OpenAIErrorResponse = response.json().await.unwrap_or(OpenAIErrorResponse {
                error: OpenAIError {
                    message: "Unknown error".to_string(),
                    error_type: "unknown".to_string(),
                    code: None,
                },
            });
            return Err(Error::Inference(format!(
                "OpenAI returned {}: {}",
                status, body.error.message
            )));
        }

        let result: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| Error::Inference(format!("Failed to parse response: {}", e)))?;

        let content = result
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        debug!("Generation complete, response length: {}", content.len());
        Ok(content)
    }

    fn model_name(&self) -> &str {
        &self.config.gen_model
    }
}

#[async_trait]
impl InferenceBackend for OpenAIBackend {
    async fn health_check(&self) -> Result<bool> {
        // For OpenAI-compatible APIs, we try a minimal models list request
        let response = self
            .build_get_request("/models")
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    info!("OpenAI health check passed");
                    Ok(true)
                } else {
                    warn!("OpenAI health check failed: {}", resp.status());
                    Ok(false)
                }
            }
            Err(e) => {
                warn!("OpenAI health check error: {}", e);
                Ok(false)
            }
        }
    }
}

#[async_trait]
impl StreamingGeneration for OpenAIBackend {
    async fn generate_stream(&self, prompt: &str) -> Result<TokenStream> {
        self.generate_with_system_stream("", prompt).await
    }

    async fn generate_with_system_stream(&self, system: &str, prompt: &str) -> Result<TokenStream> {
        debug!(
            "Streaming generation with model {}, prompt length: {}",
            self.config.gen_model,
            prompt.len()
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

        let request = ChatCompletionRequest {
            model: self.config.gen_model.clone(),
            messages,
            temperature: None,
            max_tokens: None,
            stream: true,
        };

        let response = self
            .build_request("/chat/completions")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Inference(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body: OpenAIErrorResponse = response.json().await.unwrap_or(OpenAIErrorResponse {
                error: OpenAIError {
                    message: "Unknown error".to_string(),
                    error_type: "unknown".to_string(),
                    code: None,
                },
            });
            return Err(Error::Inference(format!(
                "OpenAI returned {}: {}",
                status, body.error.message
            )));
        }

        Ok(parse_sse_stream(response.bytes_stream()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OpenAIConfig::default();
        assert_eq!(config.base_url, DEFAULT_OPENAI_URL);
        assert_eq!(config.embed_model, DEFAULT_EMBED_MODEL);
        assert_eq!(config.gen_model, DEFAULT_GEN_MODEL);
        assert_eq!(config.embed_dimension, DEFAULT_DIMENSION);
        assert_eq!(config.timeout_seconds, DEFAULT_TIMEOUT_SECS);
        assert!(!config.skip_tls_verify);
        assert!(config.api_key.is_none());
        assert!(config.http_referer.is_none());
        assert!(config.x_title.is_none());
    }

    #[test]
    fn test_custom_config() {
        let config = OpenAIConfig {
            base_url: "http://localhost:8080/v1".to_string(),
            api_key: Some("test-key".to_string()),
            embed_model: "custom-embed".to_string(),
            gen_model: "custom-gen".to_string(),
            embed_dimension: 768,
            timeout_seconds: 60,
            skip_tls_verify: true,
            http_referer: None,
            x_title: None,
        };

        assert_eq!(config.base_url, "http://localhost:8080/v1");
        assert_eq!(config.api_key, Some("test-key".to_string()));
        assert_eq!(config.embed_model, "custom-embed");
        assert_eq!(config.embed_dimension, 768);
        assert!(config.skip_tls_verify);
    }

    #[test]
    fn test_backend_creation() {
        let backend = OpenAIBackend::with_defaults();
        assert!(backend.is_ok());

        let backend = backend.unwrap();
        assert_eq!(backend.config().base_url, DEFAULT_OPENAI_URL);
    }

    #[test]
    fn test_dimension_accessor() {
        let config = OpenAIConfig {
            embed_dimension: 512,
            ..Default::default()
        };
        let backend = OpenAIBackend::new(config).unwrap();
        assert_eq!(backend.dimension(), 512);
    }

    #[test]
    fn test_model_name_accessor() {
        let config = OpenAIConfig {
            embed_model: "test-embed".to_string(),
            gen_model: "test-gen".to_string(),
            ..Default::default()
        };
        let backend = OpenAIBackend::new(config).unwrap();
        assert_eq!(EmbeddingBackend::model_name(&backend), "test-embed");
        assert_eq!(GenerationBackend::model_name(&backend), "test-gen");
    }

    #[test]
    fn test_config_clone() {
        let config = OpenAIConfig {
            base_url: "test".to_string(),
            api_key: Some("key".to_string()),
            ..Default::default()
        };
        let cloned = config.clone();
        assert_eq!(config.base_url, cloned.base_url);
        assert_eq!(config.api_key, cloned.api_key);
    }

    #[test]
    fn test_openrouter_headers_in_config() {
        let config = OpenAIConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: Some("test-key".to_string()),
            embed_model: "custom-embed".to_string(),
            gen_model: "custom-gen".to_string(),
            embed_dimension: 768,
            timeout_seconds: 60,
            skip_tls_verify: false,
            http_referer: Some("https://myapp.com".to_string()),
            x_title: Some("My App".to_string()),
        };

        assert_eq!(config.http_referer, Some("https://myapp.com".to_string()));
        assert_eq!(config.x_title, Some("My App".to_string()));
    }

    #[test]
    fn test_config_with_only_http_referer() {
        let config = OpenAIConfig {
            http_referer: Some("https://myapp.com".to_string()),
            x_title: None,
            ..Default::default()
        };

        assert_eq!(config.http_referer, Some("https://myapp.com".to_string()));
        assert!(config.x_title.is_none());
    }

    #[test]
    fn test_config_with_only_x_title() {
        let config = OpenAIConfig {
            http_referer: None,
            x_title: Some("My App".to_string()),
            ..Default::default()
        };

        assert!(config.http_referer.is_none());
        assert_eq!(config.x_title, Some("My App".to_string()));
    }
}

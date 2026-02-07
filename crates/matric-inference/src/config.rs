//! Inference configuration system.
//!
//! This module provides a unified configuration system for selecting and configuring
//! inference backends. Configuration can be loaded from:
//! - TOML files (default: ~/.config/matric-memory/inference.toml)
//! - Environment variables (MATRIC_* prefixed)
//!
//! # Example
//!
//! ```rust,no_run
//! use matric_inference::config::InferenceConfig;
//!
//! // Load from default path or fall back to env vars
//! let config = InferenceConfig::load().expect("Failed to load config");
//!
//! // Or explicitly from a file
//! let config = InferenceConfig::from_file(std::path::Path::new("inference.toml")).expect("Failed to load");
//!
//! // Or from environment variables
//! let config = InferenceConfig::from_env();
//! ```

use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, info};

/// Configuration errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Invalid backend: {0}")]
    InvalidBackend(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Missing configuration for default backend: {0}")]
    MissingBackend(String),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

/// Inference backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InferenceBackend {
    #[default]
    Ollama,
    OpenAI,
}

impl FromStr for InferenceBackend {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAI),
            _ => Err(ConfigError::InvalidBackend(s.to_string())),
        }
    }
}

impl fmt::Display for InferenceBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
        }
    }
}

/// Ollama backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL for Ollama API.
    pub base_url: String,
    /// Model to use for text generation.
    pub generation_model: String,
    /// Model to use for embeddings.
    pub embedding_model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: matric_core::defaults::OLLAMA_URL.to_string(),
            generation_model: matric_core::defaults::GEN_MODEL.to_string(),
            embedding_model: matric_core::defaults::EMBED_MODEL.to_string(),
        }
    }
}

impl OllamaConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        if self.base_url.is_empty() {
            return Err(ConfigError::Validation(
                "Ollama base_url cannot be empty".to_string(),
            ));
        }

        // Basic URL validation
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err(ConfigError::Validation(format!(
                "Ollama base_url must start with http:// or https://, got: {}",
                self.base_url
            )));
        }

        if self.generation_model.is_empty() {
            return Err(ConfigError::Validation(
                "Ollama generation_model cannot be empty".to_string(),
            ));
        }

        if self.embedding_model.is_empty() {
            return Err(ConfigError::Validation(
                "Ollama embedding_model cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

/// OpenAI backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// Base URL for OpenAI-compatible API.
    pub base_url: String,
    /// API key for authentication (optional for local endpoints).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Model to use for text generation.
    pub generation_model: String,
    /// Model to use for embeddings.
    pub embedding_model: String,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            generation_model: "gpt-4o-mini".to_string(),
            embedding_model: "text-embedding-3-small".to_string(),
        }
    }
}

impl OpenAIConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        if self.base_url.is_empty() {
            return Err(ConfigError::Validation(
                "OpenAI base_url cannot be empty".to_string(),
            ));
        }

        // Basic URL validation
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err(ConfigError::Validation(format!(
                "OpenAI base_url must start with http:// or https://, got: {}",
                self.base_url
            )));
        }

        if self.generation_model.is_empty() {
            return Err(ConfigError::Validation(
                "OpenAI generation_model cannot be empty".to_string(),
            ));
        }

        if self.embedding_model.is_empty() {
            return Err(ConfigError::Validation(
                "OpenAI embedding_model cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

/// Operation type for routing configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InferenceOperation {
    /// Text embedding generation.
    Embedding,
    /// Text generation (LLM inference).
    Generation,
}

impl fmt::Display for InferenceOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedding => write!(f, "embedding"),
            Self::Generation => write!(f, "generation"),
        }
    }
}

/// Routing configuration for operation-specific backend selection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    /// Backend to use for embedding operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<InferenceBackend>,
    /// Backend to use for generation operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<InferenceBackend>,
}

impl RoutingConfig {
    /// Get the backend for a specific operation.
    pub fn get_backend(&self, operation: InferenceOperation) -> Option<InferenceBackend> {
        match operation {
            InferenceOperation::Embedding => self.embedding,
            InferenceOperation::Generation => self.generation,
        }
    }
}

/// Fallback configuration for automatic backend failover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    /// Whether fallback is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Chain of backends to try in order.
    #[serde(default)]
    pub chain: Vec<InferenceBackend>,
    /// Maximum number of retries per backend.
    #[serde(default = "FallbackConfig::default_max_retries")]
    pub max_retries: u32,
    /// Timeout in seconds for health checks.
    #[serde(default = "FallbackConfig::default_health_check_timeout")]
    pub health_check_timeout_secs: u64,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chain: vec![],
            max_retries: Self::default_max_retries(),
            health_check_timeout_secs: Self::default_health_check_timeout(),
        }
    }
}

impl FallbackConfig {
    fn default_max_retries() -> u32 {
        1
    }

    fn default_health_check_timeout() -> u64 {
        5
    }

    /// Get the next backend in the fallback chain after the given backend.
    pub fn next_backend(&self, current: InferenceBackend) -> Option<InferenceBackend> {
        if !self.enabled || self.chain.is_empty() {
            return None;
        }

        let current_idx = self.chain.iter().position(|b| *b == current)?;
        self.chain.get(current_idx + 1).copied()
    }

    /// Get the first backend in the fallback chain.
    pub fn first_backend(&self) -> Option<InferenceBackend> {
        self.chain.first().copied()
    }

    /// Validate the fallback configuration.
    pub fn validate(&self, available_backends: &[InferenceBackend]) -> ConfigResult<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.chain.is_empty() {
            return Err(ConfigError::Validation(
                "Fallback is enabled but chain is empty".to_string(),
            ));
        }

        // Check that all backends in the chain are configured
        for backend in &self.chain {
            if !available_backends.contains(backend) {
                return Err(ConfigError::Validation(format!(
                    "Backend {} in fallback chain is not configured",
                    backend
                )));
            }
        }

        Ok(())
    }
}

/// Backend health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendHealth {
    /// Backend is healthy and responding.
    Healthy,
    /// Backend is unhealthy or not responding.
    Unhealthy,
    /// Backend health is unknown (not checked yet).
    Unknown,
}

/// Main inference configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Default backend to use.
    pub default: InferenceBackend,
    /// Ollama configuration (if enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama: Option<OllamaConfig>,
    /// OpenAI configuration (if enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<OpenAIConfig>,
    /// Operation-specific routing configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingConfig>,
    /// Fallback configuration for automatic failover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackConfig>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            default: InferenceBackend::Ollama,
            ollama: Some(OllamaConfig::default()),
            openai: None,
            routing: None,
            fallback: None,
        }
    }
}

impl InferenceConfig {
    /// Get the default config file path.
    ///
    /// Returns: ~/.config/matric-memory/inference.toml
    pub fn default_config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config"));
        path.push("matric-memory");
        path.push("inference.toml");
        path
    }

    /// Load configuration from the default path, falling back to environment variables.
    ///
    /// This tries to load from ~/.config/matric-memory/inference.toml first.
    /// If that file doesn't exist, it falls back to environment variables.
    pub fn load() -> ConfigResult<Self> {
        let path = Self::default_config_path();

        if path.exists() {
            info!("Loading inference config from: {}", path.display());
            Self::from_file(&path)
        } else {
            debug!(
                "Config file not found at {}, using environment variables",
                path.display()
            );
            Ok(Self::from_env())
        }
    }

    /// Load configuration from a TOML file.
    pub fn from_file(path: &std::path::Path) -> ConfigResult<Self> {
        let content = std::fs::read_to_string(path)?;
        let content = Self::substitute_env_vars(&content);

        #[derive(Deserialize)]
        struct TomlRoot {
            inference: TomlInferenceConfig,
        }

        #[derive(Deserialize)]
        struct TomlInferenceConfig {
            default: String,
            #[serde(default)]
            ollama: Option<OllamaConfig>,
            #[serde(default)]
            openai: Option<OpenAIConfig>,
            #[serde(default)]
            routing: Option<RoutingConfig>,
            #[serde(default)]
            fallback: Option<FallbackConfig>,
        }

        let root: TomlRoot = toml::from_str(&content)?;

        let default = root.inference.default.parse()?;

        let config = Self {
            default,
            ollama: root.inference.ollama,
            openai: root.inference.openai,
            routing: root.inference.routing,
            fallback: root.inference.fallback,
        };

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        let default = env::var("MATRIC_INFERENCE_DEFAULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default();

        let ollama = match default {
            InferenceBackend::Ollama => Some(OllamaConfig {
                base_url: env::var("MATRIC_OLLAMA_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string()),
                generation_model: env::var("MATRIC_OLLAMA_GENERATION_MODEL")
                    .unwrap_or_else(|_| "gpt-oss:20b".to_string()),
                embedding_model: env::var("MATRIC_OLLAMA_EMBEDDING_MODEL")
                    .unwrap_or_else(|_| "nomic-embed-text".to_string()),
            }),
            InferenceBackend::OpenAI => None,
        };

        let openai = match default {
            InferenceBackend::OpenAI => Some(OpenAIConfig {
                base_url: env::var("MATRIC_OPENAI_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
                api_key: env::var("MATRIC_OPENAI_API_KEY").ok(),
                generation_model: env::var("MATRIC_OPENAI_GENERATION_MODEL")
                    .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                embedding_model: env::var("MATRIC_OPENAI_EMBEDDING_MODEL")
                    .unwrap_or_else(|_| "text-embedding-3-small".to_string()),
            }),
            InferenceBackend::Ollama => None,
        };

        Self {
            default,
            ollama,
            openai,
            routing: None,
            fallback: None,
        }
    }

    /// Get the list of available (configured) backends.
    pub fn available_backends(&self) -> Vec<InferenceBackend> {
        let mut backends = Vec::new();
        if self.ollama.is_some() {
            backends.push(InferenceBackend::Ollama);
        }
        if self.openai.is_some() {
            backends.push(InferenceBackend::OpenAI);
        }
        backends
    }

    /// Get the backend to use for a specific operation.
    ///
    /// Resolution order:
    /// 1. Operation-specific routing (if configured)
    /// 2. Default backend
    pub fn get_backend_for_operation(&self, operation: InferenceOperation) -> InferenceBackend {
        // Check routing config first
        if let Some(ref routing) = self.routing {
            if let Some(backend) = routing.get_backend(operation) {
                return backend;
            }
        }

        // Fall back to default
        self.default
    }

    /// Get the fallback backend chain for a given starting backend.
    ///
    /// Returns the remaining backends to try after the given backend fails.
    pub fn get_fallback_chain(&self, current: InferenceBackend) -> Vec<InferenceBackend> {
        match &self.fallback {
            Some(fallback) if fallback.enabled => {
                let mut chain = Vec::new();
                let mut backend = Some(current);
                while let Some(next) = backend.and_then(|b| fallback.next_backend(b)) {
                    chain.push(next);
                    backend = Some(next);
                }
                chain
            }
            _ => Vec::new(),
        }
    }

    /// Check if fallback is enabled.
    pub fn is_fallback_enabled(&self) -> bool {
        self.fallback.as_ref().map(|f| f.enabled).unwrap_or(false)
    }

    /// Validate the configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        // Get available backends for validation
        let available = self.available_backends();

        // Ensure the default backend is configured
        match self.default {
            InferenceBackend::Ollama => {
                if self.ollama.is_none() {
                    return Err(ConfigError::MissingBackend(
                        "Ollama is set as default but not configured".to_string(),
                    ));
                }
            }
            InferenceBackend::OpenAI => {
                if self.openai.is_none() {
                    return Err(ConfigError::MissingBackend(
                        "OpenAI is set as default but not configured".to_string(),
                    ));
                }
            }
        }

        // Validate individual backend configs
        if let Some(ref ollama) = self.ollama {
            ollama.validate()?;
        }

        if let Some(ref openai) = self.openai {
            openai.validate()?;
        }

        // Validate routing config
        if let Some(ref routing) = self.routing {
            if let Some(backend) = routing.embedding {
                if !available.contains(&backend) {
                    return Err(ConfigError::Validation(format!(
                        "Routing embedding backend {} is not configured",
                        backend
                    )));
                }
            }
            if let Some(backend) = routing.generation {
                if !available.contains(&backend) {
                    return Err(ConfigError::Validation(format!(
                        "Routing generation backend {} is not configured",
                        backend
                    )));
                }
            }
        }

        // Validate fallback config
        if let Some(ref fallback) = self.fallback {
            fallback.validate(&available)?;
        }

        Ok(())
    }

    /// Substitute environment variables in the format ${VAR_NAME}.
    fn substitute_env_vars(content: &str) -> String {
        let re = regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}").unwrap();
        re.replace_all(content, |caps: &regex::Captures| {
            let var_name = &caps[1];
            env::var(var_name).unwrap_or_else(|_| format!("${{{}}}", var_name))
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_substitution_with_value() {
        // Create test content with a placeholder
        let content = "api_key = \"${TEST_SUBSTITUTION_VAR}\"";

        // Set the env var temporarily
        env::set_var("TEST_SUBSTITUTION_VAR", "test-value");
        let result = InferenceConfig::substitute_env_vars(content);
        env::remove_var("TEST_SUBSTITUTION_VAR");

        assert_eq!(result, "api_key = \"test-value\"");
    }

    #[test]
    fn test_env_var_substitution_missing() {
        // Ensure the var doesn't exist before testing
        let content = "api_key = \"${NONEXISTENT_TEST_VAR_12345}\"";
        let result = InferenceConfig::substitute_env_vars(content);
        assert_eq!(result, "api_key = \"${NONEXISTENT_TEST_VAR_12345}\"");
    }

    #[test]
    fn test_env_var_substitution_multiple() {
        // Set test vars
        env::set_var("TEST_VAR1_MULTI", "value1");
        env::set_var("TEST_VAR2_MULTI", "value2");

        let content = "url = \"${TEST_VAR1_MULTI}\" key = \"${TEST_VAR2_MULTI}\"";
        let result = InferenceConfig::substitute_env_vars(content);

        env::remove_var("TEST_VAR1_MULTI");
        env::remove_var("TEST_VAR2_MULTI");

        assert_eq!(result, "url = \"value1\" key = \"value2\"");
    }

    #[test]
    fn test_serialize_inference_config() {
        let config = InferenceConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("ollama"));
        assert!(serialized.contains("default"));
    }

    #[test]
    fn test_deserialize_inference_backend() {
        let toml_str = "default = \"ollama\"";
        #[derive(Deserialize)]
        struct Test {
            default: InferenceBackend,
        }
        let test: Test = toml::from_str(toml_str).unwrap();
        assert_eq!(test.default, InferenceBackend::Ollama);
    }
}

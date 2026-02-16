//! Inference provider registry with provider-qualified slug routing.
//!
//! Extends the per-operation model slug override (ADR-072 Phase 1) with
//! multi-provider support. Slugs can be provider-qualified:
//!
//! ```text
//! "qwen3:8b"                      → default provider (Ollama)
//! "ollama:qwen3:8b"               → explicit Ollama
//! "openai:gpt-4o"                 → OpenAI
//! "openrouter:anthropic/claude-sonnet-4-20250514" → OpenRouter
//! ```
//!
//! The default provider (Ollama) is always available. External providers
//! require feature flags (`openai`) and API key configuration.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use matric_core::{Error, Result};

// ---------------------------------------------------------------------------
// Provider capability enum
// ---------------------------------------------------------------------------

/// Capabilities a provider can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    Generation,
    Embedding,
    Vision,
    Transcription,
}

impl std::fmt::Display for ProviderCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generation => write!(f, "generation"),
            Self::Embedding => write!(f, "embedding"),
            Self::Vision => write!(f, "vision"),
            Self::Transcription => write!(f, "transcription"),
        }
    }
}

// ---------------------------------------------------------------------------
// Provider health status
// ---------------------------------------------------------------------------

/// Health status for a registered provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealth {
    /// Provider is healthy and accepting requests.
    Healthy,
    /// Provider is configured but has not been checked yet.
    Unknown,
    /// Last health check failed.
    Unhealthy,
}

// ---------------------------------------------------------------------------
// Provider configuration
// ---------------------------------------------------------------------------

/// Configuration for a registered inference provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider identifier (e.g., "ollama", "openai", "openrouter").
    pub id: String,
    /// Base URL for the provider's API.
    pub base_url: String,
    /// API key (None for local providers like Ollama).
    pub api_key: Option<String>,
    /// Which capabilities this provider supports.
    pub capabilities: Vec<ProviderCapability>,
    /// Default timeout for requests.
    pub timeout: Duration,
    /// Whether this is the default provider (exactly one must be default).
    pub is_default: bool,
    /// Current health status.
    pub health: ProviderHealth,
    /// OpenRouter-specific: HTTP-Referer header for rankings.
    pub http_referer: Option<String>,
    /// OpenRouter-specific: X-Title header for app name.
    pub x_title: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsed slug result
// ---------------------------------------------------------------------------

/// Result of parsing a provider-qualified model slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSlug {
    /// Provider identifier.
    pub provider_id: String,
    /// Model slug (everything after the provider prefix).
    pub model: String,
}

// ---------------------------------------------------------------------------
// Provider registry
// ---------------------------------------------------------------------------

/// Registry of configured inference providers.
///
/// Manages provider configuration and resolves provider-qualified model slugs
/// to concrete backend instances.
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderConfig>,
    default_provider: String,
}

impl ProviderRegistry {
    /// Create a new empty provider registry.
    pub fn new(default_provider: String) -> Self {
        Self {
            providers: HashMap::new(),
            default_provider,
        }
    }

    /// Register a provider.
    pub fn register(&mut self, config: ProviderConfig) {
        info!(
            provider = %config.id,
            base_url = %config.base_url,
            capabilities = ?config.capabilities,
            is_default = config.is_default,
            "Registering inference provider"
        );
        if config.is_default {
            self.default_provider = config.id.clone();
        }
        self.providers.insert(config.id.clone(), config);
    }

    /// Get the default provider ID.
    pub fn default_provider(&self) -> &str {
        &self.default_provider
    }

    /// Get all registered provider IDs.
    pub fn provider_ids(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Get a provider config by ID.
    pub fn get_provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.get(id)
    }

    /// Get a mutable provider config by ID.
    pub fn get_provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.providers.get_mut(id)
    }

    /// Check if a provider is registered.
    pub fn has_provider(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    /// Get all healthy providers.
    pub fn healthy_providers(&self) -> Vec<&ProviderConfig> {
        self.providers
            .values()
            .filter(|p| p.health != ProviderHealth::Unhealthy)
            .collect()
    }

    /// Get providers supporting a given capability.
    pub fn providers_with_capability(&self, cap: ProviderCapability) -> Vec<&ProviderConfig> {
        self.providers
            .values()
            .filter(|p| p.capabilities.contains(&cap) && p.health != ProviderHealth::Unhealthy)
            .collect()
    }

    // -----------------------------------------------------------------------
    // Slug parsing
    // -----------------------------------------------------------------------

    /// Parse a provider-qualified model slug.
    ///
    /// Returns a [`ParsedSlug`] with the provider ID and model slug. If no
    /// provider prefix is found, uses the default provider.
    ///
    /// # Parsing Strategy
    ///
    /// Ollama model slugs already contain colons (e.g., `qwen3:8b`). The parser
    /// matches against **known provider IDs** first, then treats the remainder
    /// as the model slug. Unknown prefixes are part of the model slug.
    ///
    /// | Input | Provider | Model |
    /// |-------|----------|-------|
    /// | `qwen3:8b` | default | `qwen3:8b` |
    /// | `ollama:qwen3:8b` | ollama | `qwen3:8b` |
    /// | `openai:gpt-4o` | openai | `gpt-4o` |
    /// | `openrouter:anthropic/claude-sonnet-4-20250514` | openrouter | `anthropic/claude-sonnet-4-20250514` |
    pub fn parse_slug(&self, slug: &str) -> ParsedSlug {
        // Try each known provider prefix
        for provider_id in self.providers.keys() {
            let prefix = format!("{}:", provider_id);
            if let Some(model) = slug.strip_prefix(&prefix) {
                if !model.is_empty() {
                    debug!(
                        slug = slug,
                        provider = %provider_id,
                        model = model,
                        "Parsed provider-qualified slug"
                    );
                    return ParsedSlug {
                        provider_id: provider_id.clone(),
                        model: model.to_string(),
                    };
                }
            }
        }

        // No known provider prefix — use default
        debug!(
            slug = slug,
            provider = %self.default_provider,
            "Using default provider for bare slug"
        );
        ParsedSlug {
            provider_id: self.default_provider.clone(),
            model: slug.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Backend resolution
    // -----------------------------------------------------------------------

    /// Resolve a provider-qualified slug to an [`OllamaBackend`] generation backend.
    ///
    /// Returns `Ok(Some(backend))` if the slug targets the Ollama provider,
    /// or `Ok(None)` if no override is needed (bare slug matching default model).
    ///
    /// For non-Ollama providers, use [`resolve_generation_boxed`].
    #[cfg(feature = "ollama")]
    pub fn resolve_ollama_gen_override(
        &self,
        model_override: Option<&str>,
    ) -> Option<crate::OllamaBackend> {
        let slug = model_override?;
        let parsed = self.parse_slug(slug);

        if parsed.provider_id == "ollama" || parsed.provider_id == self.default_provider {
            // Ollama provider — create fresh backend with model swapped
            let mut backend = crate::OllamaBackend::from_env();
            backend.set_gen_model(parsed.model);
            Some(backend)
        } else {
            // Non-Ollama provider — caller must use resolve_generation_boxed
            None
        }
    }

    /// Resolve a provider-qualified slug to a boxed [`GenerationBackend`].
    ///
    /// This creates a new backend instance for the resolved provider with
    /// the specified model. The backend lives on the caller's stack/heap
    /// for the duration of the operation — no shared mutable state.
    pub fn resolve_generation_boxed(
        &self,
        slug: &str,
    ) -> Result<Box<dyn matric_core::GenerationBackend>> {
        let parsed = self.parse_slug(slug);
        let config = self
            .providers
            .get(&parsed.provider_id)
            .ok_or_else(|| Error::Config(format!("Unknown provider: {}", parsed.provider_id)))?;

        if !config
            .capabilities
            .contains(&ProviderCapability::Generation)
        {
            return Err(Error::Config(format!(
                "Provider '{}' does not support generation",
                parsed.provider_id
            )));
        }

        if config.health == ProviderHealth::Unhealthy {
            warn!(
                provider = %parsed.provider_id,
                "Resolving backend for unhealthy provider"
            );
        }

        match parsed.provider_id.as_str() {
            #[cfg(feature = "ollama")]
            "ollama" => {
                let mut backend = crate::OllamaBackend::from_env();
                backend.set_gen_model(parsed.model);
                Ok(Box::new(backend))
            }
            #[cfg(feature = "openai")]
            "openai" | "openrouter" => {
                let oai_config = crate::OpenAIConfig {
                    base_url: config.base_url.clone(),
                    api_key: config.api_key.clone(),
                    gen_model: parsed.model,
                    http_referer: config.http_referer.clone(),
                    x_title: config.x_title.clone(),
                    timeout_seconds: config.timeout.as_secs(),
                    ..Default::default()
                };
                Ok(Box::new(crate::OpenAIBackend::new(oai_config)?))
            }
            _ => Err(Error::Config(format!(
                "Provider '{}' not compiled in (check feature flags)",
                parsed.provider_id
            ))),
        }
    }

    /// Resolve an optional model override to a boxed generation backend.
    ///
    /// - `None` → `Ok(None)` — caller should use its default backend
    /// - `Some("qwen3:32b")` → Ollama override → `Ok(Some(Box<OllamaBackend>))`
    /// - `Some("openai:gpt-4o")` → external → `Ok(Some(Box<OpenAIBackend>))`
    ///
    /// This is the primary entry point for job handlers that support
    /// per-operation model override with multi-provider routing.
    pub fn resolve_gen_override(
        &self,
        model_override: Option<&str>,
    ) -> Result<Option<Box<dyn matric_core::GenerationBackend>>> {
        match model_override {
            None => Ok(None),
            Some(slug) => Ok(Some(self.resolve_generation_boxed(slug)?)),
        }
    }

    /// Check if a slug targets a non-default (external) provider.
    ///
    /// Returns `true` if the slug has a provider prefix pointing to a
    /// non-default provider. Used by handlers to decide whether to use
    /// the Ollama fast-path or the boxed trait object path.
    pub fn is_external_provider(&self, slug: &str) -> bool {
        let parsed = self.parse_slug(slug);
        parsed.provider_id != self.default_provider
            && parsed.provider_id != "ollama"
            && self.providers.contains_key(&parsed.provider_id)
    }

    // -----------------------------------------------------------------------
    // Construction from environment
    // -----------------------------------------------------------------------

    /// Build a provider registry from environment variables.
    ///
    /// Always registers the Ollama provider (default). Optionally registers
    /// OpenAI and OpenRouter if their API keys are configured.
    pub fn from_env() -> Self {
        let mut registry = Self::new("ollama".to_string());

        // Ollama — always available
        let ollama_base = std::env::var("OLLAMA_BASE")
            .or_else(|_| std::env::var("OLLAMA_URL"))
            .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());

        registry.register(ProviderConfig {
            id: "ollama".to_string(),
            base_url: ollama_base,
            api_key: None,
            capabilities: vec![
                ProviderCapability::Generation,
                ProviderCapability::Embedding,
                ProviderCapability::Vision,
            ],
            timeout: Duration::from_secs(matric_core::defaults::GEN_TIMEOUT_SECS),
            is_default: true,
            health: ProviderHealth::Unknown,
            http_referer: None,
            x_title: None,
        });

        // OpenAI — opt-in via OPENAI_API_KEY
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            if !api_key.is_empty() {
                let base_url = std::env::var("OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
                let timeout = std::env::var("OPENAI_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(300);

                registry.register(ProviderConfig {
                    id: "openai".to_string(),
                    base_url,
                    api_key: Some(api_key),
                    capabilities: vec![
                        ProviderCapability::Generation,
                        ProviderCapability::Embedding,
                    ],
                    timeout: Duration::from_secs(timeout),
                    is_default: false,
                    health: ProviderHealth::Unknown,
                    http_referer: None,
                    x_title: None,
                });
            }
        }

        // OpenRouter — opt-in via OPENROUTER_API_KEY
        if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
            if !api_key.is_empty() {
                let base_url = std::env::var("OPENROUTER_BASE_URL")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
                let timeout = std::env::var("OPENROUTER_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(300);
                let http_referer = std::env::var("OPENROUTER_HTTP_REFERER").ok();
                let x_title = std::env::var("OPENROUTER_X_TITLE").ok();

                registry.register(ProviderConfig {
                    id: "openrouter".to_string(),
                    base_url,
                    api_key: Some(api_key),
                    capabilities: vec![ProviderCapability::Generation],
                    timeout: Duration::from_secs(timeout),
                    is_default: false,
                    health: ProviderHealth::Unknown,
                    http_referer,
                    x_title,
                });
            }
        }

        info!(
            providers = ?registry.provider_ids(),
            default = %registry.default_provider,
            "Provider registry initialized from environment"
        );

        registry
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new("ollama".to_string());

        registry.register(ProviderConfig {
            id: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            capabilities: vec![
                ProviderCapability::Generation,
                ProviderCapability::Embedding,
                ProviderCapability::Vision,
            ],
            timeout: Duration::from_secs(300),
            is_default: true,
            health: ProviderHealth::Healthy,
            http_referer: None,
            x_title: None,
        });

        registry.register(ProviderConfig {
            id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some("sk-test-key".to_string()),
            capabilities: vec![
                ProviderCapability::Generation,
                ProviderCapability::Embedding,
            ],
            timeout: Duration::from_secs(300),
            is_default: false,
            health: ProviderHealth::Healthy,
            http_referer: None,
            x_title: None,
        });

        registry.register(ProviderConfig {
            id: "openrouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: Some("sk-or-test-key".to_string()),
            capabilities: vec![ProviderCapability::Generation],
            timeout: Duration::from_secs(300),
            is_default: false,
            health: ProviderHealth::Healthy,
            http_referer: Some("https://fortemi.com".to_string()),
            x_title: Some("Fortemi".to_string()),
        });

        registry
    }

    // -----------------------------------------------------------------------
    // Slug parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_bare_ollama_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("qwen3:8b");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "qwen3:8b");
    }

    #[test]
    fn parse_bare_embed_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("nomic-embed-text");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "nomic-embed-text");
    }

    #[test]
    fn parse_explicit_ollama_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("ollama:qwen3:8b");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "qwen3:8b");
    }

    #[test]
    fn parse_openai_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("openai:gpt-4o");
        assert_eq!(parsed.provider_id, "openai");
        assert_eq!(parsed.model, "gpt-4o");
    }

    #[test]
    fn parse_openai_mini_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("openai:gpt-4.1-mini");
        assert_eq!(parsed.provider_id, "openai");
        assert_eq!(parsed.model, "gpt-4.1-mini");
    }

    #[test]
    fn parse_openrouter_slug() {
        let reg = test_registry();
        let parsed = reg.parse_slug("openrouter:anthropic/claude-sonnet-4-20250514");
        assert_eq!(parsed.provider_id, "openrouter");
        assert_eq!(parsed.model, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn parse_unknown_prefix_as_default_model() {
        let reg = test_registry();
        // "llava" is not a registered provider — treat as Ollama model
        let parsed = reg.parse_slug("llava:34b");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "llava:34b");
    }

    #[test]
    fn parse_empty_model_after_prefix_uses_default() {
        let reg = test_registry();
        // "openai:" with empty model — should not match (treated as default)
        let parsed = reg.parse_slug("openai:");
        // This is an edge case — the model would be empty if we stripped,
        // so we fall through to default provider with the full string.
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "openai:");
    }

    #[test]
    fn parse_model_with_slashes() {
        let reg = test_registry();
        let parsed = reg.parse_slug("openrouter:meta-llama/llama-3-70b-instruct");
        assert_eq!(parsed.provider_id, "openrouter");
        assert_eq!(parsed.model, "meta-llama/llama-3-70b-instruct");
    }

    #[test]
    fn parse_model_no_colon() {
        let reg = test_registry();
        let parsed = reg.parse_slug("mistral");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "mistral");
    }

    // -----------------------------------------------------------------------
    // Registry management tests
    // -----------------------------------------------------------------------

    #[test]
    fn default_provider_is_ollama() {
        let reg = test_registry();
        assert_eq!(reg.default_provider(), "ollama");
    }

    #[test]
    fn provider_ids_returns_all() {
        let reg = test_registry();
        let ids = reg.provider_ids();
        assert!(ids.contains(&"ollama"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"openrouter"));
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn has_provider_checks_registration() {
        let reg = test_registry();
        assert!(reg.has_provider("ollama"));
        assert!(reg.has_provider("openai"));
        assert!(!reg.has_provider("azure"));
    }

    #[test]
    fn providers_with_generation_capability() {
        let reg = test_registry();
        let gen_providers = reg.providers_with_capability(ProviderCapability::Generation);
        assert_eq!(gen_providers.len(), 3); // ollama, openai, openrouter
    }

    #[test]
    fn providers_with_embedding_capability() {
        let reg = test_registry();
        let embed_providers = reg.providers_with_capability(ProviderCapability::Embedding);
        assert_eq!(embed_providers.len(), 2); // ollama, openai
    }

    #[test]
    fn providers_with_vision_capability() {
        let reg = test_registry();
        let vision_providers = reg.providers_with_capability(ProviderCapability::Vision);
        assert_eq!(vision_providers.len(), 1); // ollama only
    }

    #[test]
    fn unhealthy_providers_excluded_from_capability_query() {
        let mut reg = test_registry();
        reg.get_provider_mut("openai").unwrap().health = ProviderHealth::Unhealthy;

        let gen_providers = reg.providers_with_capability(ProviderCapability::Generation);
        assert_eq!(gen_providers.len(), 2); // ollama + openrouter (openai excluded)
    }

    #[test]
    fn is_external_provider_detects_non_default() {
        let reg = test_registry();
        assert!(!reg.is_external_provider("qwen3:8b"));
        assert!(!reg.is_external_provider("ollama:qwen3:8b"));
        assert!(reg.is_external_provider("openai:gpt-4o"));
        assert!(reg.is_external_provider("openrouter:anthropic/claude-sonnet-4-20250514"));
    }

    #[test]
    fn is_external_provider_false_for_unknown_prefix() {
        let reg = test_registry();
        // "llava:34b" — llava is not a provider, so not external
        assert!(!reg.is_external_provider("llava:34b"));
    }

    // -----------------------------------------------------------------------
    // Provider config tests
    // -----------------------------------------------------------------------

    #[test]
    fn provider_capability_display() {
        assert_eq!(ProviderCapability::Generation.to_string(), "generation");
        assert_eq!(ProviderCapability::Embedding.to_string(), "embedding");
        assert_eq!(ProviderCapability::Vision.to_string(), "vision");
        assert_eq!(
            ProviderCapability::Transcription.to_string(),
            "transcription"
        );
    }

    #[test]
    fn provider_capability_serialization() {
        let cap = ProviderCapability::Generation;
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"generation\"");

        let deserialized: ProviderCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ProviderCapability::Generation);
    }

    #[test]
    fn provider_health_serialization() {
        let health = ProviderHealth::Healthy;
        let json = serde_json::to_string(&health).unwrap();
        assert_eq!(json, "\"healthy\"");
    }

    #[test]
    fn parsed_slug_equality() {
        let a = ParsedSlug {
            provider_id: "openai".to_string(),
            model: "gpt-4o".to_string(),
        };
        let b = ParsedSlug {
            provider_id: "openai".to_string(),
            model: "gpt-4o".to_string(),
        };
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // Resolution tests (compile-gated)
    // -----------------------------------------------------------------------

    #[cfg(feature = "ollama")]
    #[test]
    fn resolve_ollama_gen_override_with_bare_slug() {
        let reg = test_registry();
        let backend = reg.resolve_ollama_gen_override(Some("qwen3:32b"));
        assert!(backend.is_some());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn resolve_ollama_gen_override_with_explicit_prefix() {
        let reg = test_registry();
        let backend = reg.resolve_ollama_gen_override(Some("ollama:qwen3:32b"));
        assert!(backend.is_some());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn resolve_ollama_gen_override_none_for_external() {
        let reg = test_registry();
        // openai: prefix → returns None (caller must use boxed path)
        let backend = reg.resolve_ollama_gen_override(Some("openai:gpt-4o"));
        assert!(backend.is_none());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn resolve_ollama_gen_override_none_when_no_override() {
        let reg = test_registry();
        let backend = reg.resolve_ollama_gen_override(None);
        assert!(backend.is_none());
    }

    #[test]
    fn resolve_generation_boxed_unknown_prefix_routes_to_default() {
        let reg = test_registry();
        // "azure:gpt-4" — "azure" is not a registered provider, so the entire
        // string "azure:gpt-4" is treated as an Ollama model name.
        let parsed = reg.parse_slug("azure:gpt-4");
        assert_eq!(parsed.provider_id, "ollama");
        assert_eq!(parsed.model, "azure:gpt-4");
    }

    #[test]
    fn resolve_generation_boxed_no_gen_capability_errors() {
        let mut reg = test_registry();
        // Register a provider with only transcription
        reg.register(ProviderConfig {
            id: "whisper".to_string(),
            base_url: "http://localhost:8000".to_string(),
            api_key: None,
            capabilities: vec![ProviderCapability::Transcription],
            timeout: Duration::from_secs(60),
            is_default: false,
            health: ProviderHealth::Healthy,
            http_referer: None,
            x_title: None,
        });

        let result = reg.resolve_generation_boxed("whisper:large-v3");
        match result {
            Err(e) => assert!(
                e.to_string().contains("does not support generation"),
                "Expected 'does not support generation', got: {}",
                e
            ),
            Ok(_) => panic!("Expected error for provider without generation capability"),
        }
    }
}

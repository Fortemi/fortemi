//! Model discovery HTTP handler.
//!
//! Returns available LLM models with capability metadata so clients
//! can choose the right model for each operation. Includes registered
//! provider information for multi-provider routing (#432).

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use tracing::warn;

use crate::{ApiError, AppState};
use matric_inference::discovery::ModelDiscovery;
use matric_inference::{OllamaBackend, ProviderCapability, ProviderHealth};

const MODEL_DISCOVERY_FAILURE_DETAIL: &str =
    "Model discovery failed. Check server logs for diagnostics.";

/// A single model entry with capability metadata.
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ModelInfo {
    /// Model slug used in API parameters (e.g. "qwen3:8b", "nomic-embed-text").
    pub slug: String,
    /// Provider this model belongs to (e.g. "ollama", "openai").
    pub provider: String,
    /// What this model can be used for: "language", "embedding", "vision", "transcription".
    pub capabilities: Vec<String>,
    /// If this model is the configured default for a capability, lists those capabilities.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub default_for: Vec<String>,
    /// Model parameter count (e.g. "8B", "14B") if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_size: Option<String>,
    /// Quantization level (e.g. "Q4_K_M") if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Model file size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Model family (e.g. "qwen2", "llama") if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

impl std::fmt::Debug for ModelInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelInfo")
            .field("slug_len", &debug_text_len(&self.slug))
            .field("provider_len", &debug_text_len(&self.provider))
            .field("capability_count", &self.capabilities.len())
            .field("default_for_count", &self.default_for.len())
            .field(
                "parameter_size_len",
                &debug_optional_text_len(&self.parameter_size),
            )
            .field(
                "quantization_len",
                &debug_optional_text_len(&self.quantization),
            )
            .field("size_bytes", &self.size_bytes)
            .field("family_len", &debug_optional_text_len(&self.family))
            .finish()
    }
}

/// Summary of a registered inference provider.
#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ProviderInfo {
    /// Provider identifier (e.g. "ollama", "openai", "openrouter").
    pub id: String,
    /// Capabilities supported by this provider.
    pub capabilities: Vec<String>,
    /// Whether this is the default provider.
    pub is_default: bool,
    /// Current health status: "healthy", "unknown", or "unhealthy".
    pub health: String,
}

impl std::fmt::Debug for ProviderInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderInfo")
            .field("id_len", &debug_text_len(&self.id))
            .field("capability_count", &self.capabilities.len())
            .field("is_default", &self.is_default)
            .field("health_len", &debug_text_len(&self.health))
            .finish()
    }
}

/// Response from the model discovery endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ListModelsResponse {
    /// All available models with capability metadata.
    pub models: Vec<ModelInfo>,
    /// Currently configured default model slugs.
    pub defaults: ModelDefaults,
    /// Registered inference providers.
    pub providers: Vec<ProviderInfo>,
}

impl std::fmt::Debug for ListModelsResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListModelsResponse")
            .field("model_count", &self.models.len())
            .field("defaults", &self.defaults)
            .field("provider_count", &self.providers.len())
            .finish()
    }
}

/// Default model slugs from server configuration.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ModelDefaults {
    /// Default language/generation model slug.
    pub language: String,
    /// Default embedding model slug.
    pub embedding: String,
    /// Default vision model slug, if configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision: Option<String>,
    /// Default transcription model slug, if configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
}

impl std::fmt::Debug for ModelDefaults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelDefaults")
            .field("language_len", &debug_text_len(&self.language))
            .field("embedding_len", &debug_text_len(&self.embedding))
            .field("vision_len", &debug_optional_text_len(&self.vision))
            .field(
                "transcription_len",
                &debug_optional_text_len(&self.transcription),
            )
            .finish()
    }
}

fn debug_text_len(value: &str) -> usize {
    value.chars().count()
}

fn debug_optional_text_len(value: &Option<String>) -> Option<usize> {
    value.as_deref().map(debug_text_len)
}

/// Check if a model name matches known vision model patterns.
fn is_likely_vision_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("-vl")
        || lower.contains("llava")
        || lower.contains("bakllava")
        || lower.contains("moondream")
        || lower.contains("minicpm-v")
        || lower.contains("vision")
}

/// Check if a model name matches known embedding model patterns.
fn is_likely_embedding_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("embed")
        || lower.contains("nomic")
        || lower.contains("mxbai")
        || lower.contains("bge")
        || lower.contains("minilm")
        || lower.contains("e5-")
        || lower.contains("gte-")
}

/// List available models from all configured backends.
///
/// Queries Ollama for language, embedding, and vision models, and the Whisper
/// backend for transcription models. Each model includes capability metadata
/// indicating what operations it can be used for. Also returns registered
/// provider information for multi-provider slug routing.
#[utoipa::path(get, path = "/api/v1/models", tag = "Models",
    responses(
        (status = 200, description = "Available models", body = ListModelsResponse),
    )
)]
pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<ListModelsResponse>, ApiError> {
    let registry = state.provider_registry();

    // Determine configured defaults from active provider routing.
    let backend = OllamaBackend::from_env();
    let default_gen = state
        .generation_backend()
        .as_ref()
        .map(|b| b.model_name().to_string())
        .unwrap_or_else(|| matric_core::GenerationBackend::model_name(&backend).to_string());
    let default_embed = registry
        .resolve_default_embedding_boxed()
        .map(|b| b.model_name().to_string())
        .unwrap_or_else(|_| matric_core::EmbeddingBackend::model_name(&backend).to_string());
    let default_vision = state
        .vision_backend
        .as_ref()
        .map(|b| b.model_name().to_string());
    let default_transcription = state
        .transcription_backend
        .as_ref()
        .map(|b| b.model_name().to_string());

    let ollama_base_url = std::env::var("OLLAMA_BASE")
        .or_else(|_| std::env::var("OLLAMA_URL"))
        .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());

    let mut models: Vec<ModelInfo> = Vec::new();

    // Query Ollama for available models
    let discovery = ModelDiscovery::new(&ollama_base_url);
    match discovery.discover_models().await {
        Ok(result) => {
            for m in result.models {
                let mut capabilities = Vec::new();
                let mut default_for = Vec::new();

                let is_embed = is_likely_embedding_model(&m.name);
                let is_vision = is_likely_vision_model(&m.name);

                if is_embed {
                    capabilities.push("embedding".to_string());
                    if m.name == default_embed {
                        default_for.push("embedding".to_string());
                    }
                }

                if is_vision {
                    capabilities.push("vision".to_string());
                    if default_vision.as_deref() == Some(&m.name) {
                        default_for.push("vision".to_string());
                    }
                }

                // All non-embedding Ollama models can be used for language tasks
                if !is_embed {
                    capabilities.push("language".to_string());
                    if m.name == default_gen {
                        default_for.push("language".to_string());
                    }
                }

                models.push(ModelInfo {
                    slug: m.name,
                    provider: "ollama".to_string(),
                    capabilities,
                    default_for,
                    parameter_size: m.parameter_size,
                    quantization: m.quantization,
                    size_bytes: Some(m.size),
                    family: m.family,
                });
            }
        }
        Err(e) => {
            let diagnostic = e.to_string();
            warn!(
                error_len = diagnostic.chars().count(),
                detail = MODEL_DISCOVERY_FAILURE_DETAIL,
                "Failed to discover Ollama models"
            );
        }
    }

    // Add configured models from non-Ollama providers so OpenAI-compatible
    // vLLM/llama.cpp deployments surface their default chat/embedding routes
    // even when the provider's /models response is sparse or unavailable.
    for id in registry.provider_ids() {
        if id == "ollama" {
            continue;
        }
        let Some(provider) = registry.get_provider(id) else {
            continue;
        };

        let mut add_provider_model = |model: String, capability: &str, default_for: Vec<String>| {
            let slug = format!("{}:{}", provider.id, model);
            if !models
                .iter()
                .any(|m| m.slug == slug && m.provider == provider.id)
            {
                models.push(ModelInfo {
                    slug,
                    provider: provider.id.clone(),
                    capabilities: vec![capability.to_string()],
                    default_for,
                    parameter_size: None,
                    quantization: None,
                    size_bytes: None,
                    family: Some(provider.id.clone()),
                });
            }
        };

        if provider
            .capabilities
            .contains(&ProviderCapability::Generation)
        {
            let model =
                match provider.id.as_str() {
                    "openai" => std::env::var("OPENAI_GEN_MODEL")
                        .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                    "openrouter" => std::env::var("OPENROUTER_GEN_MODEL")
                        .unwrap_or_else(|_| "anthropic/claude-sonnet-4".to_string()),
                    "llamacpp" => std::env::var("LLAMACPP_GEN_MODEL")
                        .unwrap_or_else(|_| "default".to_string()),
                    _ => continue,
                };
            let is_default =
                provider.id == registry.default_provider() && model.as_str() == default_gen;
            add_provider_model(
                model,
                "language",
                if is_default {
                    vec!["language".to_string()]
                } else {
                    vec![]
                },
            );
        }

        if provider
            .capabilities
            .contains(&ProviderCapability::Embedding)
        {
            let model = match provider.id.as_str() {
                "openai" => std::env::var("OPENAI_EMBED_MODEL")
                    .unwrap_or_else(|_| "text-embedding-3-small".to_string()),
                "openrouter" => std::env::var("OPENROUTER_EMBED_MODEL")
                    .unwrap_or_else(|_| "text-embedding-3-small".to_string()),
                "llamacpp" => {
                    std::env::var("LLAMACPP_EMBED_MODEL").unwrap_or_else(|_| "default".to_string())
                }
                _ => continue,
            };
            let is_default =
                provider.id == registry.embedding_provider() && model.as_str() == default_embed;
            add_provider_model(
                model,
                "embedding",
                if is_default {
                    vec!["embedding".to_string()]
                } else {
                    vec![]
                },
            );
        }
    }

    // Query Whisper for transcription models
    if let Some(ref transcription) = state.transcription_backend {
        let whisper_model = transcription.model_name().to_string();
        let whisper_base_url = std::env::var(matric_core::defaults::ENV_WHISPER_BASE_URL)
            .unwrap_or_else(|_| matric_core::defaults::DEFAULT_WHISPER_BASE_URL.to_string());

        let client = reqwest::Client::new();
        match client
            .get(format!("{}/v1/models", whisper_base_url))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                        for model_entry in data {
                            if let Some(id) = model_entry.get("id").and_then(|v| v.as_str()) {
                                let is_default = id == whisper_model;
                                models.push(ModelInfo {
                                    slug: id.to_string(),
                                    provider: "whisper".to_string(),
                                    capabilities: vec!["transcription".to_string()],
                                    default_for: if is_default {
                                        vec!["transcription".to_string()]
                                    } else {
                                        vec![]
                                    },
                                    parameter_size: None,
                                    quantization: None,
                                    size_bytes: None,
                                    family: Some("whisper".to_string()),
                                });
                            }
                        }
                    }
                }
            }
            _ => {
                // Fall back to just listing the configured model
                models.push(ModelInfo {
                    slug: whisper_model.clone(),
                    provider: "whisper".to_string(),
                    capabilities: vec!["transcription".to_string()],
                    default_for: vec!["transcription".to_string()],
                    parameter_size: None,
                    quantization: None,
                    size_bytes: None,
                    family: Some("whisper".to_string()),
                });
            }
        }
    }

    // Build provider info from the registry
    let providers: Vec<ProviderInfo> = registry
        .provider_ids()
        .into_iter()
        .filter_map(|id| {
            registry.get_provider(id).map(|p| ProviderInfo {
                id: p.id.clone(),
                capabilities: p.capabilities.iter().map(|c| c.to_string()).collect(),
                is_default: p.is_default,
                health: match p.health {
                    ProviderHealth::Healthy => "healthy".to_string(),
                    ProviderHealth::Unknown => "unknown".to_string(),
                    ProviderHealth::Unhealthy => "unhealthy".to_string(),
                },
            })
        })
        .collect();

    Ok(Json(ListModelsResponse {
        models,
        defaults: ModelDefaults {
            language: default_gen,
            embedding: default_embed,
            vision: default_vision,
            transcription: default_transcription,
        },
        providers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_discovery_failure_detail_is_fixed_and_redacted() {
        let raw_diagnostics = [
            "http://provider.local:11434/api/tags?api_key=secret",
            "postgres://fortemi:secret@db.internal/fortemi",
            "Bearer model-token-secret",
            "/srv/fortemi/provider/cache.json",
        ];

        assert_eq!(
            MODEL_DISCOVERY_FAILURE_DETAIL,
            "Model discovery failed. Check server logs for diagnostics."
        );

        for raw in raw_diagnostics {
            assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains(raw));
        }
        assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains("http://"));
        assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains("postgres://"));
        assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains("Bearer "));
        assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains("/srv/"));
        assert!(!MODEL_DISCOVERY_FAILURE_DETAIL.contains("api_key="));
    }

    #[test]
    fn model_discovery_response_debug_redacts_model_and_provider_identifiers() {
        let model = ModelInfo {
            slug: "qwen-secret@example.com postgres://user:secret@db.internal/model".to_string(),
            provider: "openrouter-sk-live-provider".to_string(),
            capabilities: vec!["language".to_string(), "vision".to_string()],
            default_for: vec!["language".to_string()],
            parameter_size: Some("14B-private-profile".to_string()),
            quantization: Some("Q4_K_M-/srv/fortemi/models".to_string()),
            size_bytes: Some(42),
            family: Some("family-token-shaped-value".to_string()),
        };
        let provider = ProviderInfo {
            id: "provider-with-token-sk-live".to_string(),
            capabilities: vec!["language".to_string()],
            is_default: true,
            health: "healthy-internal-cluster-a".to_string(),
        };
        let response = ListModelsResponse {
            models: vec![model],
            defaults: ModelDefaults {
                language: "operator@example.com/default-language".to_string(),
                embedding: "postgres://user:secret@db.internal/default-embedding".to_string(),
                vision: Some("/srv/fortemi/private/vision-model".to_string()),
                transcription: Some("sk-live-transcription-model".to_string()),
            },
            providers: vec![provider],
        };

        let rendered = format!("{response:?}");
        let rendered_model = format!("{:?}", response.models[0]);
        let rendered_provider = format!("{:?}", response.providers[0]);
        let combined = format!("{rendered}\n{rendered_model}\n{rendered_provider}");
        for forbidden in [
            "qwen-secret@example.com",
            "postgres://user:secret@db.internal/model",
            "openrouter-sk-live-provider",
            "14B-private-profile",
            "Q4_K_M-/srv/fortemi/models",
            "family-token-shaped-value",
            "operator@example.com/default-language",
            "postgres://user:secret@db.internal/default-embedding",
            "/srv/fortemi/private/vision-model",
            "sk-live-transcription-model",
            "provider-with-token-sk-live",
            "healthy-internal-cluster-a",
        ] {
            assert!(
                !combined.contains(forbidden),
                "model discovery Debug leaked {forbidden}: {combined}"
            );
        }

        assert!(rendered.contains("model_count"));
        assert!(rendered.contains("provider_count"));
        assert!(rendered.contains("language_len"));
        assert!(rendered_model.contains("slug_len"));
        assert!(rendered_model.contains("capability_count"));
        assert!(rendered_provider.contains("id_len"));
    }
}

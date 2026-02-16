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
use matric_inference::{OllamaBackend, ProviderHealth};

/// A single model entry with capability metadata.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

/// Summary of a registered inference provider.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

/// Response from the model discovery endpoint.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListModelsResponse {
    /// All available models with capability metadata.
    pub models: Vec<ModelInfo>,
    /// Currently configured default model slugs.
    pub defaults: ModelDefaults,
    /// Registered inference providers.
    pub providers: Vec<ProviderInfo>,
}

/// Default model slugs from server configuration.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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
    // Determine configured defaults from env vars
    let backend = OllamaBackend::from_env();
    let default_gen = matric_core::GenerationBackend::model_name(&backend).to_string();
    let default_embed = matric_core::EmbeddingBackend::model_name(&backend).to_string();
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
            warn!(error = %e, "Failed to discover Ollama models");
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
    let providers: Vec<ProviderInfo> = state
        .provider_registry
        .provider_ids()
        .into_iter()
        .filter_map(|id| {
            state
                .provider_registry
                .get_provider(id)
                .map(|p| ProviderInfo {
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

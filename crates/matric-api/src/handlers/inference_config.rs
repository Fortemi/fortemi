//! Runtime inference configuration management handlers.
//!
//! Provides GET/POST/DELETE endpoints for reading, updating, and resetting
//! inference backend configuration at runtime without a server restart.
//!
//! Configuration precedence (highest → lowest):
//!   db_override → env → default

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::AppState;
use matric_core::defaults::EMBED_DIMENSION;
use matric_core::InferenceBackend as InferenceBackendTrait;
use matric_inference::OllamaBackend;

// =============================================================================
// LOCAL CONFIG TYPES
// =============================================================================

/// Source attribution for a single config value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    DbOverride,
    Env,
    Default,
}

/// A config value with its source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcedValue {
    pub value: String,
    pub source: ConfigSource,
}

/// Ollama config fields with source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcedOllamaConfig {
    pub base_url: SourcedValue,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

/// OpenAI config fields with source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcedOpenAIConfig {
    pub base_url: SourcedValue,
    /// API key redacted: first 8 chars + "...". Null if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SourcedValue>,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

/// llama.cpp config fields with source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcedLlamaCppConfig {
    pub base_url: SourcedValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SourcedValue>,
    pub generation_model: SourcedValue,
    pub embedding_model: SourcedValue,
}

/// Full effective inference config response with source attribution.
#[derive(Debug, Serialize)]
pub struct InferenceConfigResponse {
    pub default_backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama: Option<SourcedOllamaConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<SourcedOpenAIConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llamacpp: Option<SourcedLlamaCppConfig>,
    pub providers: Vec<String>,
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

/// Partial update request body (all fields optional).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateInferenceConfigRequest {
    pub ollama: Option<PartialOllamaConfig>,
    pub openai: Option<PartialOpenAIConfig>,
    pub llamacpp: Option<PartialLlamaCppConfig>,
}

/// Partial Ollama config (all fields optional).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PartialOllamaConfig {
    pub base_url: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

/// Partial OpenAI config (all fields optional).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PartialOpenAIConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

/// Partial llama.cpp config (all fields optional).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PartialLlamaCppConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub generation_model: Option<String>,
    pub embedding_model: Option<String>,
}

/// Query parameters for POST /api/v1/inference/config.
#[derive(Debug, Deserialize)]
pub struct UpdateConfigQuery {
    /// If true, probe the Ollama endpoint for reachability before persisting.
    #[serde(default)]
    pub validate: bool,
}

/// Response from POST /api/v1/inference/config.
#[derive(Debug, Serialize)]
pub struct UpdateInferenceConfigResponse {
    pub status: String,
    pub previous: Value,
    pub current: Value,
    pub warnings: Vec<String>,
}

/// Response from DELETE /api/v1/inference/config.
#[derive(Debug, Serialize)]
pub struct ResetInferenceConfigResponse {
    pub status: String,
    pub effective: Value,
}

// =============================================================================
// HELPERS
// =============================================================================

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(serde_json::json!({"error": msg.into()})))
}

/// Redact an API key: show first 8 chars + "..." or the full value if shorter.
fn redact_api_key(key: &str) -> String {
    if key.len() <= 8 {
        key.to_string()
    } else {
        format!("{}...", &key[..8])
    }
}

/// Validate an Ollama base_url and model names. Returns an error message on failure.
fn validate_ollama(base_url: &str, gen_model: &str, embed_model: &str) -> Result<(), String> {
    if base_url.is_empty() {
        return Err("Ollama base_url cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(format!(
            "Ollama base_url must start with http:// or https://, got: {base_url}"
        ));
    }
    if gen_model.is_empty() {
        return Err("Ollama generation_model cannot be empty".to_string());
    }
    if embed_model.is_empty() {
        return Err("Ollama embedding_model cannot be empty".to_string());
    }
    Ok(())
}

/// Validate an OpenAI base_url and model names. Returns an error message on failure.
fn validate_openai(base_url: &str, gen_model: &str, embed_model: &str) -> Result<(), String> {
    if base_url.is_empty() {
        return Err("OpenAI base_url cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(format!(
            "OpenAI base_url must start with http:// or https://, got: {base_url}"
        ));
    }
    if gen_model.is_empty() {
        return Err("OpenAI generation_model cannot be empty".to_string());
    }
    if embed_model.is_empty() {
        return Err("OpenAI embedding_model cannot be empty".to_string());
    }
    Ok(())
}

/// Env-sourced Ollama defaults (mirrors InferenceConfig::from_env logic).
struct EnvOllama {
    base_url: String,
    generation_model: String,
    embedding_model: String,
}

impl EnvOllama {
    fn read() -> Self {
        let base_url = std::env::var("MATRIC_OLLAMA_URL")
            .or_else(|_| std::env::var("OLLAMA_BASE"))
            .or_else(|_| std::env::var("OLLAMA_URL"))
            .or_else(|_| std::env::var("OLLAMA_HOST"))
            .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let generation_model = std::env::var("MATRIC_OLLAMA_GENERATION_MODEL")
            .or_else(|_| std::env::var("OLLAMA_GEN_MODEL"))
            .unwrap_or_else(|_| matric_core::defaults::GEN_MODEL.to_string());
        let embedding_model = std::env::var("MATRIC_OLLAMA_EMBEDDING_MODEL")
            .or_else(|_| std::env::var("OLLAMA_EMBED_MODEL"))
            .unwrap_or_else(|_| matric_core::defaults::EMBED_MODEL.to_string());
        Self {
            base_url,
            generation_model,
            embedding_model,
        }
    }
}

/// Default Ollama values (compile-time constants from matric_core::defaults).
struct DefaultOllama;

impl DefaultOllama {
    fn base_url() -> &'static str {
        matric_core::defaults::OLLAMA_URL
    }
    fn generation_model() -> &'static str {
        matric_core::defaults::GEN_MODEL
    }
    fn embedding_model() -> &'static str {
        matric_core::defaults::EMBED_MODEL
    }
}

/// Pick the first non-empty value across db > env > default, returning with source.
fn pick(db_val: Option<&str>, env_val: &str, default_val: &str) -> SourcedValue {
    if let Some(v) = db_val.filter(|s| !s.is_empty()) {
        return SourcedValue {
            value: v.to_string(),
            source: ConfigSource::DbOverride,
        };
    }
    if !env_val.is_empty() && env_val != default_val {
        return SourcedValue {
            value: env_val.to_string(),
            source: ConfigSource::Env,
        };
    }
    SourcedValue {
        value: default_val.to_string(),
        source: ConfigSource::Default,
    }
}

/// Read the raw DB override blob from user_config.
async fn read_db_override(pool: &sqlx::PgPool) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> =
        sqlx::query_as("SELECT value FROM user_config WHERE key = 'inference_override'")
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// Build the effective sourced config by layering db > env > default.
fn build_effective_config(db: Option<&Value>) -> InferenceConfigResponse {
    let env_o = EnvOllama::read();
    let db_ollama = db.and_then(|v| v.get("ollama"));
    let db_openai = db.and_then(|v| v.get("openai"));

    // Ollama is always present (it is the default backend).
    let ollama = Some(SourcedOllamaConfig {
        base_url: pick(
            db_ollama
                .and_then(|o| o.get("base_url"))
                .and_then(|v| v.as_str()),
            &env_o.base_url,
            DefaultOllama::base_url(),
        ),
        generation_model: pick(
            db_ollama
                .and_then(|o| o.get("generation_model"))
                .and_then(|v| v.as_str()),
            &env_o.generation_model,
            DefaultOllama::generation_model(),
        ),
        embedding_model: pick(
            db_ollama
                .and_then(|o| o.get("embedding_model"))
                .and_then(|v| v.as_str()),
            &env_o.embedding_model,
            DefaultOllama::embedding_model(),
        ),
    });

    // OpenAI only shown if the DB override or env has it configured.
    let openai = if db_openai.is_some() {
        let db_base = db_openai
            .and_then(|o| o.get("base_url"))
            .and_then(|v| v.as_str());
        let db_gen = db_openai
            .and_then(|o| o.get("generation_model"))
            .and_then(|v| v.as_str());
        let db_embed = db_openai
            .and_then(|o| o.get("embedding_model"))
            .and_then(|v| v.as_str());
        let db_key = db_openai
            .and_then(|o| o.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let default_openai_url = "https://api.openai.com/v1";
        let default_gen = "gpt-4o-mini";
        let default_embed = "text-embedding-3-small";

        let api_key = db_key.map(|k| SourcedValue {
            value: redact_api_key(k),
            source: ConfigSource::DbOverride,
        });

        Some(SourcedOpenAIConfig {
            base_url: pick(db_base, "", default_openai_url),
            api_key,
            generation_model: pick(db_gen, "", default_gen),
            embedding_model: pick(db_embed, "", default_embed),
        })
    } else {
        None
    };

    // llama.cpp only shown if the DB override or env has it configured.
    let db_llamacpp = db.and_then(|v| v.get("llamacpp"));
    let env_llamacpp_url = std::env::var("LLAMACPP_BASE_URL").unwrap_or_default();
    let llamacpp = if db_llamacpp.is_some() || !env_llamacpp_url.is_empty() {
        let db_base = db_llamacpp
            .and_then(|o| o.get("base_url"))
            .and_then(|v| v.as_str());
        let db_gen = db_llamacpp
            .and_then(|o| o.get("generation_model"))
            .and_then(|v| v.as_str());
        let db_embed = db_llamacpp
            .and_then(|o| o.get("embedding_model"))
            .and_then(|v| v.as_str());
        let db_key = db_llamacpp
            .and_then(|o| o.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let default_url = matric_core::defaults::LLAMACPP_URL;
        let default_gen = "default";
        let default_embed = "default";

        let api_key = db_key.map(|k| SourcedValue {
            value: redact_api_key(k),
            source: ConfigSource::DbOverride,
        });

        Some(SourcedLlamaCppConfig {
            base_url: pick(db_base, &env_llamacpp_url, default_url),
            api_key,
            generation_model: pick(db_gen, "", default_gen),
            embedding_model: pick(db_embed, "", default_embed),
        })
    } else {
        None
    };

    let mut providers = vec!["ollama".to_string()];
    if openai.is_some() {
        providers.push("openai".to_string());
    }
    if llamacpp.is_some() {
        providers.push("llamacpp".to_string());
    }

    InferenceConfigResponse {
        default_backend: "ollama".to_string(),
        ollama,
        openai,
        llamacpp,
        providers,
    }
}

// =============================================================================
// HANDLERS
// =============================================================================

/// GET /api/v1/inference/config — return effective config with source attribution.
///
/// Layers DB overrides on top of environment variables on top of compiled defaults.
/// Each field indicates whether its value came from the database, an env var, or the default.
#[utoipa::path(
    get,
    path = "/api/v1/inference/config",
    tag = "Inference",
    responses(
        (status = 200, description = "Effective inference configuration"),
        (status = 500, description = "Database error"),
    )
)]
pub async fn get_inference_config(State(state): State<AppState>) -> impl IntoResponse {
    let db_override = match read_db_override(&state.db.pool).await {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to read inference_override from user_config");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {e}"),
            )
            .into_response();
        }
    };

    let effective = build_effective_config(db_override.as_ref());
    (
        StatusCode::OK,
        Json(serde_json::to_value(effective).unwrap()),
    )
        .into_response()
}

/// POST /api/v1/inference/config — apply partial override and rebuild backend.
///
/// Accepts a partial config (any fields may be absent). Merges with any existing
/// DB override, validates, persists to `user_config`, and rebuilds the Ollama backend
/// in the `inference_runtime` slot. Pass `?validate=true` to probe endpoint reachability
/// before persisting.
#[utoipa::path(
    post,
    path = "/api/v1/inference/config",
    tag = "Inference",
    request_body = UpdateInferenceConfigRequest,
    params(
        ("validate" = Option<bool>, Query, description = "Pre-check endpoint reachability before persisting"),
    ),
    responses(
        (status = 200, description = "Config applied"),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Database or backend error"),
    )
)]
pub async fn update_inference_config(
    State(state): State<AppState>,
    Query(params): Query<UpdateConfigQuery>,
    Json(req): Json<UpdateInferenceConfigRequest>,
) -> impl IntoResponse {
    // 1. Read existing DB override as baseline.
    let existing_db = match read_db_override(&state.db.pool).await {
        Ok(v) => v.unwrap_or(serde_json::json!({})),
        Err(e) => {
            warn!(error = %e, "Failed to read existing inference_override");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {e}"),
            )
            .into_response();
        }
    };

    // 2. Capture previous effective config for the response.
    let previous = serde_json::to_value(build_effective_config(Some(&existing_db))).unwrap();

    // 3. Merge new values into existing DB override blob.
    let mut merged = existing_db;

    if let Some(partial_ollama) = &req.ollama {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("ollama")
            .or_insert(serde_json::json!({}));

        // Resolve current values for fields not being overridden.
        let env_o = EnvOllama::read();
        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let merged_base = partial_ollama.base_url.clone().unwrap_or_else(|| {
            if cur_base.is_empty() {
                env_o.base_url.clone()
            } else {
                cur_base.to_string()
            }
        });
        let merged_gen = partial_ollama.generation_model.clone().unwrap_or_else(|| {
            if cur_gen.is_empty() {
                env_o.generation_model.clone()
            } else {
                cur_gen.to_string()
            }
        });
        let merged_embed = partial_ollama.embedding_model.clone().unwrap_or_else(|| {
            if cur_embed.is_empty() {
                env_o.embedding_model.clone()
            } else {
                cur_embed.to_string()
            }
        });

        if let Err(e) = validate_ollama(&merged_base, &merged_gen, &merged_embed) {
            return err(
                StatusCode::BAD_REQUEST,
                format!("Ollama config invalid: {e}"),
            )
            .into_response();
        }

        // Optional pre-flight reachability probe.
        if params.validate {
            let probe = OllamaBackend::with_config(
                merged_base.clone(),
                merged_embed.clone(),
                merged_gen.clone(),
                EMBED_DIMENSION,
            );
            match probe.health_check().await {
                Ok(true) => {}
                Ok(false) => {
                    return err(
                        StatusCode::BAD_REQUEST,
                        format!("Ollama endpoint not healthy: {merged_base}"),
                    )
                    .into_response();
                }
                Err(e) => {
                    return err(
                        StatusCode::BAD_REQUEST,
                        format!("Cannot reach Ollama at {merged_base}: {e}"),
                    )
                    .into_response();
                }
            }
        }

        // Write changed fields into the blob.
        let obj = entry.as_object_mut().expect("json object");
        if partial_ollama.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base.clone()));
        }
        if partial_ollama.generation_model.is_some() {
            obj.insert(
                "generation_model".to_string(),
                Value::String(merged_gen.clone()),
            );
        }
        if partial_ollama.embedding_model.is_some() {
            obj.insert(
                "embedding_model".to_string(),
                Value::String(merged_embed.clone()),
            );
        }

        // Hot-swap Ollama backend.
        let new_backend = std::sync::Arc::new(OllamaBackend::with_config(
            merged_base,
            merged_embed,
            merged_gen,
            EMBED_DIMENSION,
        ));
        let mut rt = state.inference_runtime.write().unwrap();
        rt.generation_backend = Some(new_backend);
        info!("Ollama backend hot-swapped from POST /api/v1/inference/config");
    }

    if let Some(partial_openai) = &req.openai {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("openai")
            .or_insert(serde_json::json!({}));

        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_key = entry
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        let default_openai_url = "https://api.openai.com/v1";
        let merged_base = partial_openai.base_url.clone().unwrap_or_else(|| {
            if cur_base.is_empty() {
                default_openai_url.to_string()
            } else {
                cur_base.to_string()
            }
        });
        let merged_gen = partial_openai.generation_model.clone().unwrap_or_else(|| {
            if cur_gen.is_empty() {
                "gpt-4o-mini".to_string()
            } else {
                cur_gen.to_string()
            }
        });
        let merged_embed = partial_openai.embedding_model.clone().unwrap_or_else(|| {
            if cur_embed.is_empty() {
                "text-embedding-3-small".to_string()
            } else {
                cur_embed.to_string()
            }
        });

        if let Err(e) = validate_openai(&merged_base, &merged_gen, &merged_embed) {
            return err(
                StatusCode::BAD_REQUEST,
                format!("OpenAI config invalid: {e}"),
            )
            .into_response();
        }

        let obj = entry.as_object_mut().expect("json object");
        if partial_openai.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base));
        }
        if let Some(ref key) = partial_openai.api_key.as_deref().or(cur_key.as_deref()) {
            obj.insert("api_key".to_string(), Value::String(key.to_string()));
        }
        if partial_openai.generation_model.is_some() {
            obj.insert("generation_model".to_string(), Value::String(merged_gen));
        }
        if partial_openai.embedding_model.is_some() {
            obj.insert("embedding_model".to_string(), Value::String(merged_embed));
        }
    }

    if let Some(partial_llamacpp) = &req.llamacpp {
        let entry = merged
            .as_object_mut()
            .expect("json object")
            .entry("llamacpp")
            .or_insert(serde_json::json!({}));

        let cur_base = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let cur_gen = entry
            .get("generation_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_embed = entry
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cur_key = entry
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        let merged_base = partial_llamacpp
            .base_url
            .clone()
            .unwrap_or_else(|| {
                if cur_base.is_empty() {
                    matric_core::defaults::LLAMACPP_URL.to_string()
                } else {
                    cur_base.to_string()
                }
            });
        let merged_gen = partial_llamacpp
            .generation_model
            .clone()
            .unwrap_or_else(|| {
                if cur_gen.is_empty() { "default".to_string() } else { cur_gen.to_string() }
            });
        let merged_embed = partial_llamacpp
            .embedding_model
            .clone()
            .unwrap_or_else(|| {
                if cur_embed.is_empty() { "default".to_string() } else { cur_embed.to_string() }
            });

        let obj = entry.as_object_mut().expect("json object");
        if partial_llamacpp.base_url.is_some() {
            obj.insert("base_url".to_string(), Value::String(merged_base));
        }
        if let Some(ref key) = partial_llamacpp.api_key.as_deref().or(cur_key.as_deref()) {
            obj.insert("api_key".to_string(), Value::String(key.to_string()));
        }
        if partial_llamacpp.generation_model.is_some() {
            obj.insert("generation_model".to_string(), Value::String(merged_gen));
        }
        if partial_llamacpp.embedding_model.is_some() {
            obj.insert("embedding_model".to_string(), Value::String(merged_embed));
        }
    }

    // Rebuild provider registry from merged config so all providers are hot-swapped.
    {
        let mut new_registry = matric_inference::ProviderRegistry::from_env();

        // Layer DB overrides for llama.cpp (env registration may have been empty at startup).
        if let Some(db_lc) = merged.get("llamacpp") {
            if let Some(base_url) = db_lc.get("base_url").and_then(|v| v.as_str()) {
                if !base_url.is_empty() && !new_registry.has_provider("llamacpp") {
                    new_registry.register(matric_inference::ProviderConfig {
                        id: "llamacpp".to_string(),
                        base_url: base_url.to_string(),
                        api_key: db_lc
                            .get("api_key")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                        capabilities: vec![
                            matric_inference::ProviderCapability::Generation,
                            matric_inference::ProviderCapability::Embedding,
                        ],
                        timeout: std::time::Duration::from_secs(300),
                        is_default: false,
                        health: matric_inference::ProviderHealth::Unknown,
                        http_referer: None,
                        x_title: None,
                    });
                }
            }
        }

        let mut rt = state.inference_runtime.write().unwrap();
        rt.provider_registry = std::sync::Arc::new(new_registry);
        info!("Provider registry rebuilt from POST /api/v1/inference/config");
    }

    // 4. Persist merged config to DB.
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO user_config (key, value)
        VALUES ('inference_override', $1)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(&merged)
    .execute(&state.db.pool)
    .await
    {
        warn!(error = %e, "Failed to persist inference_override");
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
        .into_response();
    }

    info!("inference_override persisted via POST /api/v1/inference/config");

    // 5. Build and return current effective config.
    let current = serde_json::to_value(build_effective_config(Some(&merged))).unwrap();
    let response = UpdateInferenceConfigResponse {
        status: "applied".to_string(),
        previous,
        current,
        warnings: vec![],
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
        .into_response()
}

/// DELETE /api/v1/inference/config — remove DB overrides and revert to env/defaults.
///
/// Deletes the `inference_override` row from `user_config`, then rebuilds the Ollama
/// backend from environment variables and swaps it into `inference_runtime`.
#[utoipa::path(
    delete,
    path = "/api/v1/inference/config",
    tag = "Inference",
    responses(
        (status = 200, description = "Config reset to env/defaults"),
        (status = 500, description = "Database or backend error"),
    )
)]
pub async fn delete_inference_config(State(state): State<AppState>) -> impl IntoResponse {
    // 1. Delete DB override row.
    if let Err(e) = sqlx::query("DELETE FROM user_config WHERE key = 'inference_override'")
        .execute(&state.db.pool)
        .await
    {
        warn!(error = %e, "Failed to delete inference_override");
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
        .into_response();
    }

    info!("inference_override deleted via DELETE /api/v1/inference/config");

    // 2. Rebuild backend from env and hot-swap.
    let new_backend = std::sync::Arc::new(OllamaBackend::from_env());
    {
        let mut rt = state.inference_runtime.write().unwrap();
        rt.generation_backend = Some(new_backend);
    }

    // 3. Return effective config (now env/default only).
    let effective = serde_json::to_value(build_effective_config(None)).unwrap();
    let response = ResetInferenceConfigResponse {
        status: "reset".to_string(),
        effective,
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap()),
    )
        .into_response()
}

// =============================================================================
// TEST-CONNECTION ENDPOINT
// =============================================================================

/// Request body for the connection test endpoint.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TestConnectionRequest {
    /// Base URL to probe (e.g. "http://gpu-server:11434").
    pub base_url: String,
    /// Protocol hint: "ollama", "openai", or "auto" (try both).
    #[serde(default = "default_test_provider")]
    pub provider: String,
    /// Bearer token for OpenAI-compatible endpoints (not required for Ollama).
    #[serde(default)]
    pub api_key: Option<String>,
    /// Request timeout in seconds (default: 10, clamped to 1–120).
    #[serde(default = "default_test_timeout")]
    pub timeout_secs: u64,
}

fn default_test_provider() -> String {
    "auto".to_string()
}

fn default_test_timeout() -> u64 {
    10
}

/// Detected capability flags for the remote provider.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DetectedCapabilities {
    pub generation: bool,
    pub embedding: bool,
    pub vision: bool,
}

/// Connection test response — successful or not.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TestConnectionResponse {
    pub reachable: bool,
    /// Protocol detected: "ollama" or "openai". Null when unreachable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_provider: Option<String>,
    /// Ollama version string (e.g. "0.6.1"), when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_version: Option<String>,
    /// Model names returned by the remote endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,
    /// Round-trip latency in milliseconds for the detection probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Inferred capability flags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<DetectedCapabilities>,
    /// Human-readable error message when `reachable` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Actionable recovery suggestions when `reachable` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

// Internal detection result — not serialized directly.
struct DetectionResult {
    provider: String,
    ollama_version: Option<String>,
    models: Vec<String>,
    latency_ms: u64,
}

/// Test connectivity to an inference endpoint and auto-detect its protocol.
///
/// Probes the given URL using either the Ollama or OpenAI-compatible protocol
/// (or both when `provider` is "auto"). Returns available models and detected
/// capabilities on success, or a structured error with actionable suggestions
/// on failure.
#[utoipa::path(
    post,
    path = "/api/v1/inference/test-connection",
    tag = "Inference",
    request_body = TestConnectionRequest,
    responses(
        (status = 200, description = "Connection test result (check reachable field)", body = TestConnectionResponse),
        (status = 400, description = "Invalid request body"),
    )
)]
pub async fn test_connection(
    State(_state): State<AppState>,
    Json(req): Json<TestConnectionRequest>,
) -> (StatusCode, Json<TestConnectionResponse>) {
    let base_url = req.base_url.trim_end_matches('/').to_string();
    let timeout = std::time::Duration::from_secs(req.timeout_secs.clamp(1, 120));

    if base_url.is_empty()
        || (!base_url.starts_with("http://") && !base_url.starts_with("https://"))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(TestConnectionResponse {
                reachable: false,
                detected_provider: None,
                ollama_version: None,
                available_models: None,
                latency_ms: None,
                capabilities: None,
                error: Some("base_url must start with http:// or https://".to_string()),
                suggestions: Some(vec![
                    "Provide a full URL, e.g. http://localhost:11434".to_string()
                ]),
            }),
        );
    }

    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TestConnectionResponse {
                    reachable: false,
                    detected_provider: None,
                    ollama_version: None,
                    available_models: None,
                    latency_ms: None,
                    capabilities: None,
                    error: Some(format!("Failed to build HTTP client: {e}")),
                    suggestions: None,
                }),
            );
        }
    };

    info!(
        base_url = %base_url,
        provider = %req.provider,
        "Testing inference endpoint connection"
    );

    // Choose probe order based on hint or URL pattern.
    let providers_to_try: Vec<&str> = match req.provider.as_str() {
        "ollama" => vec!["ollama"],
        "openai" => vec!["openai"],
        _ => match auto_detect_from_url(&base_url) {
            Some("ollama") => vec!["ollama", "openai"],
            Some("openai") => vec!["openai", "ollama"],
            _ => vec!["ollama", "openai"],
        },
    };

    for provider in providers_to_try {
        match detect_provider(&client, &base_url, provider, req.api_key.as_deref()).await {
            Ok(result) => {
                let caps = infer_capabilities(provider, &result.models);
                debug!(
                    provider = %result.provider,
                    models = result.models.len(),
                    latency_ms = result.latency_ms,
                    "Inference provider detected"
                );
                return (
                    StatusCode::OK,
                    Json(TestConnectionResponse {
                        reachable: true,
                        detected_provider: Some(result.provider),
                        ollama_version: result.ollama_version,
                        available_models: Some(result.models),
                        latency_ms: Some(result.latency_ms),
                        capabilities: Some(caps),
                        error: None,
                        suggestions: None,
                    }),
                );
            }
            Err(e) => {
                debug!(provider = %provider, error = %e, "Provider probe failed");
            }
        }
    }

    let (error_msg, suggestions) = classify_connection_error(&base_url);
    warn!(base_url = %base_url, error = %error_msg, "Inference endpoint unreachable");

    (
        StatusCode::OK,
        Json(TestConnectionResponse {
            reachable: false,
            detected_provider: None,
            ollama_version: None,
            available_models: None,
            latency_ms: None,
            capabilities: None,
            error: Some(error_msg),
            suggestions: Some(suggestions),
        }),
    )
}

// =============================================================================
// PROVIDER DETECTION
// =============================================================================

/// Probe a URL for a specific provider protocol.
async fn detect_provider(
    client: &reqwest::Client,
    base_url: &str,
    provider: &str,
    api_key: Option<&str>,
) -> Result<DetectionResult, String> {
    match provider {
        "ollama" => probe_ollama(client, base_url).await,
        "openai" => probe_openai(client, base_url, api_key).await,
        other => Err(format!("Unknown provider: {other}")),
    }
}

async fn probe_ollama(client: &reqwest::Client, base_url: &str) -> Result<DetectionResult, String> {
    let t0 = Instant::now();

    let resp = client
        .get(format!("{base_url}/api/tags"))
        .send()
        .await
        .map_err(|e| classify_reqwest_error(&e))?;

    let latency_ms = t0.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Err(format!("GET /api/tags returned HTTP {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid JSON from /api/tags: {e}"))?;

    let models_arr = body
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Response missing 'models' array".to_string())?;

    let models: Vec<String> = models_arr
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    // Best-effort version fetch (non-fatal).
    let ollama_version = fetch_ollama_version(client, base_url).await;

    Ok(DetectionResult {
        provider: "ollama".to_string(),
        ollama_version,
        models,
        latency_ms,
    })
}

async fn fetch_ollama_version(client: &reqwest::Client, base_url: &str) -> Option<String> {
    let resp = client
        .get(format!("{base_url}/api/version"))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("version")
        .and_then(|s| s.as_str())
        .map(String::from)
}

async fn probe_openai(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<DetectionResult, String> {
    let t0 = Instant::now();

    let mut req = client.get(format!("{base_url}/v1/models"));
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.map_err(|e| classify_reqwest_error(&e))?;

    let latency_ms = t0.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Err(format!("GET /v1/models returned HTTP {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid JSON from /v1/models: {e}"))?;

    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "Response missing 'data' array".to_string())?;

    let models: Vec<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    Ok(DetectionResult {
        provider: "openai".to_string(),
        ollama_version: None,
        models,
        latency_ms,
    })
}

// =============================================================================
// CAPABILITY INFERENCE
// =============================================================================

fn infer_capabilities(provider: &str, models: &[String]) -> DetectedCapabilities {
    match provider {
        "ollama" => DetectedCapabilities {
            generation: models.iter().any(|m| !is_embedding_model(m)),
            embedding: models.iter().any(|m| is_embedding_model(m)),
            vision: models.iter().any(|m| is_vision_model(m)),
        },
        // OpenAI-compatible: assume generation and embedding are always available.
        _ => DetectedCapabilities {
            generation: !models.is_empty(),
            embedding: true,
            vision: false,
        },
    }
}

fn is_embedding_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("embed")
        || lower.contains("nomic")
        || lower.contains("mxbai")
        || lower.contains("bge")
        || lower.contains("minilm")
        || lower.contains("e5-")
        || lower.contains("gte-")
}

fn is_vision_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    // qwen3.5:* is natively multimodal (see CLAUDE.md OLLAMA_VISION_MODEL)
    lower.contains("llava")
        || lower.contains("bakllava")
        || lower.contains("moondream")
        || lower.contains("minicpm-v")
        || lower.contains("-vl")
        || lower.contains("vision")
        || lower.starts_with("qwen3.5:")
}

// =============================================================================
// URL-BASED INITIAL GUESS
// =============================================================================

/// Return a provider hint from URL patterns alone, without any network call.
///
/// Used to order probe attempts in "auto" mode so the most-likely protocol is
/// tried first.
pub fn auto_detect_from_url(url: &str) -> Option<&'static str> {
    if url.contains(":11434") {
        return Some("ollama");
    }
    if url.contains("api.openai.com") || url.contains("openrouter.ai") {
        return Some("openai");
    }
    if url.contains("/v1") {
        return Some("openai");
    }
    None
}

// =============================================================================
// ERROR HELPERS
// =============================================================================

fn classify_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "request timed out".to_string()
    } else if e.is_connect() {
        "connection refused".to_string()
    } else {
        format!("{e}")
    }
}

fn classify_connection_error(base_url: &str) -> (String, Vec<String>) {
    let is_local = base_url.contains("localhost") || base_url.contains("127.0.0.1");

    let error_msg = "Could not connect to the inference endpoint".to_string();

    let mut suggestions = vec![
        format!("Verify the endpoint is reachable: curl {base_url}/api/tags"),
        "Check that the inference server is running".to_string(),
    ];

    if is_local && base_url.contains(":11434") {
        suggestions.insert(
            0,
            "Check that Ollama is running: systemctl status ollama".to_string(),
        );
    } else if is_local {
        suggestions.insert(
            0,
            "For Ollama, the default port is 11434 — try http://localhost:11434".to_string(),
        );
    }

    // Warn if no port is present after the host.
    let after_scheme = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_part = after_scheme.split('/').next().unwrap_or("");
    if !host_part.contains(':') {
        suggestions
            .push("Ensure the URL includes the port number (e.g. http://host:11434)".to_string());
    }

    if base_url.contains("docker") || base_url.contains("container") {
        suggestions
            .push("If using Docker, ensure the container is on the same network".to_string());
    }

    (error_msg, suggestions)
}

// =============================================================================
// UNIT TESTS
// =============================================================================

#[cfg(test)]
mod tests_connection {
    use super::*;

    #[test]
    fn auto_detect_ollama_by_port() {
        assert_eq!(
            auto_detect_from_url("http://localhost:11434"),
            Some("ollama")
        );
        assert_eq!(
            auto_detect_from_url("http://gpu-server:11434"),
            Some("ollama")
        );
    }

    #[test]
    fn auto_detect_openai_by_domain() {
        assert_eq!(
            auto_detect_from_url("https://api.openai.com/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_openrouter() {
        assert_eq!(
            auto_detect_from_url("https://openrouter.ai/api/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_v1_path() {
        assert_eq!(
            auto_detect_from_url("http://myserver:8080/v1"),
            Some("openai")
        );
    }

    #[test]
    fn auto_detect_unknown_returns_none() {
        assert_eq!(auto_detect_from_url("http://myserver:8080"), None);
    }

    #[test]
    fn embedding_model_patterns() {
        assert!(is_embedding_model("nomic-embed-text"));
        assert!(is_embedding_model("mxbai-embed-large"));
        assert!(is_embedding_model("bge-small-en"));
        assert!(is_embedding_model("text-embedding-3-small"));
        assert!(!is_embedding_model("qwen3:8b"));
        assert!(!is_embedding_model("llama3.2:3b"));
    }

    #[test]
    fn vision_model_patterns() {
        assert!(is_vision_model("llava:34b"));
        assert!(is_vision_model("moondream:latest"));
        assert!(is_vision_model("qwen3.5:9b"));
        assert!(is_vision_model("minicpm-v:8b"));
        assert!(!is_vision_model("qwen3:8b")); // qwen3 (no .5) is not vision
        assert!(!is_vision_model("nomic-embed-text"));
    }

    #[test]
    fn ollama_mixed_models_all_caps() {
        let models = vec![
            "qwen3:8b".to_string(),
            "nomic-embed-text".to_string(),
            "llava:13b".to_string(),
        ];
        let caps = infer_capabilities("ollama", &models);
        assert!(caps.generation);
        assert!(caps.embedding);
        assert!(caps.vision);
    }

    #[test]
    fn ollama_embed_only_no_generation() {
        let models = vec!["nomic-embed-text".to_string()];
        let caps = infer_capabilities("ollama", &models);
        assert!(!caps.generation);
        assert!(caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn ollama_empty_models_no_caps() {
        let caps = infer_capabilities("ollama", &[]);
        assert!(!caps.generation);
        assert!(!caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn openai_always_has_generation_and_embedding() {
        let models = vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()];
        let caps = infer_capabilities("openai", &models);
        assert!(caps.generation);
        assert!(caps.embedding);
        assert!(!caps.vision);
    }

    #[test]
    fn error_for_ollama_local_suggests_systemctl() {
        let (msg, suggestions) = classify_connection_error("http://localhost:11434");
        assert!(!msg.is_empty());
        assert!(suggestions
            .iter()
            .any(|s| s.contains("systemctl status ollama")));
    }

    #[test]
    fn error_for_url_without_port_suggests_port() {
        let (_, suggestions) = classify_connection_error("http://myserver");
        assert!(suggestions.iter().any(|s| s.contains("port")));
    }

    #[test]
    fn error_for_url_with_port_no_redundant_hint() {
        let (_, suggestions) = classify_connection_error("http://myserver:11434");
        assert!(!suggestions.iter().any(|s| s.contains("port number")));
    }
}

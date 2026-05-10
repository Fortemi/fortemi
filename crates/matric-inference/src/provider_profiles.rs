//! Static catalog of well-known inference provider profiles.
//!
//! A `ProviderProfile` captures the metadata Fortemi knows about a named
//! provider — its wire protocol family, default base URL, capability
//! footprint, env-var conventions for credential lookup, recommended default
//! models, and any per-provider header injection rules. The catalog is a
//! `&'static [ProviderProfile]` so adding a new well-known provider is a
//! single-file PR with no parser surface to maintain.
//!
//! This module is metadata only. Constructing a live backend from a profile
//! happens in `provider.rs` (`ProviderRegistry::from_env`) and the per-backend
//! modules (`ollama.rs`, `openai/`).
//!
//! # Backend × profile model
//!
//! Two **backends** (wire protocols):
//!
//! - [`BackendKind::Ollama`] — Ollama's native `/api/generate`,
//!   `/api/embeddings` shape.
//! - [`BackendKind::OpenAICompatible`] — the standard
//!   `/v1/chat/completions`, `/v1/embeddings` shape used by OpenAI, OpenRouter,
//!   llama.cpp, vLLM, LiteLLM, Together, Groq, and many others.
//!
//! Multiple **profiles** route through one of those backends. v1 ships the
//! four advertised in the README:
//!
//! | Profile      | Backend          | Embeddings | API key |
//! |--------------|------------------|------------|---------|
//! | `ollama`     | Ollama           | yes        | none    |
//! | `openai`     | OpenAICompatible | yes        | yes     |
//! | `openrouter` | OpenAICompatible | no         | yes     |
//! | `llamacpp`   | OpenAICompatible | depends    | optional|
//!
//! Future profiles (vLLM, LiteLLM, LocalAI, Groq, Together, ...) are 5-line
//! additions to [`PROVIDER_PROFILES`] — no enum touching, no parser surface.
//!
//! # Tracking
//!
//! Filed under issue #654. PR 1 introduces the catalog without changing
//! behavior; PR 2 wires it into the API surface (`/api/v1/inference/*`,
//! `manage_inference` MCP tool); PR 3 updates docs.

use crate::provider::ProviderCapability;

/// Wire protocol family used by a provider profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Ollama's native protocol (`/api/generate`, `/api/embeddings`).
    Ollama,
    /// OpenAI-compatible protocol (`/v1/chat/completions`, `/v1/embeddings`).
    /// Covers OpenAI proper, OpenRouter, llama.cpp, vLLM, LiteLLM, etc.
    OpenAICompatible,
}

/// How a profile sources the value for an extra HTTP header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileHeaderSource {
    /// Constant default value, overridable by the named env var if it is set
    /// and non-empty. The header is always emitted (default takes effect when
    /// the env var is unset).
    Default {
        value: &'static str,
        env_var: Option<&'static str>,
    },
    /// Header is only emitted when the named env var is set and non-empty.
    /// Suitable for headers Fortemi has no business defaulting on the user's
    /// behalf (custom organization IDs, debug routing tags, etc.).
    EnvOnly { env_var: &'static str },
}

/// Env var name conventions for a provider profile's credential lookup.
///
/// These names are documented in `.env.example` and the README; profiles
/// declare them here so the runtime knows which variables to consult when
/// constructing a live backend.
#[derive(Debug, Clone, Copy)]
pub struct ProfileEnvVars {
    /// Env var holding the API key. `None` for providers that don't require
    /// auth (Ollama, sometimes llama.cpp).
    pub api_key: Option<&'static str>,
    /// Env var holding the base URL override.
    pub base_url: Option<&'static str>,
    /// Env var holding the request timeout in seconds.
    pub timeout: Option<&'static str>,
    /// Env var overriding the default generation model.
    pub generation_model: Option<&'static str>,
    /// Env var overriding the default embedding model.
    pub embedding_model: Option<&'static str>,
}

/// Static metadata describing a well-known inference provider.
///
/// One entry per id; lookups by id via [`lookup`] are stable.
#[derive(Debug, Clone, Copy)]
pub struct ProviderProfile {
    /// Stable identifier (e.g. `"ollama"`, `"openai"`, `"openrouter"`,
    /// `"llamacpp"`). This is the value the operator uses in
    /// `MATRIC_INFERENCE_DEFAULT` and in slug prefixes like
    /// `openrouter:anthropic/claude-sonnet-4`.
    pub id: &'static str,

    /// Human-readable name for UI surfaces (provider pickers, MCP tool
    /// descriptions, log lines). Free-form, not parsed.
    pub display_name: &'static str,

    /// Wire protocol family.
    pub backend: BackendKind,

    /// Default base URL when the operator hasn't set the per-profile base-URL
    /// env var. `None` when no sensible default exists (e.g. self-hosted
    /// vLLM/LiteLLM where the URL must come from the operator).
    pub default_base_url: Option<&'static str>,

    /// `true` if this profile cannot operate without an API key.
    pub requires_api_key: bool,

    /// Capabilities this profile claims to support. The runtime gates
    /// requests against this list — calling embeddings against a profile
    /// without [`ProviderCapability::Embedding`] is rejected up front
    /// rather than producing a confusing 404.
    pub capabilities: &'static [ProviderCapability],

    /// Default request timeout in seconds when no env override is set.
    pub default_timeout_secs: u64,

    /// Env var name conventions for credential lookup.
    pub env: ProfileEnvVars,

    /// Default generation model id. `None` means the operator must specify
    /// one (true for llama.cpp where the model identifier depends on the
    /// loaded GGUF).
    pub default_generation_model: Option<&'static str>,

    /// Default embedding model id. `None` means embeddings are unsupported
    /// (OpenRouter) or the operator must specify one (llama.cpp).
    pub default_embedding_model: Option<&'static str>,

    /// Extra HTTP headers always injected on requests to this profile,
    /// beyond the standard `Authorization` header.
    pub extra_headers: &'static [(&'static str, ProfileHeaderSource)],

    /// Health-check endpoint relative to `base_url`. `None` means use the
    /// backend's default convention (Ollama: `/api/tags`, OpenAI-compatible:
    /// `/v1/models` as a de facto health check).
    pub health_endpoint: Option<&'static str>,

    /// Models-listing endpoint relative to `base_url`. `None` means the
    /// profile does not expose a documented enumerable model list.
    pub models_endpoint: Option<&'static str>,
}

impl ProviderProfile {
    /// `true` if this profile supports the given capability.
    pub fn supports(&self, cap: ProviderCapability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// `true` if the profile claims embedding support. Convenience wrapper
    /// over [`Self::supports`].
    pub fn supports_embeddings(&self) -> bool {
        self.supports(ProviderCapability::Embedding)
    }

    /// `true` if the profile claims generation support. Convenience wrapper
    /// over [`Self::supports`].
    pub fn supports_generation(&self) -> bool {
        self.supports(ProviderCapability::Generation)
    }
}

// =============================================================================
// Static catalog
// =============================================================================

/// Ollama — local-first daemon with its own protocol. Default profile.
pub const OLLAMA_PROFILE: ProviderProfile = ProviderProfile {
    id: "ollama",
    display_name: "Ollama",
    backend: BackendKind::Ollama,
    default_base_url: Some("http://localhost:11434"),
    requires_api_key: false,
    capabilities: &[
        ProviderCapability::Generation,
        ProviderCapability::Embedding,
        ProviderCapability::Vision,
    ],
    default_timeout_secs: 300,
    env: ProfileEnvVars {
        api_key: None,
        base_url: Some("OLLAMA_BASE"),
        timeout: Some("MATRIC_GEN_TIMEOUT_SECS"),
        generation_model: Some("OLLAMA_GEN_MODEL"),
        embedding_model: Some("OLLAMA_EMBED_MODEL"),
    },
    // Aligned with matric_core::defaults::{GEN_MODEL, EMBED_MODEL}.
    default_generation_model: Some("qwen3.5:9b"),
    default_embedding_model: Some("nomic-embed-text"),
    extra_headers: &[],
    // Ollama exposes /api/tags as the canonical model-list endpoint;
    // /api/version works as a lighter health probe.
    health_endpoint: Some("/api/version"),
    models_endpoint: Some("/api/tags"),
};

/// OpenAI proper. Standard reference for the OpenAI-compatible protocol.
pub const OPENAI_PROFILE: ProviderProfile = ProviderProfile {
    id: "openai",
    display_name: "OpenAI",
    backend: BackendKind::OpenAICompatible,
    default_base_url: Some("https://api.openai.com/v1"),
    requires_api_key: true,
    capabilities: &[
        ProviderCapability::Generation,
        ProviderCapability::Embedding,
    ],
    default_timeout_secs: 300,
    env: ProfileEnvVars {
        api_key: Some("OPENAI_API_KEY"),
        base_url: Some("OPENAI_BASE_URL"),
        timeout: Some("OPENAI_TIMEOUT"),
        generation_model: Some("OPENAI_GEN_MODEL"),
        embedding_model: Some("OPENAI_EMBED_MODEL"),
    },
    default_generation_model: Some("gpt-4o-mini"),
    default_embedding_model: Some("text-embedding-3-small"),
    extra_headers: &[],
    // /v1/models is the de facto health check on OpenAI-compatible servers.
    health_endpoint: Some("/v1/models"),
    models_endpoint: Some("/v1/models"),
};

/// OpenRouter — meta-router across many model providers. OpenAI-compatible
/// protocol with two extra headers used for routing rules and analytics.
pub const OPENROUTER_PROFILE: ProviderProfile = ProviderProfile {
    id: "openrouter",
    display_name: "OpenRouter",
    backend: BackendKind::OpenAICompatible,
    default_base_url: Some("https://openrouter.ai/api/v1"),
    requires_api_key: true,
    // Generation only — OpenRouter does not expose an embeddings API.
    capabilities: &[ProviderCapability::Generation],
    default_timeout_secs: 300,
    env: ProfileEnvVars {
        api_key: Some("OPENROUTER_API_KEY"),
        base_url: Some("OPENROUTER_BASE_URL"),
        timeout: Some("OPENROUTER_TIMEOUT"),
        generation_model: Some("OPENROUTER_GEN_MODEL"),
        embedding_model: None,
    },
    default_generation_model: Some("anthropic/claude-sonnet-4"),
    default_embedding_model: None,
    extra_headers: &[
        // Per OpenRouter docs, these headers feed routing rules and the
        // public app leaderboard. Defaults attribute Fortemi by name; the
        // env vars let operators rebrand for downstream apps that ship
        // Fortemi as a sidecar.
        (
            "HTTP-Referer",
            ProfileHeaderSource::Default {
                value: "https://fortemi.io",
                env_var: Some("OPENROUTER_HTTP_REFERER"),
            },
        ),
        (
            "X-Title",
            ProfileHeaderSource::Default {
                value: "Fortemi",
                env_var: Some("OPENROUTER_APP_NAME"),
            },
        ),
    ],
    health_endpoint: Some("/v1/models"),
    models_endpoint: Some("/v1/models"),
};

/// llama.cpp HTTP server (`llama-server`). OpenAI-compatible protocol; the
/// API key is optional since `llama-server` ships unauthenticated by default.
pub const LLAMACPP_PROFILE: ProviderProfile = ProviderProfile {
    id: "llamacpp",
    display_name: "llama.cpp",
    backend: BackendKind::OpenAICompatible,
    default_base_url: Some("http://localhost:8080/v1"),
    requires_api_key: false,
    // Embeddings depend on the llama-server build; we declare support and
    // surface a clearer error when the running server returns 404. The
    // alternative — declaring no support — would over-restrict the common
    // case where embeddings are compiled in.
    capabilities: &[
        ProviderCapability::Generation,
        ProviderCapability::Embedding,
    ],
    default_timeout_secs: 300,
    env: ProfileEnvVars {
        api_key: Some("LLAMACPP_API_KEY"),
        base_url: Some("LLAMACPP_BASE_URL"),
        timeout: Some("LLAMACPP_TIMEOUT"),
        generation_model: Some("LLAMACPP_GEN_MODEL"),
        embedding_model: Some("LLAMACPP_EMBED_MODEL"),
    },
    // llama-server identifies the loaded model by whatever string the
    // operator passed to `--alias` or `--model-alias`; no universal default
    // is sensible. Operators must specify when calling.
    default_generation_model: None,
    default_embedding_model: None,
    extra_headers: &[],
    // llama-server has its own /health endpoint distinct from /v1/models.
    health_endpoint: Some("/health"),
    models_endpoint: Some("/v1/models"),
};

/// Static catalog of well-known provider profiles. Append-only — entries are
/// not removed even if a provider deprecates, to keep config files stable.
pub const PROVIDER_PROFILES: &[ProviderProfile] = &[
    OLLAMA_PROFILE,
    OPENAI_PROFILE,
    OPENROUTER_PROFILE,
    LLAMACPP_PROFILE,
];

/// Default profile id when nothing is configured.
pub const DEFAULT_PROFILE_ID: &str = "ollama";

// =============================================================================
// Catalog accessors
// =============================================================================

/// Look up a profile by id. `O(n)` over the catalog — fine for n < 20.
pub fn lookup(id: &str) -> Option<&'static ProviderProfile> {
    PROVIDER_PROFILES.iter().find(|p| p.id == id)
}

/// Iterate over the full catalog.
pub fn iter() -> impl Iterator<Item = &'static ProviderProfile> {
    PROVIDER_PROFILES.iter()
}

/// All profiles that support a given capability.
pub fn with_capability(cap: ProviderCapability) -> impl Iterator<Item = &'static ProviderProfile> {
    PROVIDER_PROFILES.iter().filter(move |p| p.supports(cap))
}

/// Resolve the value for an [`ProfileHeaderSource`] given the current
/// environment. Returns `None` when the header should not be emitted.
pub fn resolve_header_value(source: &ProfileHeaderSource) -> Option<String> {
    match source {
        ProfileHeaderSource::Default { value, env_var } => {
            if let Some(name) = env_var {
                if let Ok(v) = std::env::var(name) {
                    if !v.is_empty() {
                        return Some(v);
                    }
                }
            }
            Some((*value).to_string())
        }
        ProfileHeaderSource::EnvOnly { env_var } => {
            std::env::var(env_var).ok().filter(|v| !v.is_empty())
        }
    }
}

/// Compute all extra headers a profile would inject given the current env.
/// Returns `(name, value)` pairs. Headers whose source resolves to `None`
/// (env-only with the env var unset) are omitted.
pub fn resolve_extra_headers(profile: &ProviderProfile) -> Vec<(String, String)> {
    profile
        .extra_headers
        .iter()
        .filter_map(|(name, source)| resolve_header_value(source).map(|v| ((*name).to_string(), v)))
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_four_v1_profiles() {
        assert_eq!(PROVIDER_PROFILES.len(), 4);
        let ids: Vec<_> = PROVIDER_PROFILES.iter().map(|p| p.id).collect();
        assert!(ids.contains(&"ollama"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"llamacpp"));
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in PROVIDER_PROFILES {
            assert!(seen.insert(p.id), "duplicate profile id: {}", p.id);
        }
    }

    #[test]
    fn lookup_returns_known_profiles() {
        assert_eq!(lookup("ollama").map(|p| p.id), Some("ollama"));
        assert_eq!(lookup("openai").map(|p| p.id), Some("openai"));
        assert_eq!(lookup("openrouter").map(|p| p.id), Some("openrouter"));
        assert_eq!(lookup("llamacpp").map(|p| p.id), Some("llamacpp"));
    }

    #[test]
    fn lookup_returns_none_for_unknown() {
        assert!(lookup("vllm").is_none());
        assert!(lookup("").is_none());
        assert!(lookup("OLLAMA").is_none(), "lookup is case-sensitive");
    }

    #[test]
    fn default_profile_exists_in_catalog() {
        assert!(
            lookup(DEFAULT_PROFILE_ID).is_some(),
            "DEFAULT_PROFILE_ID must point at a real catalog entry"
        );
    }

    #[test]
    fn ollama_does_not_require_api_key() {
        let p = lookup("ollama").unwrap();
        assert!(!p.requires_api_key);
        assert!(p.env.api_key.is_none());
    }

    #[test]
    fn openai_requires_api_key() {
        let p = lookup("openai").unwrap();
        assert!(p.requires_api_key);
        assert_eq!(p.env.api_key, Some("OPENAI_API_KEY"));
    }

    #[test]
    fn openrouter_does_not_support_embeddings() {
        let p = lookup("openrouter").unwrap();
        assert!(!p.supports_embeddings());
        assert!(p.supports_generation());
        assert!(p.default_embedding_model.is_none());
        assert!(p.env.embedding_model.is_none());
    }

    #[test]
    fn openrouter_has_routing_headers() {
        let p = lookup("openrouter").unwrap();
        let names: Vec<_> = p.extra_headers.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"HTTP-Referer"));
        assert!(names.contains(&"X-Title"));
    }

    #[test]
    fn llamacpp_api_key_is_optional() {
        let p = lookup("llamacpp").unwrap();
        assert!(!p.requires_api_key);
        // Env var name is declared so operators *can* set one for
        // llama-server instances launched with --api-key.
        assert_eq!(p.env.api_key, Some("LLAMACPP_API_KEY"));
    }

    #[test]
    fn llamacpp_has_no_default_model() {
        // The model id depends on the GGUF the operator loaded; no universal
        // default makes sense.
        let p = lookup("llamacpp").unwrap();
        assert!(p.default_generation_model.is_none());
        assert!(p.default_embedding_model.is_none());
    }

    #[test]
    fn ollama_uses_native_backend() {
        assert_eq!(lookup("ollama").unwrap().backend, BackendKind::Ollama);
    }

    #[test]
    fn openai_compatible_profiles_share_backend() {
        for id in &["openai", "openrouter", "llamacpp"] {
            assert_eq!(
                lookup(id).unwrap().backend,
                BackendKind::OpenAICompatible,
                "{} must use OpenAICompatible backend",
                id
            );
        }
    }

    #[test]
    fn with_capability_filters_correctly() {
        let gen_count = with_capability(ProviderCapability::Generation).count();
        assert_eq!(gen_count, 4, "all 4 profiles support generation");

        let embed_count = with_capability(ProviderCapability::Embedding).count();
        // OpenRouter is the one profile that doesn't claim embeddings.
        assert_eq!(embed_count, 3);
        let embed_ids: Vec<_> = with_capability(ProviderCapability::Embedding)
            .map(|p| p.id)
            .collect();
        assert!(!embed_ids.contains(&"openrouter"));
    }

    #[test]
    fn resolve_default_header_returns_default_when_env_unset() {
        // SAFETY: tests run single-threaded by default in cargo; we touch
        // only an env var unique to this test.
        std::env::remove_var("PROFILE_TEST_HEADER_DEFAULT");
        let v = resolve_header_value(&ProfileHeaderSource::Default {
            value: "https://fortemi.io",
            env_var: Some("PROFILE_TEST_HEADER_DEFAULT"),
        });
        assert_eq!(v.as_deref(), Some("https://fortemi.io"));
    }

    #[test]
    fn resolve_default_header_overrides_with_env() {
        std::env::set_var("PROFILE_TEST_HEADER_OVERRIDE", "https://my.host");
        let v = resolve_header_value(&ProfileHeaderSource::Default {
            value: "https://fortemi.io",
            env_var: Some("PROFILE_TEST_HEADER_OVERRIDE"),
        });
        assert_eq!(v.as_deref(), Some("https://my.host"));
        std::env::remove_var("PROFILE_TEST_HEADER_OVERRIDE");
    }

    #[test]
    fn resolve_default_header_ignores_empty_env() {
        std::env::set_var("PROFILE_TEST_HEADER_EMPTY", "");
        let v = resolve_header_value(&ProfileHeaderSource::Default {
            value: "fallback",
            env_var: Some("PROFILE_TEST_HEADER_EMPTY"),
        });
        assert_eq!(v.as_deref(), Some("fallback"));
        std::env::remove_var("PROFILE_TEST_HEADER_EMPTY");
    }

    #[test]
    fn resolve_envonly_header_omits_when_unset() {
        std::env::remove_var("PROFILE_TEST_HEADER_ENVONLY");
        let v = resolve_header_value(&ProfileHeaderSource::EnvOnly {
            env_var: "PROFILE_TEST_HEADER_ENVONLY",
        });
        assert!(v.is_none());
    }

    #[test]
    fn resolve_envonly_header_emits_when_set() {
        std::env::set_var("PROFILE_TEST_HEADER_ENVONLY_2", "tag-value");
        let v = resolve_header_value(&ProfileHeaderSource::EnvOnly {
            env_var: "PROFILE_TEST_HEADER_ENVONLY_2",
        });
        assert_eq!(v.as_deref(), Some("tag-value"));
        std::env::remove_var("PROFILE_TEST_HEADER_ENVONLY_2");
    }

    #[test]
    fn openrouter_default_headers_resolve_to_fortemi_io() {
        // Make sure neither override is set before we run the resolution.
        std::env::remove_var("OPENROUTER_HTTP_REFERER");
        std::env::remove_var("OPENROUTER_APP_NAME");

        let p = lookup("openrouter").unwrap();
        let headers = resolve_extra_headers(p);
        let by_name: std::collections::HashMap<_, _> = headers.into_iter().collect();
        assert_eq!(
            by_name.get("HTTP-Referer").map(String::as_str),
            Some("https://fortemi.io")
        );
        assert_eq!(by_name.get("X-Title").map(String::as_str), Some("Fortemi"));
    }
}

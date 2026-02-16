# ADR-072: Inference Provider Abstraction & Model Slug Routing

**Status:** Accepted (Phase 1), Proposed (Phase 2)
**Date:** 2026-02-16
**Deciders:** roctinam
**Supersedes:** ADR-001 (extends, does not replace)
**Related:** Issue #431 (model slug selection), ADR-002 (feature flags)

## Context

Fortemi uses multiple AI inference backends for different capabilities:

| Capability | Current Backend | Trait | Override Pattern |
|------------|----------------|-------|-----------------|
| Text generation (revision, titles, tagging) | Ollama | `GenerationBackend` | Per-job model slug in payload |
| Text embedding | Ollama | `EmbeddingBackend` | Per-embedding-set config (NOT per-operation) |
| Vision (image description) | Ollama Vision | `VisionBackend` | Per-request multipart field |
| Audio transcription | Whisper-compatible | `TranscriptionBackend` | Per-request multipart field |

ADR-001 established the trait hierarchy and aspirationally referenced a `BackendSelector` for
multi-provider routing. This ADR formalizes the **actual implemented pattern** (Phase 1) and
proposes **provider slug routing** for external providers like OpenAI (Phase 2).

### Problem Statement

Users need to:
1. Override the model used for any LLM-backed operation (implemented in #431)
2. Use external providers (OpenAI, Anthropic, OpenRouter) for specific operations
3. Mix local and cloud providers in the same deployment
4. Discover available models and their capabilities at runtime

### Rust-Specific Design Constraints

- `OllamaBackend` is **not `Clone`** (holds `reqwest::Client` pool state)
- Trait objects (`dyn GenerationBackend`) require `Send + Sync` bounds
- Feature flags (`#[cfg(feature = "openai")]`) gate optional backends at compile time
- Job handlers hold backend instances constructed at startup via `from_env()`
- Job payloads are `serde_json::Value` — model override must serialize as JSON

## Decision

### Phase 1: Per-Operation Model Slug Override (Implemented)

All LLM-backed operations accept an optional `model` parameter that overrides the globally
configured default model **within the same provider**.

#### API Surface

**JSON body endpoints** (notes, reprocess, bulk reprocess):
```json
{
  "content": "...",
  "model": "qwen3:32b"
}
```

**Multipart endpoints** (vision, audio):
```
Content-Type: multipart/form-data
- file: <binary>
- model: "llava:34b"
```

**Job payloads** (background pipeline):
```json
{
  "schema": "public",
  "model": "qwen3:32b"
}
```

#### Implementation Pattern

The override pattern creates a **fresh backend instance** with the model swapped, rather than
mutating the shared handler backend. This avoids mutation races across concurrent jobs.

```rust
// crates/matric-api/src/handlers/jobs.rs

/// Extract an optional model override from a job's payload.
fn extract_model_override(ctx: &JobContext) -> Option<String> {
    ctx.payload()
        .and_then(|p| p.get("model"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Create an OllamaBackend with an optional generation model override.
///
/// If model_override is Some, creates a fresh backend from env config
/// with the generation model swapped. Otherwise returns None, indicating
/// the caller should use its default backend.
fn backend_with_gen_override(model_override: Option<&str>) -> Option<OllamaBackend> {
    model_override.map(|m| {
        let mut b = OllamaBackend::from_env();
        b.set_gen_model(m.to_string());
        b
    })
}

// Usage in any job handler:
let model_override = extract_model_override(&ctx);
let overridden = backend_with_gen_override(model_override.as_deref());
let backend: &OllamaBackend = overridden.as_ref().unwrap_or(&self.backend);
```

**Why this works:**
- `OllamaBackend::from_env()` is cheap (constructs `reqwest::Client` with default pool)
- The overridden backend lives on the stack for the duration of the job
- No shared mutable state — each job gets its own backend if overridden
- The `unwrap_or(&self.backend)` pattern preserves zero-cost path for default usage

**Embedding model is excluded** from per-operation override because embeddings must use
the model configured in the embedding set's `embedding_config` table. Using a different
model would produce vectors in an incompatible vector space.

#### Affected Handlers

| Handler | Override Support | Notes |
|---------|-----------------|-------|
| `AiRevisionHandler` | Yes — `model` in payload | Generation model swap |
| `TitleGenerationHandler` | Yes — `model` in payload | Generation model swap |
| `ConceptTaggingHandler` | Yes — `model` in payload | Generation model swap |
| `MetadataExtractionHandler` | Yes — `model` in payload | Generation model swap |
| `ContextUpdateHandler` | Yes — `model` in payload | Generation model swap |
| `EmbeddingHandler` | **No** — uses embedding set config | Model tied to vector space |
| `DocumentTypeInferenceHandler` | **No** — uses DB pattern matching | No LLM involved |
| `describe_image` (vision) | Yes — multipart `model` field | Creates ad-hoc `OllamaVisionBackend` |
| `transcribe_audio` (audio) | Yes — multipart `model` field | Creates ad-hoc `WhisperBackend` |

#### Model Discovery Endpoint

`GET /api/v1/models` returns all available models with capability metadata:

```json
{
  "models": [
    {
      "slug": "qwen3:8b",
      "capabilities": ["language"],
      "default_for": ["language"],
      "parameter_size": "8.2B",
      "family": "qwen3"
    },
    {
      "slug": "nomic-embed-text",
      "capabilities": ["embedding"],
      "default_for": ["embedding"],
      "family": "nomic"
    }
  ],
  "defaults": {
    "language": "qwen3:8b",
    "embedding": "nomic-embed-text",
    "vision": "qwen3-vl:8b",
    "transcription": "Systran/faster-distil-whisper-large-v3"
  }
}
```

### Phase 2: Provider Slug Routing (Proposed)

Extend the model slug format to include an optional **provider prefix**:

```
[provider:]model_slug

Examples:
  "qwen3:8b"              → default provider (Ollama local)
  "ollama:qwen3:8b"       → explicit Ollama
  "openai:gpt-4o"         → OpenAI cloud
  "openai:gpt-4.1-mini"   → OpenAI cloud (budget)
  "openrouter:anthropic/claude-sonnet-4-20250514" → OpenRouter
```

#### Provider Registry

```rust
/// A registered inference provider.
pub struct ProviderConfig {
    /// Provider identifier (e.g., "ollama", "openai", "openrouter").
    pub id: String,
    /// Base URL for the provider's API.
    pub base_url: String,
    /// API key (None for local providers).
    pub api_key: Option<String>,
    /// Which capabilities this provider supports.
    pub capabilities: Vec<ProviderCapability>,
    /// Default timeout for requests.
    pub timeout: Duration,
    /// Whether this is the default provider (exactly one must be default).
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ProviderCapability {
    Generation,
    Embedding,
    Vision,
    Transcription,
}

/// Registry of configured inference providers.
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderConfig>,
    default_provider: String,
}

impl ProviderRegistry {
    /// Parse a provider-qualified model slug.
    ///
    /// Returns (provider_id, model_slug). If no provider prefix,
    /// uses the default provider.
    pub fn parse_slug(&self, slug: &str) -> Result<(&str, &str)> {
        // Strategy: try known provider prefixes first, fall back to default
        for provider_id in self.providers.keys() {
            if let Some(model) = slug.strip_prefix(&format!("{}:", provider_id)) {
                if !model.is_empty() {
                    return Ok((provider_id, model));
                }
            }
        }
        // No provider prefix — use default
        Ok((&self.default_provider, slug))
    }

    /// Resolve a slug to a concrete backend instance.
    pub fn resolve_generation(&self, slug: &str) -> Result<Box<dyn GenerationBackend>> {
        let (provider_id, model) = self.parse_slug(slug)?;
        let config = self.providers.get(provider_id)
            .ok_or_else(|| Error::Config(format!("Unknown provider: {}", provider_id)))?;

        match provider_id {
            "ollama" => {
                let mut backend = OllamaBackend::from_env();
                backend.set_gen_model(model.to_string());
                Ok(Box::new(backend))
            }
            #[cfg(feature = "openai")]
            "openai" | "openrouter" => {
                let oai_config = OpenAIConfig {
                    base_url: config.base_url.clone(),
                    api_key: config.api_key.clone(),
                    gen_model: model.to_string(),
                    ..Default::default()
                };
                Ok(Box::new(OpenAIBackend::new(oai_config)?))
            }
            _ => Err(Error::Config(format!(
                "Provider '{}' not compiled in (check feature flags)", provider_id
            ))),
        }
    }
}
```

#### Slug Parsing Edge Cases

Ollama model slugs already contain colons (e.g., `qwen3:8b`, `llava:34b`). The parser
must handle this correctly:

| Input | Provider | Model |
|-------|----------|-------|
| `qwen3:8b` | default (ollama) | `qwen3:8b` |
| `ollama:qwen3:8b` | ollama | `qwen3:8b` |
| `openai:gpt-4o` | openai | `gpt-4o` |
| `openrouter:anthropic/claude-sonnet-4-20250514` | openrouter | `anthropic/claude-sonnet-4-20250514` |

**Parsing strategy:** Match against known provider IDs first (exact prefix match on `{id}:`),
then treat the remainder as the model slug. Unknown prefixes are treated as part of the model
slug for the default provider. This avoids ambiguity with Ollama's `name:tag` format because
Ollama model names never collide with provider IDs (`ollama`, `openai`, `openrouter`, etc.).

#### Configuration (Environment Variables)

```bash
# Default provider (always available, no config needed beyond existing vars)
OLLAMA_GEN_MODEL=qwen3:8b
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_VISION_MODEL=qwen3-vl:8b

# OpenAI provider (opt-in via feature flag + env vars)
OPENAI_API_KEY=sk-...
OPENAI_BASE_URL=https://api.openai.com/v1    # default

# OpenRouter provider (uses OpenAI-compatible API)
OPENROUTER_API_KEY=sk-or-...
OPENROUTER_BASE_URL=https://openrouter.ai/api/v1
```

#### Configuration (Database — Future)

For dynamic provider management without restarts, store provider configs in a
`inference_providers` table:

```sql
CREATE TABLE inference_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_id TEXT UNIQUE NOT NULL,       -- "ollama", "openai", "openrouter"
    base_url TEXT NOT NULL,
    api_key_encrypted BYTEA,               -- PKE-encrypted API key
    capabilities TEXT[] NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## Consequences

### Positive

- (+) Users can use the best model for each task without changing global config
- (+) External providers (OpenAI, OpenRouter) available for one-off high-quality operations
- (+) Provider slug format is backward-compatible — bare slugs use default provider
- (+) Fresh-backend pattern avoids shared mutable state and race conditions
- (+) Feature flags keep binary size small when external providers aren't needed
- (+) Model discovery endpoint enables MCP agents to make informed model choices
- (+) Provenance records capture which specific model was used per operation

### Negative

- (-) `from_env()` per override has minor overhead (constructs reqwest::Client)
- (-) Ollama colon-in-slug format requires careful provider prefix parsing
- (-) API key management for external providers adds operational complexity
- (-) Different providers have different timeout/retry characteristics
- (-) No compile-time guarantee that a provider feature flag is enabled

### Risks

| Risk | Mitigation |
|------|-----------|
| API key leaked in job payload | Never store API keys in payloads — resolve at execution time from registry |
| Provider down mid-job | Retry with exponential backoff; fall back to default provider on repeated failure |
| Model not available at provider | Validate slug against provider's model list before queuing job |
| Embedding dimension mismatch | Embedding model is NOT overridable per-operation (enforced by design) |
| Concurrent backend creation overhead | `reqwest::Client` uses connection pool sharing; overhead is ~microseconds |

## Rust Best Practices Applied

### 1. Trait Object Safety

All backend traits (`GenerationBackend`, `EmbeddingBackend`, `VisionBackend`,
`TranscriptionBackend`) are object-safe:
- No generic methods
- No `Self` in return position
- All methods take `&self`
- `Send + Sync` bounds via `#[async_trait]`

This enables `Box<dyn GenerationBackend>` for runtime dispatch in the provider registry.

### 2. Ownership & Borrowing

The override pattern uses stack-local ownership with fallback borrow:
```rust
let overridden: Option<OllamaBackend> = backend_with_gen_override(model_override.as_deref());
let backend: &OllamaBackend = overridden.as_ref().unwrap_or(&self.backend);
```

This avoids:
- `Arc<Mutex<>>` (unnecessary for single-threaded job execution)
- `Clone` requirement on `OllamaBackend` (which holds non-cloneable state)
- Lifetime issues from returning references to temporaries

### 3. Feature Flags for Optional Dependencies

```toml
[features]
default = ["ollama"]
ollama = []
openai = ["dep:reqwest-tls"]  # Only pull in TLS deps when needed
```

Provider-specific code is gated:
```rust
#[cfg(feature = "openai")]
"openai" => { /* OpenAI path */ }
```

This keeps the default binary lean (Ollama-only, no TLS overhead for local inference).

### 4. Error Propagation

Provider resolution errors use the existing `matric_core::Error` hierarchy:
- `Error::Config` — provider not found, feature flag missing
- `Error::Inference` — backend communication failure
- `Error::Embedding` — embedding-specific failures

No new error variants needed. The `?` operator propagates naturally through
the existing `Result<T, matric_core::Error>` chain.

### 5. No `std::env::set_var` in Runtime

Provider configuration reads env vars at construction time via `from_env()`.
The values are captured into owned `String` fields. No global state mutation.
This is safe for parallel `cargo test` execution (per MEMORY.md directive).

### 6. Graceful Degradation

If an external provider is unavailable:
1. Health check fails during startup → log warning, mark provider as unhealthy
2. Runtime resolution failure → return `Error::Inference` with provider context
3. Model discovery omits unhealthy providers from results
4. Default provider (Ollama local) is always available as fallback

## Implementation Plan

### Phase 1 (Complete — Issue #431)

- [x] `GET /api/v1/models` endpoint with capability metadata
- [x] `model` parameter on `CreateNoteBody`, `UpdateNoteBody`, `ReprocessNoteBody`, `BulkReprocessBody`
- [x] `extract_model_override()` + `backend_with_gen_override()` helpers
- [x] 5 generation job handlers updated (revision, title, tagging, metadata, context)
- [x] Vision and audio handlers accept `model` multipart field
- [x] MCP `get_available_models` tool
- [x] MCP `capture_knowledge` and `bulk_reprocess_notes` accept `model` param
- [x] Provenance records capture actual model used

### Phase 2 (Proposed — Separate Issue)

- [ ] `ProviderRegistry` type in `matric-inference`
- [ ] Provider slug parsing (`provider:model` format)
- [ ] OpenAI provider integration (already implemented behind `openai` feature flag)
- [ ] OpenRouter support (OpenAI-compatible, reuses `OpenAIBackend`)
- [ ] Provider configuration via env vars
- [ ] Model discovery aggregates across providers
- [ ] Provider health monitoring
- [ ] MCP tools accept provider-qualified slugs

### Phase 3 (Future)

- [ ] Database-backed provider configuration
- [ ] Per-provider timeout/retry policies
- [ ] Provider cost tracking (token usage × price per model)
- [ ] Automatic provider failover
- [ ] Vision/transcription provider routing (not just generation)

## References

- ADR-001: Trait-Based Backend Abstraction (foundational trait hierarchy)
- ADR-002: Feature Flags & Optional Backends (compile-time gating)
- Issue #431: Allow model slug selection on all LLM operations
- `crates/matric-core/src/traits.rs` — `EmbeddingBackend`, `GenerationBackend`, `InferenceBackend`
- `crates/matric-inference/src/ollama.rs` — `OllamaBackend` implementation
- `crates/matric-inference/src/openai/backend.rs` — `OpenAIBackend` implementation
- `crates/matric-inference/src/selector.rs` — `ModelSelector` (task-based selection)
- `crates/matric-api/src/handlers/jobs.rs` — `extract_model_override()`, `backend_with_gen_override()`
- `crates/matric-api/src/handlers/models.rs` — `GET /api/v1/models` endpoint

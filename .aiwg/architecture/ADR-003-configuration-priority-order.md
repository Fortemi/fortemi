# ADR-003: Configuration Priority Order

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-010-openai-backend.md

## Context

Configuration can come from multiple sources:
1. Configuration files (TOML)
2. Environment variables
3. Compile-time defaults

Need clear precedence rules so users understand which value will be used when multiple sources specify the same setting.

## Decision

Priority order (highest to lowest):
1. **Environment variables** - Always win, enable runtime overrides
2. **Configuration file** - Persistent settings for development/deployment
3. **Compile-time defaults** - Sensible fallbacks when nothing specified

Environment variables use the prefix `MATRIC_` followed by the config path in screaming snake case.

## Consequences

### Positive
- (+) Easy 12-factor app deployment with env vars
- (+) Development can use config files without polluting env
- (+) Sensible defaults reduce required configuration
- (+) Container deployments can override without modifying files
- (+) Secrets can be injected via env vars (API keys)

### Negative
- (-) May be confusing when settings seem to be ignored (overridden by env)
- (-) Debugging requires checking multiple sources
- (-) Env var names can get long for nested config

## Implementation

**Code Location:** `crates/matric-inference/src/config.rs`

**Environment Variable Examples:**

| Config Path | Environment Variable |
|-------------|---------------------|
| `openai.api_key` | `MATRIC_OPENAI_API_KEY` |
| `openai.base_url` | `MATRIC_OPENAI_BASE_URL` |
| `ollama.host` | `MATRIC_OLLAMA_HOST` |
| `embedding.backend` | `MATRIC_EMBEDDING_BACKEND` |

**Configuration Loading:**

```rust
pub fn load_config() -> Result<InferenceConfig> {
    // 1. Start with defaults
    let mut config = InferenceConfig::default();

    // 2. Overlay config file if exists
    if let Ok(file_config) = load_config_file() {
        config.merge(file_config);
    }

    // 3. Overlay environment variables (highest priority)
    config.apply_env_overrides();

    Ok(config)
}
```

## References

- ARCH-010-openai-backend.md (Section 13)
- 12-factor app configuration principles

# ADR-002: Feature Flags for Optional Backends

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-010-openai-backend.md

## Context

Not all deployments need all backends. A user running matric-memory with local Ollama shouldn't need to compile OpenAI dependencies (reqwest async runtime, etc.). Similarly, a cloud deployment using only OpenAI shouldn't need Ollama client code.

## Decision

Use Cargo feature flags to conditionally compile backends:
- `ollama` feature - enables Ollama backend (default)
- `openai` feature - enables OpenAI-compatible backend
- `all-backends` feature - convenience for enabling all backends

Default features include only `ollama` to minimize dependencies for the common local development case.

## Consequences

### Positive
- (+) Smaller binary size for single-backend deployments
- (+) Reduced compile time when features disabled
- (+) Clearer dependency tree for each configuration
- (+) Users can audit exactly which network clients are included

### Negative
- (-) More complex conditional compilation with `#[cfg(feature = "...")]`
- (-) Testing requires multiple feature combinations
- (-) CI needs to test all feature permutations
- (-) Documentation must note feature requirements

## Implementation

**Code Location:** `crates/matric-inference/Cargo.toml`

**Cargo.toml Configuration:**

```toml
[features]
default = ["ollama"]
ollama = []
openai = ["dep:reqwest"]
all-backends = ["ollama", "openai"]

[dependencies]
reqwest = { version = "0.11", features = ["json", "stream"], optional = true }
```

**Conditional Compilation:**

```rust
// lib.rs
#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "openai")]
pub mod openai;

// selector.rs
pub struct BackendSelector {
    #[cfg(feature = "ollama")]
    ollama: Option<OllamaBackend>,

    #[cfg(feature = "openai")]
    openai: Option<OpenAIBackend>,
}
```

## References

- ARCH-010-openai-backend.md (Section 13)
- Cargo feature documentation

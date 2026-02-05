# ADR-001: Trait-Based Backend Abstraction

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-010-openai-backend.md

## Context

matric-memory needs to support multiple inference backends (Ollama for local models, OpenAI-compatible APIs for cloud providers) with the ability to mix and match them for different operations. The system should allow using Ollama for embeddings while using OpenAI for generation, or vice versa.

## Decision

Use the existing trait hierarchy from `matric-core`:
- `EmbeddingBackend` - for text embedding operations
- `GenerationBackend` - for text generation/completion
- `InferenceBackend` - combined trait for backends supporting both

Each backend implements these traits, and a `BackendSelector` coordinates between them at runtime based on configuration.

## Consequences

### Positive
- (+) Existing code using traits works unchanged
- (+) Easy to add new backends in the future (vLLM, LocalAI, etc.)
- (+) Backends can be tested independently with mock implementations
- (+) Runtime backend selection without code changes
- (+) Consistent API regardless of underlying provider

### Negative
- (-) Some duplication in trait implementations across backends
- (-) Cannot easily share state between backends (e.g., connection pools)
- (-) Trait objects add minor runtime overhead vs static dispatch

## Implementation

**Code Location:**
- Traits: `crates/matric-core/src/inference.rs`
- Ollama: `crates/matric-inference/src/ollama.rs`
- OpenAI: `crates/matric-inference/src/openai/backend.rs`
- Selector: `crates/matric-inference/src/selector.rs`

**Key Implementation:**

```rust
// matric-core trait definition
#[async_trait]
pub trait EmbeddingBackend: Send + Sync {
    async fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn embedding_dimensions(&self) -> usize;
}

// Backend selector coordinates between implementations
pub struct BackendSelector {
    ollama: Option<OllamaBackend>,
    openai: Option<OpenAIBackend>,
    config: InferenceConfig,
}

impl BackendSelector {
    pub fn embedding_backend(&self) -> &dyn EmbeddingBackend {
        match self.config.embedding_backend {
            BackendType::Ollama => self.ollama.as_ref().unwrap(),
            BackendType::OpenAI => self.openai.as_ref().unwrap(),
        }
    }
}
```

## References

- ARCH-010-openai-backend.md (Section 13)
- matric-core trait definitions

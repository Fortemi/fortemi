# Requirements: OpenAI-Compatible Backend (#10)

**Document ID:** REQ-010
**Status:** Inception Complete
**Created:** 2026-01-22
**Stakeholder Input:** Interactive session

---

## 1. Overview

Implement an OpenAI-compatible API adapter for matric-inference that allows users to connect to any OpenAI-compatible endpoint (OpenAI cloud, Ollama in OpenAI mode, vLLM, LocalAI, LM Studio, etc.).

## 2. Business Requirements

### BR-1: Provider Flexibility
Users should be able to point matric-memory at any OpenAI-compatible API endpoint, not just OpenAI cloud. Most users will use this to connect to their local Ollama or vLLM instances via the OpenAI compatibility layer.

### BR-2: Operation Parity
The OpenAI backend must support the same operations as the existing Ollama backend:
- Text generation (chat completions)
- Embedding generation

### BR-3: Configuration-Driven
Backend selection must be fully configurable per-operation, allowing users to mix backends (e.g., Ollama for embeddings, OpenAI for generation).

## 3. Functional Requirements

### FR-1: OpenAI API Compatibility
- Implement `/v1/chat/completions` for text generation
- Implement `/v1/embeddings` for embedding generation
- Support custom `base_url` configuration
- Support API key authentication (optional for local endpoints)

### FR-2: Streaming Support
- Full streaming response support for generation
- Real-time token delivery for better UX on long generations
- Graceful degradation to batch mode if streaming unavailable

### FR-3: Configuration Schema
```toml
[inference]
default_generation = "openai"  # or "ollama"
default_embedding = "ollama"   # or "openai"

[inference.openai]
base_url = "https://api.openai.com/v1"  # or local endpoint
api_key = "${OPENAI_API_KEY}"           # optional for local
generation_model = "gpt-4"
embedding_model = "text-embedding-3-small"
timeout_seconds = 300

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "gpt-oss:20b"
embedding_model = "nomic-embed-text"
```

### FR-4: Backend Trait
- Extract `InferenceBackend` trait from existing code
- Both Ollama and OpenAI backends implement the same trait
- Runtime backend selection via configuration

### FR-5: Health Checking
- Implement health check for OpenAI endpoints
- Model availability verification
- Graceful error messages for connection failures

## 4. Non-Functional Requirements

### NFR-1: Performance
- Streaming latency: First token within 500ms of API response
- Connection pooling for efficiency
- Request timeout configurable (default 300s)

### NFR-2: Security
- API keys never logged
- Support for environment variable expansion in config
- Optional TLS certificate verification for self-signed local endpoints

### NFR-3: Compatibility
- OpenAI API v1 specification
- Compatible with: OpenAI, Azure OpenAI, Ollama (OpenAI mode), vLLM, LocalAI, LM Studio

## 5. Acceptance Criteria

- [ ] AC-1: Can configure OpenAI endpoint via TOML config
- [ ] AC-2: Can generate text using OpenAI-compatible API
- [ ] AC-3: Can generate embeddings using OpenAI-compatible API
- [ ] AC-4: Streaming works for text generation
- [ ] AC-5: Can mix backends (Ollama for X, OpenAI for Y)
- [ ] AC-6: Health check reports endpoint status
- [ ] AC-7: Works with local Ollama in OpenAI compatibility mode
- [ ] AC-8: All existing tests pass
- [ ] AC-9: New unit tests for OpenAI backend

## 6. Out of Scope (v1.0)

- Azure-specific authentication (AAD tokens)
- Function calling / tool use
- Vision/image inputs
- Fine-tuning API
- Assistants API

## 7. Dependencies

- `reqwest` with streaming support
- `tokio` for async
- `serde` for JSON serialization
- `futures` for stream handling

## 8. Risks

| Risk | Mitigation |
|------|------------|
| API rate limiting | Implement retry with exponential backoff |
| Model name differences | Allow explicit model override in config |
| Streaming format variations | Detect and handle SSE vs NDJSON |

---

*Document approved for Elaboration phase*

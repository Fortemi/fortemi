# Inference Backends Guide

This guide covers configuring and using different LLM inference backends in Fortémi.

> **Running the Local Workstation stack?** You don't have to read this guide to switch backends. The workstation ships an interactive picker:
>
> ```bash
> ./workstation configure-llm
> ```
>
> It walks through five options (ollama / vllm / openai / openrouter / llamacpp), prompts API keys silently, handles host-to-container networking, and writes the right env vars to `.env.workstation`. The doctor probes the configured endpoint to catch misconfigurations before `up`. See [WORKSTATION-SETUP.md → "LLM backend selection"](https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/WORKSTATION-SETUP.md) for the full ops reference.
>
> Keep reading this guide if you're deploying the **Docker bundle** or **from-source**, where you set the env vars by hand.

## Overview

Fortémi uses pluggable inference backends for:
- **Sentence Embeddings** - Converting text to fixed-dimensional vector representations for dense retrieval (Reimers & Gurevych, 2019)
- **Text Generation** - Retrieval-Augmented Generation (RAG) for note revision, title generation, and summaries (Lewis et al., 2020)

## Supported Backends

| Backend | Type | Use Case |
|---------|------|----------|
| **Ollama** | Local | Default, privacy-focused, no API costs |
| **OpenAI** | Cloud/Local | OpenAI API, or any OpenAI-compatible endpoint |

## Ollama Backend (Default)

Ollama runs models locally on your hardware. This is the default backend.

### Requirements

- [Ollama](https://ollama.ai) installed and running
- Sufficient GPU VRAM (6GB+ recommended)
- Models pulled locally

### Configuration

```bash
# Environment variables
export OLLAMA_BASE=http://localhost:11434   # Default (also: MATRIC_OLLAMA_URL > OLLAMA_BASE > OLLAMA_URL > OLLAMA_HOST)
export OLLAMA_EMBED_MODEL=nomic-embed-text
export OLLAMA_GEN_MODEL=qwen3.5:9b
export OLLAMA_EMBEDDING_DIMENSION=768
```

Or in your application config:

```toml
[inference.ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"
generation_model = "qwen3.5:9b"
embedding_dimension = 768
```

### Recommended Models

| Task | Model | VRAM | Notes |
|------|-------|------|-------|
| Embeddings | `nomic-embed-text` | ~2GB | Best quality/speed balance (default) |
| Embeddings | `mxbai-embed-large` | ~2GB | Alternative high-quality |
| Generation | `qwen3.5:9b` | ~8GB | Default — multimodal, vision-capable |
| Generation | `qwen2.5:7b` | ~6GB | Alternative, strong reasoning |
| Generation | `llama3.1:8b` | ~8GB | Alternative, slower |
| Code | `qwen2.5-coder:7b` | ~6GB | Code-focused tasks |

### Installing Models

```bash
# Pull embedding model
ollama pull nomic-embed-text

# Pull generation model (default)
ollama pull qwen3.5:9b

# List installed models
ollama list
```

### Health Check

```bash
# Check Ollama is running
curl http://localhost:11434/api/tags

# Test embedding
curl http://localhost:11434/api/embeddings \
  -d '{"model": "nomic-embed-text", "prompt": "test"}'
```

## OpenAI Backend

The OpenAI backend works with:
- OpenAI cloud API
- Azure OpenAI
- Ollama (OpenAI compatibility mode)
- vLLM
- LocalAI
- LM Studio
- text-generation-webui
- Any OpenAI-compatible API

### Configuration

```bash
# Environment variables
export OPENAI_API_KEY=<OPENAI_API_KEY>                          # Required for OpenAI cloud
export OPENAI_BASE_URL=https://api.openai.com/v1      # Default
export OPENAI_EMBEDDING_MODEL=text-embedding-3-small
export OPENAI_GENERATION_MODEL=gpt-4o-mini
export OPENAI_EMBEDDING_DIMENSION=1536
```

Or in your application config:

```toml
[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
embedding_model = "text-embedding-3-small"
generation_model = "gpt-4o-mini"
embedding_dimension = 1536
```

### OpenAI Cloud

For OpenAI's cloud API:

```bash
export OPENAI_API_KEY=<OPENAI_API_KEY>
export OPENAI_BASE_URL=https://api.openai.com/v1
export OPENAI_EMBEDDING_MODEL=text-embedding-3-small
export OPENAI_GENERATION_MODEL=gpt-4o-mini
```

**Model Recommendations:**

| Task | Model | Cost | Notes |
|------|-------|------|-------|
| Embeddings | `text-embedding-3-small` | $0.02/1M tokens | Best value |
| Embeddings | `text-embedding-3-large` | $0.13/1M tokens | Higher quality |
| Generation | `gpt-4o-mini` | $0.15/1M in | Fast, capable |
| Generation | `gpt-4o` | $2.50/1M in | Most capable |

### Ollama (OpenAI Mode)

Use Ollama with OpenAI-compatible API:

```bash
export OPENAI_BASE_URL=http://localhost:11434/v1
export OPENAI_API_KEY=ollama  # Required but not validated
export OPENAI_EMBEDDING_MODEL=nomic-embed-text
export OPENAI_GENERATION_MODEL=llama3.2:3b
```

### vLLM

```bash
export OPENAI_BASE_URL=http://localhost:8000/v1
export OPENAI_API_KEY=token  # If required
export OPENAI_GENERATION_MODEL=meta-llama/Llama-3.1-8B-Instruct
```

### LocalAI

```bash
export OPENAI_BASE_URL=http://localhost:8080/v1
export OPENAI_API_KEY=localai
export OPENAI_EMBEDDING_MODEL=text-embedding-ada-002
export OPENAI_GENERATION_MODEL=gpt-3.5-turbo
```

### LM Studio

```bash
export OPENAI_BASE_URL=http://localhost:1234/v1
export OPENAI_API_KEY=lm-studio
export OPENAI_GENERATION_MODEL=local-model
```

### Azure OpenAI

```bash
export OPENAI_BASE_URL=https://YOUR-RESOURCE.openai.azure.com/openai/deployments/YOUR-DEPLOYMENT
export OPENAI_API_KEY=your-azure-key
export OPENAI_EMBEDDING_MODEL=text-embedding-ada-002
export OPENAI_GENERATION_MODEL=gpt-4
```

## Backend Selection

### Compile-Time Features

Backends are feature-gated at compile time:

```bash
# Ollama only (default)
cargo build -p matric-api

# OpenAI only
cargo build -p matric-api --no-default-features --features openai

# Both backends
cargo build -p matric-api --features openai
```

### Runtime Selection

When both backends are compiled in, select at runtime:

```bash
# Use Ollama (default)
export MATRIC_INFERENCE_DEFAULT=ollama

# Use OpenAI
export MATRIC_INFERENCE_DEFAULT=openai
```

Or in config:

```toml
[inference]
default = "openai"  # or "ollama"
```

## API Endpoints

The inference HTTP surface lives under `/api/v1/inference/*`, plus `/api/v1/models` for model discovery.

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/v1/models` | List available models across all providers |
| `GET` | `/api/v1/inference/providers` | Discover providers (`server_configured`, `supports_embeddings`) |
| `GET` | `/api/v1/inference/config` | View current config with source attribution |
| `POST` | `/api/v1/inference/config` | Hot-swap configuration at runtime |
| `DELETE` | `/api/v1/inference/config` | Reset overrides back to env/defaults |
| `GET` | `/api/v1/inference/config/audit` | Config change audit log |
| `POST` | `/api/v1/inference/test-connection` | Probe a backend |
| `POST` | `/api/v1/inference/complete` | Chat completion |
| `POST` | `/api/v1/inference/stream` | Streaming chat completion |

> Backend health is reported through the server's `/health` endpoint under `capabilities`, not a dedicated inference health route. Embeddings are generated internally by the embedding pipeline (e.g. the `regenerate_embeddings` batch job), not via a standalone ad-hoc HTTP endpoint.

### Chat Completion

The completion request takes a `model`, a `messages` array of `{role, content}` objects, and `max_tokens`. Optional `provider_id`, `api_key`, and `base_url` fields override the configured backend for a single call.

```bash
curl -X POST http://localhost:3000/api/v1/inference/complete \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3.5:9b",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Summarize this note: ..."}
    ],
    "max_tokens": 500
  }'
```

## Streaming

The streaming endpoint accepts the same request body and returns Server-Sent Events:

```bash
curl -X POST http://localhost:3000/api/v1/inference/stream \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3.5:9b",
    "messages": [
      {"role": "user", "content": "Write a poem about knowledge"}
    ],
    "max_tokens": 500
  }'
```

## Error Handling

### Retryable Errors

The OpenAI backend automatically retries on:
- Rate limits (429)
- Server errors (500, 502, 503)
- Network timeouts

### Non-Retryable Errors

- Authentication errors (401)
- Model not found (404)
- Invalid request (400)

### Error Response

```json
{
  "error": "inference_error",
  "message": "Rate limit exceeded",
  "retryable": true,
  "retry_after": 60
}
```

## Performance Tuning

### Ollama

```bash
# Increase context window
export OLLAMA_NUM_CTX=8192

# Use GPU layers
export OLLAMA_NUM_GPU=99  # All layers on GPU

# Concurrent requests
export OLLAMA_NUM_PARALLEL=4
```

### OpenAI

```bash
# Request timeout
export OPENAI_TIMEOUT=120

# Max retries
export OPENAI_MAX_RETRIES=3
```

## Monitoring

### Logging

Enable debug logging for inference:

```bash
export RUST_LOG=matric_inference=debug
```

## Troubleshooting

### "Connection refused"

- Ollama: Ensure `ollama serve` is running
- OpenAI: Check base URL is correct

### "Model not found"

- Ollama: Run `ollama pull <model>`
- OpenAI: Verify model name matches API

### "Authentication failed"

- Check API key is set and valid
- For local servers, ensure dummy key is provided

### "Context length exceeded"

- Reduce input size
- Use a model with larger context window
- Enable automatic chunking

### "Rate limit exceeded"

- Add delays between requests
- Use a higher-tier API plan
- Switch to local inference

## Migration Between Backends

When switching backends, note that:

1. **Embedding dimensions may differ** - Regenerate embeddings after switching
2. **Model capabilities vary** - Test generation quality
3. **Costs change** - Cloud vs local tradeoffs

```bash
# Regenerate all embeddings after backend switch
curl -X POST http://localhost:3000/api/v1/jobs/batch \
  -H "Content-Type: application/json" \
  -d '{"job_type": "regenerate_embeddings", "scope": "all"}'
```

## Technical Background

### Sentence Embeddings

Fortémi uses **bi-encoder architecture** (Sentence-BERT) for embedding generation. This produces fixed-dimensional representations that can be compared efficiently using cosine similarity. See [Research Background](#/resources-research) for details.

### Embedding Aggregation

The default aggregation strategy is **mean pooling** over token embeddings, which outperforms CLS token extraction for sentence-level similarity tasks (Reimers & Gurevych, 2019).

### Dense Retrieval

Generated embeddings power the **dense retrieval** component of hybrid search. Documents are encoded offline; queries are encoded at search time. Similarity is computed via cosine distance in the shared embedding space (Karpukhin et al., 2020).

## Related Documentation

- [Architecture](#/getting-started-architecture) - System design overview
- [Research Background](#/resources-research) - Technical foundation
- [Operations](./operations.md) - Deployment and maintenance
- [Embedding Sets](#/core-systems-embeddings) - Managing embedding configurations
- [Glossary](#/resources-glossary) - Professional terminology definitions

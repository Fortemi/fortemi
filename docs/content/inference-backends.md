# Inference Backends Guide

This guide covers configuring and using different LLM inference backends in Fortémi.

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
export OLLAMA_URL=http://localhost:11434   # Default
export OLLAMA_EMBEDDING_MODEL=nomic-embed-text
export OLLAMA_GENERATION_MODEL=llama3.2:3b
export OLLAMA_EMBEDDING_DIMENSION=768
```

Or in your application config:

```toml
[inference.ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"
generation_model = "llama3.2:3b"
embedding_dimension = 768
```

### Recommended Models

| Task | Model | VRAM | Notes |
|------|-------|------|-------|
| Embeddings | `nomic-embed-text` | ~2GB | Best quality/speed balance |
| Embeddings | `mxbai-embed-large` | ~2GB | Alternative high-quality |
| Generation | `llama3.2:3b` | ~4GB | Fast, good quality |
| Generation | `llama3.1:8b` | ~8GB | Better quality, slower |
| Generation | `qwen2.5:7b` | ~6GB | Strong reasoning |
| Code | `qwen2.5-coder:7b` | ~6GB | Code-focused tasks |

### Installing Models

```bash
# Pull embedding model
ollama pull nomic-embed-text

# Pull generation model
ollama pull llama3.2:3b

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
export OPENAI_API_KEY=sk-...                          # Required for OpenAI cloud
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
export OPENAI_API_KEY=sk-proj-...
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
export INFERENCE_BACKEND=ollama

# Use OpenAI
export INFERENCE_BACKEND=openai
```

Or in config:

```toml
[inference]
backend = "openai"  # or "ollama"
```

## API Endpoints

### Health Check

```bash
# Check inference backend health
curl http://localhost:3000/api/v1/inference/health

# Response:
{
  "backend": "ollama",
  "status": "healthy",
  "embedding_model": "nomic-embed-text",
  "generation_model": "llama3.2:3b"
}
```

### Generate Embeddings

```bash
curl -X POST http://localhost:3000/api/v1/inference/embed \
  -H "Content-Type: application/json" \
  -d '{"texts": ["Hello world", "Another text"]}'

# Response:
{
  "embeddings": [[0.1, 0.2, ...], [0.3, 0.4, ...]],
  "model": "nomic-embed-text",
  "dimension": 768
}
```

### Generate Text

```bash
curl -X POST http://localhost:3000/api/v1/inference/generate \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Summarize this note: ...",
    "system": "You are a helpful assistant.",
    "max_tokens": 500
  }'

# Response:
{
  "text": "This note discusses...",
  "model": "llama3.2:3b",
  "tokens": 150
}
```

## Streaming

The OpenAI backend supports streaming responses:

```bash
curl -X POST http://localhost:3000/api/v1/inference/generate/stream \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Write a poem about knowledge",
    "stream": true
  }'

# Server-Sent Events:
data: {"text": "In ", "done": false}
data: {"text": "the ", "done": false}
data: {"text": "realm ", "done": false}
...
data: {"text": "", "done": true}
```

## Model Profiles

Fortémi includes model profiles with recommended settings:

```bash
# Use best model for task
curl -X POST http://localhost:3000/api/v1/inference/generate \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "...",
    "profile": "reasoning"  # fast, general, reasoning, code, long_context
  }'
```

| Profile | Optimized For | Example Models |
|---------|--------------|----------------|
| `fast` | Speed | llama3.2:1b, gpt-4o-mini |
| `general` | Balance | llama3.2:3b, gpt-4o-mini |
| `reasoning` | Complex tasks | qwen2.5:7b, gpt-4o |
| `code` | Programming | qwen2.5-coder:7b, gpt-4o |
| `long_context` | Large documents | llama3.1:8b, gpt-4o |

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

### Metrics

```bash
# Get inference metrics
curl http://localhost:3000/api/v1/metrics/inference

# Response:
{
  "embedding_requests": 1500,
  "generation_requests": 200,
  "average_embedding_latency_ms": 45,
  "average_generation_latency_ms": 2500,
  "errors": 3
}
```

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

Fortémi uses **bi-encoder architecture** (Sentence-BERT) for embedding generation. This produces fixed-dimensional representations that can be compared efficiently using cosine similarity. See [Research Background](./research-background.md#sentence-embeddings) for details.

### Embedding Aggregation

The default aggregation strategy is **mean pooling** over token embeddings, which outperforms CLS token extraction for sentence-level similarity tasks (Reimers & Gurevych, 2019).

### Dense Retrieval

Generated embeddings power the **dense retrieval** component of hybrid search. Documents are encoded offline; queries are encoded at search time. Similarity is computed via cosine distance in the shared embedding space (Karpukhin et al., 2020).

## Related Documentation

- [Architecture](./architecture.md) - System design overview
- [Research Background](./research-background.md) - Technical foundation
- [Operations](./operations.md) - Deployment and maintenance
- [Embedding Sets](./embedding-sets.md) - Managing embedding configurations
- [Glossary](./glossary.md) - Professional terminology definitions

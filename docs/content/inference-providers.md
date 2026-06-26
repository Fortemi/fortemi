# Inference Providers

Fortemi supports multiple LLM inference providers for text generation and embeddings. The default is a local Ollama instance. OpenAI-compatible providers (vLLM, LiteLLM, LocalAI, OpenRouter) require no additional code — point the OpenAI backend at their URL.

## Environment Variable Reference

### Ollama URL resolution order

| Priority | Variable | Example |
|----------|----------|---------|
| 1 | `MATRIC_OLLAMA_URL` | `http://gpu-server:11434` |
| 2 | `OLLAMA_BASE` | `http://gpu-server:11434` |
| 3 | `OLLAMA_URL` | `http://gpu-server:11434` |
| 4 | `OLLAMA_HOST` | `http://gpu-server:11434` |
| default | *(hardcoded)* | `http://127.0.0.1:11434` |

### All variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MATRIC_INFERENCE_DEFAULT` | `ollama` | Active backend (`ollama` or `openai`) |
| `MATRIC_OLLAMA_URL` | `http://127.0.0.1:11434` | Ollama base URL |
| `MATRIC_OLLAMA_GENERATION_MODEL` | `qwen3.5:27b` | Ollama generation model |
| `MATRIC_OLLAMA_EMBEDDING_MODEL` | `nomic-embed-text` | Ollama embedding model |
| `MATRIC_OPENAI_URL` | `https://api.openai.com/v1` | OpenAI-compatible base URL |
| `MATRIC_OPENAI_API_KEY` | — | API key (falls back to `OPENAI_API_KEY`) |
| `MATRIC_OPENAI_GENERATION_MODEL` | `gpt-4o-mini` | Generation model for OpenAI backend |
| `MATRIC_OPENAI_EMBEDDING_MODEL` | `text-embedding-3-small` | Embedding model for OpenAI backend |
| `OPENAI_API_KEY` | — | Fallback API key; also enables OpenRouter auto-discovery |
| `OPENROUTER_API_KEY` | — | Enables OpenRouter provider for per-request model overrides |
| `LLAMACPP_BASE_URL` | — | Enables llama.cpp provider (e.g. `http://localhost:8080`) |
| `LLAMACPP_API_KEY` | — | Optional API key for llama.cpp server |

Legacy variable names (`OLLAMA_BASE`, `OLLAMA_GEN_MODEL`, `OLLAMA_EMBED_MODEL`) remain supported as fallbacks.

## Configuration File

TOML configuration takes priority over environment variables.

Default path: `~/.config/matric-memory/inference.toml`

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://127.0.0.1:11434"
generation_model = "qwen3.5:27b"
embedding_model = "nomic-embed-text"
```

Environment variable substitution is supported:

```toml
[inference.openai]
api_key = "${OPENAI_API_KEY}"
```

## Provider Setup

### Ollama (local, default)

No configuration required. Fortemi connects to `http://127.0.0.1:11434` by default.

Pull required models before starting:

```bash
ollama pull nomic-embed-text
ollama pull qwen3.5:27b
```

### Ollama (remote or Docker)

```bash
# Remote GPU server
MATRIC_OLLAMA_URL=http://gpu-server:11434

# Docker Compose (container name as hostname)
MATRIC_OLLAMA_URL=http://ollama:11434
```

**Pitfall:** Ollama's default bind is `127.0.0.1`. For remote access, start it with:

```bash
OLLAMA_HOST=0.0.0.0 ollama serve
```

### OpenAI

```bash
MATRIC_INFERENCE_DEFAULT=openai
MATRIC_OPENAI_API_KEY=<OPENAI_API_KEY>
# MATRIC_OPENAI_URL defaults to https://api.openai.com/v1
MATRIC_OPENAI_GENERATION_MODEL=gpt-4o-mini
MATRIC_OPENAI_EMBEDDING_MODEL=text-embedding-3-small
```

Switching embedding models changes the vector dimension — regenerate all embeddings afterwards (see Troubleshooting).

### OpenRouter

OpenRouter provides access to cloud models (Anthropic, Google, Meta, etc.) through an OpenAI-compatible API. It supports generation only — embeddings must use Ollama or OpenAI.

```bash
# Use as primary backend
MATRIC_INFERENCE_DEFAULT=openai
MATRIC_OPENAI_URL=https://openrouter.ai/api/v1
MATRIC_OPENAI_API_KEY=<OPENROUTER_API_KEY>
MATRIC_OPENAI_GENERATION_MODEL=anthropic/claude-sonnet-4-20250514

# Or set OPENROUTER_API_KEY to enable it for per-request model overrides only
OPENROUTER_API_KEY=<OPENROUTER_API_KEY>
```

With `OPENROUTER_API_KEY` set, you can route individual operations to OpenRouter using provider-qualified slugs:

```bash
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{"note_id": "...", "job_type": "ai_revision", "model_override": "openrouter:anthropic/claude-sonnet-4-20250514"}'
```

**Pitfall:** OpenRouter does not host embedding models. Configure a separate embedding backend (Ollama or OpenAI) and use `[inference.routing]` to split traffic.

### llama.cpp

llama.cpp's HTTP server exposes an OpenAI-compatible API. Point Fortémi at it with `LLAMACPP_BASE_URL`:

```bash
LLAMACPP_BASE_URL=http://localhost:8080
LLAMACPP_API_KEY=         # optional; omit if not required
```

Use provider-qualified slugs for per-request routing:

```bash
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{"note_id": "...", "job_type": "ai_revision", "model_override": "llamacpp:llama-3.2-3b"}'
```

llama.cpp supports generation only — use Ollama or OpenAI for embeddings.

**Hot-swap:** Change `LLAMACPP_BASE_URL` at runtime without restarting the server by sending a PUT to the runtime config API (see [Runtime Configuration API](#runtime-configuration-api)).

### vLLM

vLLM exposes an OpenAI-compatible API. Use the OpenAI backend pointed at the vLLM server.

```bash
MATRIC_INFERENCE_DEFAULT=openai
MATRIC_OPENAI_URL=http://host:8000/v1
MATRIC_OPENAI_API_KEY=token-if-required
MATRIC_OPENAI_GENERATION_MODEL=meta-llama/Llama-3.1-8B-Instruct
```

**Pitfall:** vLLM does not serve embedding models by default. Run a separate Ollama instance for embeddings and use operation routing (see below).

### LiteLLM

LiteLLM is an OpenAI-compatible proxy that can front multiple upstream providers.

```bash
MATRIC_INFERENCE_DEFAULT=openai
MATRIC_OPENAI_URL=http://host:4000/v1
MATRIC_OPENAI_API_KEY=<API_KEY>
MATRIC_OPENAI_GENERATION_MODEL=your-model-alias
```

### LocalAI

```bash
MATRIC_INFERENCE_DEFAULT=openai
MATRIC_OPENAI_URL=http://host:8080/v1
# No API key required
MATRIC_OPENAI_GENERATION_MODEL=gpt-3.5-turbo
MATRIC_OPENAI_EMBEDDING_MODEL=all-MiniLM-L6-v2
```

## Operation Routing and Fallback

Route embeddings and generation to different backends. Useful for keeping embeddings local while using a cloud API for generation quality. Configure via TOML only (not environment variables).

```toml
[inference.routing]
embedding = "ollama"    # always local
generation = "openai"   # cloud for quality

[inference.fallback]
enabled = true
chain = ["openai", "ollama"]   # try openai first, fall back to ollama
max_retries = 1
health_check_timeout_secs = 5
```

Every backend named in `routing` or `fallback` must have a corresponding `[inference.ollama]` or `[inference.openai]` section. See [inference-configuration.md](./inference-configuration.md) for a full hybrid example.

## Per-Request Model Overrides

Any generation job accepts a `model_override` using a provider-qualified slug:

| Slug format | Routes to |
|-------------|-----------|
| `qwen3.5:9b` | Default provider (Ollama) |
| `ollama:qwen3.5:27b` | Explicit Ollama |
| `openai:gpt-4o` | OpenAI |
| `openrouter:anthropic/claude-sonnet-4-20250514` | OpenRouter |
| `llamacpp:model-name` | llama.cpp |

```bash
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{"note_id": "...", "job_type": "ai_revision", "model_override": "openai:gpt-4o-mini"}'
```

List all available models and provider health: `GET /api/v1/models`

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| "Connection refused" (Ollama) | Confirm `ollama serve` is running. For remote, start with `OLLAMA_HOST=0.0.0.0 ollama serve`. |
| "Connection refused" (Docker) | Confirm container name matches `MATRIC_OLLAMA_URL`. |
| "Model not found" (Ollama) | Run `ollama pull <model-name>`. |
| "Model not found" (cloud) | Verify the model slug matches the provider's API exactly. |
| "Unauthorized" | Set `MATRIC_OPENAI_API_KEY`. For local servers, use a dummy value: `MATRIC_OPENAI_API_KEY=local`. |
| Routing/fallback ignored | Every backend in `[inference.routing]` or `[inference.fallback]` must have a configured section. Missing sections fail validation at startup. |
| Config file not loading | Verify `~/.config/matric-memory/inference.toml` exists. Enable `RUST_LOG=matric_inference=debug` to trace loading. |

### Embeddings dimension mismatch after backend change

Embeddings from different models have different dimensions and are incompatible. After changing the embedding model or backend, regenerate all embeddings:

```bash
curl -X POST http://localhost:3000/api/v1/jobs/batch \
  -H "Content-Type: application/json" \
  -d '{"job_type": "regenerate_embeddings", "scope": "all"}'
```

## Related Documentation

- [inference-backends.md](./inference-backends.md) — API endpoints, streaming, model profiles
- [inference-configuration.md](./inference-configuration.md) — full TOML reference, routing, fallback
- [embedding-model-selection.md](./embedding-model-selection.md) — choosing the right model
- [ollama-optimization.md](./ollama-optimization.md) — GPU tuning, context window, parallelism

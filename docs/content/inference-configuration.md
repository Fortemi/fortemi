# Inference Configuration System

The Fortémi inference configuration system provides a flexible way to select and configure LLM inference backends.

## Supported Backends

- **Ollama** (default): Local inference server
- **OpenAI**: OpenAI API
- **OpenRouter**: Multi-provider API gateway (Anthropic, Google, Meta, etc.)
- **OpenAI-compatible**: Any OpenAI-compatible endpoint (Azure, LocalAI, vLLM, etc.)

## Configuration Methods

### 1. TOML Configuration File

Default location: `~/.config/fortemi/inference.toml`

#### Example: Ollama Only

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"
```

#### Example: OpenAI Only

```toml
[inference]
default = "openai"

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"  # Environment variable substitution
generation_model = "gpt-4o-mini"
embedding_model = "text-embedding-3-small"
```

#### Example: Both Backends

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
generation_model = "gpt-4"
embedding_model = "text-embedding-3-small"
```

### 2. Environment Variables

All configuration options can be specified via environment variables:

#### Backend Selection

- `MATRIC_INFERENCE_DEFAULT` - Set to "ollama" or "openai"

#### Ollama Configuration

- `MATRIC_OLLAMA_URL` - Base URL (default: `http://localhost:11434`)
- `MATRIC_OLLAMA_GENERATION_MODEL` - Model for text generation (default: `gpt-oss:20b`)
- `MATRIC_OLLAMA_EMBEDDING_MODEL` - Model for embeddings (default: `nomic-embed-text`)

#### OpenAI Configuration

- `MATRIC_OPENAI_URL` - Base URL (default: `https://api.openai.com/v1`)
- `MATRIC_OPENAI_API_KEY` - API key (required for cloud endpoints)
- `MATRIC_OPENAI_GENERATION_MODEL` - Model for text generation (default: `gpt-4o-mini`)
- `MATRIC_OPENAI_EMBEDDING_MODEL` - Model for embeddings (default: `text-embedding-3-small`)

#### Example

```bash
export MATRIC_INFERENCE_DEFAULT=ollama
export MATRIC_OLLAMA_URL=http://localhost:11434
export MATRIC_OLLAMA_GENERATION_MODEL=llama3.1:8b
export MATRIC_OLLAMA_EMBEDDING_MODEL=nomic-embed-text
```

## Loading Priority

1. **TOML file** (if exists at `~/.config/fortemi/inference.toml`)
2. **Environment variables** (fallback if no config file)

## Environment Variable Substitution in TOML

You can reference environment variables in TOML files using `${VAR_NAME}` syntax:

```toml
[inference.openai]
api_key = "${OPENAI_API_KEY}"
```

This is useful for keeping secrets out of configuration files.

## Validation

The configuration system validates:

- URLs must start with `http://` or `https://`
- Model names cannot be empty
- The default backend must be configured
- Each backend's configuration must be valid

## Usage in Code

### Loading Configuration

```rust
use matric_inference::config::InferenceConfig;

// Load from default path or fall back to env vars
let config = InferenceConfig::load()?;

// Or explicitly from a file
let config = InferenceConfig::from_file(Path::new("custom.toml"))?;

// Or from environment variables only
let config = InferenceConfig::from_env();
```

### Accessing Configuration

```rust
match config.default {
    InferenceBackend::Ollama => {
        let ollama_config = config.ollama.unwrap();
        println!("Using Ollama at {}", ollama_config.base_url);
    }
    InferenceBackend::OpenAI => {
        let openai_config = config.openai.unwrap();
        println!("Using OpenAI at {}", openai_config.base_url);
    }
}
```

## Migration from Legacy Environment Variables

If you're currently using the legacy environment variables (`OLLAMA_BASE`, `OLLAMA_EMBED_MODEL`, etc.), those will continue to work. The new system uses the `MATRIC_` prefix to avoid conflicts with system-wide Ollama configuration.

## Common Configurations

### Local Ollama

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"
```

### OpenRouter

```toml
[inference]
default = "openai"

[inference.openai]
base_url = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"
generation_model = "anthropic/claude-3-sonnet"
embedding_model = "openai/text-embedding-3-small"
```

### Azure OpenAI

```toml
[inference]
default = "openai"

[inference.openai]
base_url = "https://your-resource.openai.azure.com/openai/deployments"
api_key = "${AZURE_OPENAI_KEY}"
generation_model = "gpt-4"
embedding_model = "text-embedding-ada-002"
```

### LocalAI

```toml
[inference]
default = "openai"

[inference.openai]
base_url = "http://localhost:8080/v1"
api_key = ""  # Not required for local
generation_model = "gpt-3.5-turbo"
embedding_model = "all-MiniLM-L6-v2"
```

## Operation-Specific Routing

Route different operations to different backends. This is useful for:
- Using local Ollama for embeddings (privacy) and API for generation (quality)
- Leveraging the strengths of different backends for different tasks

### Example: Hybrid Configuration

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
generation_model = "gpt-4o"
embedding_model = "text-embedding-3-small"

[inference.routing]
embedding = "ollama"    # Use local for privacy
generation = "openai"   # Use API for better quality
```

### Usage in Code

```rust
use matric_inference::config::{InferenceConfig, InferenceOperation};

let config = InferenceConfig::load()?;

// Get the backend for a specific operation
let embedding_backend = config.get_backend_for_operation(InferenceOperation::Embedding);
let generation_backend = config.get_backend_for_operation(InferenceOperation::Generation);
```

## Automatic Fallback

Configure automatic failover when the primary backend is unavailable:

### Example: Fallback Configuration

```toml
[inference]
default = "openai"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
generation_model = "gpt-4o"
embedding_model = "text-embedding-3-small"

[inference.fallback]
enabled = true
chain = ["openai", "ollama"]  # Try in order
max_retries = 2                # Retries per backend
health_check_timeout_secs = 5  # Timeout for health checks
```

### Fallback Chain

The fallback chain specifies the order in which backends are tried:
1. Primary backend attempts the request
2. If it fails, the next backend in the chain is tried
3. Each backend is retried up to `max_retries` times before moving to the next

### Usage in Code

```rust
use matric_inference::config::InferenceConfig;

let config = InferenceConfig::load()?;

// Check if fallback is enabled
if config.is_fallback_enabled() {
    // Get the remaining backends to try
    let fallback_chain = config.get_fallback_chain(current_backend);
    for backend in fallback_chain {
        // Try the next backend...
    }
}
```

### Combining Routing and Fallback

You can use both routing and fallback together:

```toml
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
generation_model = "llama3.1:8b"
embedding_model = "nomic-embed-text"

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
generation_model = "gpt-4o"
embedding_model = "text-embedding-3-small"

[inference.routing]
embedding = "ollama"    # Always local for privacy
generation = "openai"   # API for generation

[inference.fallback]
enabled = true
chain = ["openai", "ollama"]  # Generation falls back to Ollama if API is down
```

## Provider-Qualified Model Slugs

All LLM-backed operations (AI revision, title generation, concept tagging, metadata extraction, context update) support **per-operation model override** using provider-qualified slugs.

### Slug Format

```
[provider:]model_slug

Examples:
  "qwen3:8b"                                        → default provider (Ollama)
  "ollama:qwen3:8b"                                  → explicit Ollama
  "openai:gpt-4o"                                    → OpenAI
  "openai:gpt-4.1-mini"                              → OpenAI budget tier
  "openrouter:anthropic/claude-sonnet-4-20250514"     → OpenRouter
```

Bare slugs (no provider prefix) always route to the default provider (Ollama) for backward compatibility.

### Auto-Discovery

The `ProviderRegistry` automatically discovers available providers from environment variables:

| Environment Variable | Provider |
|---------------------|----------|
| *(always available)* | Ollama (default, local) |
| `OPENAI_API_KEY` | OpenAI |
| `OPENROUTER_API_KEY` | OpenRouter |

No additional configuration needed — set the API key and the provider becomes available.

### Usage Examples

```bash
# Use OpenAI for AI revision
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "...",
    "job_type": "ai_revision",
    "model_override": "openai:gpt-4o"
  }'

# Use OpenRouter for concept tagging
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "...",
    "job_type": "concept_tagging",
    "model_override": "openrouter:anthropic/claude-sonnet-4-20250514"
  }'

# Bulk reprocess with a specific model
# Via MCP: bulk_reprocess_notes with model_override: "openai:gpt-4o-mini"
```

### Model Discovery

List all available models across all providers:

```bash
curl http://localhost:3000/api/v1/models
```

Returns models grouped by provider with health status:

```json
{
  "models": [
    { "slug": "qwen3:8b", "provider": "ollama", "type": "generation" },
    { "slug": "gpt-4o", "provider": "openai", "type": "generation" }
  ],
  "providers": [
    { "id": "ollama", "is_default": true, "health": "healthy" },
    { "id": "openai", "is_default": false, "health": "healthy" }
  ]
}
```

### Security

API keys are **never exposed** in job payloads, API responses, or logs. The `ProviderRegistry` resolves keys from environment variables at job execution time.

## Troubleshooting

### Configuration not loading

Check the default path:
```bash
echo ~/.config/fortemi/inference.toml
```

Enable debug logging to see what configuration is being loaded:
```bash
export RUST_LOG=matric_inference=debug
```

### Invalid configuration

Run validation:
```rust
let config = InferenceConfig::load()?;
config.validate()?; // Will return detailed error messages
```

### Environment variables not working

Ensure you're using the `MATRIC_` prefix, not the legacy variable names.

### Routing or fallback not working

Ensure all backends referenced in routing or fallback are configured:
```toml
# This will fail validation - OpenAI is referenced but not configured
[inference]
default = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
# ...

[inference.routing]
embedding = "openai"  # ERROR: OpenAI not configured!
```

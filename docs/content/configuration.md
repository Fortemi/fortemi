# Configuration Reference

This document is the single source of truth for all Fortémi configuration options. It covers environment variables, TOML configuration files, feature flags, and deployment-specific settings.

## Overview

### Configuration Hierarchy

Fortemi uses a layered configuration approach:

1. **TOML configuration files** - Structured config for inference backends
2. **Environment variables** - Runtime settings, overrides, and secrets
3. **Built-in defaults** - Sensible defaults for most use cases

Environment variables take precedence over TOML files, which take precedence over defaults.

### Configuration Files

| File | Location | Purpose |
|------|----------|---------|
| `.env` | Project root | Environment variables for local development |
| `inference.toml` | Config directory | Inference backend configuration |
| `docker-compose.bundle.yml` | Project root | Docker environment variables |

### Docker Bundle Considerations

In Docker bundle deployments:
- Environment variables are set in `docker-compose.bundle.yml` or `.env` file
- The API container reads environment variables on startup
- Changes require container restart: `docker compose -f docker-compose.bundle.yml down && docker compose -f docker-compose.bundle.yml up -d`
- Use `host.docker.internal` to access services on the Docker host (e.g., Ollama)
- Use `172.17.0.1` on Linux when `host.docker.internal` is unavailable

## Environment Variables

### Database

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `DATABASE_URL` | String | `postgres://matric:matric@localhost:5432/matric` | PostgreSQL connection URL with user, password, host, port, and database name |

**Example:**
```bash
DATABASE_URL=postgres://myuser:mypass@db.example.com:5432/matric_prod
```

### API Server

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `HOST` | String | `0.0.0.0` | IP address to bind the API server (0.0.0.0 = all interfaces) |
| `PORT` | Integer | `3000` | Port number for the HTTP API server |

**Example:**
```bash
HOST=127.0.0.1  # Localhost only
PORT=8080       # Custom port
```

### Authentication

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `AUTH_ENABLED` | Boolean | `false` | Enable OAuth authentication (requires ISSUER_URL) |
| `RATE_LIMIT_ENABLED` | Boolean | `false` | Enable rate limiting for API endpoints |
| `RATE_LIMIT_REQUESTS` | Integer | `100` | Maximum requests per time period |
| `RATE_LIMIT_PERIOD_SECS` | Integer | `60` | Rate limit time window in seconds |
| `ISSUER_URL` | String | None | External base URL for OAuth discovery and MCP (e.g., https://memory.example.com) |

**Example (Personal Use):**
```bash
AUTH_ENABLED=false
RATE_LIMIT_ENABLED=false
```

**Example (Team Deployment):**
```bash
AUTH_ENABLED=true
RATE_LIMIT_ENABLED=true
RATE_LIMIT_REQUESTS=1000
RATE_LIMIT_PERIOD_SECS=60
ISSUER_URL=https://memory.team.com
```

### Logging

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `RUST_LOG` | String | `info` | Log level and filtering for Rust components (uses env_logger syntax) |
| `LOG_FORMAT` | String | `pretty` | Log output format: `pretty`, `json`, or `compact` |
| `LOG_FILE` | String | None | Path to log file (logs to stdout if not set) |
| `LOG_ANSI` | Boolean | `true` | Enable ANSI color codes in logs |

**Common Configurations:**

**Production (default):**
```bash
RUST_LOG=info
LOG_FORMAT=json
LOG_FILE=/var/log/matric/api.log
LOG_ANSI=false
```

**API debugging:**
```bash
RUST_LOG=matric_api=debug,info
```

**Inference debugging:**
```bash
RUST_LOG=matric_inference=debug,info
```

**Search debugging:**
```bash
RUST_LOG=matric_db=debug,matric_search=debug,info
```

**Full debug (verbose):**
```bash
RUST_LOG=debug
```

**Specific module debugging:**
```bash
RUST_LOG=matric_api::routes::search=trace,info
```

### Background Worker

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `WORKER_ENABLED` | Boolean | `true` | Enable background job processing (embeddings, linking, cleanup) |
| `WORKER_THREADS` | Integer | CPU cores | Number of worker threads for background jobs |
| `JOB_POLL_INTERVAL` | Integer | `5` | Polling interval in seconds for checking new jobs |

**Example:**
```bash
WORKER_ENABLED=true
WORKER_THREADS=4
JOB_POLL_INTERVAL=10
```

### Memory Management

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `MAX_MEMORIES` | Integer | `100` | Maximum number of memory archives allowed |

**Example:**
```bash
MAX_MEMORIES=200
```

**Memory Limits:**
- Attempting to create memories beyond `MAX_MEMORIES` returns HTTP 400
- Check current usage via `GET /api/v1/memories/overview`
- Each memory adds minimal overhead (<1MB metadata + indexes)

### Request Headers

| Header | Values | Description |
|--------|--------|-------------|
| `X-Fortemi-Memory` | Memory name | Routes request to specified memory (default: "default") |
| `Authorization` | Bearer token | API authentication (when `AUTH_ENABLED=true`) |

The `X-Fortemi-Memory` header routes all API requests to a specific memory archive. Without this header, requests operate on the `default` memory. See the [Multi-Memory Guide](./multi-memory.md) for details.

### Ollama Inference

Ollama is the default inference backend for local LLM inference without API costs.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `OLLAMA_URL` | String | `http://localhost:11434` | Ollama API endpoint URL |
| `OLLAMA_BASE` | String | `http://localhost:11434` | Alias for OLLAMA_URL (for compatibility) |
| `OLLAMA_HOST` | String | `http://localhost:11434` | Alias for OLLAMA_URL (for compatibility) |
| `OLLAMA_EMBEDDING_MODEL` | String | `nomic-embed-text` | Model name for generating embeddings |
| `OLLAMA_EMBED_MODEL` | String | `nomic-embed-text` | Alias for OLLAMA_EMBEDDING_MODEL |
| `OLLAMA_GENERATION_MODEL` | String | None | Model name for text generation (optional) |
| `OLLAMA_GEN_MODEL` | String | None | Alias for OLLAMA_GENERATION_MODEL |
| `OLLAMA_EMBEDDING_DIMENSION` | Integer | `768` | Vector dimensionality for embeddings |
| `OLLAMA_EMBED_DIM` | Integer | `768` | Alias for OLLAMA_EMBEDDING_DIMENSION |
| `OLLAMA_NUM_CTX` | Integer | Model default | Context window size in tokens |
| `OLLAMA_NUM_GPU` | Integer | `99` | Number of GPU layers to offload (99 = all layers) |
| `OLLAMA_NUM_PARALLEL` | Integer | `1` | Number of concurrent requests to process |

**Example (Docker Desktop - macOS/Windows):**
```bash
OLLAMA_URL=http://host.docker.internal:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_GENERATION_MODEL=llama3.2:3b
OLLAMA_EMBEDDING_DIMENSION=768
OLLAMA_NUM_GPU=99
```

**Example (Linux with Docker):**
```bash
OLLAMA_URL=http://172.17.0.1:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_GENERATION_MODEL=qwen2.5:7b
OLLAMA_EMBEDDING_DIMENSION=768
```

**Example (Performance Tuning):**
```bash
OLLAMA_URL=http://localhost:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_GENERATION_MODEL=qwen2.5:7b
OLLAMA_NUM_CTX=16384        # Larger context window
OLLAMA_NUM_GPU=99           # All layers on GPU
OLLAMA_NUM_PARALLEL=4       # Process 4 requests concurrently
```

### OpenAI Inference

The OpenAI backend supports OpenAI's cloud API and any OpenAI-compatible endpoint (Azure OpenAI, vLLM, LocalAI, LM Studio, etc.).

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `INFERENCE_BACKEND` | String | `ollama` | Backend selection: `ollama` or `openai` |
| `OPENAI_API_KEY` | String | None | API key for OpenAI cloud (required for OpenAI cloud) |
| `OPENAI_BASE_URL` | String | `https://api.openai.com/v1` | OpenAI API base URL or compatible endpoint |
| `OPENAI_EMBEDDING_MODEL` | String | `text-embedding-3-small` | Model name for embeddings |
| `OPENAI_EMBED_MODEL` | String | `text-embedding-3-small` | Alias for OPENAI_EMBEDDING_MODEL |
| `OPENAI_GENERATION_MODEL` | String | `gpt-4o-mini` | Model name for text generation |
| `OPENAI_GEN_MODEL` | String | `gpt-4o-mini` | Alias for OPENAI_GENERATION_MODEL |
| `OPENAI_EMBEDDING_DIMENSION` | Integer | `1536` | Vector dimensionality for embeddings |
| `OPENAI_EMBED_DIM` | Integer | `1536` | Alias for OPENAI_EMBEDDING_DIMENSION |
| `OPENAI_TIMEOUT` | Integer | `120` | Request timeout in seconds |
| `OPENAI_MAX_RETRIES` | Integer | `3` | Maximum number of retry attempts for failed requests |
| `OPENAI_SKIP_TLS_VERIFY` | Boolean | `false` | Disable TLS certificate verification (insecure, for testing only) |

**Example (OpenAI Cloud):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxx
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_EMBEDDING_MODEL=text-embedding-3-small
OPENAI_GENERATION_MODEL=gpt-4o-mini
OPENAI_EMBEDDING_DIMENSION=1536
OPENAI_TIMEOUT=120
OPENAI_MAX_RETRIES=3
```

**Example (Azure OpenAI):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=your-azure-key
OPENAI_BASE_URL=https://your-resource.openai.azure.com/openai/deployments/your-deployment
OPENAI_EMBEDDING_MODEL=text-embedding-ada-002
OPENAI_GENERATION_MODEL=gpt-4
```

**Example (vLLM Self-Hosted):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=token
OPENAI_BASE_URL=http://vllm-server:8000/v1
OPENAI_GENERATION_MODEL=meta-llama/Llama-3.1-8B-Instruct
OPENAI_TIMEOUT=180
```

**Example (LocalAI):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=localai
OPENAI_BASE_URL=http://localhost:8080/v1
OPENAI_EMBEDDING_MODEL=text-embedding-ada-002
OPENAI_GENERATION_MODEL=gpt-3.5-turbo
```

### MCP Server

The MCP (Model Context Protocol) server provides Claude/AI integration.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `ISSUER_URL` | String | None | External base URL for OAuth and MCP discovery (required for MCP) |
| `MCP_CLIENT_ID` | String | None | OAuth client ID for token introspection (required for MCP auth) |
| `MCP_CLIENT_SECRET` | String | None | OAuth client secret for token introspection (required for MCP auth) |
| `MCP_BASE_URL` | String | `${ISSUER_URL}/mcp` | MCP protected resource URL (derived from ISSUER_URL) |
| `MCP_TRANSPORT` | String | `http` | Transport mode: `stdio` (direct process) or `http` (network) |
| `MCP_PORT` | Integer | `3001` | Port for MCP HTTP server (when transport=http) |
| `MATRIC_API_URL` | String | `http://localhost:3000` | API server URL for MCP to connect to |

**Example (Docker Bundle):**
```bash
ISSUER_URL=https://memory.example.com
MCP_CLIENT_ID=mm_xxxxxxxxxxxxx
MCP_CLIENT_SECRET=xxxxxxxxxxxxx
MCP_BASE_URL=https://memory.example.com/mcp
MCP_TRANSPORT=http
MCP_PORT=3001
```

**Example (Claude Desktop - stdio):**
```bash
MCP_TRANSPORT=stdio
MATRIC_API_URL=http://localhost:3000
```

**OAuth Client Registration:**

Before configuring MCP, register an OAuth client for token introspection:

```bash
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "MCP Server",
    "grant_types": ["client_credentials"],
    "scope": "mcp read"
  }'
```

Save the returned `client_id` and `client_secret` to `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET`.

### Search Tuning

These feature flags control advanced search capabilities. They are disabled by default because they increase database complexity and require specific PostgreSQL extensions.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FTS_WEBSEARCH_TO_TSQUERY` | Boolean | `true` | Enable search operators (OR, NOT, phrase search with quotes) |
| `FTS_SCRIPT_DETECTION` | Boolean | `false` | Auto-detect query language/script for optimal tokenization |
| `FTS_TRIGRAM_FALLBACK` | Boolean | `false` | Enable emoji and symbol search via pg_trgm trigram indexes |
| `FTS_BIGRAM_CJK` | Boolean | `false` | Enable optimized CJK (Chinese/Japanese/Korean) search via pg_bigm |
| `FTS_MULTILINGUAL_CONFIGS` | Boolean | `false` | Enable language-specific FTS configurations for stemming |

**Why These Are Disabled by Default:**

- **FTS_SCRIPT_DETECTION**: Adds complexity to query processing; only needed for mixed-language queries
- **FTS_TRIGRAM_FALLBACK**: Requires pg_trgm extension; only needed for emoji/symbol search
- **FTS_BIGRAM_CJK**: Requires pg_bigm extension (not installed by default); only for CJK languages
- **FTS_MULTILINGUAL_CONFIGS**: Requires multiple FTS dictionaries; increases storage and index size

**Example (Minimal - English Only):**
```bash
FTS_WEBSEARCH_TO_TSQUERY=true
FTS_SCRIPT_DETECTION=false
FTS_TRIGRAM_FALLBACK=false
FTS_BIGRAM_CJK=false
FTS_MULTILINGUAL_CONFIGS=false
```

**Example (Multilingual Team):**
```bash
FTS_WEBSEARCH_TO_TSQUERY=true
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_BIGRAM_CJK=false
FTS_MULTILINGUAL_CONFIGS=true
```

**Example (Full CJK Support):**
```bash
FTS_WEBSEARCH_TO_TSQUERY=true
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_BIGRAM_CJK=true
FTS_MULTILINGUAL_CONFIGS=true
```

**Performance Impact:**

Enabling all flags increases:
- Index storage by approximately 30-50%
- Index build time by 2-3x
- Query planning overhead by 10-20ms per query

For small installations (< 10,000 notes), enable only the features you need. For large installations (> 100,000 notes), test performance impact before enabling.

## Inference Configuration (inference.toml)

The `inference.toml` file provides structured configuration for inference backends. It supports both Ollama and OpenAI configurations, with the ability to use different backends for different operations.

### File Location

```bash
# Development
./inference.toml

# Production (Docker)
/app/inference.toml
```

### Full Configuration Example

```toml
# =============================================================================
# Inference Backend Configuration
# =============================================================================
# This file configures LLM inference backends for embeddings and generation.
# Supports Ollama (local) and OpenAI (cloud or compatible APIs).

[inference]
# Backend selection: "ollama" or "openai"
# Can be overridden by INFERENCE_BACKEND environment variable
backend = "ollama"

# =============================================================================
# Ollama Configuration (Local LLM)
# =============================================================================
[inference.ollama]
# Ollama API endpoint
url = "http://localhost:11434"

# Model for generating embeddings
# Recommended: nomic-embed-text (768d), mxbai-embed-large (1024d)
embedding_model = "nomic-embed-text"

# Model for text generation (optional)
# Recommended: llama3.2:3b (fast), qwen2.5:7b (quality), llama3.1:8b (balanced)
generation_model = "llama3.2:3b"

# Embedding vector dimensionality
# Must match the model's output dimension
embedding_dimension = 768

# Context window size in tokens (optional)
# Larger values allow more context but use more memory
# num_ctx = 8192

# GPU layers to offload (optional)
# 99 = all layers on GPU (recommended for dedicated GPU)
# 0 = CPU only
# num_gpu = 99

# Concurrent request processing (optional)
# Higher values improve throughput but increase memory usage
# num_parallel = 1

# =============================================================================
# OpenAI Configuration (Cloud or Compatible APIs)
# =============================================================================
[inference.openai]
# OpenAI API base URL
# OpenAI cloud: https://api.openai.com/v1
# Azure: https://YOUR-RESOURCE.openai.azure.com/openai/deployments/YOUR-DEPLOYMENT
# vLLM: http://localhost:8000/v1
# LocalAI: http://localhost:8080/v1
base_url = "https://api.openai.com/v1"

# API key (can use environment variable reference)
# For cloud: sk-proj-xxxxx
# For local servers: any value (usually ignored)
api_key = "${OPENAI_API_KEY}"

# Model for generating embeddings
# OpenAI: text-embedding-3-small, text-embedding-3-large
# Azure: text-embedding-ada-002
embedding_model = "text-embedding-3-small"

# Model for text generation
# OpenAI: gpt-4o-mini, gpt-4o, gpt-4-turbo
# Azure: gpt-4, gpt-35-turbo
generation_model = "gpt-4o-mini"

# Embedding vector dimensionality
# text-embedding-3-small: 1536
# text-embedding-3-large: 3072
embedding_dimension = 1536

# Request timeout in seconds (optional)
# timeout = 120

# Maximum retry attempts (optional)
# max_retries = 3

# Disable TLS verification (insecure, testing only)
# skip_tls_verify = false
```

### Backend Selection

The `[inference]` section controls which backend is used at runtime:

```toml
[inference]
backend = "ollama"  # Use Ollama
```

```toml
[inference]
backend = "openai"  # Use OpenAI
```

This can be overridden by the `INFERENCE_BACKEND` environment variable:

```bash
export INFERENCE_BACKEND=openai
```

### Routing by Operation

You can configure different backends for embeddings vs generation by using both configurations and selecting models:

**Use local Ollama for embeddings, cloud OpenAI for generation:**

```toml
[inference]
backend = "ollama"  # Default to Ollama

[inference.ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"
embedding_dimension = 768
# No generation_model specified

[inference.openai]
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
generation_model = "gpt-4o-mini"
```

The system will use Ollama for embeddings (cost-free, private) and OpenAI for generation (higher quality).

### Fallback Chains

To implement fallback behavior (try local first, fall back to cloud):

1. Configure both backends in `inference.toml`
2. Set primary backend: `backend = "ollama"`
3. When Ollama fails (connection refused, model not found), manually switch to OpenAI via API retry or configuration update

Current implementation does not support automatic fallback. For high availability, consider deploying multiple Ollama instances with load balancing.

## MCP Server Configuration

The MCP server enables integration with Claude Desktop, Claude Code, and other MCP-compatible clients.

### OAuth Setup

Before using the MCP server, you must configure OAuth:

**Step 1: Set ISSUER_URL**

The ISSUER_URL is the external base URL where your Fortémi API is accessible:

```bash
# .env
ISSUER_URL=https://memory.example.com
```

This URL is used for OAuth discovery, token verification, and MCP resource identification.

**Step 2: Register OAuth Client**

Register a client for the MCP server to introspect tokens:

```bash
curl -X POST https://memory.example.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "MCP Server",
    "grant_types": ["client_credentials"],
    "scope": "mcp read"
  }'
```

Response:
```json
{
  "client_id": "mm_xxxxxxxxxxxxx",
  "client_secret": "xxxxxxxxxxxxx",
  "client_name": "MCP Server",
  "grant_types": ["client_credentials"],
  "scope": "mcp read"
}
```

**Step 3: Configure MCP Credentials**

Add the credentials to `.env`:

```bash
# .env
ISSUER_URL=https://memory.example.com
MCP_CLIENT_ID=mm_xxxxxxxxxxxxx
MCP_CLIENT_SECRET=xxxxxxxxxxxxx
```

**Step 4: Restart Services**

```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

**Step 5: Verify**

```bash
curl https://memory.example.com/mcp/.well-known/oauth-protected-resource
```

Expected response:
```json
{
  "resource": "https://memory.example.com/mcp",
  "authorization_servers": ["https://memory.example.com"],
  "scopes_supported": ["mcp", "read", "write"]
}
```

### Transport Modes

The MCP server supports two transport modes:

#### HTTP Transport (Default)

Used for Docker bundle deployments and network access:

```bash
# .env
MCP_TRANSPORT=http
MCP_PORT=3001
MCP_BASE_URL=https://memory.example.com/mcp
```

The MCP server listens on port 3001 and is accessible via HTTP. Configure nginx to proxy `/mcp` to `localhost:3001`.

#### stdio Transport

Used for Claude Desktop when running as a direct subprocess:

```bash
# .env
MCP_TRANSPORT=stdio
MATRIC_API_URL=http://localhost:3000
```

The MCP server communicates via stdin/stdout instead of HTTP. This is configured in Claude Desktop's configuration file.

### Claude Desktop Integration

To connect to Fortemi via stdio transport:

**Configuration File Location:**
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- Linux: `~/.config/Claude/claude_desktop_config.json`

**Configuration:**

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": [
        "/absolute/path/to/Fortémi/mcp-server/build/index.js"
      ],
      "env": {
        "MCP_TRANSPORT": "stdio",
        "MATRIC_API_URL": "http://localhost:3000"
      }
    }
  }
}
```

Restart Claude Desktop to load the configuration.

### Claude Code Integration

Claude Code uses URL-based MCP transport:

**Project .mcp.json:**

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "https://memory.example.com/mcp"
    }
  }
}
```

Claude Code will authenticate using OAuth and connect to the MCP server via HTTPS.

## Example Configurations

### Personal (Minimal)

For personal use with local Ollama, no authentication:

```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric
OLLAMA_URL=http://localhost:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_EMBEDDING_DIMENSION=768
RUST_LOG=info
AUTH_ENABLED=false
RATE_LIMIT_ENABLED=false
```

This provides:
- Full-text search (immediate)
- Semantic search (after embedding generation)
- No rate limiting or authentication
- Local inference (no API costs)

### Team (With Auth)

For team deployment with authentication and rate limiting:

```bash
# .env
DATABASE_URL=postgres://matric:matric@db.internal:5432/matric_prod
HOST=0.0.0.0
PORT=3000
RUST_LOG=info

# Authentication
AUTH_ENABLED=true
RATE_LIMIT_ENABLED=true
RATE_LIMIT_REQUESTS=1000
RATE_LIMIT_PERIOD_SECS=60
ISSUER_URL=https://memory.team.com

# MCP (for Claude integration)
MCP_CLIENT_ID=mm_xxxxxxxxxxxxx
MCP_CLIENT_SECRET=xxxxxxxxxxxxx
MCP_BASE_URL=https://memory.team.com/mcp
MCP_TRANSPORT=http
MCP_PORT=3001

# Ollama (local inference)
OLLAMA_URL=http://ollama.internal:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_GENERATION_MODEL=qwen2.5:7b
OLLAMA_EMBEDDING_DIMENSION=768
OLLAMA_NUM_CTX=8192
OLLAMA_NUM_GPU=99
OLLAMA_NUM_PARALLEL=4

# Background worker
WORKER_ENABLED=true
WORKER_THREADS=8
JOB_POLL_INTERVAL=5

# Logging
LOG_FORMAT=json
LOG_FILE=/var/log/matric/api.log
LOG_ANSI=false
```

This provides:
- OAuth authentication for all API endpoints
- Rate limiting (1000 requests/minute per user)
- MCP integration for Claude
- Optimized Ollama configuration for performance
- Structured JSON logging for analysis

### Enterprise (Full)

For large enterprise deployment with multilingual search, cloud AI, and monitoring:

```bash
# .env
DATABASE_URL=postgres://matric:matric@db-cluster.internal:5432/matric_prod
HOST=0.0.0.0
PORT=3000
RUST_LOG=matric_api=info,matric_db=warn,matric_inference=info

# Authentication and rate limiting
AUTH_ENABLED=true
RATE_LIMIT_ENABLED=true
RATE_LIMIT_REQUESTS=10000
RATE_LIMIT_PERIOD_SECS=60
ISSUER_URL=https://knowledge.corp.com

# MCP server
MCP_CLIENT_ID=mm_xxxxxxxxxxxxx
MCP_CLIENT_SECRET=xxxxxxxxxxxxx
MCP_BASE_URL=https://knowledge.corp.com/mcp
MCP_TRANSPORT=http
MCP_PORT=3001

# Hybrid inference: Local embeddings + Cloud generation
INFERENCE_BACKEND=ollama
OLLAMA_URL=http://ollama-cluster.internal:11434
OLLAMA_EMBEDDING_MODEL=nomic-embed-text
OLLAMA_EMBEDDING_DIMENSION=768
OLLAMA_NUM_CTX=16384
OLLAMA_NUM_GPU=99
OLLAMA_NUM_PARALLEL=8

OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxx
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_GENERATION_MODEL=gpt-4o
OPENAI_TIMEOUT=180
OPENAI_MAX_RETRIES=5

# Multilingual full-text search
FTS_WEBSEARCH_TO_TSQUERY=true
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_BIGRAM_CJK=false
FTS_MULTILINGUAL_CONFIGS=true

# Background worker optimization
WORKER_ENABLED=true
WORKER_THREADS=16
JOB_POLL_INTERVAL=3

# Production logging
LOG_FORMAT=json
LOG_FILE=/var/log/matric/api.log
LOG_ANSI=false

# Backup configuration
BACKUP_DEST=/var/backups/Fortémi
BACKUP_SCRIPT_PATH=/app/scripts/backup.sh
```

This provides:
- Enterprise-grade authentication and rate limiting
- Hybrid inference (local embeddings for privacy, cloud generation for quality)
- Full multilingual search support
- Optimized worker configuration for high throughput
- Structured logging for monitoring and analysis
- Automated backup configuration

## Docker-Specific Considerations

### Accessing Services on Docker Host

When running Fortemi in Docker and accessing services on the host machine:

**macOS and Windows (Docker Desktop):**
```bash
# Use host.docker.internal to access host services
OLLAMA_URL=http://host.docker.internal:11434
```

**Linux:**
```bash
# Use Docker bridge network gateway IP
OLLAMA_URL=http://172.17.0.1:11434

# Or use host network mode in docker-compose.bundle.yml:
# network_mode: "host"
```

### Environment Variable Files

Docker Compose loads `.env` automatically from the project root. Variables set in `docker-compose.bundle.yml` take precedence over `.env`.

**Precedence (highest to lowest):**
1. Environment variables set in shell
2. Environment variables in `docker-compose.bundle.yml`
3. Variables in `.env` file
4. Built-in defaults

### Container Restart After Changes

Environment variable changes require container restart:

```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

Configuration changes take effect immediately on startup.

## Related Documentation

- [Inference Backends](./inference-backends.md) - Detailed backend documentation and model selection
- [Operations Guide](./operations.md) - Deployment, monitoring, and troubleshooting
- [Getting Started](./getting-started.md) - Quick start guide for new users
- [MCP Server](./mcp.md) - Claude integration and MCP protocol details
- [Multilingual FTS](./multilingual-fts.md) - Search feature flags and language support
- [Authentication](./authentication.md) - OAuth setup and user management
- [Multi-Memory Guide](./multi-memory.md) - Parallel memory archives and federated search
- [Search Guide](./search-guide.md) - Search modes and query syntax
- [Hardware Planning](./hardware-planning.md) - Capacity planning and performance optimization

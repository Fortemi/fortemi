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
| `ALLOWED_ORIGINS` | String | `http://localhost:3000` | Comma-separated list of allowed CORS origins |
| `MATRIC_MAX_BODY_SIZE_BYTES` | Integer | `2147483648` | Maximum request body size in bytes (default: 2 GB, needed for database backup uploads) |
| `MATRIC_MAX_UPLOAD_SIZE_BYTES` | Integer | `52428800` | Maximum file upload size in bytes (default: 50 MB). Enforced at the multipart upload route and validated per-file. |

**Example:**
```bash
HOST=127.0.0.1  # Localhost only
PORT=8080       # Custom port
ALLOWED_ORIGINS=https://memory.example.com,http://localhost:3000
MATRIC_MAX_BODY_SIZE_BYTES=2147483648
MATRIC_MAX_UPLOAD_SIZE_BYTES=104857600  # 100 MB
```

### Authentication

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `REQUIRE_AUTH` | Boolean | `false` | Require authentication on all `/api/v1/*` endpoints. When `false`, all endpoints are publicly accessible. |
| `ISSUER_URL` | String | `http://<HOST>:<PORT>` | External base URL for OAuth discovery and MCP (e.g., https://memory.example.com). Required for OAuth/MCP. |
| `OAUTH_TOKEN_LIFETIME_SECS` | Integer | `3600` | OAuth access token lifetime in seconds (1 hour). Shorter = more secure; longer = less re-authentication friction. |
| `OAUTH_MCP_TOKEN_LIFETIME_SECS` | Integer | `86400` | MCP OAuth access token lifetime in seconds (24 hours). MCP sessions are interactive — shorter tokens cause mid-session disconnects. |

**Example (Personal Use):**
```bash
REQUIRE_AUTH=false
```

**Example (Team Deployment):**
```bash
REQUIRE_AUTH=true
ISSUER_URL=https://memory.team.com
OAUTH_TOKEN_LIFETIME_SECS=3600
OAUTH_MCP_TOKEN_LIFETIME_SECS=86400
```

### Rate Limiting

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `RATE_LIMIT_ENABLED` | Boolean | `false` | Enable rate limiting for API endpoints |
| `RATE_LIMIT_REQUESTS` | Integer | `100` | Maximum requests per time window |
| `RATE_LIMIT_PERIOD_SECS` | Integer | `60` | Rate limit time window in seconds |

**Example:**
```bash
RATE_LIMIT_ENABLED=true
RATE_LIMIT_REQUESTS=1000
RATE_LIMIT_PERIOD_SECS=60
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
| `WORKER_ENABLED` | Boolean | `true` | Enable background job processing (embeddings, linking, cleanup). Alias: `JOB_WORKER_ENABLED`. |
| `JOB_WORKER_ENABLED` | Boolean | `true` | Enable/disable job processing in the worker process (takes precedence when set). |
| `WORKER_THREADS` | Integer | CPU cores | Number of Tokio worker threads for background jobs |
| `JOB_POLL_INTERVAL_MS` | Integer | `60000` | Safety-net polling interval in milliseconds. The worker is event-driven (woken by NOTIFY); this interval only triggers as a fallback for crash recovery and race conditions. |
| `JOB_MAX_CONCURRENT` | Integer | `4` | Maximum number of jobs that can run concurrently in the worker |

**Example:**
```bash
WORKER_ENABLED=true
WORKER_THREADS=4
JOB_POLL_INTERVAL_MS=60000
JOB_MAX_CONCURRENT=4
```

### Real-Time Events

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `MATRIC_EVENT_BUS_CAPACITY` | Integer | `256` | Broadcast channel capacity for the internal event bus. Increase for high-traffic deployments. |
| `SSE_REPLAY_BUFFER_SIZE` | Integer | `1024` | Number of past events retained in the SSE replay buffer for `Last-Event-ID` reconnection support. |
| `SSE_COALESCE_WINDOW_MS` | Integer | `500` | Deduplication window in milliseconds for low-priority SSE events (e.g., `job.progress`). Events with the same coalescing key are deduplicated within this window, keeping only the latest. Set to `0` to disable. |
| `MATRIC_WEBHOOK_TIMEOUT_SECS` | Integer | `10` | Timeout in seconds for outgoing webhook HTTP requests. |

**Example:**
```bash
MATRIC_EVENT_BUS_CAPACITY=512
SSE_REPLAY_BUFFER_SIZE=2048
SSE_COALESCE_WINDOW_MS=500
MATRIC_WEBHOOK_TIMEOUT_SECS=10
```

### File Storage

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FILE_STORAGE_PATH` | String | `/var/lib/matric/files` | Directory for storing uploaded file attachments on disk |

**Example:**
```bash
FILE_STORAGE_PATH=/mnt/data/matric/files
```

### Memory Management

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `MAX_MEMORIES` | Integer | `10` | Maximum number of **live** memory archives in the database |
| `DEFAULT_ARCHIVE_CACHE_TTL` | Integer | `60` | Cache TTL in seconds for the default archive lookup. Reduces database lookups for the default memory on high-traffic deployments. |
| `DISABLE_SUPPORT_MEMORY` | Boolean | `false` | Set to `true` to skip automatic loading of the built-in `fortemi-docs` support archive on first boot. |

**Example:**
```bash
# Scale with your hardware (see capacity formula below)
MAX_MEMORIES=50   # 16GB RAM, 100GB disk
MAX_MEMORIES=200  # 32GB RAM, 500GB disk
MAX_MEMORIES=500  # 64GB+ RAM, 1TB+ disk
```

**Capacity Planning:**

Each empty memory adds ~1MB schema overhead (41 tables + indexes). The real cost is data growth within each memory. Average storage per note (with 20% attachment rate):

| Component | Per Note | Per 1,000 Notes |
|-----------|----------|-----------------|
| Note metadata + content | ~11 KB | 11 MB |
| Embeddings (768-dim) | ~3 KB | 3 MB |
| Attachments (avg 500KB, 20% rate) | ~100 KB | 100 MB |
| Thumbnails (100KB, 20% rate) | ~20 KB | 20 MB |
| **Total average** | **~134 KB** | **~134 MB** |

**Capacity formula:**
```
max_total_notes = available_storage / 134 KB
MAX_MEMORIES = max_total_notes / target_notes_per_memory
```

**Recommended limits by hardware tier:**

| Tier | RAM | Storage | MAX_MEMORIES | Notes per Memory | Total Notes |
|------|-----|---------|--------------|------------------|-------------|
| Tier 1 (Minimum) | 8 GB | 10 GB | 10 | ~5,000 | ~50,000 |
| Tier 2 (Standard) | 16 GB | 100 GB | 50 | ~20,000 | ~1,000,000 |
| Tier 3 (Performance) | 32 GB | 500 GB | 200 | ~50,000 | ~10,000,000 |
| Tier 4 (Professional) | 64 GB+ | 1 TB+ | 500 | ~50,000 | ~25,000,000 |

**Memory Limits:**
- `MAX_MEMORIES` limits **live** memories (schemas in the database), not the total number you can ever create
- Export memories as shards (`POST /api/v1/shards/export`), delete them to free slots, and re-import later — there is no limit on the number of archived shards you can store on disk
- Attempting to create memories beyond `MAX_MEMORIES` returns HTTP 400
- Check current usage via `GET /api/v1/memories/overview`
- Each memory adds minimal overhead (<1MB metadata + indexes); data growth is the real constraint
- Notes without attachments are much smaller (~14 KB each) — adjust estimates for your workload

### Request Headers

| Header | Values | Description |
|--------|--------|-------------|
| `X-Fortemi-Memory` | Memory name | Routes request to specified memory (default: "default") |
| `Authorization` | Bearer token | API authentication (when `REQUIRE_AUTH=true`) |

The `X-Fortemi-Memory` header routes all API requests to a specific memory archive. Without this header, requests operate on the `default` memory. See the [Multi-Memory Guide](./multi-memory.md) for details.

### Ollama Inference

Ollama is the default inference backend for local LLM inference without API costs.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `OLLAMA_BASE` | String | `http://127.0.0.1:11434` | Ollama API endpoint URL (primary variable read by the backend) |
| `OLLAMA_URL` | String | `http://127.0.0.1:11434` | Alias for `OLLAMA_BASE` (checked as fallback by the vision handler and content summarizer) |
| `OLLAMA_HOST` | String | `http://localhost:11434` | Alias used by the Ollama discovery service |
| `OLLAMA_EMBED_MODEL` | String | `nomic-embed-text` | Model name for generating embeddings |
| `OLLAMA_GEN_MODEL` | String | `gpt-oss:20b` | Model name for text generation (standard/failover tier) |
| `OLLAMA_EMBED_DIM` | Integer | `768` | Vector dimensionality for embeddings. Must match the model's output dimension. |
| `MATRIC_EMBED_TIMEOUT_SECS` | Integer | `30` | Timeout in seconds for embedding requests to Ollama |
| `MATRIC_GEN_TIMEOUT_SECS` | Integer | `120` | Timeout in seconds for generation requests to Ollama |
| `MATRIC_OLLAMA_URL` | String | `http://127.0.0.1:11434` | Ollama URL used by the TOML-based inference config path |
| `MATRIC_OLLAMA_EMBEDDING_MODEL` | String | `nomic-embed-text` | Embedding model used by the TOML-based inference config path |
| `MATRIC_OLLAMA_GENERATION_MODEL` | String | `gpt-oss:20b` | Generation model used by the TOML-based inference config path |

**Example (Docker Desktop - macOS/Windows):**
```bash
OLLAMA_BASE=http://host.docker.internal:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=llama3.2:3b
OLLAMA_EMBED_DIM=768
```

**Example (Linux with Docker):**
```bash
OLLAMA_BASE=http://172.17.0.1:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=qwen2.5:7b
OLLAMA_EMBED_DIM=768
```

**Example (Performance Tuning):**
```bash
OLLAMA_BASE=http://localhost:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=qwen2.5:7b
MATRIC_EMBED_TIMEOUT_SECS=30
MATRIC_GEN_TIMEOUT_SECS=180
```

### OpenAI Inference

The OpenAI backend supports OpenAI's cloud API and any OpenAI-compatible endpoint (Azure OpenAI, vLLM, LocalAI, LM Studio, etc.).

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `INFERENCE_BACKEND` | String | `ollama` | Backend selection: `ollama` or `openai` |
| `OPENAI_API_KEY` | String | None | API key for OpenAI cloud (required for OpenAI cloud) |
| `OPENAI_BASE_URL` | String | `https://api.openai.com/v1` | OpenAI API base URL or compatible endpoint |
| `OPENAI_EMBED_MODEL` | String | `text-embedding-3-small` | Model name for embeddings |
| `OPENAI_GEN_MODEL` | String | `gpt-oss:20b` | Model name for text generation |
| `OPENAI_EMBED_DIM` | Integer | `1536` | Vector dimensionality for embeddings |
| `OPENAI_TIMEOUT` | Integer | `30` | Request timeout in seconds |
| `OPENAI_SKIP_TLS_VERIFY` | Boolean | `false` | Disable TLS certificate verification (insecure, for testing only) |
| `OPENAI_HTTP_REFERER` | String | None | Optional `HTTP-Referer` header sent with requests (useful for OpenRouter and compatible proxies) |
| `OPENAI_X_TITLE` | String | None | Optional `X-Title` header for identification in compatible API dashboards |
| `MATRIC_OPENAI_URL` | String | `https://api.openai.com/v1` | OpenAI URL used by the TOML-based inference config path |
| `MATRIC_OPENAI_API_KEY` | String | None | API key used by the TOML-based inference config path |
| `MATRIC_OPENAI_EMBEDDING_MODEL` | String | `text-embedding-3-small` | Embedding model used by the TOML-based inference config path |
| `MATRIC_OPENAI_GENERATION_MODEL` | String | `gpt-4o-mini` | Generation model used by the TOML-based inference config path |

**Example (OpenAI Cloud):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxx
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_EMBED_MODEL=text-embedding-3-small
OPENAI_GEN_MODEL=gpt-4o-mini
OPENAI_EMBED_DIM=1536
OPENAI_TIMEOUT=120
```

**Example (Azure OpenAI):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=your-azure-key
OPENAI_BASE_URL=https://your-resource.openai.azure.com/openai/deployments/your-deployment
OPENAI_EMBED_MODEL=text-embedding-ada-002
OPENAI_GEN_MODEL=gpt-4
```

**Example (vLLM Self-Hosted):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=token
OPENAI_BASE_URL=http://vllm-server:8000/v1
OPENAI_GEN_MODEL=meta-llama/Llama-3.1-8B-Instruct
OPENAI_TIMEOUT=180
```

**Example (LocalAI):**
```bash
INFERENCE_BACKEND=openai
OPENAI_API_KEY=localai
OPENAI_BASE_URL=http://localhost:8080/v1
OPENAI_EMBED_MODEL=text-embedding-ada-002
OPENAI_GEN_MODEL=gpt-3.5-turbo
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
| `MCP_BASE_PATH` | String | `/mcp` | URL path prefix for the MCP server (when transport=http) |
| `MATRIC_API_URL` | String | `http://localhost:3000` | API server URL for the MCP server to connect to. Alias: `FORTEMI_URL`. |
| `FORTEMI_URL` | String | `http://localhost:3000` | Alias for `MATRIC_API_URL`. Used in Docker bundle deployments. |
| `FORTEMI_API_KEY` | String | None | API key for the MCP server to authenticate with the Fortemi API (when `REQUIRE_AUTH=true`). |

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

### Extraction Pipeline

These variables control the multi-tier concept extraction cascade: GLiNER (tier 0, CPU-based NER) → fast model (tier 1) → standard model (tier 2).

#### Concept Extraction

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `GLINER_BASE_URL` | String | `http://gliner:8090` (Docker bundle) | GLiNER NER service URL for CPU-based entity extraction (tier 0). Set to empty to disable. |
| `GLINER_MODEL` | String | (set by GLiNER sidecar) | GLiNER model name, consumed by the GLiNER sidecar container (e.g., `urchade/gliner_large-v2.1`). |
| `GLINER_THRESHOLD` | Float | (set by GLiNER sidecar) | Entity confidence threshold for the GLiNER sidecar (e.g., `0.3`). |
| `EXTRACTION_TARGET_CONCEPTS` | Integer | `5` | Target number of concepts to extract per note. GLiNER→fast model escalation triggers when below this threshold; fast→standard model escalation triggers at < target/2 (i.e., 3 with the default of 5). |
| `MATRIC_FAST_GEN_MODEL` | String | `qwen3:8b` | Fast generation model (tier 1) used for concept tagging and reference extraction when GLiNER yields too few results. Large documents are automatically chunked. Set to empty to disable. |
| `MATRIC_FAST_GEN_TIMEOUT_SECS` | Integer | `60` | Timeout in seconds for fast model generation requests. |
| `OLLAMA_GEN_MODEL` | String | `gpt-oss:20b` | Standard generation model (tier 2) used as failover when the fast model also yields insufficient concepts. |

**Extraction cascade:**
```
GLiNER (tier 0, ~300ms, CPU)
  → if concepts < EXTRACTION_TARGET_CONCEPTS
    → MATRIC_FAST_GEN_MODEL (tier 1, chunked)
      → if concepts < EXTRACTION_TARGET_CONCEPTS / 2
        → OLLAMA_GEN_MODEL (tier 2, full context)
```

**Example (Docker bundle defaults):**
```bash
GLINER_BASE_URL=http://gliner:8090
EXTRACTION_TARGET_CONCEPTS=5
MATRIC_FAST_GEN_MODEL=qwen3:8b
OLLAMA_GEN_MODEL=gpt-oss:20b
```

**Example (disable GLiNER, LLM-only extraction):**
```bash
GLINER_BASE_URL=
EXTRACTION_TARGET_CONCEPTS=5
MATRIC_FAST_GEN_MODEL=qwen3:8b
OLLAMA_GEN_MODEL=gpt-oss:20b
```

**Example (higher concept density for rich taxonomies):**
```bash
EXTRACTION_TARGET_CONCEPTS=10
MATRIC_FAST_GEN_MODEL=qwen3:8b
OLLAMA_GEN_MODEL=gpt-oss:20b
```

#### Embedding Enrichment

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `EMBED_CONCEPT_MAX_DOC_FREQ` | Float | `0.8` | Maximum document frequency ratio for concepts included in embedding text enrichment. Concepts appearing in more than this fraction of notes are treated as "stopwords" and excluded. Range: 0.01–1.0. |
| `EMBED_INSTRUCTION_PREFIX` | String | `clustering: ` | Instruction prefix prepended to embedding text. `nomic-embed-text` supports `clustering: `, `search_document: `, and `classification: `. Set to empty string to disable. |

**Example:**
```bash
EMBED_CONCEPT_MAX_DOC_FREQ=0.8
EMBED_INSTRUCTION_PREFIX=clustering:
```

#### Vision (Image Description)

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `OLLAMA_VISION_MODEL` | String | `qwen3-vl:8b` | Ollama vision model for image description and 3D model rendering. Set to empty to disable image extraction. Requires Ollama with a vision-capable model pulled. |

**Example:**
```bash
OLLAMA_VISION_MODEL=qwen3-vl:8b
# OLLAMA_VISION_MODEL=llava:7b  # Alternative
# OLLAMA_VISION_MODEL=          # Disable
```

#### Audio Transcription (Whisper)

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `WHISPER_BASE_URL` | String | `http://localhost:8000` | URL for the Whisper-compatible transcription service. Set to empty to disable audio transcription. Deploy via `docker-compose.whisper.yml`. |
| `WHISPER_MODEL` | String | `Systran/faster-distil-whisper-large-v3` | Whisper model name to use for transcription. |

**Example:**
```bash
WHISPER_BASE_URL=http://host.docker.internal:8000
WHISPER_MODEL=Systran/faster-distil-whisper-large-v3
```

#### 3D Model Rendering

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `RENDERER_URL` | String | `http://localhost:8080` | URL for the Three.js renderer used for GLB/3D model keyframe extraction. The Docker bundle includes the renderer at this default address. Set to a custom URL for external renderer deployments. |

**Example:**
```bash
RENDERER_URL=http://localhost:8080
```

#### OCR and Document Processing

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `OCR_ENABLED` | Boolean | `false` | Enable OCR-based text extraction for scanned PDFs and images. Requires LibreOffice and Tesseract. |
| `LIBREOFFICE_PATH` | String | `/usr/bin/libreoffice` | Path to the LibreOffice binary for document conversion (DOCX, XLSX, PPTX to PDF). |

**Example:**
```bash
OCR_ENABLED=true
LIBREOFFICE_PATH=/usr/bin/libreoffice
```

### Graph Linking

These variables tune the knowledge graph structure. All graph variables are read at job execution time — no restart required for changes.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `GRAPH_LINKING_STRATEGY` | String | `hnsw_heuristic` | Linking strategy: `hnsw_heuristic` (HNSW Algorithm 4, diverse neighbor selection — recommended) or `threshold` (legacy epsilon-threshold). |
| `GRAPH_K_NEIGHBORS` | Integer | `0` (adaptive) | Maximum neighbors per node (M in HNSW). `0` enables adaptive mode: k = log₂(N) clamped to [5, 15]. Set explicitly (e.g., `8`) to override adaptive computation. |
| `GRAPH_MIN_SIMILARITY` | Float | `0.5` | Absolute similarity floor — no edges are created below this cosine similarity regardless of strategy. Range: 0.0–1.0. |
| `GRAPH_EXTEND_CANDIDATES` | Boolean | `false` | Extend HNSW candidate set with neighbors-of-neighbors (Algorithm 4 option). Increases recall at the cost of more comparisons. |
| `GRAPH_KEEP_PRUNED` | Boolean | `false` | Fill remaining neighbor slots from pruned candidates when the candidate set is exhausted (Algorithm 4 option). |
| `GRAPH_TAG_BOOST_WEIGHT` | Float | `0.3` | Weight for SKOS tag overlap in the blended linking score. `blended = (embedding_sim * (1 - w)) + (tag_overlap * w)`. Set to `0.0` to disable tag-based boost. Range: 0.0–1.0. |
| `GRAPH_NORMALIZATION_GAMMA` | Float | `1.0` | Gamma exponent for edge weight normalization during graph traversal. Applied as `normalized = ((score - min) / (max - min)) ^ gamma`. Values >1.0 amplify top-end differences; <1.0 compress them. Range: 0.1–5.0. |
| `GRAPH_SNN_THRESHOLD` | Float | `0.10` | Shared Nearest Neighbor pruning threshold. Edges with SNN score below this are pruned during `recompute_snn_scores`. SNN(A,B) = \|kNN(A) ∩ kNN(B)\| / k. Range: 0.0–1.0. |
| `GRAPH_COMMUNITY_RESOLUTION` | Float | `1.0` | Louvain community detection resolution parameter. Higher = more, smaller communities; lower = fewer, larger communities. Range: 0.1–10.0. |
| `GRAPH_PFNET_Q` | Integer | `2` | PFNET graph sparsification q parameter. q=2 is equivalent to the Relative Neighborhood Graph (Toussaint 1980). Higher q produces sparser graphs approaching the MST. Range: 2–10. |
| `GRAPH_STRUCTURAL_SCORE` | Float | `0.5` | Edge score assigned to structural (same-collection) edges. Controls the "gravity well" strength pulling exploration toward notes in the same collection. Range: 0.0–1.0. |

**Example (defaults — suitable for most deployments):**
```bash
GRAPH_LINKING_STRATEGY=hnsw_heuristic
GRAPH_K_NEIGHBORS=0
GRAPH_MIN_SIMILARITY=0.5
GRAPH_EXTEND_CANDIDATES=false
GRAPH_KEEP_PRUNED=false
GRAPH_TAG_BOOST_WEIGHT=0.3
GRAPH_NORMALIZATION_GAMMA=1.0
GRAPH_SNN_THRESHOLD=0.10
GRAPH_COMMUNITY_RESOLUTION=1.0
GRAPH_PFNET_Q=2
GRAPH_STRUCTURAL_SCORE=0.5
```

**Example (denser graph for tightly-related content):**
```bash
GRAPH_LINKING_STRATEGY=hnsw_heuristic
GRAPH_K_NEIGHBORS=12
GRAPH_MIN_SIMILARITY=0.6
GRAPH_TAG_BOOST_WEIGHT=0.4
GRAPH_NORMALIZATION_GAMMA=1.5
```

### OpenRouter Inference

OpenRouter provides access to 100+ LLMs via a single API. It is opt-in: the `OPENROUTER_API_KEY` variable activates the provider.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `OPENROUTER_API_KEY` | String | None | OpenRouter API key. Setting this variable enables the OpenRouter provider for generation tasks. |
| `OPENROUTER_BASE_URL` | String | `https://openrouter.ai/api/v1` | OpenRouter API base URL. |
| `OPENROUTER_TIMEOUT` | Integer | `300` | Request timeout in seconds for OpenRouter calls. |
| `OPENROUTER_HTTP_REFERER` | String | None | Optional `HTTP-Referer` header sent to OpenRouter for attribution and rate limit exemptions. |
| `OPENROUTER_X_TITLE` | String | None | Optional `X-Title` header sent to OpenRouter for display in the OpenRouter dashboard. |

**Example:**
```bash
OPENROUTER_API_KEY=sk-or-v1-xxxxxxxxxxxxx
OPENROUTER_BASE_URL=https://openrouter.ai/api/v1
OPENROUTER_TIMEOUT=300
OPENROUTER_HTTP_REFERER=https://memory.example.com
OPENROUTER_X_TITLE=Matric Memory
```

### Build Information

These variables are set automatically by the CI/CD pipeline and are read-only at runtime. They are exposed via the `/health` endpoint for build tracing.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `MATRIC_GIT_SHA` | String | `unknown` | Git commit SHA of the running build. Set by CI during image build. |
| `MATRIC_BUILD_DATE` | String | `unknown` | Build timestamp. Set by CI during image build. |

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
OLLAMA_BASE=http://localhost:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_EMBED_DIM=768
RUST_LOG=info
REQUIRE_AUTH=false
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
REQUIRE_AUTH=true
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
OLLAMA_BASE=http://ollama.internal:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=qwen2.5:7b
OLLAMA_EMBED_DIM=768

# Background worker
WORKER_ENABLED=true
WORKER_THREADS=8
JOB_POLL_INTERVAL_MS=60000

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
REQUIRE_AUTH=true
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
OLLAMA_BASE=http://ollama-cluster.internal:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_EMBED_DIM=768

OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxx
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_GEN_MODEL=gpt-4o
OPENAI_TIMEOUT=180

# Multilingual full-text search
FTS_WEBSEARCH_TO_TSQUERY=true
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_BIGRAM_CJK=false
FTS_MULTILINGUAL_CONFIGS=true

# Background worker optimization
WORKER_ENABLED=true
WORKER_THREADS=16
JOB_POLL_INTERVAL_MS=60000

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
OLLAMA_BASE=http://host.docker.internal:11434
```

**Linux:**
```bash
# Use Docker bridge network gateway IP
OLLAMA_BASE=http://172.17.0.1:11434

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

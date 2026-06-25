# Matric Memory


@AIWG.md

> **Repository**: `Fortemi/fortemi` on Gitea (primary remote: `origin`)
> **Issue Tracker**: Gitea is authoritative. Do not file repository issues on GitHub unless the user explicitly requests a GitHub mirror; close and redirect accidental GitHub issues to Gitea.
> **Gitea MCP**: `owner=Fortemi`, `repo=fortemi`
> **NOT** the `roctinam/matric` monorepo - this is the standalone matric-memory project

AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines.

## CI/CD

### Gitea Actions Workflows

Six workflows in `.gitea/workflows/`:

| Workflow | Purpose | Trigger |
|----------|---------|---------|
| `ci-builder.yaml` | Main CI pipeline (build, test, deploy) | Push to main |
| `test.yml` | Unit & integration tests with coverage | Push to main, PRs |
| `build-builder.yaml` | Build the builder Docker image | Manual/tag |
| `build-gliner.yaml` | Build the GLiNER sidecar image | Manual/tag |
| `build-pyannote.yaml` | Build the pyannote diarization sidecar image | Manual/tag |
| `publish-sidecar.yml` | Publish sidecar images to registry | Tag (`sidecar-*-v*`) |

### Monitoring Builds

```bash
# Check recent runs via MCP
# Use mcp__gitea__list_repo_action_runs with owner=fortemi, repo=fortemi

# Or via Gitea web UI
# https://github.com/fortemi/fortemi/actions
```

### CI Runner

Workflows run on `matric-builder` runner with:
- Docker for PostgreSQL + pgvector + PostGIS test containers
- Rust toolchain with coverage tools (cargo-llvm-cov)
- Caching for cargo registry and build artifacts

## Architecture

- **crates/matric-api** - Axum HTTP API server
- **crates/matric-core** - Core types, traits, models
- **crates/matric-db** - PostgreSQL repositories (sqlx)
- **crates/matric-search** - Hybrid search (FTS + semantic + RRF)
- **crates/matric-inference** - Multi-provider inference (Ollama, OpenAI, OpenRouter, llama.cpp)
- **crates/matric-crypto** - PKE encryption for secure note sharing
- **crates/matric-jobs** - Background job worker
- **mcp-server/** - MCP (Model Context Protocol) server in Node.js

## Development

### Git Hooks

Pre-commit hooks ensure code quality by running formatting and lint checks before commits.

**First-time setup:**
```bash
./scripts/install-hooks.sh
```

The pre-commit hook automatically runs:
1. `cargo fmt --check` - Verify code formatting
2. `cargo clippy -- -D warnings` - Check for lint issues

If checks fail, fix issues with:
```bash
cargo fmt --all                    # Fix formatting
cargo clippy --fix --all-targets   # Auto-fix clippy issues
```

To bypass hooks (not recommended): `git commit --no-verify`

See `scripts/README.md` for more details.

## Authentication

Authentication is **fail-closed by default** (ADR-094, fixes Gitea
fortemi/fortemi#709). The API requires a valid Bearer token on all
`/api/v1/*` endpoints unless explicitly opted out via a paired acknowledgment.

### Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `REQUIRE_AUTH` | `true` | Require a valid Bearer token on all `/api/v1/*` endpoints |
| `I_UNDERSTAND_NO_AUTH` | `false` | Required acknowledgment to run with `REQUIRE_AUTH=false`. Startup refuses if `REQUIRE_AUTH=false` is set without this flag |
| `FORTEMI_MULTI_TENANT` | `false` | Multi-tenant deployments must run with auth. Setting both `FORTEMI_MULTI_TENANT=true` and `REQUIRE_AUTH=false` is a startup error |
| `ISSUER_URL` | `https://localhost:3000` | OAuth2 issuer URL (REQUIRED for OAuth/MCP) |
| `MAX_MEMORIES` | `10` | Maximum memory archives. Scale with hardware: 10 (8GB), 50 (16GB), 200 (32GB), 500 (64GB+) |

**Default (`REQUIRE_AUTH=true`):** every `/api/v1/*` endpoint requires a valid
Bearer token. Safe for any deployment reachable beyond localhost.

**Anonymous mode (`REQUIRE_AUTH=false` + `I_UNDERSTAND_NO_AUTH=true`):**
suitable only for single-user desktop sidecar and local development. The
startup emits a loud warning at boot and every 60 seconds for the process
lifetime, so the no-auth posture cannot hide in long-running logs. Multi-tenant
builds refuse anonymous regardless of the acknowledgment.

Public endpoints (always accessible regardless of `REQUIRE_AUTH`):
- `/health`, `/api/v1/health/*`
- `/oauth/*`, `/.well-known/*`
- `/docs` (Swagger UI), `/openapi.yaml`

### Authentication Methods

1. **OAuth2 Access Token**: Obtain via `/oauth/token` (client_credentials or authorization_code grant)
2. **API Key**: Create via `POST /api/v1/api-keys`, use as Bearer token

### Example Usage

```bash
# Register an OAuth client
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"My App","grant_types":["client_credentials"],"scope":"read write"}'

# Get an access token
curl -X POST https://your-domain.com/oauth/token \
  -u "CLIENT_ID:CLIENT_SECRET" \
  -d "grant_type=client_credentials&scope=read write"

# Use the token
curl https://your-domain.com/api/v1/notes \
  -H "Authorization: Bearer mm_at_xxxx"
```

### Migrating from Anonymous to Authenticated

Pre-existing deployments running anonymous (which was the historical default before
ADR-094) follow this sequence:

1. Confirm anonymous is currently acknowledged: ensure `REQUIRE_AUTH=false` is paired
   with `I_UNDERSTAND_NO_AUTH=true` in `.env`. Without the acknowledgment the next
   restart will refuse to start.
2. Register OAuth clients and create API keys for existing integrations:
   `POST /api/v1/api-keys`.
3. Update all clients to include `Authorization: Bearer <token>` headers.
4. Flip to authenticated: remove `REQUIRE_AUTH=false` and `I_UNDERSTAND_NO_AUTH=true`
   from `.env` (or set `REQUIRE_AUTH=true`), then restart:
   `docker compose -f docker-compose.bundle.yml up -d`.

## Inference Providers

Multi-provider inference with hot-swappable runtime configuration. The default provider is Ollama (always available). External providers are opt-in via environment variables.

### Provider-Qualified Model Slugs

Use provider prefixes to route requests to specific backends:

```
qwen3:8b                → default provider (Ollama)
ollama:qwen3:8b         → explicit Ollama
openai:gpt-4o           → OpenAI
openrouter:anthropic/claude-sonnet-4-20250514 → OpenRouter
llamacpp:my-model       → llama.cpp
```

### Ollama (default)

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_BASE` | `http://localhost:11434` | Ollama API URL (precedence: `MATRIC_OLLAMA_URL` → `OLLAMA_BASE` → `OLLAMA_URL` → `OLLAMA_HOST`) |
| `OLLAMA_GEN_MODEL` | `qwen3.5:9b` | Generation + vision model |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model |

### llama.cpp

Uses the OpenAI-compatible API protocol (`/v1/chat/completions`, `/v1/embeddings`). Opt-in via `LLAMACPP_BASE_URL`.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LLAMACPP_BASE_URL` | Yes (opt-in) | *(none)* | Base URL (e.g., `http://127.0.0.1:8080`) |
| `LLAMACPP_API_KEY` | No | *(none)* | API key if llama-server started with `--api-key` |
| `LLAMACPP_TIMEOUT` | No | `300` | Request timeout in seconds |

**Docker bundle networking**: When llama-server runs on the host alongside the Docker bundle, use `http://host.docker.internal:8080` (macOS/Windows) or `http://172.17.0.1:8080` (Linux default bridge) from inside containers.

### OpenAI

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OPENAI_API_KEY` | Yes (opt-in) | *(none)* | API key |
| `OPENAI_BASE_URL` | No | `https://api.openai.com/v1` | Override for self-hosted OpenAI-compatible servers (vLLM, LiteLLM, etc.) |
| `OPENAI_GEN_MODEL` | No | `gpt-4o-mini` | Generation model |
| `OPENAI_EMBED_MODEL` | No | `text-embedding-3-small` | Embedding model |

### OpenRouter

OpenAI-compatible protocol with mandatory routing/attribution headers. **No embedding support** — pair with `MATRIC_EMBEDDING_PROVIDER` to embed via Ollama or llama.cpp.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OPENROUTER_API_KEY` | Yes (opt-in) | *(none)* | API key |
| `OPENROUTER_BASE_URL` | No | `https://openrouter.ai/api/v1` | Endpoint override |
| `OPENROUTER_GEN_MODEL` | No | `anthropic/claude-sonnet-4` | Generation model |
| `OPENROUTER_HTTP_REFERER` | No | `https://fortemi.io` | Routing rules + analytics |
| `OPENROUTER_APP_NAME` | No | `Fortemi` | App attribution (`X-Title` header) |

### Independent Embedding/Generation Routing

Set `MATRIC_EMBEDDING_PROVIDER` to a different provider id than `MATRIC_INFERENCE_DEFAULT` to route embedding calls separately from chat. The override is validated against the catalog: it must point at a registered provider with the Embedding capability.

```bash
MATRIC_INFERENCE_DEFAULT=openrouter
MATRIC_EMBEDDING_PROVIDER=ollama
```

### Runtime Configuration (hot-swap)

All providers can be reconfigured at runtime without restarting via `POST /api/v1/inference/config`:

```bash
# View current config (with source attribution: default/env/db_override)
curl http://localhost:3000/api/v1/inference/config

# Discover available providers (catalog-driven; reports server_configured + supports_embeddings)
curl http://localhost:3000/api/v1/inference/providers

# Configure llama.cpp at runtime
curl -X POST http://localhost:3000/api/v1/inference/config \
  -H "Content-Type: application/json" \
  -d '{"llamacpp": {"base_url": "http://127.0.0.1:8080"}}'

# Configure OpenRouter (OpenAI-compatible + routing headers)
curl -X POST http://localhost:3000/api/v1/inference/config \
  -H "Content-Type: application/json" \
  -d '{"openrouter": {"api_key": "sk-or-v1-...", "generation_model": "anthropic/claude-3.5-sonnet"}}'

# Independent embedding provider (route embeddings to Ollama)
curl -X POST http://localhost:3000/api/v1/inference/config \
  -H "Content-Type: application/json" \
  -d '{"embedding_backend": "ollama"}'

# Clear the embedding override (revert to default for embeddings too)
curl -X POST http://localhost:3000/api/v1/inference/config \
  -H "Content-Type: application/json" \
  -d '{"embedding_backend": null}'

# Dry-run: validate without persisting
curl -X POST 'http://localhost:3000/api/v1/inference/config?dry_run=true' \
  -H "Content-Type: application/json" \
  -d '{"openai": {"api_key": "sk-..."}}'

# Atomic swap: probe every changed backend; abort with 503 on any failure
curl -X POST 'http://localhost:3000/api/v1/inference/config?atomic=true' \
  -H "Content-Type: application/json" \
  -d '{"openrouter": {"api_key": "sk-or-v1-..."}}'

# Test connection to a backend
curl -X POST http://localhost:3000/api/v1/inference/test-connection \
  -H "Content-Type: application/json" \
  -d '{"base_url": "http://127.0.0.1:8080", "provider": "auto"}'

# Reset all overrides back to env/defaults
curl -X DELETE http://localhost:3000/api/v1/inference/config
```

Configuration precedence: `db_override` → `env` → `default`.

## Deployment

### Docker Bundle

All-in-one Docker bundle includes PostgreSQL, Redis, API, MCP server, and Open3D renderer in a single deployment.

#### Hardware Profiles

Select a profile based on your GPU VRAM. Default (no profile) targets edge hardware:

| Profile | GPU VRAM | Audio/Diarization | Gen Model | Example GPUs |
|---------|----------|-------------------|-----------|--------------|
| *(default)* | 6-8GB | CPU | qwen3.5:9b | RTX 3060 8GB, 4060, 5060 |
| `gpu-12gb` | 12-16GB | GPU | qwen3.5:9b | RTX 3060 12GB, 4070, 5070 |
| `gpu-24gb` | 24GB+ | GPU | configurable | RTX 3090, 4090, 5090 |

Set `COMPOSE_PROFILES` in `.env` (or pass `--profile` on command line):

```bash
# Edge (default) — CPU sidecars, works on any GPU 6GB+
COMPOSE_PROFILES=edge   # in .env

# Mid-range — GPU-accelerated Whisper + pyannote
COMPOSE_PROFILES=gpu-12gb

# High-end — GPU sidecars + larger models
COMPOSE_PROFILES=gpu-24gb
# Also set: OLLAMA_GEN_MODEL=qwen3.5:27b
```

#### Environment Configuration

Create `.env` file with required settings for OAuth/MCP:

```bash
# .env
ISSUER_URL=https://your-domain.com

# Generation + vision model (qwen3.5:9b is natively multimodal — single VRAM load)
# Override for 24GB+ GPUs: OLLAMA_GEN_MODEL=qwen3.5:27b
OLLAMA_GEN_MODEL=qwen3.5:9b

# llama.cpp provider (optional — enables llamacpp:model-name slugs)
# LLAMACPP_BASE_URL=http://localhost:8080
# LLAMACPP_API_KEY=          # omit if not required
```

**Guided installer:** `installer/scripts/` provides 8 shell scripts for step-by-step deployment: `clone.sh`, `configure.sh`, `deploy.sh`, `pull-models.sh`, `check-ports.sh`, `setup-nvidia.sh`, `verify.sh`, `reset.sh`. A `setup.manifest.yaml` machine-readable manifest is available for the AIWG installer framework.

**MCP OAuth credentials** are auto-managed: the bundle entrypoint auto-registers an MCP OAuth client on startup if credentials are missing or invalid. Credentials persist at `$PGDATA/.fortemi-mcp-credentials` and survive container restarts. Manual registration is only needed for standalone (non-Docker) deployments:
```bash
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read write"}'
```

#### Commands

```bash
# Start (or restart with existing data)
docker compose -f docker-compose.bundle.yml up -d

# Clean install (wipe database and start fresh)
docker compose -f docker-compose.bundle.yml down -v
docker compose -f docker-compose.bundle.yml up -d

# Rebuild after code changes
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml up -d

# View logs
docker compose -f docker-compose.bundle.yml logs -f

# Check health
curl http://localhost:3000/health

# Restart (after .env changes)
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

The bundle automatically:
- Initializes PostgreSQL on first run
- Runs all migrations
- Auto-registers MCP OAuth credentials (persisted across restarts)
- Starts Redis (required dependency)
- Starts the API on port 3000
- Starts the MCP server on port 3001
- Starts the Open3D renderer on port 8080 (3D model processing)
- Seeds the support archive in background (unless `DISABLE_SUPPORT_MEMORY` is set)

#### Nginx Reverse Proxy

Configure nginx to proxy to the container:
- `https://your-domain.com` → `localhost:3000` (API)
- `https://your-domain.com/mcp` → `localhost:3001` (MCP)

## Database

- PostgreSQL 18 with pgvector (vector similarity) and PostGIS (spatial queries) extensions
- Connection: `postgres://matric:matric@localhost/matric`
- Migrations: `migrations/` directory
- Extensions must be created as superuser before migrations run (handled by entrypoint/CI)

## MCP Server

The MCP server provides Claude/AI integration. In Docker bundle deployment, it runs automatically on port 3001.

**Tool modes:** Default is "core" (43 agent-friendly tools). Core tools: `capture_knowledge`, `search`, `record_provenance`, `manage_tags`, `manage_collection`, `manage_concepts`, `manage_embeddings`, `manage_archives`, `manage_encryption`, `manage_backups`, `manage_jobs`, `manage_inference`, `manage_attachments`, `trigger_graph_maintenance`, `coarse_community_detection`, `explore_graph`, `get_topology_stats`, `get_graph_diagnostics`, `pfnet_sparsify`, `recompute_snn_scores`, `get_knowledge_health`, `get_related_notes`, `select_memory`, `get_active_memory`, `bulk_reprocess_notes`, `get_cold_spots`, `get_access_frequency`, `purge_note`, `purge_notes`, `purge_all_notes`, `list_notes`, `get_note`, `update_note`, `delete_note`, `restore_note`, `export_note`, `get_note_links`, `capture_diagnostics_snapshot`, `list_diagnostics_snapshots`, `compare_diagnostics_snapshots`, `get_documentation`, `get_system_info`, `health_check`. Set `MCP_TOOL_MODE=full` for all 205 granular tools.

For Claude Code integration, configure `.mcp.json`:
```json
{
  "mcpServers": {
    "matric-memory": {
      "url": "https://your-domain.com/mcp"
    }
  }
}
```

## Testing

```bash
cargo test              # Unit tests
cargo test --workspace  # All crates
```

### Testing Standards

**NEVER use `#[ignore]` to skip failing tests.** Fix tests properly instead:
- CI has full test infrastructure (dedicated PostgreSQL containers, migrations, etc.)
- If tests need serial execution, configure CI to run them with `--test-threads=1`
- If tests need isolation, use unique identifiers (timestamp prefixes, UUIDs)

**NEVER skip database tests in CI/CD.** A test PostgreSQL instance is always available during CI builds. Do not gate tests behind `SKIP_INTEGRATION_TESTS` or similar env vars. If a test needs a database, it gets a database — fix isolation issues instead of skipping.

### PostgreSQL Migration Compatibility

`#[sqlx::test]` runs migrations in a transaction. Some PostgreSQL operations **cannot run in transactions**:
- `CREATE INDEX CONCURRENTLY`
- `ALTER TYPE ... ADD VALUE` (enum values)

**Solution:** Use `#[tokio::test]` with manual pool setup:
```rust
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url).await.expect("Failed to create pool")
}

#[tokio::test]
async fn test_something() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);
    // Test logic...
}
```

### Test Isolation Strategies

For tests sharing database state without transactional rollback:

1. **Unique identifiers**: Use UUIDs (`uuid::Uuid::new_v4()`) — timestamp millis can collide in parallel tests
2. **Track created resources**: Store IDs and verify only those records
3. **Serial execution**: Run with `--test-threads=1` (configured in CI for worker tests)

See `docs/testing-guide.md` for comprehensive testing documentation.

## Key Features

- Hybrid search (FTS + semantic + RRF fusion)
- **Multilingual FTS** (English, German, French, Spanish, Portuguese, Russian, CJK)
- **Search operators** (OR, NOT, phrase search via `websearch_to_tsquery`)
- **Emoji search** via pg_trgm trigram indexes
- **CJK support** via pg_bigm bigram indexes (graceful fallback to pg_trgm)
- **Script detection** for automatic search strategy routing
- Strict tag filtering for guaranteed data isolation
- W3C SKOS semantic tagging system
- AI revision with context from related notes
- Automatic semantic linking (>70% similarity)
- Collections/folders with hierarchy
- Note templates with variable substitution
- Graph exploration with recursive CTE
- **Graph quality maintenance pipeline** (`normalize → SNN → PFNET → diagnostics snapshot` via `POST /api/v1/graph/maintenance`)
- **Louvain community detection** with SKOS-derived labels
- **SNN similarity scoring** and PFNET sparsification for topology-preserving graph analysis
- Temporal-spatial memory search (PostGIS location + time range queries)
- PKE encryption for secure note sharing
- Export to markdown with YAML frontmatter
- Real-time event streaming (SSE + WebSocket + webhooks)
- Knowledge health dashboard (orphan tags, stale notes, unlinked notes)
- **Document Type Registry** with 131 pre-configured types
- **Smart chunking** per document type (code uses syntactic, prose uses semantic)
- **Auto-detection** from filename patterns and magic content
- **Vision (image description)** via Ollama vision LLM (qwen3.5:9b natively multimodal, llava)
- **Audio transcription** via Whisper-compatible backend (attachment pipeline + ad-hoc API)
- **Speaker diarization** via pyannote sidecar (speaker-labeled captions, editable speaker names)
- **Video multimodal extraction** via attachment pipeline (keyframe extraction, scene detection, transcription alignment)
- **3D model understanding** via attachment pipeline (Open3D multi-view rendering + vision description)
- **Media optimization** via ffmpeg (faststart, web-compatible remux, audio extraction, 720p preview)
- **Email extraction** (RFC 2822/MIME parsing, embedded attachment extraction)
- **Spreadsheet extraction** (xlsx/xls/ods → markdown tables per sheet)
- **Archive extraction** (ZIP/tar/gz → file listing + text content extraction)
- **Derived attachments** (thumbnails, transcripts, caption files, media variants as child attachments)
- **TUS resumable uploads** — tus v1.0.0 protocol for reliable large-file uploads with Creation, Termination, and Checksum extensions
- **Thumbnail sprite sheets** — CSS sprite grids with WebVTT maps for video seek-bar previews
- **HTTP Range requests** for partial content download of large attachments
- **Synchronous chat** via `POST /api/v1/chat` with GPU concurrency gating, multi-turn history, model selection, and model metadata

### Multi-Memory Architecture

Fortemi supports parallel memory archives for data isolation. Each memory operates as a separate PostgreSQL schema.

**Key concepts:**
- `X-Fortemi-Memory` HTTP header selects the target memory per request
- Default memory maps to `public` schema (backward compatible - no header needed)
- All 91 API handlers route through `SchemaContext` with `SET LOCAL search_path`
- 14 shared tables (auth, jobs, config) + 41 per-memory tables (notes, tags, embeddings, etc.)
- Archives created via `POST /api/v1/archives` with automatic schema cloning
- Auto-migration ensures existing archives gain new tables on access

**Current limitations:**
- No cross-archive note linking
- Per-memory search works via per-schema connection pools; use federated search (`POST /api/v1/search/federated`) to search across multiple archives simultaneously

**For agents:** See `docs/content/multi-memory-agent-guide.md` for segmentation strategies and decision framework.

### Embedding Sets

- **Filter Sets** (default): Share embeddings from the default embedding set
- **Full Sets**: Maintain independent embeddings with dedicated configuration
- **MRL Support**: Matryoshka Representation Learning for 12× storage savings
- **Auto-embed Rules**: Automatic embedding lifecycle management
- **Two-stage Retrieval**: Coarse-to-fine search for 128× compute reduction

See `docs/content/embedding-model-selection.md` for model selection guidance.

## Search Capabilities

### Query Syntax
```
hello world        # Match all words (AND)
apple OR orange    # Match either word
apple -orange      # Exclude word
"hello world"      # Match exact phrase
```

### Multilingual Support
- **Latin scripts**: Full stemming (English, German, French, Spanish, Portuguese, Russian)
- **CJK (Chinese, Japanese, Korean)**: Bigram/trigram character matching
- **Emoji & symbols**: Trigram substring matching
- **Arabic, Cyrillic, Greek, Hebrew**: Basic tokenization

### Feature Flags
```bash
export FTS_SCRIPT_DETECTION=true      # Auto-detect query language
export FTS_TRIGRAM_FALLBACK=true      # Emoji/symbol search
export FTS_BIGRAM_CJK=true            # Optimized CJK search
export FTS_MULTILINGUAL_CONFIGS=true  # Language-specific stemming
export FTS_WEBSEARCH_TO_TSQUERY=true  # websearch_to_tsquery() search operators (default: true)
```

## Releasing

### Version Format

matric-memory uses **CalVer**: `YYYY.M.PATCH` (e.g., `2026.1.0`)

- No leading zeros (semver rejects them)
- PATCH resets each month
- Git tags use `v` prefix: `v2026.1.0`

### Quick Release Checklist

1. **Pre-release checks**
   ```bash
   cargo test --workspace
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

2. **Update versions**
   - `Cargo.toml` - workspace version
   - `CHANGELOG.md` - add release section
   - `mcp-server/package.json` - if applicable

3. **Commit and tag**
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: release vYYYY.M.PATCH"
   git tag -a vYYYY.M.PATCH -m "vYYYY.M.PATCH - Release title"
   git push origin main --tags
   ```

4. **Create Gitea release** with highlights from CHANGELOG.md

See `docs/content/releasing.md` for full details.

<!-- AIWG:claude-md-hook:start -->

# AIWG

@AIWG.md

<!--
  This block is managed by `aiwg regenerate` and `aiwg use`.
  Operator content above and below this block is preserved on regenerate.
  To change AIWG.md content, edit .aiwg/AIWG.md (the normalized source)
  then run `aiwg regenerate`.
-->

<!-- AIWG:claude-md-hook:end -->

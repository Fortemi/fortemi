# Matric Memory

> **Repository**: `fortemi/fortemi` on GitHub
> **Gitea MCP**: `owner=fortemi`, `repo=fortemi`
> **NOT** the `roctinam/matric` monorepo - this is the standalone matric-memory project

AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines.

## CI/CD

### Gitea Actions Workflows

Three workflows in `.gitea/workflows/`:

| Workflow | Purpose | Trigger |
|----------|---------|---------|
| `ci-builder.yaml` | Main CI pipeline (build, test, deploy) | Push to main |
| `test.yml` | Unit & integration tests with coverage | Push to main, PRs |
| `build-builder.yaml` | Build the builder Docker image | Manual/tag |

### Monitoring Builds

```bash
# Check recent runs via MCP
# Use mcp__gitea__list_repo_action_runs with owner=roctinam, repo=matric-memory

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
- **crates/matric-inference** - Ollama embedding/generation
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

The API supports opt-in authentication via the `REQUIRE_AUTH` environment variable.

### Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `REQUIRE_AUTH` | `false` | Set to `true` to require auth on all `/api/v1/*` endpoints |
| `ISSUER_URL` | `https://localhost:3000` | OAuth2 issuer URL (REQUIRED for OAuth/MCP) |
| `MAX_MEMORIES` | `10` | Maximum memory archives. Scale with hardware: 10 (8GB), 50 (16GB), 200 (32GB), 500 (64GB+) |

When `REQUIRE_AUTH=false` (default), all endpoints are publicly accessible.
When `REQUIRE_AUTH=true`, all `/api/v1/*` endpoints require a valid Bearer token.

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

### Enabling Auth on Existing Deployments

1. Deploy with `REQUIRE_AUTH=false` (default) and register OAuth clients
2. Create API keys for existing integrations: `POST /api/v1/api-keys`
3. Update all clients to include `Authorization: Bearer <token>` headers
4. Set `REQUIRE_AUTH=true` in `.env` and restart: `docker compose -f docker-compose.bundle.yml up -d`

## Deployment

### Docker Bundle

All-in-one Docker bundle includes PostgreSQL, API, and MCP server in a single container.

#### Environment Configuration

Create `.env` file with required settings for OAuth/MCP:

```bash
# .env
ISSUER_URL=https://your-domain.com
MCP_CLIENT_ID=mm_xxxxx      # Register via POST /oauth/register
MCP_CLIENT_SECRET=xxxxx

# Vision model for image description (enabled by default, set to empty to disable)
OLLAMA_VISION_MODEL=qwen3-vl:8b

# Audio transcription backend (enabled by default, set to empty to disable)
WHISPER_BASE_URL=http://localhost:8000
WHISPER_MODEL=Systran/faster-distil-whisper-large-v3
```

**First-time MCP setup:** Register an OAuth client for token introspection:
```bash
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'
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
- Starts the API on port 3000
- Starts the MCP server on port 3001

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

**Tool modes:** Default is "core" (23 agent-friendly tools with discriminated-union pattern: `capture_knowledge`, `search`, `record_provenance`, `manage_tags`, `manage_collection`, `manage_concepts`). Set `MCP_TOOL_MODE=full` for all 187 granular tools.

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

1. **Unique identifiers**: `format!("test-{}", chrono::Utc::now().timestamp_millis())`
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
- Temporal-spatial memory search (PostGIS location + time range queries)
- PKE encryption for secure note sharing
- Export to markdown with YAML frontmatter
- Real-time event streaming (SSE + WebSocket + webhooks)
- Knowledge health dashboard (orphan tags, stale notes, unlinked notes)
- **Document Type Registry** with 131 pre-configured types
- **Smart chunking** per document type (code uses syntactic, prose uses semantic)
- **Auto-detection** from filename patterns and magic content
- **Vision (image description)** via Ollama vision LLM (qwen3-vl, llava)
- **Audio transcription** via Whisper-compatible backend (attachment pipeline + ad-hoc API)
- **Video multimodal extraction** via attachment pipeline (keyframe extraction, scene detection, transcription alignment)
- **3D model understanding** via attachment pipeline (Three.js multi-view rendering + vision description)

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

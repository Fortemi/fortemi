# Matric Memory

AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines.

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

## Deployment

### Docker Bundle

All-in-one Docker bundle includes PostgreSQL, API, and MCP server in a single container.

#### Environment Configuration

Create `.env` file with required settings for OAuth/MCP:

```bash
# .env
ISSUER_URL=https://memory.integrolabs.net
MCP_CLIENT_ID=mm_xxxxx      # Register via POST /oauth/register
MCP_CLIENT_SECRET=xxxxx
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

- PostgreSQL 16 with pgvector extension
- Connection: `postgres://matric:matric@localhost/matric`
- Migrations: `migrations/` directory

## MCP Server

The MCP server provides Claude/AI integration. In Docker bundle deployment, it runs automatically on port 3001.

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

## Key Features

- Hybrid search (FTS + semantic + RRF fusion)
- Strict tag filtering for guaranteed data isolation
- W3C SKOS semantic tagging system
- AI revision with context from related notes
- Automatic semantic linking (>70% similarity)
- Collections/folders with hierarchy
- Note templates with variable substitution
- Graph exploration with recursive CTE
- PKE encryption for secure note sharing
- Export to markdown with YAML frontmatter

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

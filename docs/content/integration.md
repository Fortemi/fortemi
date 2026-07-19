# Integration Guide

This guide covers integrating Fortémi into your application.

## Prerequisites

- PostgreSQL 18 with pgvector and PostGIS extensions
- Rust 1.70+ (if building from source)
- Ollama (optional, for local inference) OR OpenAI API key (for cloud inference)

## Installation

### Option 1: As HTTP API

Use the Fortémi API server directly:

```bash
# Clone repository
git clone https://github.com/fortemi/fortemi

# Build
cargo build --release -p matric-api

# Run
DATABASE_URL="<DATABASE_URL>" ./target/release/matric-api
```

### Option 2: As Rust Crate (Future)

```toml
# Cargo.toml
[dependencies]
Fortémi = { git = "https://github.com/fortemi/fortemi" }
```

## Database Setup

### 1. Create Database

```sql
CREATE DATABASE matric;
\c matric
CREATE EXTENSION IF NOT EXISTS vector;
```

### 2. Run Migrations

```bash
cd Fortémi
DATABASE_URL="<DATABASE_URL>" sqlx migrate run
```

### 3. Verify Setup

```bash
curl http://localhost:3000/health
# {"status":"healthy","version":"2026.2.13"}
```

## API Integration

Fortemi provides three integration paths:

| Path | Best For | Documentation |
|------|----------|---------------|
| **REST API** | Direct HTTP integration | [Curated API reference](#/developers-api) |
| **MCP Server** | AI agent integration (Claude, etc.) | [MCP Guide](#/developers-mcp) |
| **Rust Crate** (future) | Embedded Rust integration | Coming soon |

### Authentication

The API supports two authentication methods:

- **API Keys**: Simple token-based auth for server-to-server integration
- **OAuth 2.0 with PKCE**: For user-facing applications and MCP clients

See [Authentication Guide](#/security-authentication) for setup instructions.

### Quick Example

```bash
# Create a note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content": "# My Note\n\nThis is a note.", "tags": ["example"]}'

# Search
curl "http://localhost:3000/api/v1/search?q=my+note"
```

For supported consumer endpoints, request/response schemas, and examples, see
the [curated API reference](#/developers-api). Administrators can fetch the
full generated schema from `/api/v1/operator/openapi.yaml` with an
admin-scoped bearer token.

### Strict Tag Filtering

Use strict filters for guaranteed data isolation (e.g., multi-tenancy, client segregation):

```bash
# Search within a specific client's notes only
curl -X POST "http://localhost:3000/api/v1/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "quarterly report",
    "strict_filter": {
      "required_schemes": ["client-acme"]
    }
  }'
```

**Filter Types:**
- `required_tags`: Notes MUST have ALL these (AND logic)
- `any_tags`: Notes MUST have AT LEAST ONE (OR logic)
- `excluded_tags`: Notes MUST NOT have ANY (exclusion)
- `required_schemes`: Notes ONLY from these SKOS vocabulary schemes
- `excluded_schemes`: Notes NOT from these schemes

See [Strict Tag Filtering](#/core-systems-tags) for details.

## HotM Migration

If migrating from HotM's embedded backend:

### 1. Export Existing Data

```bash
# From HotM database
pg_dump hotm > hotm_backup.sql
```

### 2. Update Configuration

```typescript
// HotM frontend config
const API_URL = "http://localhost:3000";
```

### 3. Update API Calls

```typescript
// Before (HotM backend)
const notes = await fetch('/api/notes').then(r => r.json());

// After (Fortémi)
const notes = await fetch(`${API_URL}/api/v1/notes`).then(r => r.json());
```

### 4. Schema Mapping

| HotM Field | Fortémi Field |
|------------|---------------------|
| id | id |
| content | note.original.content |
| revised | note.revised.content |
| embedding | embedding.embedding |
| created_at | note.created_at_utc |
| tags | tags[] |

## MCP Server Integration

For AI agent integration (Claude Desktop, Claude Code, etc.):

### 1. Install Dependencies

```bash
cd mcp-server
npm install
```

### 2. Configure Claude Desktop

Add to `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["/path/to/Fortémi/mcp-server/index.js"],
      "env": {
        "FORTEMI_URL": "http://localhost:3000"
      }
    }
  }
}
```

### 3. Available Tools

The MCP server provides tools for:

- **Note Management**: Create, read, update, delete, search
- **Backup & Export**: JSON export, knowledge shards, database backups
- **Encryption (PKE)**: X25519 keypair generation, encrypt/decrypt for multiple recipients
- **Tag Management**: List, filter, and update note tags
- **Job Queue**: Queue and monitor background jobs

See [MCP Tools Reference](#/developers-mcp) for the complete list of available tools including note management, backup, encryption, and search capabilities.

For encryption details, see the [Encryption Guide](#/security-encryption).

## See Also

- [Getting Started](./getting-started.md) - First-time setup walkthrough
- [Configuration Reference](#/operations-configuration) - All environment variables and settings
- [Authentication Guide](#/security-authentication) - API keys and OAuth setup
- [Troubleshooting](#/operations-troubleshooting) - Common issues and fixes
- [Best Practices](#/resources-best-practices) - Performance tuning and optimization
- [MCP Guide](#/developers-mcp) - Model Context Protocol server integration
- [Tags Guide](#/core-systems-tags) - Tag management and strict filtering

## Support

- **Issues**: https://github.com/fortemi/fortemi/issues
- **Consumer API Reference**: https://docs.fortemi.com/server/#/developers-api
- **Operator OpenAPI Spec**: http://localhost:3000/api/v1/operator/openapi.yaml (admin bearer required)

# Fortémi MCP Server

MCP (Model Context Protocol) server that exposes the Fortémi API as tools for AI agents.

## Installation

```bash
cd mcp-server
npm install
```

## Transport Modes

The server supports two transport modes:

### Stdio Transport (Default)

For local CLI integration with Claude Desktop or Claude Code. No authentication required - uses API key.

### HTTP Transport

For remote access with OAuth2 authentication. Supports both:
- **StreamableHTTP** - Modern transport using `POST/GET/DELETE /` with `MCP-Session-Id` header
- **SSE** - Legacy transport using `GET /sse` + `POST /messages?sessionId=X`

## Usage

### With Claude Desktop (Stdio)

Add to your Claude Desktop config (`~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["/path/to/fortemi/mcp-server/index.js"],
      "env": {
        "FORTEMI_URL": "https://fortemi.com",
        "FORTEMI_API_KEY": "your-api-key"
      }
    }
  }
}
```

### With Claude Code (Stdio)

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["./mcp-server/index.js"]
    }
  }
}
```

### HTTP Mode (Remote Access)

Start the server in HTTP mode:

```bash
MCP_TRANSPORT=http MCP_PORT=3001 node index.js
```

The server exposes:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | POST | StreamableHTTP: Initialize session or send messages |
| `/` | GET | StreamableHTTP: Receive server messages (SSE stream) |
| `/` | DELETE | StreamableHTTP: Terminate session |
| `/sse` | GET | SSE: Open SSE connection |
| `/messages` | POST | SSE: Send messages to session |
| `/health` | GET | Health check with session counts |
| `/.well-known/oauth-authorization-server` | GET | OAuth2 authorization server metadata |
| `/.well-known/oauth-protected-resource` | GET | OAuth2 protected resource metadata (RFC 9728) |

## Tool Surface Modes

The MCP server provides two tool surface modes via `MCP_TOOL_MODE`:

### Core Mode (Default) — 37 Tools

Agent-optimized surface using consolidated discriminated-union tools. Reduces token overhead by ~82% compared to full mode.

**Core tools:**

| Category | Tools | Count |
|----------|-------|-------|
| **Notes CRUD** | `list_notes`, `get_note`, `update_note`, `delete_note`, `restore_note` | 5 |
| **Consolidated** | `capture_knowledge`, `search`, `record_provenance`, `manage_tags`, `manage_collection`, `manage_concepts`, `manage_embeddings`, `manage_archives`, `manage_encryption`, `manage_backups` | 10 |
| **Graph** | `explore_graph`, `get_topology_stats`, `get_graph_diagnostics`, `capture_diagnostics_snapshot`, `list_diagnostics_snapshots`, `compare_diagnostics_snapshots`, `recompute_snn_scores`, `pfnet_sparsify`, `coarse_community_detection`, `trigger_graph_maintenance`, `get_note_links` | 11 |
| **Export** | `export_note` | 1 |
| **System** | `get_documentation`, `get_system_info`, `health_check` | 3 |
| **Multi-memory** | `select_memory`, `get_active_memory` | 2 |
| **Attachments** | `manage_attachments` | 1 |
| **Observability** | `get_knowledge_health` | 1 |
| **Jobs & inference** | `manage_jobs`, `manage_inference` | 2 |
| **Bulk ops** | `bulk_reprocess_notes` | 1 |

**Total:** 37 tools

### Full Mode — 202 Tools

All granular API operations exposed as individual tools. For backward compatibility and specialized use cases requiring fine-grained control.

Enable with:
```bash
MCP_TOOL_MODE=full node index.js
```

### Consolidated Tools Pattern

Core mode uses discriminated-union tools with an `action` parameter:

**`capture_knowledge`** — Create notes
- `action: "create"` — Single note creation
- `action: "bulk_create"` — Batch note creation
- `action: "from_template"` — Instantiate template
- `action: "upload"` — Upload file attachment

**`search`** — Search knowledge base
- `action: "text"` — Full-text and semantic search
- `action: "spatial"` — Location-based search
- `action: "temporal"` — Time-range search
- `action: "spatial_temporal"` — Combined location + time
- `action: "federated"` — Cross-archive search

**`record_provenance`** — Track note origins
- `action: "location"` — GPS coordinates
- `action: "named_location"` — Semantic place name
- `action: "device"` — Capture device
- `action: "file"` — File attachment metadata
- `action: "note"` — Note-level provenance

**`manage_tags`** — Tag operations
- `action: "list"` — List all tags
- `action: "set"` — Set note tags
- `action: "tag_concept"` — Tag with SKOS concept
- `action: "untag_concept"` — Remove SKOS tag
- `action: "get_concepts"` — Get note's SKOS concepts

**`manage_collection`** — Collection/folder operations
- `action: "list"` — List collections
- `action: "create"` — Create collection
- `action: "get"` — Get collection details
- `action: "update"` — Update collection
- `action: "delete"` — Delete collection
- `action: "list_notes"` — List notes in collection
- `action: "move_note"` — Move note to collection
- `action: "export"` — Export collection

**`manage_concepts`** — SKOS concept operations
- `action: "search"` — Search concepts
- `action: "autocomplete"` — Type-ahead search
- `action: "get"` — Get basic concept
- `action: "get_full"` — Get concept with relations
- `action: "stats"` — Governance statistics
- `action: "top"` — Get top-level concepts
- `action: "list_schemes"` — List all concept schemes
- `action: "create_scheme"` — Create a concept scheme
- `action: "get_scheme"` — Get scheme details
- `action: "update_scheme"` — Update scheme metadata
- `action: "delete_scheme"` — Delete a concept scheme

**`manage_embeddings`** — Embedding set operations
- `action: "list"` — List all embedding sets
- `action: "get"` — Get set details by slug
- `action: "create"` — Create a new set with criteria
- `action: "update"` — Update set metadata or criteria
- `action: "delete"` — Delete a set
- `action: "list_members"` — List notes in a set
- `action: "add_members"` — Add notes to a set
- `action: "remove_member"` — Remove a note from a set
- `action: "refresh"` — Recompute auto-criteria membership

**`manage_archives`** — Memory archive operations
- `action: "list"` — List all archives
- `action: "create"` — Create a new archive
- `action: "get"` — Get archive details
- `action: "update"` — Update archive description
- `action: "delete"` — Permanently delete archive
- `action: "set_default"` — Set as session default
- `action: "stats"` — Archive statistics
- `action: "clone"` — Deep copy to new archive

**`manage_encryption`** — PKE encryption operations
- `action: "generate_keypair"` — Generate a new keypair
- `action: "get_address"` — Derive address from public key
- `action: "encrypt"` — Encrypt plaintext for recipients
- `action: "decrypt"` — Decrypt ciphertext
- `action: "list_recipients"` — List recipients from ciphertext
- `action: "verify_address"` — Verify a PKE address
- `action: "list_keysets"` — List local keysets
- `action: "create_keyset"` — Create and store a keyset
- `action: "get_active_keyset"` — Get active keyset
- `action: "set_active_keyset"` — Set active keyset
- `action: "export_keyset"` — Export keyset to directory
- `action: "import_keyset"` — Import keyset from files
- `action: "delete_keyset"` — Delete a local keyset

**`manage_backups`** — Backup and restore operations
- `action: "export_shard"` — Download knowledge shard (.tar.gz)
- `action: "import_shard"` — Upload/import a knowledge shard
- `action: "snapshot"` — Create a named database snapshot
- `action: "restore"` — Restore from a snapshot
- `action: "list"` — List available backups
- `action: "get_info"` — Get backup file details
- `action: "get_metadata"` — Get backup metadata
- `action: "update_metadata"` — Update backup title/description
- `action: "download_archive"` — Download a knowledge archive
- `action: "upload_archive"` — Upload a knowledge archive
- `action: "swap"` — Swap in a backup as active memory
- `action: "download_memory"` — Download memory archive as SQL

**`manage_jobs`** — Job queue management
- `action: "list"` — List jobs with filters
- `action: "get"` — Get single job details
- `action: "create"` — Queue a processing job
- `action: "stats"` — Queue statistics
- `action: "pending_count"` — Quick pending count
- `action: "extraction_stats"` — Extraction pipeline analytics
- `action: "pause_status"` — Get pause state
- `action: "pause"` — Pause globally or per archive
- `action: "resume"` — Resume globally or per archive

## Advanced Features

Most functionality is available directly through the 37 core tools. A small set of features requires full mode or the REST API:

**Requires full mode or REST API (not in core):**
- Note versioning (version history, diffs, restore)
- SKOS concept CRUD (create/update/delete individual concepts, relation management)
- SKOS collections (`list_skos_collections`, etc.)
- OAuth client management
- Embedding configs (model-level configuration)
- Document types (create, update, delete, detection)
- Note purge operations (hard delete)
- Timeline and activity views

Use the `get_documentation` tool to learn about these features:

```javascript
get_documentation({ topic: "versioning" })
get_documentation({ topic: "pke" })          // PKE is also in core via manage_encryption
get_documentation({ topic: "embedding-sets" }) // Also in core via manage_embeddings
get_documentation({ topic: "skos" })
```

Or switch to full mode for all 202 granular tools: `MCP_TOOL_MODE=full`

## Document Types

Fortémi uses a Document Type Registry to automatically configure content processing based on document characteristics.

### What are Document Types?

Document types define how content should be chunked, embedded, and processed:

- **131 pre-configured types** across 19 categories (code, prose, config, markup, data, API specs, IaC, etc.)
- **Auto-detection** from filename patterns, extensions, and content magic
- **Optimized chunking** strategies per document type (semantic for prose, syntactic for code, per-section for docs)
- **Extensible** with custom document types

### Using Document Types via MCP

**Core mode:**
```javascript
// Document type auto-detection happens automatically during note creation
capture_knowledge({
  action: "upload",
  filename: "main.rs",
  content: "...",
  content_type: "text/plain"
})
// Auto-detects: { type: "rust", category: "code", chunking: "syntactic" }
```

**Full mode:**
```javascript
detect_document_type({ filename: "main.rs" })
// Returns: { type: "rust", confidence: 0.9, category: "code" }

list_document_types({ category: "code" })
// Returns types: rust, python, javascript, typescript, etc.

get_document_type({ name: "rust" })
// Returns full configuration

create_document_type({
  name: "my-custom-type",
  display_name: "My Custom Type",
  category: "custom",
  file_extensions: [".mytype"],
  chunking_strategy: "semantic"
})
```

### Detection Priority

Document types are detected using a confidence-based system:
- **Filename pattern** (1.0): Exact pattern match (e.g., `docker-compose.yml`)
- **Extension** (0.9): File extension match (e.g., `.rs` → rust)
- **Content magic** (0.7): Content pattern recognition (e.g., `openapi:` → OpenAPI)
- **Default** (0.1): Fallback to generic type

### Chunking Strategies

Different document types use different chunking strategies for optimal embedding:

- **semantic**: Natural paragraph/section boundaries (prose, docs)
- **syntactic**: Language-aware code parsing (source code)
- **fixed**: Fixed token windows (logs, raw data)
- **per_section**: Heading-based splits (structured docs, markdown)
- **whole**: No splitting (atomic content like tweets)

## Embedding Sets

Embedding sets are curated collections of notes optimized for focused semantic search. They allow you to create domain-specific search contexts instead of always searching the entire knowledge base.

### What are Embedding Sets?

Every note in Fortémi has vector embeddings for semantic search. By default, all notes belong to the `default` embedding set, which provides global search. Embedding sets allow you to create focused collections:

- **Domain-specific search**: Create sets for specific topics (e.g., "ml-research", "project-alpha")
- **Improved relevance**: Search results are scoped to relevant notes only
- **Performance**: Smaller sets search faster than the entire knowledge base
- **Context control**: AI agents can choose the appropriate set based on query context

### Using Embedding Sets via MCP

**Core mode:**
```javascript
// Search within a specific set
search({
  action: "text",
  query: "transformer architecture attention mechanism",
  mode: "semantic",
  set: "ml-research"  // Restricts search to this set
})
```

**Full mode:**
```javascript
// List all embedding sets to see what's available
list_embedding_sets()
// Returns: [{ slug: "default", name: "All Notes", ... }, { slug: "ml-research", ... }]

// Create a new set
create_embedding_set({
  name: "Machine Learning Research",
  slug: "ml-research",
  description: "Notes about ML algorithms, papers, and experiments",
  purpose: "Semantic search for ML-related content",
  usage_hints: "Use when queries are about machine learning, neural networks, or AI research",
  keywords: ["machine learning", "AI", "neural networks"],
  mode: "auto",
  criteria: {
    tags: ["ml", "research", "ai"]
  }
})

// Manage membership (manual/mixed mode)
add_set_members({
  slug: "ml-research",
  note_ids: ["note-uuid-1", "note-uuid-2"]
})

remove_set_member({
  slug: "ml-research",
  note_id: "note-uuid-1"
})

// Refresh auto-criteria membership
refresh_embedding_set({ slug: "ml-research" })
```

### Membership Modes

- **auto**: Notes automatically included based on criteria (tags, collections, FTS query, date filters)
- **manual**: Only explicitly added notes are included
- **mixed**: Auto criteria plus manual additions/exclusions

### Example Workflows

**Research project tracking:**
```javascript
// Create a set for a specific project
create_embedding_set({
  name: "Project Alpha",
  slug: "project-alpha",
  mode: "auto",
  criteria: {
    collections: ["project-alpha-collection-uuid"],
    exclude_archived: true
  }
})

// Search within project context
search({
  action: "text",
  query: "API authentication design decisions",
  set: "project-alpha"
})
```

**Meeting notes corpus:**
```javascript
create_embedding_set({
  name: "Meeting Notes",
  slug: "meetings",
  mode: "auto",
  criteria: {
    tags: ["meeting"],
    created_after: "2024-01-01T00:00:00Z"
  }
})
```

## Data Deletion

Fortémi supports both soft delete and hard delete (purge) operations.

### Soft Delete vs Hard Delete

**Soft Delete** (`delete_note`):
- Marks note as deleted but preserves all data
- Can be restored via `restore_note`
- Embeddings, links, and tags remain in database
- Use for normal deletion when you might want to recover

**Hard Delete** (full mode only: `purge_note`, `purge_notes`, `purge_all_notes`):
- Permanently removes note and ALL related data
- Cannot be recovered - this is irreversible
- Deletes embeddings, links, tags, revisions, set memberships
- Use for permanent cleanup or privacy compliance

### When to Use Each

**Use soft delete when:**
- Normal note cleanup
- User requested deletion (they might want it back)
- Archiving old content
- Temporary removal

**Use purge when:**
- Privacy/GDPR compliance (user data removal)
- Development cleanup (resetting test data)
- Permanent removal of sensitive information
- Bulk cleanup of unwanted content

### Purge Operations (Full Mode)

**Single note purge:**
```javascript
purge_note({ id: "note-uuid" })
// Returns: { job_id: "...", status: "queued" }
```

**Batch purge:**
```javascript
purge_notes({
  note_ids: ["uuid-1", "uuid-2", "uuid-3"]
})
// Returns: { queued: ["uuid-1", "uuid-2"], failed: [] }
```

**Complete system reset:**
```javascript
purge_all_notes({ confirm: true })
// WARNING: Deletes ALL notes in the system!
// Returns: { queued: [...], failed: [...], total: 150 }
```

### How Purge Works

Purge operations queue high-priority background jobs that:
1. Delete all embeddings for the note(s)
2. Remove all links (from and to the note)
3. Remove tag associations
4. Remove embedding set memberships
5. Delete revision history
6. Finally, delete the note record itself

Use `list_jobs({ status: "processing" })` to monitor purge job progress.

## Testing

Integration tests validate end-to-end MCP connectivity against the real API.

### Quick Test (Recommended)

If you have an API key:

```bash
FORTEMI_URL=http://localhost:3000 \
FORTEMI_API_KEY=your-api-key \
npm test
```

### Test with OAuth Credentials

If you have OAuth client credentials:

```bash
FORTEMI_URL=http://localhost:3000 \
MCP_CLIENT_ID=mm_xxx \
MCP_CLIENT_SECRET=xxx \
npm test
```

### Auto-Registration (Development)

The tests can automatically register an OAuth client if neither API key nor client credentials are provided (requires API to allow dynamic client registration):

```bash
FORTEMI_URL=http://localhost:3000 npm test
```

### What the Tests Cover

- **API Reachability**: Health endpoint and authentication
- **HTTP Transport**: StreamableHTTP protocol, session management
- **SSE Transport**: Server-Sent Events connection
- **Tool Execution**: Calling tools through MCP (list_notes, search, etc.)
- **Session Isolation**: Multiple concurrent sessions work independently
- **Stdio Transport**: Direct stdin/stdout communication

### CI/CD Integration

For CI/CD, ensure these environment variables are set:

```yaml
env:
  FORTEMI_URL: ${{ secrets.API_URL }}
  FORTEMI_API_KEY: ${{ secrets.API_KEY }}
```

## Production Deployment (Docker Bundle)

The MCP server runs automatically as part of the Docker bundle deployment.

### Critical: External URL Configuration

The `.env` file in the project root **must** set `ISSUER_URL` to your external domain:

```bash
# .env (project root)
ISSUER_URL=https://your-domain.com
```

This sets both:
- **ISSUER_URL** - OAuth authorization server URL
- **MCP_BASE_URL** - OAuth protected resource URL (derived as `${ISSUER_URL}/mcp`)

Without this, OAuth metadata will advertise `localhost` URLs, causing authentication failures.

### MCP OAuth Credentials (Auto-Managed)

The MCP server needs OAuth client credentials to introspect incoming bearer tokens. **These are managed automatically** by the Docker bundle entrypoint:

1. On startup, the entrypoint waits for the API to be healthy
2. If no valid credentials exist, it registers a new OAuth client automatically
3. Credentials are persisted on the database volume (`$PGDATA/.fortemi-mcp-credentials`)
4. On subsequent restarts, persisted credentials are loaded and validated

No manual configuration of `MCP_CLIENT_ID` or `MCP_CLIENT_SECRET` is needed. For manual override or advanced credential management, see the [MCP Deployment Guide](../docs/content/mcp-deployment.md).

### Nginx Configuration

The MCP server runs on port 3001 inside the container. Configure nginx to proxy `/mcp` routes:

```nginx
# API routes
location / {
    proxy_pass http://localhost:3000;
    # ... standard proxy headers
}

# MCP routes (exact match for root, prefix for sub-paths)
location = /mcp {
    proxy_pass http://localhost:3001/;
    # ... standard proxy headers
}

location /mcp/ {
    proxy_pass http://localhost:3001/;
    # ... standard proxy headers
}
```

### Restart After Configuration Changes

```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### Verify Configuration

Check the protected resource metadata returns correct URLs:

```bash
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
# Should return: { "resource": "https://your-domain.com/mcp", ... }
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FORTEMI_URL` | `https://fortemi.com` | API base URL |
| `FORTEMI_API_KEY` | - | API key for stdio mode |
| `MCP_TRANSPORT` | `stdio` | Transport mode: `stdio` or `http` |
| `MCP_PORT` | `3001` | HTTP server port (http mode only) |
| `MCP_TOOL_MODE` | `core` | Tool surface: `core` (37 tools) or `full` (202 granular tools) |
| `ISSUER_URL` | `https://localhost:3000` | External URL for OAuth (set in .env) |
| `MCP_BASE_URL` | `${ISSUER_URL}/mcp` | Base URL for OAuth metadata |
| `MCP_BASE_PATH` | - | Path prefix when behind proxy (e.g., `/mcp`) |
| `MCP_CLIENT_ID` | (auto) | OAuth client ID for token introspection (auto-managed) |
| `MCP_CLIENT_SECRET` | (auto) | OAuth client secret for token introspection (auto-managed) |

## Troubleshooting

See [MCP Troubleshooting Guide](../docs/content/mcp-troubleshooting.md) for:
- Diagnostic commands
- Common issues and fixes
- Token validation
- First-time setup checklist

## OAuth2 Authentication (HTTP Mode)

The HTTP transport requires OAuth2 bearer tokens. The server validates tokens against the main API's introspection endpoint.

Required scopes: `mcp` or `read`

401 responses include RFC 9728 compliant `WWW-Authenticate` headers pointing to the protected resource metadata.

## Example

```
User: Search my notes for anything about API design

Claude: [uses search tool with action="text", query="API design"]

Found 3 notes about API design:
1. "REST API Best Practices" - discusses versioning and error handling
2. "GraphQL vs REST" - comparison of approaches
3. "API Documentation" - notes on OpenAPI specs
```

## Advanced Examples

### Using Embedding Sets for Focused Search

```
User: I need to find notes about neural network architectures, but only from my ML research

Claude: [uses search with action="text", query="neural network architectures", set="ml-research"]

Found 5 notes in ML research set:
1. "Transformer Architecture Deep Dive" - attention mechanism details
2. "CNN vs RNN Comparison" - architectural differences
3. "ResNet Implementation Notes" - skip connections
...
```

### Creating Notes from Templates

```
User: Create a meeting note for today's standup

Claude: [uses capture_knowledge with action="from_template", template_slug="meeting-notes"]

Created note "Standup 2024-03-15" from template with pre-filled sections:
- Attendees
- Agenda
- Action Items
- Next Steps
```

### Multi-Archive Search

```
User: Search for notes about "database migration" across all my archives

Claude: [uses search with action="federated", query="database migration"]

Found 12 results across 3 archives:
- personal (5 notes)
- work-projects (6 notes)
- learning (1 note)
```

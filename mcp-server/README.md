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

## Available Tools

### Core Operations

| Tool | Description |
|------|-------------|
| `list_notes` | List all notes with summaries |
| `get_note` | Get full note details |
| `create_note` | Create a new note with full AI pipeline |
| `bulk_create_notes` | Create multiple notes in batch |
| `update_note` | Update note content/status |
| `delete_note` | Soft delete a note (recoverable) |
| `search_notes` | Full-text and semantic search (supports embedding sets) |
| `list_tags` | List all tags |
| `set_note_tags` | Set tags for a note |
| `get_note_links` | Get note relationships |
| `export_note` | Export note as markdown |

### Collections (Folders)

| Tool | Description |
|------|-------------|
| `list_collections` | List all collections |
| `create_collection` | Create a new collection |
| `get_collection` | Get collection details |
| `delete_collection` | Delete a collection |
| `get_collection_notes` | List notes in a collection |
| `move_note_to_collection` | Move note to collection |
| `explore_graph` | Explore knowledge graph from a note |

### Templates

| Tool | Description |
|------|-------------|
| `list_templates` | List all note templates |
| `create_template` | Create a new template |
| `get_template` | Get template details |
| `delete_template` | Delete a template |
| `instantiate_template` | Create note from template |

### Background Jobs

| Tool | Description |
|------|-------------|
| `create_job` | Queue specific AI processing jobs |
| `list_jobs` | List and filter background jobs |
| `get_queue_stats` | Get job queue statistics |

### Embedding Sets

| Tool | Description |
|------|-------------|
| `list_embedding_sets` | List all embedding sets with stats |
| `get_embedding_set` | Get embedding set details by slug |
| `create_embedding_set` | Create a new embedding set |
| `list_set_members` | List notes in an embedding set |
| `add_set_members` | Add notes to an embedding set |
| `remove_set_member` | Remove a note from an embedding set |
| `refresh_embedding_set` | Refresh set membership based on criteria |
| `reembed_all` | Regenerate embeddings for all notes or a specific set |

### Data Deletion

| Tool | Description |
|------|-------------|
| `purge_note` | Permanently delete a note and ALL related data |
| `purge_notes` | Batch permanently delete multiple notes |
| `purge_all_notes` | Delete ALL notes (requires confirm: true) |

### Document Type Tools

| Tool | Description |
|------|-------------|
| `list_document_types` | List all types with optional category filter |
| `get_document_type` | Get type details by name |
| `create_document_type` | Create custom document type |
| `update_document_type` | Update type configuration |
| `delete_document_type` | Delete non-system type |
| `detect_document_type` | Auto-detect from filename/content |

### File Attachments

| Tool | Description |
|------|-------------|
| `upload_attachment` | Upload file attachment to a note (base64 content) |
| `list_attachments` | List all attachments for a note |
| `get_attachment` | Download attachment binary content |
| `get_attachment_metadata` | Get EXIF metadata and provenance data |
| `delete_attachment` | Permanently remove an attachment |

### Memory Search

| Tool | Description |
|------|-------------|
| `search_memories_by_location` | Find memories near geographic coordinates |
| `search_memories_by_time` | Find memories within a time range |
| `search_memories_combined` | Combined location and time search |
| `get_memory_provenance` | Get full provenance chain with EXIF data |

## Document Types

Fortémi uses a Document Type Registry to automatically configure content processing based on document characteristics.

### What are Document Types?

Document types define how content should be chunked, embedded, and processed:

- **131 pre-configured types** across 19 categories (code, prose, config, markup, data, API specs, IaC, etc.)
- **Auto-detection** from filename patterns, extensions, and content magic
- **Optimized chunking** strategies per document type (semantic for prose, syntactic for code, per-section for docs)
- **Extensible** with custom document types

### Using Document Types via MCP

**1. Auto-detect document type from filename:**
```javascript
detect_document_type({ filename: "main.rs" })
// Returns: { type: "rust", confidence: 0.9, category: "code" }
```

**2. Auto-detect from content:**
```javascript
detect_document_type({ content: "openapi: 3.1.0\ninfo:" })
// Returns: { type: "openapi", confidence: 0.7, category: "api-spec" }
```

**3. List all document types:**
```javascript
// List all types
list_document_types()

// Filter by category
list_document_types({ category: "code" })
// Returns types: rust, python, javascript, typescript, etc.
```

**4. Get specific type details:**
```javascript
get_document_type({ name: "rust" })
// Returns: {
//   name: "rust",
//   display_name: "Rust",
//   category: "code",
//   file_extensions: [".rs"],
//   chunking_strategy: "syntactic",
//   ...
// }
```

**5. Create custom document type:**
```javascript
create_document_type({
  name: "my-custom-type",
  display_name: "My Custom Type",
  category: "custom",
  file_extensions: [".mytype"],
  chunking_strategy: "semantic",
  filename_patterns: ["*.mytype"]
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

**1. Discover available sets:**
```javascript
// List all embedding sets to see what's available
list_embedding_sets()
// Returns: [{ slug: "default", name: "All Notes", ... }, { slug: "ml-research", ... }]
```

**2. Create a new set:**
```javascript
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
```

**3. Search within a set:**
```javascript
// Search only within the ml-research set
search_notes({
  query: "transformer architecture attention mechanism",
  mode: "semantic",
  set: "ml-research"  // Restricts search to this set
})
```

**4. Manage membership (manual/mixed mode):**
```javascript
// Add specific notes to a set
add_set_members({
  slug: "ml-research",
  note_ids: ["note-uuid-1", "note-uuid-2"]
})

// Remove a note from a set
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
search_notes({
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
- Can be restored later (future feature)
- Embeddings, links, and tags remain in database
- Use for normal deletion when you might want to recover

**Hard Delete** (`purge_note`, `purge_notes`, `purge_all_notes`):
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

### Purge Operations

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
- **Tool Execution**: Calling tools through MCP (list_notes, search_notes, etc.)
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

### Critical: MCP OAuth Client Credentials

The MCP server needs OAuth client credentials to introspect incoming bearer tokens:

```bash
# Register an OAuth client for MCP token introspection
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'

# Add the returned credentials to .env
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx
```

Without `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET`, the MCP server cannot validate OAuth tokens and will reject all authenticated requests.

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
| `ISSUER_URL` | `https://localhost:3000` | External URL for OAuth (set in .env) |
| `MCP_BASE_URL` | `${ISSUER_URL}/mcp` | Base URL for OAuth metadata |
| `MCP_BASE_PATH` | - | Path prefix when behind proxy (e.g., `/mcp`) |
| `MCP_CLIENT_ID` | - | OAuth client ID for token introspection |
| `MCP_CLIENT_SECRET` | - | OAuth client secret for token introspection |

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

Claude: [uses search_notes tool with query "API design"]

Found 3 notes about API design:
1. "REST API Best Practices" - discusses versioning and error handling
2. "GraphQL vs REST" - comparison of approaches
3. "API Documentation" - notes on OpenAPI specs
```

## Advanced Examples

### Using Embedding Sets for Focused Search

```
User: I need to find notes about neural network architectures, but only from my ML research

Claude: [uses list_embedding_sets to discover "ml-research" set]
        [uses search_notes with query="neural network architectures" and set="ml-research"]

Found 5 notes in ML research set:
1. "Transformer Architecture Deep Dive" - attention mechanism details
2. "CNN vs RNN Comparison" - architectural differences
3. "ResNet Implementation Notes" - skip connections
...
```

### Creating and Using Project-Specific Sets

```
User: Create an embedding set for my quantum computing research

Claude: [uses create_embedding_set with appropriate criteria]
        [uses add_set_members to add existing relevant notes]

Created "quantum-computing" embedding set with 15 existing notes.
You can now use set="quantum-computing" in search_notes for focused searches.
```

### Development Cleanup

```
User: I need to clean up all my test notes before deploying

Claude: [uses list_notes with filter for test data]
        [uses purge_notes with the test note IDs]

WARNING: This will permanently delete 23 test notes. Confirm?

User: Yes, purge them

Claude: [executes purge_notes]

Queued 23 notes for permanent deletion. Use list_jobs to monitor progress.
```

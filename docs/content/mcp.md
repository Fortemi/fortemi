# Fortémi MCP Server

Complete documentation for the Model Context Protocol (MCP) server that provides AI agent access to Fortémi.

## Connecting to Fortémi

### Remote Access (Recommended)

Add this to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "http://localhost:3001"
    }
  }
}
```

On first connection, your MCP client will perform an OAuth2 authentication flow. No manual credential setup is needed — the server handles credential management automatically.

**Requirements:**
- `ISSUER_URL` must be set in your `.env` (e.g., `http://localhost:3000`)
- For remote access, configure nginx to proxy `/mcp` to port 3001

### Local Access (Development)

For local development with the source code:

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["./mcp-server/index.js"],
      "env": {
        "FORTEMI_URL": "http://localhost:3000"
      }
    }
  }
}
```

Local (stdio) transport requires no authentication.

### How Authentication Works

The MCP server uses OAuth2 for secure access:

1. Your MCP client connects and discovers the OAuth server via `.well-known` endpoints
2. The client authenticates and receives a bearer token
3. Every MCP request includes this token
4. The MCP server validates the token against the API's introspection endpoint

**Credentials are managed automatically.** The Docker bundle registers its own OAuth client on startup and persists the credentials. You never need to manually configure `MCP_CLIENT_ID` or `MCP_CLIENT_SECRET` unless you want explicit control.

For advanced credential management, security considerations, and manual configuration, see the [MCP Deployment Guide](./mcp-deployment.md).

## Overview

The MCP server provides AI assistants (Claude, etc.) with access to your knowledge base through two distinct tool surfaces:

### Core Mode (Default)

**23 consolidated tools** using discriminated-union pattern for agent-optimized operation:

- **~78% token reduction** compared to full mode (23 vs 187 tools)
- **Action-based design** groups related operations under unified tools
- **Cognitive load reduction** improves agent decision-making and response time
- **Backward compatible** all functionality available, just organized differently

### Full Mode (Optional)

**187 granular tools** exposing every API endpoint individually:

- Set `MCP_TOOL_MODE=full` environment variable
- Useful for programmatic access requiring precise endpoint control
- Higher token overhead and cognitive complexity for agents

**Recommendation:** Use default core mode unless you have specific requirements for granular tool access.

## Core Tools Reference

The 23 core tools provide complete access to Fortémi functionality through action-based interfaces.

### Notes Operations

#### `list_notes`

List notes with filtering and pagination.

**Parameters:**
- `limit` (optional) - Maximum number of notes to return (default: 50, max: 1000)
- `offset` (optional) - Number of notes to skip (default: 0)
- `tags` (optional) - Filter by tags (array of strings)
- `collection_id` (optional) - Filter by collection UUID
- `deleted` (optional) - Include soft-deleted notes (default: false)

```json
{
  "limit": 20,
  "tags": ["research", "ai"],
  "collection_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `get_note`

Retrieve full details for a specific note by ID.

**Parameters:**
- `id` (required) - UUID of the note

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `update_note`

Update note content, title, or status.

**Parameters:**
- `id` (required) - UUID of the note
- `content` (optional) - New markdown content
- `title` (optional) - New title
- `status` (optional) - New status (active, archived, etc.)

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "content": "# Updated content\n\nNew information...",
  "status": "active"
}
```

#### `delete_note`

Soft delete a note (recoverable via `restore_note`).

**Parameters:**
- `id` (required) - UUID of the note

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `restore_note`

Restore a soft-deleted note with all original metadata, tags, and content.

**Parameters:**
- `id` (required) - UUID of the deleted note

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### `capture_knowledge`

Unified tool for creating notes and uploading content with full AI enhancement pipeline.

**Actions:**

#### `create` - Create a single note

Creates a note with AI revision, embedding generation, title generation, and automatic semantic linking.

**Revision Modes:**

| Mode | Use When | Behavior |
|------|----------|----------|
| `light` (default) | Facts, opinions, quick thoughts | Formatting only, no invented details |
| `full` | Technical concepts, research | Full contextual expansion with related notes |
| `none` | Exact quotes, citations, raw data | No AI processing, auto-queuing disabled |

**Parameters:**
- `action: "create"`
- `content` (required) - Markdown content
- `tags` (optional) - Array of tag strings
- `revision_mode` (optional) - "light" (default), "full", or "none"
- `collection_id` (optional) - UUID to assign note to collection

```json
{
  "action": "create",
  "content": "# Research Note\n\nTransformer architecture details...",
  "tags": ["research", "ai", "transformers"],
  "revision_mode": "light",
  "collection_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `bulk_create` - Create multiple notes at once

Batch create up to 100 notes in a single operation.

**Parameters:**
- `action: "bulk_create"`
- `notes` (required) - Array of note objects (max 100)

```json
{
  "action": "bulk_create",
  "notes": [
    {
      "content": "# Note 1",
      "tags": ["batch"],
      "revision_mode": "light"
    },
    {
      "content": "# Note 2",
      "tags": ["batch"],
      "revision_mode": "light"
    }
  ]
}
```

#### `from_template` - Create note from template

Instantiate a template with variable substitution.

**Parameters:**
- `action: "from_template"`
- `template_id` (required) - UUID of the template
- `variables` (required) - Object with variable values (e.g., `{"title": "My Title"}`)
- `tags` (optional) - Additional tags beyond template defaults

```json
{
  "action": "from_template",
  "template_id": "660e8400-e29b-41d4-a716-446655440000",
  "variables": {
    "title": "Meeting Notes",
    "date": "2026-02-14",
    "attendees": "Alice, Bob"
  },
  "tags": ["meeting"]
}
```

#### `upload` - Upload file attachment to note

Upload a file from disk with automatic metadata extraction.

**Parameters:**
- `action: "upload"`
- `note_id` (required) - UUID of the note
- `file_path` (required) - Absolute path to file on disk
- `content_type` (required) - MIME type (e.g., "image/jpeg")
- `filename` (optional) - Override filename (defaults to basename)

```json
{
  "action": "upload",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "file_path": "/home/user/photos/diagram.png",
  "content_type": "image/png"
}
```

### `search`

Unified search tool supporting text, spatial, temporal, and federated search modes.

**Actions:**

#### `text` - Hybrid semantic + full-text search

Combines keyword matching and semantic similarity for comprehensive search.

**Search Modes:**
- `hybrid` (default) - Combines keyword + semantic
- `fts` - Exact keyword matching only
- `semantic` - Conceptual similarity only

**Parameters:**
- `action: "text"`
- `query` (required) - Search query string
- `mode` (optional) - "hybrid", "fts", or "semantic"
- `limit` (optional) - Max results (default: 50)
- `offset` (optional) - Skip N results (default: 0)
- `set` (optional) - Embedding set slug to search within
- `collection_id` (optional) - Restrict to collection
- `strict_filter` (optional) - Tag-based filtering object

**Query Syntax:**
```
hello world        # Match all words (AND)
apple OR orange    # Match either word
apple -orange      # Exclude word
"hello world"      # Match exact phrase
```

**Strict Filtering:**

```json
{
  "action": "text",
  "query": "authentication",
  "strict_filter": {
    "required_tags": ["project:matric"],
    "excluded_tags": ["draft", "archived"],
    "any_tags": ["status:active", "status:review"],
    "required_schemes": ["client-acme"],
    "excluded_schemes": ["internal"]
  }
}
```

**Filter Types:**

| Parameter | Logic | Description |
|-----------|-------|-------------|
| `required_tags` | AND | Notes MUST have ALL these tags |
| `any_tags` | OR | Notes MUST have AT LEAST ONE |
| `excluded_tags` | NOT | Notes MUST NOT have ANY of these |
| `required_schemes` | Isolation | Notes ONLY from these vocabularies |
| `excluded_schemes` | Exclusion | Notes NOT from these vocabularies |

#### `spatial` - Search by geographic location

Find memories near coordinates using PostGIS spatial queries.

**Parameters:**
- `action: "spatial"`
- `latitude` (required) - Latitude in degrees
- `longitude` (required) - Longitude in degrees
- `radius_km` (optional) - Search radius in kilometers (default: 10)
- `limit` (optional) - Max results (default: 50)

```json
{
  "action": "spatial",
  "latitude": 37.7749,
  "longitude": -122.4194,
  "radius_km": 5.0,
  "limit": 20
}
```

#### `temporal` - Search by time range

Find memories within a specific time period.

**Parameters:**
- `action: "temporal"`
- `start_time` (required) - ISO 8601 timestamp
- `end_time` (required) - ISO 8601 timestamp
- `limit` (optional) - Max results (default: 50)

```json
{
  "action": "temporal",
  "start_time": "2026-01-01T00:00:00Z",
  "end_time": "2026-01-31T23:59:59Z",
  "limit": 50
}
```

#### `spatial_temporal` - Combined location and time search

Find memories matching both location and time criteria.

**Parameters:**
- `action: "spatial_temporal"`
- `latitude` (required) - Latitude in degrees
- `longitude` (required) - Longitude in degrees
- `radius_km` (required) - Search radius in kilometers
- `start_time` (required) - ISO 8601 timestamp
- `end_time` (required) - ISO 8601 timestamp
- `limit` (optional) - Max results (default: 50)

```json
{
  "action": "spatial_temporal",
  "latitude": 37.7749,
  "longitude": -122.4194,
  "radius_km": 2.0,
  "start_time": "2026-02-01T00:00:00Z",
  "end_time": "2026-02-14T23:59:59Z"
}
```

#### `federated` - Search across multiple memory archives

Search across all memories or a specific set simultaneously.

**Parameters:**
- `action: "federated"`
- `query` (required) - Search query string
- `memories` (optional) - Array of memory names, or `["all"]` (default: all)
- `mode` (optional) - "hybrid", "fts", or "semantic" (default: "hybrid")
- `limit` (optional) - Max results per memory (default: 50)

```json
{
  "action": "federated",
  "query": "project documentation",
  "memories": ["work", "research", "personal"],
  "mode": "hybrid",
  "limit": 20
}
```

### `record_provenance`

Create spatial-temporal provenance records for notes, files, locations, and devices.

**Actions:**

#### `location` - Record anonymous location

Create a provenance location without a named place.

**Parameters:**
- `action: "location"`
- `latitude` (required) - Latitude in degrees
- `longitude` (required) - Longitude in degrees
- `accuracy_meters` (optional) - GPS accuracy
- `altitude_meters` (optional) - Altitude above sea level
- `created_at` (optional) - ISO 8601 timestamp (defaults to now)

```json
{
  "action": "location",
  "latitude": 37.7749,
  "longitude": -122.4194,
  "accuracy_meters": 10.0,
  "altitude_meters": 15.0
}
```

#### `named_location` - Record named location

Create a location with a human-readable name.

**Parameters:**
- `action: "named_location"`
- `name` (required) - Location name
- `latitude` (required) - Latitude in degrees
- `longitude` (required) - Longitude in degrees
- `place_type` (optional) - Type of place (e.g., "office", "cafe")
- `accuracy_meters` (optional) - GPS accuracy

```json
{
  "action": "named_location",
  "name": "San Francisco Office",
  "latitude": 37.7749,
  "longitude": -122.4194,
  "place_type": "office"
}
```

#### `device` - Record device provenance

Track which device created or modified content.

**Parameters:**
- `action: "device"`
- `device_id` (required) - Unique device identifier
- `name` (optional) - Human-readable device name
- `device_type` (optional) - Type (e.g., "laptop", "phone")
- `os` (optional) - Operating system
- `app_version` (optional) - Application version

```json
{
  "action": "device",
  "device_id": "macbook-pro-2023",
  "name": "Work Laptop",
  "device_type": "laptop",
  "os": "macOS 14.0"
}
```

#### `file` - Record file provenance

Track file origins and metadata.

**Parameters:**
- `action: "file"`
- `filename` (required) - File name
- `file_path` (optional) - Full path
- `mime_type` (optional) - MIME type
- `size_bytes` (optional) - File size
- `sha256` (optional) - File hash
- `created_at` (optional) - ISO 8601 timestamp

```json
{
  "action": "file",
  "filename": "research-paper.pdf",
  "file_path": "/documents/research-paper.pdf",
  "mime_type": "application/pdf",
  "size_bytes": 2048576
}
```

#### `note` - Record note-level provenance

Associate spatial-temporal provenance with a note.

**Parameters:**
- `action: "note"`
- `note_id` (required) - UUID of the note
- `location_id` (optional) - UUID of provenance location
- `device_id` (optional) - UUID of provenance device
- `recorded_at` (optional) - ISO 8601 timestamp (defaults to now)

```json
{
  "action": "note",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "location_id": "660e8400-e29b-41d4-a716-446655440001",
  "device_id": "770e8400-e29b-41d4-a716-446655440002"
}
```

### `manage_tags`

Unified tag management including listing, setting, and SKOS concept tagging.

**Actions:**

#### `list` - List all tags with usage counts

**Parameters:**
- `action: "list"`
- `limit` (optional) - Max results (default: 100)
- `offset` (optional) - Skip N results (default: 0)
- `min_count` (optional) - Minimum usage count (default: 1)

```json
{
  "action": "list",
  "limit": 50,
  "min_count": 5
}
```

#### `set` - Replace note's user tags

Sets the complete tag list for a note (replaces existing tags).

**Parameters:**
- `action: "set"`
- `note_id` (required) - UUID of the note
- `tags` (required) - Array of tag strings

```json
{
  "action": "set",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "tags": ["research", "ai", "transformers"]
}
```

#### `tag_concept` - Tag note with SKOS concept

Apply hierarchical semantic tag from concept scheme.

**Parameters:**
- `action: "tag_concept"`
- `note_id` (required) - UUID of the note
- `concept_id` (required) - UUID of the SKOS concept

```json
{
  "action": "tag_concept",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "concept_id": "880e8400-e29b-41d4-a716-446655440000"
}
```

#### `untag_concept` - Remove SKOS concept tag

Remove hierarchical semantic tag from note.

**Parameters:**
- `action: "untag_concept"`
- `note_id` (required) - UUID of the note
- `concept_id` (required) - UUID of the SKOS concept

```json
{
  "action": "untag_concept",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "concept_id": "880e8400-e29b-41d4-a716-446655440000"
}
```

#### `get_concepts` - Get note's SKOS concepts

Retrieve all SKOS concept tags applied to a note.

**Parameters:**
- `action: "get_concepts"`
- `note_id` (required) - UUID of the note

```json
{
  "action": "get_concepts",
  "note_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### `manage_collection`

Hierarchical folder organization for notes.

**Actions:**

#### `list` - List collections with optional parent filter

**Parameters:**
- `action: "list"`
- `parent_id` (optional) - UUID of parent collection (null for root)
- `limit` (optional) - Max results (default: 100)

```json
{
  "action": "list",
  "parent_id": "550e8400-e29b-41d4-a716-446655440000",
  "limit": 50
}
```

#### `create` - Create new collection

**Parameters:**
- `action: "create"`
- `name` (required) - Collection name
- `description` (optional) - Description
- `parent_id` (optional) - UUID of parent collection (null for root)

```json
{
  "action": "create",
  "name": "Research Projects",
  "description": "Active research work",
  "parent_id": null
}
```

#### `get` - Get collection details

**Parameters:**
- `action: "get"`
- `id` (required) - UUID of the collection

```json
{
  "action": "get",
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `update` - Update collection metadata or hierarchy

Change name, description, or parent to reorganize collections.

**Parameters:**
- `action: "update"`
- `id` (required) - UUID of the collection
- `name` (optional) - New name
- `description` (optional) - New description
- `parent_id` (optional) - New parent UUID (null to move to root)

```json
{
  "action": "update",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "AI Research 2026",
  "parent_id": "660e8400-e29b-41d4-a716-446655440001"
}
```

#### `delete` - Delete collection

Soft delete collection (does not delete notes within).

**Parameters:**
- `action: "delete"`
- `id` (required) - UUID of the collection

```json
{
  "action": "delete",
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `list_notes` - List notes in collection

**Parameters:**
- `action: "list_notes"`
- `id` (required) - UUID of the collection
- `limit` (optional) - Max results (default: 50)
- `offset` (optional) - Skip N results (default: 0)

```json
{
  "action": "list_notes",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "limit": 20
}
```

#### `move_note` - Move note to collection

**Parameters:**
- `action: "move_note"`
- `note_id` (required) - UUID of the note
- `collection_id` (required) - UUID of the target collection (null to remove from all)

```json
{
  "action": "move_note",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "collection_id": "660e8400-e29b-41d4-a716-446655440001"
}
```

#### `export` - Export collection as JSON

Export collection with all notes, metadata, and structure.

**Parameters:**
- `action: "export"`
- `id` (required) - UUID of the collection
- `include_notes` (optional) - Include note content (default: true)

```json
{
  "action": "export",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "include_notes": true
}
```

### `manage_concepts`

W3C SKOS-compliant hierarchical tagging system operations.

**Actions:**

#### `search` - Search concepts by label or definition

**Parameters:**
- `action: "search"`
- `query` (required) - Search query string
- `scheme_id` (optional) - Restrict to specific scheme UUID
- `status` (optional) - Filter by status ("candidate", "controlled", "deprecated")
- `limit` (optional) - Max results (default: 50)

```json
{
  "action": "search",
  "query": "machine learning",
  "scheme_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "controlled"
}
```

#### `autocomplete` - Type-ahead concept search

**Parameters:**
- `action: "autocomplete"`
- `prefix` (required) - Search prefix
- `scheme_id` (optional) - Restrict to specific scheme
- `limit` (optional) - Max results (default: 10)

```json
{
  "action": "autocomplete",
  "prefix": "mach",
  "limit": 5
}
```

#### `get` - Get concept details

**Parameters:**
- `action: "get"`
- `id` (required) - UUID of the concept

```json
{
  "action": "get",
  "id": "880e8400-e29b-41d4-a716-446655440000"
}
```

#### `get_full` - Get concept with all relations

Retrieve concept with broader, narrower, and related concepts.

**Parameters:**
- `action: "get_full"`
- `id` (required) - UUID of the concept

```json
{
  "action": "get_full",
  "id": "880e8400-e29b-41d4-a716-446655440000"
}
```

#### `stats` - Get governance statistics

Usage metrics for tag health monitoring.

**Parameters:**
- `action: "stats"`
- `scheme_id` (optional) - Restrict to specific scheme

```json
{
  "action": "stats",
  "scheme_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### `top` - Get top-level concepts in scheme

Root concepts without broader relations.

**Parameters:**
- `action: "top"`
- `scheme_id` (required) - UUID of the concept scheme

```json
{
  "action": "top",
  "scheme_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### `explore_graph`

Traverse the knowledge graph recursively from a starting note.

**Parameters:**
- `start_note_id` (required) - UUID of starting note
- `max_depth` (optional) - Maximum traversal depth (default: 3)
- `max_results` (optional) - Maximum total nodes (default: 50)
- `min_similarity` (optional) - Minimum link score (default: 0.70)

```json
{
  "start_note_id": "550e8400-e29b-41d4-a716-446655440000",
  "max_depth": 3,
  "max_results": 50,
  "min_similarity": 0.70
}
```

Returns a tree structure showing all connected notes within the specified depth, useful for discovering clusters of related knowledge.

### `get_note_links`

Get semantic connections for a specific note.

**Parameters:**
- `id` (required) - UUID of the note

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Returns:**
- `outgoing` - Notes this note links TO
- `incoming` - BACKLINKS - Notes that link TO this note

Backlinks are crucial for discovering how concepts connect in your knowledge graph.

### `export_note`

Export a note as markdown with YAML frontmatter.

**Parameters:**
- `id` (required) - UUID of the note

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

Returns markdown file with frontmatter including title, tags, created/updated timestamps, and full content.

### `get_documentation`

Access built-in documentation for AI agents.

**Parameters:**
- `topic` (required) - Documentation topic name

**Available topics:**
- `overview` - System overview and capabilities
- `search` - Search features and query syntax
- `tags` - SKOS hierarchical tagging
- `embedding-sets` - Focused search contexts
- `templates` - Note templates
- `collections` - Folder organization
- `revision-modes` - AI enhancement modes
- `versioning` - Version history
- `pke` - Public-key encryption
- `backup` - Backup and export
- `jobs` - Background job system
- `health` - Knowledge health metrics
- `multi-memory` - Parallel memory archives
- `document-types` - Content type detection
- `attachments` - File upload and management
- `provenance` - Spatial-temporal tracking
- `mcp-deployment` - MCP server deployment
- `api` - REST API reference
- `architecture` - System design

```json
{
  "topic": "search"
}
```

### `get_system_info`

Comprehensive system diagnostics including version, health status, configuration, statistics, and component health.

**Returns:**
- `version` - Fortémi version
- `status` - Overall health status
- `configuration` - Chunking, AI revision settings, enabled features
- `stats` - Note counts, embedding counts, job queue depth
- `components` - Database, inference, storage health
- `capabilities` - Enabled extraction strategies

```json
{}
```

### `health_check`

Simple health check indicating if the system is operational.

**Returns:**
- `status` - "healthy" or error state
- `version` - Fortémi version
- `components` - Component health summary

```json
{}
```

### `select_memory`

Set the active memory archive for all subsequent MCP operations in this session.

**Parameters:**
- `name` (required) - Memory archive name

```json
{
  "name": "work-notes"
}
```

All future operations (create_note, search, list_tags, etc.) will operate on the selected memory until changed or session ends.

### `get_active_memory`

Check which memory archive is currently active for this session.

**Returns:**
- `name` - Active memory name (null if default)

```json
{}
```

### `manage_attachments`

Manage file attachments on notes. Upload, list, get metadata, download, and delete. Image/audio/video attachments are automatically processed by the extraction pipeline.

**Parameters:**
- `action` (required) - "list", "upload", "get", "download", "delete"
- `note_id` - Note UUID (required for list/upload)
- `id` - Attachment UUID (required for get/download/delete)
- `filename` - Filename hint for upload curl command
- `content_type` - MIME type hint for upload
- `document_type_id` - Explicit document type UUID override

```json
{
  "action": "list",
  "note_id": "019c5e67-4261-7122-b1ec-88bede99ee92"
}
```

### `get_knowledge_health`

Get overall knowledge base health metrics and diagnostics.

**Returns:**
- `total_notes` - Total note count
- `orphan_tags` - Tags not used by any notes
- `stale_notes` - Notes not updated recently
- `unlinked_notes` - Notes with no semantic links
- `tag_cooccurrence` - Tag usage patterns
- `recommendations` - Suggested maintenance actions

```json
{}
```

### `bulk_reprocess_notes`

Re-run pipeline steps (embedding, linking, revision) on multiple notes.

**Parameters:**
- `note_ids` (required) - Array of note UUIDs (max 100)
- `steps` (optional) - Array of steps: ["embed", "link", "revise"] (default: all)

```json
{
  "note_ids": [
    "550e8400-e29b-41d4-a716-446655440000",
    "660e8400-e29b-41d4-a716-446655440001"
  ],
  "steps": ["embed", "link"]
}
```

## Memory-Scoped Operations

All MCP tools operate within the context of the **active memory**. Use `select_memory` to switch memories:

1. `select_memory({ name: "work-2026" })` - Sets active memory for the session
2. All subsequent tool calls operate on `work-2026`
3. `get_active_memory()` - Check which memory is active
4. Omitting `select_memory` = operations target the default memory

**Multi-memory tools** (not memory-scoped):
- Memory management actions in `search` tool (`federated` action)
- `select_memory`, `get_active_memory`

These operate on the global memory registry, not the active memory context.

## Common Workflows

### Capture and Organize Knowledge

```javascript
// Create a research note with AI enhancement
const note = await capture_knowledge({
  action: "create",
  content: "# Transformer Architecture\n\nKey innovation: self-attention...",
  tags: ["research", "ai", "transformers"],
  revision_mode: "light"
})

// Move to collection
await manage_collection({
  action: "move_note",
  note_id: note.id,
  collection_id: "research-collection-uuid"
})

// Tag with SKOS concept
await manage_tags({
  action: "tag_concept",
  note_id: note.id,
  concept_id: "machine-learning-concept-uuid"
})
```

### Search Across Types

```javascript
// Text search with strict filtering
const textResults = await search({
  action: "text",
  query: "neural networks",
  mode: "hybrid",
  strict_filter: {
    required_tags: ["research"],
    excluded_tags: ["draft"]
  }
})

// Spatial search
const spatialResults = await search({
  action: "spatial",
  latitude: 37.7749,
  longitude: -122.4194,
  radius_km: 5.0
})

// Federated search across all memories
const federatedResults = await search({
  action: "federated",
  query: "project documentation",
  memories: ["all"]
})
```

### Record Provenance Chain

```javascript
// Create named location
const location = await record_provenance({
  action: "named_location",
  name: "San Francisco Office",
  latitude: 37.7749,
  longitude: -122.4194,
  place_type: "office"
})

// Create device
const device = await record_provenance({
  action: "device",
  device_id: "macbook-2023",
  name: "Work Laptop",
  device_type: "laptop"
})

// Associate with note
await record_provenance({
  action: "note",
  note_id: "note-uuid",
  location_id: location.id,
  device_id: device.id
})
```

### Use Get Documentation for Advanced Features

```javascript
// Learn about embedding sets
const embeddingDocs = await get_documentation({
  topic: "embedding-sets"
})

// Learn about SKOS tagging
const skosDocs = await get_documentation({
  topic: "tags"
})

// Learn about multi-memory architecture
const memoryDocs = await get_documentation({
  topic: "multi-memory"
})
```

## Full Mode

Set `MCP_TOOL_MODE=full` environment variable to expose all 187 granular tools instead of the 23 core consolidated tools.

**When to use:**
- Programmatic access requiring precise endpoint control
- Legacy integrations expecting granular tool names
- Debugging or development scenarios

**Tradeoffs:**
- ~78% higher token overhead (187 vs 23 tools)
- Increased cognitive complexity for agents
- Slower agent decision-making due to larger tool surface

**Example configuration:**

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["./mcp-server/index.js"],
      "env": {
        "FORTEMI_URL": "http://localhost:3000",
        "MCP_TOOL_MODE": "full"
      }
    }
  }
}
```

## API-Only Features

The following features are available via REST API but not exposed in the core MCP tool surface. Use `MCP_TOOL_MODE=full` for MCP access, or call the REST API directly.

**Not in core MCP:**
- Note versioning (version history, diffs, restore)
- PKE encryption (keypair generation, encrypt/decrypt, keyset management)
- SKOS scheme administration (create/delete schemes, concept CRUD, relation management)
- SKOS collections (concept grouping)
- OAuth client management and token endpoints
- Embedding sets (create, update, delete, refresh, member management)
- Embedding configs (model configuration)
- Background jobs (create, list, queue stats)
- Document types (create, update, delete, detection)
- Backup/restore (full backup, knowledge shards, database snapshots)
- Cache management (invalidation, statistics)
- File attachments (list, download, delete after upload)

**Full API reference:** See [API Documentation](./api.md) and [OpenAPI Spec](https://github.com/fortemi/fortemi/blob/main/crates/matric-api/src/openapi.yaml)

## Related Documentation

- [API Reference](./api.md) - REST API documentation
- [Multi-Memory Guide](./multi-memory.md) - Parallel memory archives and federated search
- [Multi-Memory Agent Guide](./multi-memory-agent-guide.md) - Segmentation strategies for agents
- [SKOS Tags](./tags.md) - Hierarchical tagging system
- [Architecture](./architecture.md) - System design
- [Backup Guide](./backup.md) - Backup strategies
- [Real-Time Events](./real-time-events.md) - SSE, WebSocket, and webhook event streaming
- [Document Types Guide](./document-types-guide.md) - Content type detection and chunking
- [Embedding Model Selection](./embedding-model-selection.md) - Model selection guidance
- [MCP Deployment Guide](./mcp-deployment.md) - Deployment and security considerations

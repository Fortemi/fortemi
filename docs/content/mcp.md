# Fortémi MCP Server

Complete documentation for the Model Context Protocol (MCP) server that provides AI agent access to Fortémi.

## Overview

The MCP server enables AI assistants (Claude, etc.) to interact with your knowledge base through a standardized protocol. It provides **155 tools** organized into these categories:

| Category | Tools | Description |
|----------|-------|-------------|
| Notes | 13 | Create, read, update, delete, restore notes |
| Search | 2 | Hybrid semantic + full-text + strict filtering |
| Memory Search | 4 | Spatial/temporal memory search and provenance |
| Collections | 7 | Hierarchical folder organization |
| Templates | 6 | Reusable note structures |
| Document Types | 6 | Content type detection and management |
| Embedding Sets | 10 | Focused search contexts with full CRUD |
| Embedding Configs | 6 | Embedding model configuration |
| Jobs | 6 | Background processing control and monitoring |
| Backup/Export | 15 | Data portability and backups |
| Archives | 7 | Parallel memory archive management |
| SKOS Concepts | 24 | Hierarchical tagging with relation removal |
| SKOS Collections | 7 | Group concepts into ordered/unordered collections |
| Versioning | 5 | Note version history |
| PKE Encryption | 13 | Public-key encrypted note sharing |
| File Attachments | 5 | Upload, manage, and retrieve file attachments |
| Content Retrieval | 4 | Chunk-aware document handling |
| Knowledge Health | 7 | Knowledge base health metrics and diagnostics |
| Notes Timeline | 2 | Note timeline and activity feed |
| System | 4 | Health check and diagnostics |
| Export | 1 | SKOS Turtle RDF export |
| Documentation | 1 | Self-documentation system |

### Tool Categories by Permission

Tools are categorized by their effect on system state to help you understand permission requirements in restricted environments.

#### Read-Only Tools

These tools retrieve information without modifying system state:

**Search & Discovery:**
- `search_notes`, `list_tags`
- `explore_graph`, `get_note_links`
- `search_memories_by_location`, `search_memories_by_time`, `search_memories_combined`
- `get_memory_provenance`

**Retrieval:**
- `list_notes`, `get_note`
- `list_collections`, `get_collection`, `get_collection_notes`
- `list_templates`, `get_template`
- `list_embedding_sets`, `get_embedding_set`, `list_set_members`
- `list_embedding_configs`, `get_default_embedding_config`, `get_embedding_config`
- `list_document_types`, `get_document_type`
- `list_archives`, `get_archive`, `get_archive_stats`
- `list_attachments`, `get_attachment`, `download_attachment`
- `get_full_document`, `search_with_dedup`, `get_chunk_chain`

**SKOS Concepts:**
- `list_concept_schemes`, `get_concept_scheme`, `search_concepts`
- `get_concept`, `get_concept_full`, `autocomplete_concepts`
- `get_broader`, `get_narrower`, `get_related`
- `get_note_concepts`, `get_governance_stats`, `get_top_concepts`
- `list_skos_collections`, `get_skos_collection`

**Versioning:**
- `list_note_versions`, `get_note_version`, `diff_note_versions`

**Jobs & System:**
- `list_jobs`, `get_job`, `get_queue_stats`, `get_pending_jobs_count`
- `health_check`, `get_system_info`, `memory_info`

**Knowledge Health:**
- `get_knowledge_health`, `get_orphan_tags`, `get_stale_notes`
- `get_unlinked_notes`, `get_tag_cooccurrence`, `get_note_backlinks`, `get_note_provenance`

**Notes Timeline:**
- `get_notes_timeline`, `get_notes_activity`

**Export:**
- `export_note`, `export_all_notes`, `export_skos_turtle`
- `backup_status`, `backup_download`
- `list_backups`, `get_backup_info`, `get_backup_metadata`
- `knowledge_archive_download`

**Documentation:**
- `get_documentation`

#### Mutating Tools

These tools modify system state and may require elevated permissions:

**Note Operations:**
- `create_note`, `bulk_create_notes`, `update_note`
- `delete_note`, `restore_note`, `purge_note`, `purge_notes`, `purge_all_notes`
- `set_note_tags`
- `instantiate_template`

**Collections:**
- `create_collection`, `update_collection`, `delete_collection`
- `move_note_to_collection`

**Templates:**
- `create_template`, `update_template`, `delete_template`

**Embedding Sets:**
- `create_embedding_set`, `update_embedding_set`, `delete_embedding_set`
- `add_set_members`, `remove_set_member`, `refresh_embedding_set`, `reembed_all`

**Embedding Configs:**
- `create_embedding_config`, `update_embedding_config`, `delete_embedding_config`

**Document Types:**
- `create_document_type`, `update_document_type`, `delete_document_type`
- `detect_document_type` (read-only but may trigger auto-configuration)

**Archives:**
- `create_archive`, `update_archive`, `delete_archive`, `set_default_archive`

**File Attachments:**
- `upload_attachment`, `delete_attachment`

**SKOS Concepts:**
- `create_concept_scheme`, `delete_concept_scheme`
- `create_concept`, `update_concept`, `delete_concept`
- `add_broader`, `remove_broader`
- `add_narrower`, `remove_narrower`
- `add_related`, `remove_related`
- `tag_note_concept`, `untag_note_concept`

**SKOS Collections:**
- `create_skos_collection`, `update_skos_collection`, `delete_skos_collection`
- `add_skos_collection_member`, `remove_skos_collection_member`

**Versioning:**
- `restore_note_version`, `delete_note_version`

**Jobs:**
- `create_job`, `reprocess_note`

**Backup & Import:**
- `backup_now`, `backup_import`, `update_backup_metadata`
- `knowledge_shard`, `knowledge_shard_import`
- `database_snapshot`, `database_restore`
- `knowledge_archive_upload`

**Note about permissions:** Some MCP client environments (e.g., Claude Desktop) may restrict or prompt for user approval when using mutating tools. Read-only tools typically require no special permissions.

### Knowledge Graph

Fortémi automatically builds a semantic knowledge graph by analyzing relationships between notes through their embeddings.

#### How Automatic Linking Works

When you create or update a note with AI enhancement enabled:

1. **Embedding generation** - Content is converted to vector embeddings using the configured model (default: `mxbai-embed-large`)
2. **Similarity calculation** - The system compares the new note's embedding against all existing notes using cosine similarity
3. **Link creation** - Notes exceeding the similarity threshold (default: 70% or 0.70) are automatically linked
4. **Bidirectional backlinks** - Links are created in both directions, so each note knows what it links to AND what links to it

**Similarity threshold**: The 70% threshold balances precision and recall. Lower thresholds create more connections but may include less relevant links. Higher thresholds create fewer, more precise connections.

#### Exploring the Knowledge Graph

Use these tools to navigate the semantic relationships:

**`get_note_links`** - Get direct connections for a specific note:

```javascript
const links = await get_note_links({ id: noteId })

// Outgoing links: notes this note references
links.outgoing.forEach(link => {
  console.log(`Links to: ${link.to_note_id} (similarity: ${link.score})`)
})

// Incoming links (backlinks): notes that reference this note
links.incoming.forEach(link => {
  console.log(`Referenced by: ${link.from_note_id} (similarity: ${link.score})`)
})
```

**`explore_graph`** - Traverse the knowledge graph recursively:

```javascript
// Explore 3 levels deep from a starting note
const graph = await explore_graph({
  start_note_id: noteId,
  max_depth: 3,
  max_results: 50,
  min_similarity: 0.70
})

// Returns a tree structure showing all connected notes
// Useful for discovering clusters of related knowledge
```

#### Relationship Between Embeddings and Links

- **Embeddings** are the foundation - they encode semantic meaning as vectors
- **Links** are derived from embeddings - they represent high-similarity relationships
- **Embedding sets** control link scope - notes in different sets won't link to each other
- **Re-embedding** updates links - when you refresh embeddings, links are recalculated

**Best practices:**

1. Use `get_note_links` to understand how concepts connect in your knowledge base
2. Explore backlinks to discover unexpected relationships and knowledge clusters
3. Use `explore_graph` for broader discovery when researching interconnected topics
4. Adjust similarity thresholds per embedding set for domain-specific requirements

### Storage & Capacity Planning

The `memory_info` tool provides comprehensive storage and capacity metrics for planning system resources.

#### Storage Metrics

**Database usage breakdown:**

```javascript
const info = await memory_info()

console.log(`Total notes: ${info.total_notes}`)
console.log(`Total embeddings: ${info.total_embeddings}`)
console.log(`Total links: ${info.total_links}`)
console.log(`Total collections: ${info.total_collections}`)
console.log(`Total tags: ${info.total_tags}`)
console.log(`Total templates: ${info.total_templates}`)

// Storage breakdown
console.log(`Database total: ${info.database_total_bytes / 1e9} GB`)
console.log(`Embeddings table: ${info.embedding_table_bytes / 1e9} GB`)
console.log(`Notes table: ${info.notes_table_bytes / 1e9} GB`)
```

**Memory requirements:**

```javascript
// RAM needed for search operations (approximate)
console.log(`Estimated search memory: ${info.estimated_memory_for_search / 1e9} GB`)
console.log(`Minimum RAM: ${info.min_ram_gb} GB`)
console.log(`Recommended RAM: ${info.recommended_ram_gb} GB`)
```

#### Hardware Recommendations

**Minimum configuration:**
- **RAM**: 4 GB for small deployments (< 10,000 notes)
- **Storage**: 20 GB SSD for database and embeddings
- **CPU**: 2 cores for API and background jobs

**Recommended configuration:**
- **RAM**: 8-16 GB for production (10,000-100,000 notes)
- **Storage**: 100 GB SSD with PostgreSQL on dedicated disk
- **CPU**: 4-8 cores for concurrent operations
- **GPU**: Optional, for faster embedding generation

#### GPU vs CPU Considerations

**Ollama for embeddings:**
- **CPU mode**: Works on any system, slower embedding generation (1-2 notes/second)
- **GPU mode**: Requires NVIDIA GPU with CUDA, 10-50x faster (20-100 notes/second)
- **Recommendation**: Use GPU for bulk imports or large knowledge bases (> 10,000 notes)

**pgvector for search:**
- Always runs on CPU (PostgreSQL does not use GPU)
- Search speed scales with RAM (larger working set fits in memory)
- HNSW indexes provide sub-millisecond search even on CPU
- **Recommendation**: Prioritize RAM and fast SSD over GPU for search performance

**Capacity planning formula:**

```
Embeddings storage ≈ total_notes × chunks_per_note × embedding_dimensions × 4 bytes
Default: N notes × 3 chunks × 1024 dims × 4 bytes ≈ N × 12 KB

Example: 50,000 notes ≈ 600 MB embeddings
```

**When to scale:**
- **Add RAM**: Search becomes slow, high disk I/O
- **Add storage**: Database exceeds 80% capacity
- **Add GPU**: Embedding jobs take > 1 hour to process backlog
- **Optimize indexes**: Search queries exceed 100ms consistently

## Quick Start

### Installation

```bash
# Clone repository
git clone https://github.com/Fortemi/fortemi

# Install MCP server dependencies
cd mcp-server
npm install
```

### Configuration

Set environment variables:

```bash
# Required: API endpoint
export MATRIC_MEMORY_URL="http://localhost:3000"

# Optional: API key for stdio mode
export MATRIC_MEMORY_API_KEY="your-api-key"

# Transport mode: "stdio" (default) or "http"
export MCP_TRANSPORT="stdio"

# HTTP mode settings (if MCP_TRANSPORT=http)
export MCP_PORT="3001"
export MCP_BASE_URL="http://localhost:3001"
```

### Running

```bash
# Stdio mode (for Claude Desktop, etc.)
node index.js

# HTTP mode (for web integrations)
MCP_TRANSPORT=http node index.js
```

### Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "fortemi": {
      "command": "node",
      "args": ["/path/to/fortemi/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "http://localhost:3000",
        "MATRIC_MEMORY_API_KEY": "your-api-key"
      }
    }
  }
}
```

---

## Tool Categories

### Notes (Core Operations)

The primary tools for knowledge management.

#### `create_note` ⭐ Most Important

Creates a note with **full AI enhancement pipeline**:

1. **AI Revision** - Enhances content using context from related notes
2. **Embedding** - Generates vectors for semantic search
3. **Title Generation** - Creates descriptive title
4. **Linking** - Creates bidirectional semantic links

**Revision Modes:**

| Mode | Use When | Behavior |
|------|----------|----------|
| `full` (default) | Technical concepts, research | Full contextual expansion with related notes |
| `light` | Facts, opinions, quick thoughts | Formatting only, no invented details |
| `none` | Exact quotes, citations, raw data | No AI processing |

```json
{
  "content": "# Your note content in markdown",
  "tags": ["optional", "tags"],
  "revision_mode": "full",
  "collection_id": "optional-collection-uuid"
}
```

**Optional Parameters:**
- `collection_id` - Assign note directly to a collection on creation

#### `search_notes`

Hybrid search combining full-text and semantic similarity.

**Search Modes:**

| Mode | Best For |
|------|----------|
| `hybrid` (default) | General search - combines keyword + semantic |
| `fts` | Exact keyword matching |
| `semantic` | Finding conceptually related content |

**Embedding Sets:** Use `set` parameter to restrict search to specific contexts (e.g., "work-projects", "research-papers").

**Collection Filtering:** Use `collection_id` parameter to restrict search to a specific collection:

```json
{
  "query": "authentication",
  "collection_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Strict Filtering:** Use `strict_filter` parameter for guaranteed tag-based isolation:

```json
{
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

**Use Cases:**

- **Client isolation**: `required_schemes: ["client-acme"]`
- **Project + priority**: `required_tags: ["project:x"], any_tags: ["high", "critical"]`
- **Exclude drafts**: `excluded_tags: ["draft", "wip"]`

#### `get_note_links`

Get semantic connections for a note. Returns:

- `outgoing`: Notes this note links TO
- `incoming`: **BACKLINKS** - Notes that link TO this note

Backlinks are crucial for discovering how concepts connect in your knowledge graph.

#### `restore_note`

Restore a soft-deleted note. Recovers the note with all original metadata, tags, and content.

```javascript
restore_note({
  id: "deleted-note-uuid"
})
```

### Collections (Organization)

Hierarchical folder structure for notes.

| Tool | Purpose |
|------|---------|
| `list_collections` | List folders (use `parent_id` for children) |
| `create_collection` | Create folder (set `parent_id` for nesting) |
| `get_collection` | Get folder details |
| `update_collection` | Update name, description, or parent |
| `delete_collection` | Delete folder (soft delete) |
| `get_collection_notes` | List notes in a folder |
| `move_note_to_collection` | Move note to folder |

#### `update_collection`

Rename collections, add descriptions, or reorganize hierarchy by changing parent.

```javascript
update_collection({
  id: "collection-uuid",
  name: "New Name",
  description: "Updated description",
  parent_id: "new-parent-uuid"  // or null to move to root
})
```

### Embedding Sets (Focused Contexts)

Create focused search contexts for specific domains.

**Use Cases:**

- "work-projects" - Only search work-related notes
- "research-ai" - AI/ML research papers only
- "personal-journal" - Personal reflections

| Tool | Purpose |
|------|---------|
| `list_embedding_sets` | List all embedding sets |
| `get_embedding_set` | Get set details |
| `create_embedding_set` | Create new set with criteria |
| `update_embedding_set` | Update set metadata/criteria |
| `delete_embedding_set` | Delete set (not default) |
| `list_set_members` | List notes in set |
| `add_set_members` | Add notes to a set |
| `remove_set_member` | Remove note from set |
| `refresh_embedding_set` | Regenerate set embeddings |
| `reembed_all` | Re-embed all notes in set |

#### `update_embedding_set`

Modify name, description, purpose, usage hints, keywords, criteria, or mode. Changing criteria or mode triggers a background refresh job.

```javascript
update_embedding_set({
  slug: "work-projects",
  name: "Work Projects 2026",
  description: "Active work projects",
  mode: "auto",  // auto, manual, or mixed
  criteria: { required_tags: ["work", "active"] }
})
```

#### `delete_embedding_set`

Delete an embedding set. The default set cannot be deleted. Notes remain in the database; only the embedding set index is removed.

```javascript
delete_embedding_set({
  slug: "old-project"  // cannot be "default"
})
```

### SKOS Concepts (Hierarchical Tags)

Full W3C SKOS-compliant hierarchical tagging. See [tags.md](./tags.md) for details.

**Key Tools:**

| Tool | Purpose |
|------|---------|
| `list_concept_schemes` | List vocabularies |
| `create_concept_scheme` | Create vocabulary |
| `get_concept_scheme` | Get scheme details |
| `delete_concept_scheme` | Delete scheme (with force option) |
| `search_concepts` | Find existing concepts |
| `create_concept` | Add new concept with relations |
| `add_broader` / `add_narrower` | Build hierarchy |
| `remove_broader` / `remove_narrower` | Remove hierarchy relations |
| `add_related` / `remove_related` | Manage related relations |
| `tag_note_concept` | Tag note with concept |
| `get_governance_stats` | Tag health metrics |

**Concept Status:**

- `candidate` - Auto-created, needs review
- `controlled` - Approved for use
- `deprecated` - Replaced, don't use

#### `delete_concept_scheme`

Delete a concept scheme. If the scheme has concepts, use `force=true` to delete them as well. System and default schemes are protected.

```javascript
delete_concept_scheme({
  id: "scheme-uuid",
  force: true  // required if scheme has concepts
})
```

#### SKOS Relation Removal

Remove hierarchical or associative relationships between concepts:

```javascript
// Remove parent relationship
remove_broader({ concept_id: "uuid", broader_id: "parent-uuid" })

// Remove child relationship
remove_narrower({ concept_id: "uuid", narrower_id: "child-uuid" })

// Remove related relationship
remove_related({ concept_id: "uuid", related_id: "related-uuid" })
```

### SKOS Collections

Group concepts into ordered or unordered collections for navigation and organization.

| Tool | Purpose |
|------|---------|
| `list_skos_collections` | List concept collections |
| `create_skos_collection` | Create concept collection |
| `get_skos_collection` | Get collection details |
| `update_skos_collection` | Update collection |
| `delete_skos_collection` | Delete collection |
| `add_skos_collection_member` | Add concept to collection |
| `remove_skos_collection_member` | Remove concept from collection |

### Note Versioning

Dual-track versioning preserves both original and AI-enhanced content.

| Tool | Purpose |
|------|---------|
| `list_note_versions` | Get version history |
| `get_note_version` | Retrieve specific version |
| `restore_note_version` | Restore to previous version |
| `diff_note_versions` | Compare two versions |
| `delete_note_version` | Delete specific version |

### Backup & Export

Comprehensive data portability.

**Quick Export:**

- `export_note` - Single note as markdown
- `export_all_notes` - All notes as JSON
- `backup_now` - Trigger full backup

**Knowledge Shards:**

Self-contained archives with notes, embeddings, links, and metadata.

- `knowledge_shard` - Create compressed archive
- `knowledge_shard_import` - Restore from archive

### File Attachments

Upload and manage files attached to notes with full metadata tracking.

| Tool | Description |
|------|-------------|
| `upload_attachment` | Upload file to note with automatic metadata extraction |
| `list_attachments` | List all attachments for a note |
| `get_attachment` | Get attachment metadata |
| `download_attachment` | Download attachment binary content |
| `delete_attachment` | Remove attachment permanently |

#### `upload_attachment`

Upload a file to an existing note. Files are stored with content hashing and automatic EXIF extraction for images.

**Parameters:**
- `note_id` (required) - UUID of the note to attach the file to
- `filename` (required) - Original filename with extension
- `content_base64` (required) - Base64-encoded file content
- `content_type` (optional) - MIME type (e.g., "image/jpeg", "application/pdf")

**Example:**

```javascript
upload_attachment({
  note_id: "550e8400-e29b-41d4-a716-446655440000",
  filename: "vacation-photo.jpg",
  content_base64: "/9j/4AAQSkZJRg...",
  content_type: "image/jpeg"
})
```

#### `list_attachments`

List all attachments for a specific note.

**Parameters:**
- `note_id` (required) - UUID of the note

**Returns:** Array of attachments with metadata (ID, filename, size, content type, created timestamp).

**Example:**

```javascript
list_attachments({
  note_id: "550e8400-e29b-41d4-a716-446655440000"
})
// Returns: [
//   {
//     id: "660e8400-e29b-41d4-a716-446655440001",
//     filename: "vacation-photo.jpg",
//     size: 2048576,
//     content_type: "image/jpeg",
//     created_at: "2026-02-02T10:30:00Z"
//   }
// ]
```

#### `get_attachment`

Get attachment metadata including ID, filename, size, content type, and timestamps.

**Parameters:**
- `attachment_id` (required) - UUID of the attachment

**Returns:** Object with attachment metadata.

**Example:**

```javascript
get_attachment({
  attachment_id: "660e8400-e29b-41d4-a716-446655440001"
})
// Returns: {
//   id: "660e8400-e29b-41d4-a716-446655440001",
//   filename: "vacation-photo.jpg",
//   size: 2048576,
//   content_type: "image/jpeg",
//   created_at: "2026-02-02T10:30:00Z"
// }
```

#### `download_attachment`

Download the binary content of an attachment as base64.

**Parameters:**
- `attachment_id` (required) - UUID of the attachment

**Returns:** Object with `content_base64` (file content) and `content_type` (MIME type).

**Example:**

```javascript
download_attachment({
  attachment_id: "660e8400-e29b-41d4-a716-446655440001"
})
// Returns: {
//   content_base64: "/9j/4AAQSkZJRg...",
//   content_type: "image/jpeg"
// }
```

#### `delete_attachment`

Remove an attachment from a note. The file is permanently deleted.

**Parameters:**
- `attachment_id` (required) - UUID of the attachment

**Example:**

```javascript
delete_attachment({
  attachment_id: "660e8400-e29b-41d4-a716-446655440001"
})
```

### Content Retrieval

Chunk-aware document handling for reconstructing and navigating chunked content.

| Tool | Description |
|------|-------------|
| `get_full_document` | Reconstruct chunked document |
| `search_with_dedup` | Search with chunk deduplication |
| `get_chunk_chain` | Get all chunks in document chain |

### Knowledge Health

Diagnostics and health metrics for your knowledge base.

| Tool | Description |
|------|-------------|
| `get_knowledge_health` | Overall knowledge base health metrics |
| `get_orphan_tags` | Tags not used by any notes |
| `get_stale_notes` | Notes not updated recently |
| `get_unlinked_notes` | Notes with no semantic links |
| `get_tag_cooccurrence` | Tag co-occurrence statistics |
| `get_note_backlinks` | Backlinks for a specific note |
| `get_note_provenance` | Provenance chain for a note |

### Notes Timeline

Timeline and activity views for note history.

| Tool | Description |
|------|-------------|
| `get_notes_timeline` | Timeline view of notes |
| `get_notes_activity` | Activity feed for notes |

### Documentation

Access built-in documentation for AI agents.

| Tool | Description |
|------|-------------|
| `get_documentation` | Get built-in documentation by topic |

The `get_documentation` tool provides access to 19 documentation topics including overview, search, tags, embedding sets, templates, and more. Useful for AI agents to learn about system capabilities.

---

## Usage Considerations for AI Agents

### Best Practices

1. **Search Before Create**
   ```
   Always search_notes before creating to avoid duplicates.
   Search with mode="semantic" to find conceptually similar content.
   ```

2. **Use Appropriate Revision Mode**
   ```
   - Full: Technical content, concepts, research
   - Light: Facts, opinions, personal notes
   - None: Exact quotes, citations, data imports
   ```

3. **Respect the Pipeline**
   ```
   After create_note or update_note:
   - Wait for jobs to complete before searching (embeddings need time)
   - Use list_jobs(note_id="...") to monitor progress
   ```

4. **Leverage Semantic Links**
   ```
   - Use get_note_links to discover related content
   - Explore backlinks to understand how concepts connect
   - Use explore_graph for broader knowledge discovery
   ```

5. **Use Embedding Sets Strategically**
   ```
   - Create sets for focused domains
   - Search within sets to reduce noise
   - Refresh sets after adding many notes
   ```

6. **Attach Files with Context**
   ```
   - Upload photos/documents to relevant notes
   - Use attachment tools to manage file lifecycle
   ```

### Rate Limiting

The API implements rate limiting:

- **Standard tier**: 100 requests/minute
- **Burst**: 200 requests/minute (short bursts allowed)

Check `X-RateLimit-*` headers in responses.

### Error Handling

Common error responses:

| Status | Meaning | Action |
|--------|---------|--------|
| 400 | Bad request | Check parameters |
| 401 | Unauthorized | Check API key |
| 404 | Not found | Verify UUID exists |
| 429 | Rate limited | Wait and retry |
| 500 | Server error | Report to admin |

### Performance Tips

1. **Batch Operations**
   - Use `bulk_create_notes` for multiple notes (max 100)
   - Group related operations to minimize roundtrips

2. **Pagination**
   - Always use `limit` and `offset` for large results
   - Default limit is 50, max is 1000

3. **Selective Fetching**
   - Use `list_notes` for summaries
   - Use `get_note` only when full content needed

---

## Complete Tool Reference

### Notes

| Tool | Description |
|------|-------------|
| `list_notes` | List notes with filtering, pagination, and optional `collection_id` filter |
| `get_note` | Get full note details |
| `create_note` | Create note with AI pipeline, supports `collection_id` parameter |
| `bulk_create_notes` | Batch create (max 100) |
| `update_note` | Update content or status |
| `delete_note` | Soft delete (recoverable) |
| `restore_note` | Restore soft-deleted note |
| `purge_note` | Permanent delete |
| `purge_notes` | Batch permanent delete |
| `purge_all_notes` | Delete everything (requires confirm) |
| `set_note_tags` | Replace user tags |
| `get_note_links` | Get semantic links/backlinks |
| `export_note` | Export as markdown |

### Search

| Tool | Description |
|------|-------------|
| `search_notes` | Hybrid/FTS/semantic search with optional `collection_id` and strict filtering |
| `list_tags` | List all tags with counts |

### Memory Search

| Tool | Description |
|------|-------------|
| `search_memories_by_location` | Find memories near geographic coordinates (PostGIS spatial) |
| `search_memories_by_time` | Find memories within a time range (temporal) |
| `search_memories_combined` | Find memories by location AND time |
| `get_memory_provenance` | Get file provenance chain for a note's attachments |

### Collections

| Tool | Description |
|------|-------------|
| `list_collections` | List folders |
| `create_collection` | Create folder |
| `get_collection` | Get folder details |
| `update_collection` | Update name, description, or parent |
| `delete_collection` | Delete folder (soft delete) |
| `get_collection_notes` | List notes in folder |
| `move_note_to_collection` | Move note |

### Templates

| Tool | Description |
|------|-------------|
| `list_templates` | List all templates |
| `create_template` | Create with {{variables}} |
| `get_template` | Get template details |
| `update_template` | Update template content and settings |
| `delete_template` | Delete template |
| `instantiate_template` | Create note from template |

#### `update_template`

Modify template name, description, content, format, default tags, or default collection.

```javascript
update_template({
  id: "template-uuid",
  name: "Updated Template",
  content: "# {{title}}\n\nNew content...",
  default_tags: ["template", "updated"]
})
```

### Embedding Sets

| Tool | Description |
|------|-------------|
| `list_embedding_sets` | List all sets |
| `get_embedding_set` | Get set details |
| `create_embedding_set` | Create new set |
| `update_embedding_set` | Update set metadata/criteria |
| `delete_embedding_set` | Delete set (not default) |
| `list_set_members` | List notes in set |
| `add_set_members` | Add notes to set |
| `remove_set_member` | Remove note from set |
| `refresh_embedding_set` | Regenerate embeddings |
| `reembed_all` | Re-embed all notes in set |

### Embedding Configs

| Tool | Description |
|------|-------------|
| `list_embedding_configs` | List all embedding configurations |
| `get_default_embedding_config` | Get the default embedding configuration |
| `get_embedding_config` | Get specific embedding config by ID |
| `create_embedding_config` | Create new embedding configuration |
| `update_embedding_config` | Update embedding configuration |
| `delete_embedding_config` | Delete embedding configuration |

### Document Types

Document types control how content is detected, chunked, and embedded. Fortémi includes 131 pre-configured types across 20 categories.

| Tool | Description |
|------|-------------|
| `list_document_types` | List all document types with optional category filter |
| `get_document_type` | Get details for a specific document type |
| `create_document_type` | Create a custom document type |
| `update_document_type` | Update a custom document type |
| `delete_document_type` | Delete a custom document type |
| `detect_document_type` | Auto-detect type from filename/content |

**Categories:** prose, code, config, markup, data, api-spec, iac, database, shell, docs, package, observability, legal, communication, research, creative, media, personal, agentic, custom

**Example - Auto-detect type:**

```javascript
detect_document_type({
  filename: "docker-compose.yml",
  content: "version: '3.8'\nservices:"
})
// Returns: { detected_type: "docker-compose", confidence: 0.9, category: "iac" }
```

**Example - Create custom type:**

```javascript
create_document_type({
  name: "meeting-notes",
  display_name: "Meeting Notes",
  category: "communication",
  chunking_strategy: "per_section",
  file_extensions: [".meeting.md"],
  filename_patterns: ["*-meeting-*.md"],
  magic_patterns: ["## Attendees", "## Action Items"]
})
```

See [Document Types Guide](./document-types-guide.md) for best practices.

### File Attachments

| Tool | Description |
|------|-------------|
| `upload_attachment` | Upload file to note with automatic metadata extraction |
| `list_attachments` | List all attachments for a note |
| `get_attachment` | Get attachment metadata |
| `download_attachment` | Download attachment binary content |
| `delete_attachment` | Remove attachment permanently |

### Content Retrieval

| Tool | Description |
|------|-------------|
| `get_full_document` | Reconstruct chunked document |
| `search_with_dedup` | Search with chunk deduplication |
| `get_chunk_chain` | Get all chunks in document chain |
| `get_documentation` | Get built-in documentation by topic |

### Knowledge Health

| Tool | Description |
|------|-------------|
| `get_knowledge_health` | Overall knowledge base health metrics |
| `get_orphan_tags` | Tags not used by any notes |
| `get_stale_notes` | Notes not updated recently |
| `get_unlinked_notes` | Notes with no semantic links |
| `get_tag_cooccurrence` | Tag co-occurrence statistics |
| `get_note_backlinks` | Backlinks for a specific note |
| `get_note_provenance` | Provenance chain for a note |

### Notes Timeline

| Tool | Description |
|------|-------------|
| `get_notes_timeline` | Timeline view of notes |
| `get_notes_activity` | Activity feed for notes |

### Jobs

| Tool | Description |
|------|-------------|
| `create_job` | Queue single processing step |
| `list_jobs` | List/filter jobs |
| `get_job` | Get job details by ID |
| `get_queue_stats` | Queue health summary |
| `get_pending_jobs_count` | Count of pending jobs |
| `reprocess_note` | Re-run pipeline steps on a note |

### Archives

| Tool | Description |
|------|-------------|
| `list_archives` | List all archives |
| `create_archive` | Create new archive |
| `get_archive` | Get archive details |
| `update_archive` | Update archive metadata |
| `delete_archive` | Delete archive |
| `set_default_archive` | Set the default archive |
| `get_archive_stats` | Get archive statistics |

#### Archive Workflow

Archives provide complete isolation for different knowledge domains:

```javascript
// Create a work archive
create_archive({
  name: "work-2026",
  description: "Work-related notes for 2026"
})

// Set as default for new notes
set_default_archive({ name: "work-2026" })

// List all archives
list_archives()
// Returns: [{ name: "default", is_default: false, note_count: 150 }, ...]
```

### Backup & Export

| Tool | Description |
|------|-------------|
| `export_all_notes` | Export notes as JSON |
| `backup_now` | Trigger backup |
| `backup_status` | Check backup status |
| `backup_download` | Download backup file |
| `backup_import` | Import backup data |
| `knowledge_shard` | Create full archive |
| `knowledge_shard_import` | Import archive |
| `database_snapshot` | Create DB snapshot |
| `database_restore` | Restore from snapshot |
| `knowledge_archive_download` | Download .archive file |
| `knowledge_archive_upload` | Upload .archive file |
| `list_backups` | List backup files |
| `get_backup_info` | Get backup details |
| `get_backup_metadata` | Get backup metadata |
| `update_backup_metadata` | Update metadata |

### SKOS Concepts

| Tool | Description |
|------|-------------|
| `list_concept_schemes` | List vocabularies |
| `create_concept_scheme` | Create vocabulary |
| `get_concept_scheme` | Get scheme details |
| `delete_concept_scheme` | Delete scheme (with force option) |
| `search_concepts` | Search concepts |
| `create_concept` | Create concept |
| `get_concept` | Get concept details |
| `get_concept_full` | Get with all relations |
| `update_concept` | Update concept |
| `delete_concept` | Delete unused concept |
| `autocomplete_concepts` | Type-ahead search |
| `get_broader` | Get parent concepts |
| `add_broader` | Add parent relation |
| `remove_broader` | Remove parent relation |
| `get_narrower` | Get child concepts |
| `add_narrower` | Add child relation |
| `remove_narrower` | Remove child relation |
| `get_related` | Get related concepts |
| `add_related` | Add related relation |
| `remove_related` | Remove related relation |
| `tag_note_concept` | Tag note |
| `untag_note_concept` | Remove tag |
| `get_note_concepts` | Get note's concepts |
| `get_governance_stats` | Usage statistics |
| `get_top_concepts` | Root concepts in scheme |

### SKOS Collections

| Tool | Description |
|------|-------------|
| `list_skos_collections` | List concept collections |
| `create_skos_collection` | Create concept collection |
| `get_skos_collection` | Get collection details |
| `update_skos_collection` | Update collection |
| `delete_skos_collection` | Delete collection |
| `add_skos_collection_member` | Add concept to collection |
| `remove_skos_collection_member` | Remove concept from collection |

### Versioning

| Tool | Description |
|------|-------------|
| `list_note_versions` | Version history |
| `get_note_version` | Get specific version |
| `restore_note_version` | Restore version |
| `delete_note_version` | Delete version |
| `diff_note_versions` | Compare versions |

### PKE Encryption

| Tool | Description |
|------|-------------|
| `pke_generate_keypair` | Generate new keypair |
| `pke_get_address` | Get public key address |
| `pke_encrypt` | Encrypt note for recipients |
| `pke_decrypt` | Decrypt note |
| `pke_list_recipients` | List note recipients |
| `pke_verify_address` | Verify address format |
| `pke_list_keysets` | List all keysets |
| `pke_create_keyset` | Create new keyset |
| `pke_get_active_keyset` | Get active keyset |
| `pke_set_active_keyset` | Set active keyset |
| `pke_export_keyset` | Export keyset |
| `pke_import_keyset` | Import keyset |
| `pke_delete_keyset` | Delete keyset |

### System

| Tool | Description |
|------|-------------|
| `memory_info` | Storage/memory statistics |
| `explore_graph` | Knowledge graph traversal |
| `health_check` | System health status |
| `get_system_info` | Comprehensive diagnostics |

#### `health_check`

Simple health check indicating if the system is operational.

```javascript
health_check()
// Returns: { status: "healthy", version: "2026.2.0", components: {...} }
```

#### `get_system_info`

Comprehensive system diagnostics including:
- Version and health status
- Configuration (chunking, AI revision settings)
- Statistics (note counts, embedding counts, job queue)
- Component health

```javascript
get_system_info()
// Returns: {
//   version: "2026.2.0",
//   status: "healthy",
//   configuration: { chunking: {...}, ai_revision: { enabled: true } },
//   stats: { total_notes: 1500, total_embeddings: 45000, pending_jobs: 3 },
//   components: { database: "healthy", inference: "healthy" }
// }
```

### Export

| Tool | Description |
|------|-------------|
| `export_skos_turtle` | Export SKOS taxonomy as W3C RDF/Turtle |

---

## Examples

### Creating a Research Note

```javascript
// Create with full AI enhancement
create_note({
  content: `# Research: Transformer Architecture

The transformer architecture revolutionized NLP through self-attention mechanisms.

## Key Components
- Multi-head attention
- Position encoding
- Feed-forward networks

## Applications
- GPT models
- BERT
- Translation systems`,
  tags: ["research", "ai", "transformers"],
  revision_mode: "full"
})
```

### Building a Concept Hierarchy

```javascript
// Create parent concept
const ai = await create_concept({
  scheme_id: "main",
  pref_label: "Artificial Intelligence",
  definition: "Computer systems performing tasks requiring human intelligence"
})

// Create child concept
await create_concept({
  scheme_id: "main",
  pref_label: "Machine Learning",
  broader_ids: [ai.id],
  definition: "AI systems that learn from data"
})
```

### Exploring Related Knowledge

```javascript
// Start from a note about neural networks
const links = await get_note_links({ id: noteId })

// Explore outgoing links (what this note references)
for (const link of links.outgoing) {
  const related = await get_note({ id: link.to_note_id })
  console.log(`Related: ${related.title} (score: ${link.score})`)
}

// Explore backlinks (what references this note)
for (const link of links.incoming) {
  const referrer = await get_note({ id: link.from_note_id })
  console.log(`Referenced by: ${referrer.title}`)
}
```

### Uploading Files to Notes

```javascript
// Create a documentation note
const note = await create_note({
  content: "# Project Documentation\n\nArchitecture diagrams and specifications.",
  tags: ["documentation", "project"],
  revision_mode: "light"
})

// Upload architecture diagram
const attachment = await upload_attachment({
  note_id: note.id,
  filename: "architecture.png",
  content_base64: readFileAsBase64("architecture.png"),
  content_type: "image/png"
})

// Get attachment metadata
const metadata = await get_attachment({
  attachment_id: attachment.id
})
console.log(`Uploaded: ${metadata.filename} (${metadata.size} bytes)`)
```

---

## Troubleshooting

### "API error 401"

- Check `MATRIC_MEMORY_API_KEY` is set correctly
- Verify the key hasn't expired
- Ensure the key has appropriate permissions

### "Note not found" after create

- Pipeline jobs are asynchronous
- Wait for jobs to complete: `list_jobs({ note_id: "..." })`
- The note exists, but embedding/links may still be processing

### Search returns unexpected results

- Check search mode (`hybrid` vs `fts` vs `semantic`)
- Verify embedding set if using `set` parameter
- Wait for embedding jobs to complete after recent updates

### Slow responses

- Use pagination (`limit`, `offset`)
- Check queue stats - many pending jobs slow the system
- Consider batch operations for multiple items

---

## Related Documentation

- [API Reference](./api.md) - REST API documentation
- [SKOS Tags](./tags.md) - Hierarchical tagging system
- [Architecture](./architecture.md) - System design
- [Backup Guide](./backup.md) - Backup strategies
- [Real-Time Events](./real-time-events.md) - SSE, WebSocket, and webhook event streaming

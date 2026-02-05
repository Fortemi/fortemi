# Fortémi MCP Server

Complete documentation for the Model Context Protocol (MCP) server that provides AI agent access to Fortémi.

## Overview

The MCP server enables AI assistants (Claude, etc.) to interact with your knowledge base through a standardized protocol. It provides **122 tools** organized into these categories:

| Category | Tools | Description |
|----------|-------|-------------|
| Notes | 13 | Create, read, update, delete, restore notes |
| Search | 3 | Hybrid semantic + full-text + strict filtering |
| Collections | 7 | Hierarchical folder organization with update support |
| Templates | 6 | Reusable note structures with update support |
| Document Types | 6 | Content type detection and management |
| Embedding Sets | 9 | Focused search contexts with full CRUD |
| Embedding Configs | 2 | Embedding model configuration |
| Jobs | 3 | Background processing control |
| Backup/Export | 15 | Data portability and backups |
| Archives | 5 | Parallel memory archive management |
| SKOS Concepts | 24 | Hierarchical tagging with relation removal |
| Versioning | 5 | Note version history |
| PKE Encryption | 15 | Public-key encrypted note sharing |
| File Attachments | 5 | Upload, manage, and retrieve file attachments |
| Memory Search | 4 | Location and time-based memory retrieval |
| System | 4 | Health check, diagnostics, rate limiting |

### Tool Categories by Permission

Tools are categorized by their effect on system state to help you understand permission requirements in restricted environments.

#### Read-Only Tools

These tools retrieve information without modifying system state:

**Search & Discovery:**
- `search_notes`, `search_notes_strict`, `list_tags`
- `search_memories_by_location`, `search_memories_by_time`, `search_memories_combined`
- `explore_graph`, `get_note_links`

**Retrieval:**
- `list_notes`, `get_note`
- `list_collections`, `get_collection`, `get_collection_notes`
- `list_templates`, `get_template`
- `list_embedding_sets`, `get_embedding_set`, `list_set_members`
- `list_embedding_configs`, `get_default_embedding_config`
- `list_document_types`, `get_document_type`
- `list_archives`
- `list_attachments`, `get_attachment`, `get_attachment_metadata`
- `get_memory_provenance`

**SKOS Concepts:**
- `list_concept_schemes`, `get_concept_scheme`, `search_concepts`
- `get_concept`, `get_concept_full`, `autocomplete_concepts`
- `get_broader`, `get_narrower`, `get_related`
- `get_note_concepts`, `get_governance_stats`, `get_top_concepts`

**Versioning:**
- `list_note_versions`, `get_note_version`, `diff_note_versions`

**Jobs & System:**
- `list_jobs`, `get_queue_stats`
- `health_check`, `get_system_info`, `get_rate_limit_status`, `memory_info`

**Export:**
- `export_note`, `export_all_notes`
- `backup_status`, `backup_download`
- `list_backups`, `get_backup_info`, `get_backup_metadata`
- `knowledge_archive_download`

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
- `add_set_members`, `remove_set_member`, `refresh_embedding_set`

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

**Versioning:**
- `restore_note_version`, `delete_note_version`

**Jobs:**
- `create_job`

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
    "excluded_tags": ["draft", "archived"]
  }
}
```

#### `search_notes_strict`

Dedicated tool for strict tag-filtered search. Guarantees results match filter criteria exactly.

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

```javascript
search_notes_strict({
  query: "API design",
  required_tags: ["project:matric"],
  any_tags: ["status:active", "status:review"],
  excluded_schemes: ["internal"],
  mode: "hybrid",
  limit: 20
})
```

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
| `search_notes` (with `set`) | Search within set context |

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

### Note Versioning

Dual-track versioning preserves both original and AI-enhanced content.

| Tool | Purpose |
|------|---------|
| `list_note_versions` | Get version history |
| `get_note_version` | Retrieve specific version |
| `restore_note_version` | Restore to previous version |
| `diff_note_versions` | Compare two versions |

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

Upload and manage files attached to notes with full metadata and provenance tracking.

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

Download the binary content of an attachment.

**Parameters:**
- `attachment_id` (required) - UUID of the attachment

**Returns:** Object with `content_base64` (file content) and `content_type` (MIME type).

**Example:**

```javascript
get_attachment({
  attachment_id: "660e8400-e29b-41d4-a716-446655440001"
})
// Returns: {
//   content_base64: "/9j/4AAQSkZJRg...",
//   content_type: "image/jpeg"
// }
```

#### `get_attachment_metadata`

Retrieve EXIF metadata and provenance information for an attachment.

**Parameters:**
- `attachment_id` (required) - UUID of the attachment

**Returns:**
- **EXIF data**: Camera make/model, GPS coordinates, timestamp, orientation, etc.
- **Provenance**: Original filename, upload timestamp, file hash, dimensions

**Example:**

```javascript
get_attachment_metadata({
  attachment_id: "660e8400-e29b-41d4-a716-446655440001"
})
// Returns: {
//   exif: {
//     make: "Canon",
//     model: "EOS 5D Mark IV",
//     gps_latitude: 47.6062,
//     gps_longitude: -122.3321,
//     datetime_original: "2026:01:15 14:23:45",
//     orientation: 1
//   },
//   provenance: {
//     filename: "vacation-photo.jpg",
//     uploaded_at: "2026-02-02T10:30:00Z",
//     content_hash: "sha256:abc123...",
//     width: 1920,
//     height: 1080
//   }
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

### Memory Search

Search notes by geographic location and time using provenance metadata extracted from attachments.

#### `search_memories_by_location`

Find memories (notes with attachments) near a specific geographic point.

**Parameters:**
- `latitude` (required) - Latitude in decimal degrees (-90 to 90)
- `longitude` (required) - Longitude in decimal degrees (-180 to 180)
- `radius_meters` (optional) - Search radius in meters (default: 1000)
- `limit` (optional) - Maximum results (default: 20)

**Returns:** Array of notes with distance from query point.

**Example:**

```javascript
// Find memories near Seattle Space Needle
search_memories_by_location({
  latitude: 47.6205,
  longitude: -122.3493,
  radius_meters: 5000
})
// Returns: [
//   {
//     note_id: "550e8400-...",
//     title: "Seattle Trip 2026",
//     distance_meters: 342.5,
//     attachment_count: 12,
//     earliest_capture: "2026-01-15T10:00:00Z"
//   }
// ]
```

#### `search_memories_by_time`

Find memories captured within a specific time range.

**Parameters:**
- `start_time` (required) - ISO 8601 timestamp (e.g., "2026-01-01T00:00:00Z")
- `end_time` (required) - ISO 8601 timestamp
- `limit` (optional) - Maximum results (default: 20)

**Returns:** Array of notes with capture time information.

**Example:**

```javascript
// Find memories from January 2026
search_memories_by_time({
  start_time: "2026-01-01T00:00:00Z",
  end_time: "2026-01-31T23:59:59Z"
})
// Returns: [
//   {
//     note_id: "550e8400-...",
//     title: "New Year Celebration",
//     attachment_count: 5,
//     earliest_capture: "2026-01-01T00:15:00Z",
//     latest_capture: "2026-01-01T02:30:00Z"
//   }
// ]
```

#### `search_memories_combined`

Search memories by both location and time simultaneously.

**Parameters:**
- `latitude` (required) - Latitude in decimal degrees
- `longitude` (required) - Longitude in decimal degrees
- `radius_meters` (optional) - Search radius in meters (default: 1000)
- `start_time` (required) - ISO 8601 timestamp
- `end_time` (required) - ISO 8601 timestamp
- `limit` (optional) - Maximum results (default: 20)

**Returns:** Array of notes matching both geographic and temporal criteria.

**Example:**

```javascript
// Find memories from Seattle in January 2026
search_memories_combined({
  latitude: 47.6205,
  longitude: -122.3493,
  radius_meters: 10000,
  start_time: "2026-01-01T00:00:00Z",
  end_time: "2026-01-31T23:59:59Z"
})
// Returns: [
//   {
//     note_id: "550e8400-...",
//     title: "Pike Place Market",
//     distance_meters: 1250.8,
//     attachment_count: 8,
//     earliest_capture: "2026-01-15T14:23:00Z"
//   }
// ]
```

#### `get_memory_provenance`

Retrieve complete provenance chain for a note, including all attachment metadata and EXIF data.

**Parameters:**
- `note_id` (required) - UUID of the note

**Returns:** Comprehensive provenance information including:
- Note creation and modification timestamps
- All attachments with EXIF data (GPS, timestamps, camera info)
- Location clustering analysis
- Temporal distribution of captures

**Example:**

```javascript
get_memory_provenance({
  note_id: "550e8400-e29b-41d4-a716-446655440000"
})
// Returns: {
//   note: {
//     created_at: "2026-01-15T20:00:00Z",
//     modified_at: "2026-01-16T10:30:00Z"
//   },
//   attachments: [
//     {
//       id: "660e8400-...",
//       filename: "IMG_0123.jpg",
//       exif: {
//         gps_latitude: 47.6062,
//         gps_longitude: -122.3321,
//         datetime_original: "2026-01-15T14:23:45Z",
//         make: "Canon",
//         model: "EOS 5D Mark IV"
//       }
//     }
//   ],
//   location_summary: {
//     center_latitude: 47.6062,
//     center_longitude: -122.3321,
//     max_distance_meters: 450.2,
//     photo_count: 12
//   },
//   temporal_summary: {
//     earliest: "2026-01-15T14:00:00Z",
//     latest: "2026-01-15T18:30:00Z",
//     duration_hours: 4.5
//   }
// }
```

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
   - Use get_attachment_metadata to extract location/time from photos
   - Search memories by location/time for spatial-temporal discovery
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
| `search_notes_strict` | Strict tag-filtered search with guaranteed isolation |
| `list_tags` | List all tags with counts |

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

### Embedding Configs

| Tool | Description |
|------|-------------|
| `list_embedding_configs` | List all embedding configurations |
| `get_default_embedding_config` | Get the default embedding configuration |

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
| `get_attachment` | Download attachment binary content |
| `get_attachment_metadata` | Get EXIF and provenance data |
| `delete_attachment` | Remove attachment permanently |

### Memory Search

| Tool | Description |
|------|-------------|
| `search_memories_by_location` | Find memories near geographic coordinates |
| `search_memories_by_time` | Find memories within time range |
| `search_memories_combined` | Combined location and time search |
| `get_memory_provenance` | Get complete provenance chain with EXIF |

### Jobs

| Tool | Description |
|------|-------------|
| `create_job` | Queue single processing step |
| `list_jobs` | List/filter jobs |
| `get_queue_stats` | Queue health summary |

### Archives

| Tool | Description |
|------|-------------|
| `list_archives` | List all archives with stats |
| `create_archive` | Create new archive schema |
| `update_archive` | Update archive metadata |
| `delete_archive` | Delete archive (requires force) |
| `set_default_archive` | Set the default archive |

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

### Versioning

| Tool | Description |
|------|-------------|
| `list_note_versions` | Version history |
| `get_note_version` | Get specific version |
| `restore_note_version` | Restore version |
| `delete_note_version` | Delete version |
| `diff_note_versions` | Compare versions |

### Archives

Parallel memory archives for isolated knowledge bases.

| Tool | Description |
|------|-------------|
| `list_archives` | List all archives with stats |
| `create_archive` | Create new archive schema |
| `update_archive` | Update archive metadata |
| `delete_archive` | Delete archive (requires force) |
| `set_default_archive` | Set the default archive |

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

### System

| Tool | Description |
|------|-------------|
| `memory_info` | Storage/memory statistics |
| `explore_graph` | Knowledge graph traversal |
| `health_check` | System health status |
| `get_system_info` | Comprehensive diagnostics |
| `get_rate_limit_status` | Check API rate limits |

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

#### `get_rate_limit_status`

Check current rate limit status for the API.

```javascript
get_rate_limit_status()
// Returns: { limit: 100, remaining: 85, reset_at: "2026-02-02T12:00:00Z" }
```

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

### Uploading Photos with Location Data

```javascript
// Create a travel note
const note = await create_note({
  content: "# Seattle Trip January 2026\n\nExplored Pike Place Market and Space Needle.",
  tags: ["travel", "seattle", "2026"],
  revision_mode: "light"
})

// Upload photos from the trip
const photo1 = await upload_attachment({
  note_id: note.id,
  filename: "space-needle.jpg",
  content_base64: readFileAsBase64("space-needle.jpg"),
  content_type: "image/jpeg"
})

// Get location from EXIF
const metadata = await get_attachment_metadata({
  attachment_id: photo1.id
})
console.log(`Photo taken at: ${metadata.exif.gps_latitude}, ${metadata.exif.gps_longitude}`)

// Later, find all Seattle memories
const memories = await search_memories_by_location({
  latitude: 47.6062,
  longitude: -122.3321,
  radius_meters: 10000
})
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

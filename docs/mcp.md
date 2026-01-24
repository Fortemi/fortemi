# Matric Memory MCP Server

Complete documentation for the Model Context Protocol (MCP) server that provides AI agent access to Matric Memory.

## Overview

The MCP server enables AI assistants (Claude, etc.) to interact with your knowledge base through a standardized protocol. It provides **65+ tools** organized into these categories:

| Category | Tools | Description |
|----------|-------|-------------|
| Notes | 12 | Create, read, update, delete notes |
| Search | 3 | Hybrid semantic + full-text + strict filtering |
| Collections | 6 | Hierarchical folder organization |
| Templates | 5 | Reusable note structures |
| Embedding Sets | 6 | Focused search contexts |
| Jobs | 3 | Background processing control |
| Backup/Export | 15 | Data portability and backups |
| SKOS Concepts | 20 | Hierarchical tagging system |
| Versioning | 5 | Note version history |
| System | 1 | Memory/storage info |

## Quick Start

### Installation

```bash
# Clone repository
git clone https://git.integrolabs.net/roctinam/matric-memory

# Install MCP server dependencies
cd mcp-server
npm install
```

### Configuration

Set environment variables:

```bash
# Required: API endpoint
export MATRIC_MEMORY_URL="https://memory.integrolabs.net"

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
    "matric-memory": {
      "command": "node",
      "args": ["/path/to/matric-memory/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "https://memory.integrolabs.net",
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
  "revision_mode": "full"
}
```

#### `search_notes`

Hybrid search combining full-text and semantic similarity.

**Search Modes:**

| Mode | Best For |
|------|----------|
| `hybrid` (default) | General search - combines keyword + semantic |
| `fts` | Exact keyword matching |
| `semantic` | Finding conceptually related content |

**Embedding Sets:** Use `set` parameter to restrict search to specific contexts (e.g., "work-projects", "research-papers").

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

### Collections (Organization)

Hierarchical folder structure for notes.

| Tool | Purpose |
|------|---------|
| `list_collections` | List folders (use `parent_id` for children) |
| `create_collection` | Create folder (set `parent_id` for nesting) |
| `get_collection_notes` | List notes in a folder |
| `move_note_to_collection` | Move note to folder |

### Embedding Sets (Focused Contexts)

Create focused search contexts for specific domains.

**Use Cases:**

- "work-projects" - Only search work-related notes
- "research-ai" - AI/ML research papers only
- "personal-journal" - Personal reflections

| Tool | Purpose |
|------|---------|
| `create_embedding_set` | Create new set with criteria |
| `add_set_members` | Add notes to a set |
| `refresh_embedding_set` | Regenerate set embeddings |
| `search_notes` (with `set`) | Search within set context |

### SKOS Concepts (Hierarchical Tags)

Full W3C SKOS-compliant hierarchical tagging. See [tags.md](./tags.md) for details.

**Key Tools:**

| Tool | Purpose |
|------|---------|
| `search_concepts` | Find existing concepts |
| `create_concept` | Add new concept with relations |
| `add_broader` / `add_narrower` | Build hierarchy |
| `tag_note_concept` | Tag note with concept |
| `get_governance_stats` | Tag health metrics |

**Concept Status:**

- `candidate` - Auto-created, needs review
- `controlled` - Approved for use
- `deprecated` - Replaced, don't use

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
| `list_notes` | List notes with filtering and pagination |
| `get_note` | Get full note details |
| `create_note` | Create note with AI pipeline |
| `bulk_create_notes` | Batch create (max 100) |
| `update_note` | Update content or status |
| `delete_note` | Soft delete (recoverable) |
| `purge_note` | Permanent delete |
| `purge_notes` | Batch permanent delete |
| `purge_all_notes` | Delete everything (requires confirm) |
| `set_note_tags` | Replace user tags |
| `get_note_links` | Get semantic links/backlinks |
| `export_note` | Export as markdown |

### Search

| Tool | Description |
|------|-------------|
| `search_notes` | Hybrid/FTS/semantic search with optional strict filtering |
| `search_notes_strict` | Strict tag-filtered search with guaranteed isolation |
| `list_tags` | List all tags with counts |

### Collections

| Tool | Description |
|------|-------------|
| `list_collections` | List folders |
| `create_collection` | Create folder |
| `get_collection` | Get folder details |
| `delete_collection` | Delete folder |
| `get_collection_notes` | List notes in folder |
| `move_note_to_collection` | Move note |

### Templates

| Tool | Description |
|------|-------------|
| `list_templates` | List all templates |
| `create_template` | Create with {{variables}} |
| `get_template` | Get template details |
| `delete_template` | Delete template |
| `instantiate_template` | Create note from template |

### Embedding Sets

| Tool | Description |
|------|-------------|
| `list_embedding_sets` | List all sets |
| `get_embedding_set` | Get set details |
| `create_embedding_set` | Create new set |
| `list_set_members` | List notes in set |
| `add_set_members` | Add notes to set |
| `remove_set_member` | Remove note from set |
| `refresh_embedding_set` | Regenerate embeddings |

### Jobs

| Tool | Description |
|------|-------------|
| `create_job` | Queue single processing step |
| `list_jobs` | List/filter jobs |
| `get_queue_stats` | Queue health summary |

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
| `search_concepts` | Search concepts |
| `create_concept` | Create concept |
| `get_concept` | Get concept details |
| `get_concept_full` | Get with all relations |
| `update_concept` | Update concept |
| `delete_concept` | Delete unused concept |
| `autocomplete_concepts` | Type-ahead search |
| `get_broader` | Get parent concepts |
| `add_broader` | Add parent relation |
| `get_narrower` | Get child concepts |
| `add_narrower` | Add child relation |
| `get_related` | Get related concepts |
| `add_related` | Add related relation |
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

### System

| Tool | Description |
|------|-------------|
| `memory_info` | Storage/memory statistics |
| `explore_graph` | Knowledge graph traversal |

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

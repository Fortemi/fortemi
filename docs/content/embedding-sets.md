# Embedding Sets

**Status:** Production
**Version:** 0.3.0
**Schema Migrations:**
- `20260117000000_embedding_sets.sql` - Initial implementation
- `20260201500000_full_embedding_sets.sql` - Full sets, MRL, auto-embed

## Overview

Embedding Sets enable tiered semantic search with focused embedding collections. They allow power users to create specialized semantic search indexes optimized for specific domains, projects, or use cases while maintaining backward compatibility with the global default search behavior.

## Concepts

### What is an Embedding Set?

An embedding set is a named collection of notes with dedicated vector embeddings optimized for semantic search within a specific domain or context. Each set maintains its own embedding index, allowing focused semantic queries that ignore irrelevant content.

### Default Behavior

By default, all semantic search operates on the "default" embedding set, which contains all notes. This maintains backward compatibility and provides the expected global search behavior without configuration.

### Set Types (v0.3.0)

Embedding sets can be one of two types:

#### Filter Sets (Default)

Filter sets share embeddings from the default embedding set. When you search a filter set, it filters results from the pre-existing default embeddings.

- **Advantages:** No additional storage, instant creation
- **Limitations:** Same embedding model/dimensions as default set

#### Full Sets

Full sets maintain their own independent embeddings with dedicated configuration.

- **Advantages:** Custom embedding model, MRL dimension truncation, auto-embed rules
- **Limitations:** Additional storage, embedding generation time

**Example:**

```json
{
  "name": "Fast Search",
  "set_type": "full",
  "truncate_dim": 256,
  "auto_embed_rules": {
    "on_create": true,
    "on_update": true
  }
}
```

### Matryoshka Representation Learning (MRL)

MRL-enabled models encode information hierarchically, allowing embeddings to be truncated to smaller dimensions while preserving quality.

**Supported Models:**
- `nomic-embed-text` (dims: 768, 512, 256, 128, 64)
- `mxbai-embed-large-v1` (dims: 1024, 512, 256, 128, 64)

**Trade-offs:**

| Dimension | Storage Reduction | Quality Loss |
|-----------|------------------|--------------|
| 64-dim    | 12×              | ~3-5%        |
| 128-dim   | 6×               | ~2%          |
| 256-dim   | 3×               | ~1%          |
| Full      | 1×               | 0%           |

See `docs/content/embedding-model-selection.md` for detailed guidance.

### Auto-Embed Rules

Full embedding sets can automatically manage embedding lifecycle:

```json
{
  "on_create": true,         // Generate on note creation
  "on_update": true,         // Regenerate on note update
  "update_threshold_percent": 10.0,  // Min change to trigger update
  "max_embedding_age_secs": 86400,   // Max age before refresh
  "priority": 5,             // Job queue priority (higher = sooner)
  "batch_size": 10,          // Batch size for bulk operations
  "rate_limit": 100          // Max embeddings per minute
}
```

### Use Cases

**Domain-Specific Knowledge Bases:**
- Separate machine learning research from software architecture notes
- Isolate project-specific documentation from general knowledge
- Create topic-specific indexes (medical, legal, technical)

**Project Isolation:**
- Create per-project embedding sets for focused project search
- Prevent cross-contamination between unrelated projects
- Maintain project-specific semantic relationships

**Temporal Segmentation:**
- Shard historical notes in separate sets
- Focus search on recent content only
- Maintain different retention policies per set

**Access Control Foundation:**
- Prepare for future multi-tenant features
- Isolate sensitive content in separate indexes
- Enable selective sharing of knowledge domains

## Architecture

### Database Schema

#### embedding_set Table

Stores embedding set metadata and configuration:

- **Identity:** name, slug, description, purpose, usage_hints, keywords
- **Membership:** mode (auto/manual/mixed), criteria (JSON)
- **Configuration:** embedding_config_id reference
- **Index Status:** index_status, index_type, last_indexed_at
- **Statistics:** document_count, embedding_count, index_size_bytes
- **Lifecycle:** is_system, is_active, auto_refresh, refresh_interval
- **Agent Metadata:** agent_metadata (JSON for AI discovery)

#### embedding_set_member Table

Many-to-many relationship between sets and notes:

- **Primary Key:** (embedding_set_id, note_id)
- **Membership Type:** auto, manual_include, manual_exclude
- **Audit:** added_at, added_by

#### embedding_config Table

Embedding configuration profiles:

- **Model Settings:** model, dimension
- **Chunking:** chunk_size, chunk_overlap
- **Index Settings:** hnsw_m, hnsw_ef_construction, ivfflat_lists

#### embedding Table Extension

Existing embedding table extended with:

- **embedding_set_id:** Foreign key to embedding_set

### Modes

#### Auto Mode

Notes automatically included based on criteria evaluation. Criteria evaluated on:
- Set creation
- Manual refresh via API
- Scheduled auto-refresh (if enabled)

**Criteria Options:**
- `include_all: true` - Include all notes (default set behavior)
- `tags: ["ml", "research"]` - Notes with ANY of these tags
- `collections: [uuid1, uuid2]` - Notes in ANY of these collections
- `fts_query: "machine learning"` - Notes matching FTS query
- `created_after: "2024-01-01T00:00:00Z"` - Date range filters
- `created_before: "2024-12-31T23:59:59Z"`
- `exclude_archived: true` - Skip archived notes (default)

#### Manual Mode

Notes only included by explicit add_members API calls. No automatic membership evaluation.

**Use Cases:**
- Curated collections requiring human judgment
- One-off specialized indexes
- Testing or experimental sets

#### Mixed Mode

Combines auto criteria with manual additions/exclusions. Notes added via criteria can be manually removed. Additional notes can be manually added beyond criteria matches.

**Use Cases:**
- Auto-populated sets with manual overrides
- Exclude specific notes from auto-matched set
- Include outliers not matching criteria

### Index Status

#### pending
Initial state after set creation. Index needs to be built.

#### building
Background job actively building embeddings and index.

#### ready
Index is current and available for search.

#### stale
Membership changed (new notes added). Index needs rebuild.

#### disabled
No index built. Used for very small sets or manual-only collections.

## API Reference

### Base URL

All endpoints use the base path: `/api/v1/embedding-sets`

### List Embedding Sets

```http
GET /api/v1/embedding-sets
```

Returns all active embedding sets with summary statistics.

**Response:**

```json
[
  {
    "id": "019b76da-a800-7001-8000-000000000001",
    "name": "Default",
    "slug": "default",
    "description": "Primary embedding set containing all notes",
    "purpose": "Provides semantic search across the entire knowledge base",
    "document_count": 1523,
    "embedding_count": 8947,
    "index_status": "ready",
    "is_system": true,
    "keywords": ["all", "general", "default"],
    "model": "nomic-embed-text",
    "dimension": 768
  },
  {
    "id": "a1b2c3d4-...",
    "name": "ML Research",
    "slug": "ml-research",
    "description": "Machine learning research papers and notes",
    "purpose": "Focused semantic search for ML concepts and techniques",
    "document_count": 237,
    "embedding_count": 1456,
    "index_status": "ready",
    "is_system": false,
    "keywords": ["machine-learning", "research", "papers"],
    "model": "nomic-embed-text",
    "dimension": 768
  }
]
```

### Get Embedding Set

```http
GET /api/v1/embedding-sets/:slug
```

Returns detailed information about a specific embedding set.

**Parameters:**
- `slug` (path) - URL-friendly set identifier (e.g., "default", "ml-research")

**Response:**

```json
{
  "id": "a1b2c3d4-...",
  "name": "ML Research",
  "slug": "ml-research",
  "description": "Machine learning research papers and notes",
  "purpose": "Focused semantic search for ML concepts",
  "usage_hints": "Use for queries about neural networks, deep learning, optimization",
  "keywords": ["ml", "research", "papers", "neural-networks"],
  "mode": "auto",
  "criteria": {
    "tags": ["machine-learning", "research"],
    "exclude_archived": true
  },
  "embedding_config_id": "019b76da-a800-7001-8000-000000000001",
  "document_count": 237,
  "embedding_count": 1456,
  "index_status": "ready",
  "index_size_bytes": 11239424,
  "is_system": false,
  "is_active": true,
  "auto_refresh": true,
  "agent_metadata": {
    "created_by_agent": "claude-opus-4.5",
    "rationale": "Separate ML content for focused research queries",
    "performance_notes": "Best for conceptual ML queries, not code"
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-20T14:22:00Z"
}
```

### Create Embedding Set

```http
POST /api/v1/embedding-sets
```

Creates a new embedding set. Background job automatically queued to build index.

**Request Body:**

```json
{
  "name": "Project Alpha",
  "slug": "project-alpha",
  "description": "Documentation and notes for Project Alpha",
  "purpose": "Focused search for project-specific knowledge",
  "usage_hints": "Use for queries about Project Alpha architecture, requirements, decisions",
  "keywords": ["project-alpha", "project", "alpha"],
  "mode": "auto",
  "criteria": {
    "tags": ["project-alpha"],
    "collections": ["uuid-of-project-alpha-collection"],
    "exclude_archived": true
  }
}
```

**Fields:**
- `name` (required) - Display name for the set
- `slug` (optional) - URL-friendly identifier (auto-generated from name if omitted)
- `description` (optional) - Short description of the set's contents
- `purpose` (optional) - Detailed purpose for AI agent decision-making
- `usage_hints` (optional) - When and how to use this set
- `keywords` (optional) - Discovery keywords for AI agents
- `mode` (optional) - Membership mode: "auto", "manual", "mixed" (default: "auto")
- `criteria` (optional) - Auto-membership criteria (required for auto/mixed mode)
- `embedding_config_id` (optional) - Custom embedding config (defaults to global default)

**Response:**

Returns the created embedding set (same structure as GET).

### Update Embedding Set

```http
PATCH /api/v1/embedding-sets/:slug
```

Updates embedding set metadata. Cannot rename or deactivate system sets.

**Request Body:**

```json
{
  "description": "Updated description",
  "purpose": "Updated purpose",
  "usage_hints": "New usage guidelines",
  "keywords": ["updated", "keywords"],
  "is_active": true,
  "auto_refresh": false,
  "mode": "mixed",
  "criteria": {
    "tags": ["new-tag", "another-tag"],
    "exclude_archived": true
  }
}
```

All fields are optional. Only provided fields are updated.

**Response:**

Returns the updated embedding set.

### Delete Embedding Set

```http
DELETE /api/v1/embedding-sets/:slug
```

Permanently deletes an embedding set. Cannot delete system sets (e.g., "default").

**Response:**

```http
204 No Content
```

### List Set Members

```http
GET /api/v1/embedding-sets/:slug/members?limit=50&offset=0
```

Lists notes that are members of the embedding set.

**Query Parameters:**
- `limit` (optional) - Maximum members to return (default: 50)
- `offset` (optional) - Pagination offset (default: 0)

**Response:**

```json
[
  {
    "embedding_set_id": "a1b2c3d4-...",
    "note_id": "e5f6g7h8-...",
    "membership_type": "auto",
    "added_at": "2024-01-15T10:30:00Z",
    "added_by": null
  },
  {
    "embedding_set_id": "a1b2c3d4-...",
    "note_id": "i9j0k1l2-...",
    "membership_type": "manual_include",
    "added_at": "2024-01-16T14:22:00Z",
    "added_by": "user@example.com"
  }
]
```

### Add Set Members

```http
POST /api/v1/embedding-sets/:slug/members
```

Manually adds notes to an embedding set. Marks index as stale.

**Request Body:**

```json
{
  "note_ids": [
    "e5f6g7h8-i9j0-k1l2-m3n4-o5p6q7r8s9t0",
    "u1v2w3x4-y5z6-a7b8-c9d0-e1f2g3h4i5j6"
  ],
  "added_by": "claude-opus-4.5"
}
```

**Response:**

```json
{
  "count": 2
}
```

### Remove Set Member

```http
DELETE /api/v1/embedding-sets/:slug/members/:note_id
```

Removes a note from an embedding set. Also removes the note's embeddings from this set.

**Response:**

```http
204 No Content
```

### Refresh Embedding Set

```http
POST /api/v1/embedding-sets/:slug/refresh
```

Re-evaluates criteria and updates membership for auto/mixed mode sets. Queues background jobs to rebuild embeddings for new members.

**Response:**

```json
{
  "added": 15
}
```

Returns count of newly added members.

### Search with Embedding Sets

```http
GET /api/v1/search?q=neural+networks&mode=semantic&set=ml-research
```

Performs semantic search restricted to a specific embedding set.

**Query Parameters:**
- `q` (required) - Search query
- `mode` (optional) - "hybrid", "fts", "semantic" (default: "hybrid")
- `set` (optional) - Embedding set slug (default: "default")
- `limit` (optional) - Maximum results (default: 20)

**Response:**

Same as standard search, but results only from the specified set.

## MCP Tools

The MCP server provides tools for AI agents to discover and use embedding sets.

### list_embedding_sets

Lists all available embedding sets for discovery.

**Input Schema:**

```json
{}
```

**Output:**

Array of embedding set summaries (same as API GET /embedding-sets).

**Usage:**

```javascript
const sets = await mcpClient.callTool("list_embedding_sets", {});
```

### get_embedding_set

Gets detailed information about a specific embedding set.

**Input Schema:**

```json
{
  "slug": "ml-research"
}
```

**Output:**

Full embedding set details (same as API GET /embedding-sets/:slug).

**Usage:**

```javascript
const set = await mcpClient.callTool("get_embedding_set", {
  slug: "ml-research"
});
```

### create_embedding_set

Creates a new embedding set.

**Input Schema:**

```json
{
  "name": "Project Beta",
  "slug": "project-beta",
  "description": "Project Beta documentation",
  "purpose": "Focused search for Project Beta",
  "usage_hints": "Use for Project Beta queries",
  "keywords": ["project-beta", "beta"],
  "mode": "auto",
  "criteria": {
    "tags": ["project-beta"],
    "exclude_archived": true
  }
}
```

**Output:**

Created embedding set.

**Usage:**

```javascript
const set = await mcpClient.callTool("create_embedding_set", {
  name: "Research Papers 2024",
  mode: "auto",
  criteria: {
    tags: ["research", "papers"],
    created_after: "2024-01-01T00:00:00Z",
    exclude_archived: true
  }
});
```

### list_set_members

Lists notes in an embedding set.

**Input Schema:**

```json
{
  "slug": "ml-research",
  "limit": 50,
  "offset": 0
}
```

**Output:**

Array of set members.

**Usage:**

```javascript
const members = await mcpClient.callTool("list_set_members", {
  slug: "ml-research",
  limit: 100
});
```

### add_set_members

Adds notes to an embedding set.

**Input Schema:**

```json
{
  "slug": "ml-research",
  "note_ids": ["uuid1", "uuid2"],
  "added_by": "claude-opus-4.5"
}
```

**Output:**

```json
{
  "count": 2
}
```

**Usage:**

```javascript
const result = await mcpClient.callTool("add_set_members", {
  slug: "ml-research",
  note_ids: [noteId1, noteId2],
  added_by: "claude-opus-4.5"
});
```

### remove_set_member

Removes a note from an embedding set.

**Input Schema:**

```json
{
  "slug": "ml-research",
  "note_id": "uuid"
}
```

**Output:**

```json
{
  "success": true
}
```

**Usage:**

```javascript
await mcpClient.callTool("remove_set_member", {
  slug: "ml-research",
  note_id: noteId
});
```

### refresh_embedding_set

Refreshes an embedding set by re-evaluating criteria.

**Input Schema:**

```json
{
  "slug": "ml-research"
}
```

**Output:**

```json
{
  "added": 15
}
```

**Usage:**

```javascript
const result = await mcpClient.callTool("refresh_embedding_set", {
  slug: "ml-research"
});
```

### search_notes with Embedding Sets

Search within a specific embedding set.

**Input Schema:**

```json
{
  "query": "transformer architecture",
  "mode": "semantic",
  "set": "ml-research",
  "limit": 20
}
```

**Output:**

Search results restricted to the specified set.

**Usage:**

```javascript
const results = await mcpClient.callTool("search_notes", {
  query: "attention mechanisms in transformers",
  mode: "semantic",
  set: "ml-research"
});
```

## Criteria Configuration

### Criteria Structure

Criteria is a JSON object with the following optional fields:

```json
{
  "include_all": false,
  "tags": ["tag1", "tag2"],
  "collections": ["uuid1", "uuid2"],
  "fts_query": "search terms",
  "created_after": "2024-01-01T00:00:00Z",
  "created_before": "2024-12-31T23:59:59Z",
  "exclude_archived": true
}
```

### Field Semantics

#### include_all

**Type:** boolean
**Default:** false

If true, includes all non-archived notes. Typically used only for the default set.

**Example:**

```json
{
  "include_all": true,
  "exclude_archived": true
}
```

#### tags

**Type:** array of strings
**Default:** []

Includes notes with ANY of the specified tags (OR logic).

**Example:**

```json
{
  "tags": ["machine-learning", "deep-learning", "research"]
}
```

Matches notes tagged with "machine-learning" OR "deep-learning" OR "research".

#### collections

**Type:** array of UUIDs
**Default:** []

Includes notes in ANY of the specified collections (OR logic).

**Example:**

```json
{
  "collections": [
    "a1b2c3d4-e5f6-4747-8888-999999999999",
    "b2c3d4e5-f6a7-4848-9999-000000000000"
  ]
}
```

#### fts_query

**Type:** string
**Default:** null

Includes notes matching the full-text search query.

**Example:**

```json
{
  "fts_query": "neural network optimization"
}
```

Uses PostgreSQL FTS with English language configuration.

#### created_after

**Type:** ISO 8601 datetime string
**Default:** null

Includes notes created after the specified timestamp.

**Example:**

```json
{
  "created_after": "2024-01-01T00:00:00Z"
}
```

#### created_before

**Type:** ISO 8601 datetime string
**Default:** null

Includes notes created before the specified timestamp.

**Example:**

```json
{
  "created_before": "2024-12-31T23:59:59Z"
}
```

#### exclude_archived

**Type:** boolean
**Default:** true

If true, excludes archived notes from the set.

**Example:**

```json
{
  "tags": ["research"],
  "exclude_archived": false
}
```

### Criteria Combination Logic

Multiple criteria are combined with AND logic:

```json
{
  "tags": ["research", "papers"],
  "collections": ["uuid1"],
  "created_after": "2024-01-01T00:00:00Z",
  "exclude_archived": true
}
```

Matches notes that:
- Have tag "research" OR "papers", AND
- Are in collection "uuid1", AND
- Were created after 2024-01-01, AND
- Are not archived

### Example Configurations

#### Recent Work

```json
{
  "created_after": "2024-11-01T00:00:00Z",
  "exclude_archived": true
}
```

#### Project-Specific

```json
{
  "tags": ["project-alpha"],
  "collections": ["project-alpha-collection-uuid"],
  "exclude_archived": true
}
```

#### Topic Archive

```json
{
  "tags": ["machine-learning", "deep-learning"],
  "created_before": "2024-01-01T00:00:00Z",
  "exclude_archived": false
}
```

#### Meeting Notes

```json
{
  "fts_query": "meeting notes attendees action items",
  "tags": ["meeting"],
  "exclude_archived": true
}
```

## Agent Metadata

Agent metadata provides structured information for AI agents to discover and use embedding sets effectively.

### Metadata Structure

```json
{
  "created_by_agent": "claude-opus-4.5",
  "rationale": "Why this set was created",
  "performance_notes": "When this set performs well/poorly",
  "related_sets": ["set-slug-1", "set-slug-2"],
  "suggested_queries": [
    "example query 1",
    "example query 2"
  ]
}
```

### Field Descriptions

#### created_by_agent

**Type:** string
**Optional:** yes

Identifies the AI agent or tool that created the set.

**Example:** "claude-opus-4.5", "auto-discovery-service"

#### rationale

**Type:** string
**Optional:** yes

Explains why this set was created and what problem it solves.

**Example:** "Separate ML research from general software notes to reduce noise in ML-specific queries"

#### performance_notes

**Type:** string
**Optional:** yes

Documents performance characteristics or usage patterns.

**Example:** "Best for conceptual ML queries. Use 'ml-code' set for implementation queries."

#### related_sets

**Type:** array of strings
**Optional:** yes

Lists related or complementary embedding sets.

**Example:** ["ml-code", "ml-datasets", "ml-papers"]

#### suggested_queries

**Type:** array of strings
**Optional:** yes

Example queries that work well with this set.

**Example:**
```json
[
  "transformer attention mechanisms",
  "backpropagation optimization",
  "gradient descent variants"
]
```

### Discovery Pattern

AI agents should use agent metadata to:

1. List available embedding sets
2. Filter by keywords matching query domain
3. Check suggested_queries for query pattern match
4. Consider performance_notes for set selection
5. Explore related_sets for comprehensive coverage

**Example Agent Logic:**

```javascript
// User query: "How do transformers work?"
const sets = await listEmbeddingSets();

// Filter by keywords
const candidates = sets.filter(s =>
  s.keywords.some(k =>
    ["ml", "machine-learning", "transformers", "neural-networks"]
      .includes(k)
  )
);

// Check suggested queries
const bestMatch = candidates.find(s =>
  s.agent_metadata?.suggested_queries?.some(q =>
    q.toLowerCase().includes("transformer")
  )
) || candidates[0];

// Search in best matching set
const results = await searchNotes({
  query: "transformer architecture",
  set: bestMatch.slug
});
```

## Implementation Details

### Database Triggers

#### update_embedding_set_stats()

Automatically updates document_count and embedding_count when:
- Notes added to or removed from set
- Embeddings created or deleted for set

#### trigger_update_set_stats()

Fires on INSERT/UPDATE/DELETE to embedding_set_member.

#### trigger_update_embedding_set_stats()

Fires on INSERT/UPDATE/DELETE to embedding table when embedding_set_id is involved.

### Functions

#### get_default_embedding_set_id()

Returns the UUID of the default embedding set (fast lookup).

#### get_default_embedding_config_id()

Returns the UUID of the default embedding config.

#### update_embedding_set_stats(set_id UUID)

Manually updates statistics for a specific set. Called by triggers and after bulk operations.

### Index Status Management

Index status transitions:

```
pending -> building -> ready
ready -> stale (when membership changes)
stale -> building -> ready (on refresh)
```

Status updated by:
- Background jobs (building)
- API operations (pending, stale)
- Refresh operations (stale -> building)

### Background Jobs

#### create_embedding_set Job

Queued automatically on set creation. Builds initial embeddings and index.

#### refresh_embedding_set Job

Queued on manual refresh API call or scheduled auto-refresh. Re-evaluates criteria and rebuilds index.

#### build_set_index Job

Queued after membership changes. Builds pgvector HNSW or IVFFlat index.

### Migration Safety

The migration maintains backward compatibility:

- Existing embeddings migrated to default set
- Default set marked as system (cannot delete)
- All existing notes added as default set members
- No changes to search behavior without explicit set parameter

## Best Practices

### Set Design

**Keep sets focused:**
- Specific domain or project
- Clear membership criteria
- Distinct from other sets

**Avoid over-segmentation:**
- Too many small sets reduce discovery
- Prefer fewer, well-defined sets over many narrow ones
- Consider merged set with good criteria vs. multiple tiny sets

**Document set purpose:**
- Clear description and purpose
- Usage hints for when to use this set
- Keywords for AI agent discovery

### Criteria Design

**Use tag-based criteria for flexibility:**
```json
{
  "tags": ["project-alpha", "architecture"],
  "exclude_archived": true
}
```

**Combine tags and collections for precision:**
```json
{
  "tags": ["research"],
  "collections": ["ml-research-collection-uuid"],
  "exclude_archived": true
}
```

**Use date ranges for temporal segmentation:**
```json
{
  "tags": ["meeting"],
  "created_after": "2024-01-01T00:00:00Z",
  "exclude_archived": true
}
```

### Performance

**Index status matters:**
- Only search ready indexes
- Monitor stale status and refresh periodically
- Disable indexes for very small sets (< 10 notes)

**Set size considerations:**
- Small sets (< 100 notes): Manual mode acceptable
- Medium sets (100-1000): Auto mode with specific criteria
- Large sets (> 1000): Consider sub-segmentation

**Refresh frequency:**
- Enable auto_refresh for active sets
- Set appropriate refresh_interval (default: 1 day)
- Manual refresh after bulk note imports

### Agent Usage

**Set discovery:**

```javascript
// List sets and filter by domain
const sets = await listEmbeddingSets();
const mlSets = sets.filter(s =>
  s.keywords.includes("machine-learning")
);
```

**Query routing:**

```javascript
// Route query to appropriate set
const queryDomain = detectDomain(userQuery); // "ml", "project-alpha", etc.
const set = sets.find(s => s.keywords.includes(queryDomain)) || sets.find(s => s.slug === "default");

const results = await searchNotes({
  query: userQuery,
  set: set.slug
});
```

**Fallback strategy:**

```javascript
// Search specific set first, fall back to default
let results = await searchNotes({ query, set: "ml-research" });
if (results.length === 0) {
  results = await searchNotes({ query, set: "default" });
}
```

## Future Enhancements

### Planned Features

**Cross-set search:**
- Search multiple sets simultaneously
- Weighted combination of results
- De-duplication across sets

**Set hierarchies:**
- Parent-child set relationships
- Inheritance of criteria
- Automatic propagation of updates

**Custom embedding models per set:**
- Domain-specific embedding models
- Different dimensions per set
- Model performance tracking

**Set performance analytics:**
- Query success rate per set
- Average result quality
- Usage statistics

**AI-driven set creation:**
- Automatic set discovery from usage patterns
- Suggested sets based on query analysis
- Dynamic criteria optimization

### Migration Path

Current schema supports future enhancements:

- `agent_metadata` field extensible for new agent features
- `criteria` JSONB allows new filter types
- `embedding_config` separation enables per-set models
- `index_type` field supports alternative index algorithms

## Troubleshooting

### Set Not Appearing in Search

**Check index status:**

```http
GET /api/v1/embedding-sets/:slug
```

If `index_status` is not "ready", wait for background job to complete or check job queue.

**Verify membership:**

```http
GET /api/v1/embedding-sets/:slug/members
```

Ensure expected notes are members.

**Refresh set:**

```http
POST /api/v1/embedding-sets/:slug/refresh
```

### Empty Search Results

**Check criteria matches notes:**

List members to verify criteria is matching expected notes.

**Verify embedding set parameter:**

Ensure search query uses correct `set` parameter.

**Check embedding status:**

Verify notes in set have embeddings:

```sql
SELECT COUNT(*) FROM embedding WHERE embedding_set_id = 'set-uuid';
```

### Stale Index

**Manual refresh:**

```http
POST /api/v1/embedding-sets/:slug/refresh
```

**Enable auto-refresh:**

```http
PATCH /api/v1/embedding-sets/:slug
{
  "auto_refresh": true
}
```

### Criteria Not Matching

**Test criteria with direct SQL:**

```sql
SELECT n.id, n.title
FROM note n
LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
WHERE n.deleted_at IS NULL
  AND n.id IN (SELECT note_id FROM note_tag WHERE tag_name = 'your-tag')
  AND (n.archived IS FALSE OR n.archived IS NULL);
```

Adjust criteria based on results.

### Performance Issues

**Check set size:**

Very large sets (> 10,000 notes) may have slow index builds.

**Monitor index type:**

HNSW indices faster for large sets than IVFFlat.

**Consider set segmentation:**

Split very large sets into smaller focused sets.

## References

### Database Schema

- Migration: `/migrations/20260117000000_embedding_sets.sql`
- Repository: `/crates/matric-db/src/embedding_sets.rs`
- Models: `/crates/matric-core/src/models.rs`

### API Implementation

- Routes: `/crates/matric-api/src/main.rs` (lines 268-289)
- Handlers: `/crates/matric-api/src/main.rs` (lines 1285-1397)

### MCP Implementation

- Tools: `/mcp-server/index.js` (lines 1007-1165)

### Related Documentation

- Hybrid Search: `/docs/hybrid-search.md` (when available)
- API Reference: `/docs/api-reference.md` (when available)
- MCP Integration: `/docs/mcp-integration.md` (when available)

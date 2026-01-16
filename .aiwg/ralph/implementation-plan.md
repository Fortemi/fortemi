# Matric Memory Feature Implementation Plan

**Issues**: #67-#74 (8 features)
**Target**: 70%+ test coverage with all tests passing
**Scope**: API (Rust) + MCP Server (JavaScript)

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                      MCP Server (JS)                        │
│                     mcp-server/index.js                     │
└─────────────────────────┬───────────────────────────────────┘
                          │ HTTP API calls
┌─────────────────────────▼───────────────────────────────────┐
│                   matric-api (Rust)                         │
│  main.rs (routes) │ handlers.rs (job handlers)              │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                    matric-db (Rust)                         │
│  notes.rs │ links.rs │ search.rs │ embeddings.rs │ tags.rs  │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                  matric-core (Rust)                         │
│          models.rs │ traits.rs │ error.rs                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Issue #67 (P0): search_notes should return title and tags

### Current State
- `SearchHit` struct: `{ note_id, score, snippet }`
- Missing: `title`, `tags`

### Implementation Plan

1. **Extend SearchHit model** (`matric-core/src/models.rs`):
```rust
pub struct SearchHit {
    pub note_id: Uuid,
    pub score: f32,
    pub snippet: Option<String>,
    pub title: Option<String>,      // NEW
    pub tags: Vec<String>,          // NEW
}
```

2. **Update search queries** (`matric-db/src/search.rs`):
   - Join with `note` table for title
   - Join with `note_tag` for tags (aggregated)
   - Update `search()` and `search_filtered()`

3. **Update embeddings search** (`matric-db/src/embeddings.rs`):
   - `find_similar()` needs title and tags join

4. **Update MCP tool** (`mcp-server/index.js`):
   - No changes needed - already passes through API response

### Files Changed
- `crates/matric-core/src/models.rs`
- `crates/matric-db/src/search.rs`
- `crates/matric-db/src/embeddings.rs`
- `crates/matric-search/src/hybrid.rs` (if needed)

---

## Issue #68 (P1): get_note_links should show incoming backlinks

### Current State
- API already returns both `outgoing` and `incoming` in `NoteLinksResponse`
- `links.rs` has `get_outgoing()` and `get_incoming()` methods
- MCP tool may not be exposing incoming links properly

### Analysis
Looking at `mcp-server/index.js:135-136`:
```javascript
case "get_note_links":
  result = await apiRequest("GET", `/api/v1/notes/${args.id}/links`);
```

The API endpoint already returns `{ outgoing, incoming }`. Need to verify this works and update tool description.

### Implementation Plan

1. **Verify API endpoint** (`matric-api/src/main.rs:579-585`):
   - Confirmed: Returns `NoteLinksResponse { outgoing, incoming }`

2. **Update MCP tool description** (`mcp-server/index.js`):
```javascript
{
  name: "get_note_links",
  description: `Get semantic links for a note.

Returns:
- outgoing: Notes this note links TO (related concepts it references)
- incoming: Notes that link TO this note (backlinks from other notes)

Links include note snippets for context.`,
  ...
}
```

3. **Add note titles to link responses** - Enhancement:
   - Update `get_outgoing()` and `get_incoming()` to include linked note titles

### Files Changed
- `mcp-server/index.js` (description update)
- `crates/matric-db/src/links.rs` (optional: add title to Link)

---

## Issue #69 (P1): Add date range filtering

### Current State
- No date filtering in search or list
- Notes have: `created_at_utc`, `updated_at_utc`, `last_accessed_at`

### Implementation Plan

1. **Add date params to ListNotesRequest** (`matric-core/src/traits.rs`):
```rust
pub struct ListNotesRequest {
    // ... existing fields
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub updated_after: Option<DateTime<Utc>>,
    pub updated_before: Option<DateTime<Utc>>,
}
```

2. **Update list() query** (`matric-db/src/notes.rs`):
   - Add WHERE clauses for date filters

3. **Update API endpoint** (`matric-api/src/main.rs`):
   - Add query params to `ListNotesQuery`

4. **Add date filters to search** (`matric-db/src/search.rs`):
   - Add date range support to `search()` and `search_filtered()`

5. **Update MCP tools** (`mcp-server/index.js`):
   - Add `created_after`, `created_before`, etc. to `list_notes` and `search_notes`

### Files Changed
- `crates/matric-core/src/traits.rs`
- `crates/matric-db/src/notes.rs`
- `crates/matric-db/src/search.rs`
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Issue #70 (P2): Bulk create_notes for batch import

### Implementation Plan

1. **Add bulk insert method to NoteRepository** (`matric-core/src/traits.rs`):
```rust
async fn insert_bulk(&self, notes: Vec<CreateNoteRequest>) -> Result<Vec<Uuid>>;
```

2. **Implement bulk insert** (`matric-db/src/notes.rs`):
   - Transaction-based batch insert
   - Return all created IDs

3. **Add API endpoint** (`matric-api/src/main.rs`):
```rust
// POST /api/v1/notes/bulk
async fn create_notes_bulk(
    State(state): State<AppState>,
    Json(body): Json<BulkCreateNotesBody>,
) -> Result<impl IntoResponse, ApiError>
```

4. **Add MCP tool** (`mcp-server/index.js`):
```javascript
{
  name: "bulk_create_notes",
  description: "Create multiple notes in a single batch operation.",
  inputSchema: {
    type: "object",
    properties: {
      notes: {
        type: "array",
        items: {
          type: "object",
          properties: {
            content: { type: "string" },
            tags: { type: "array", items: { type: "string" } },
            revision_mode: { type: "string", enum: ["full", "light", "none"] }
          },
          required: ["content"]
        }
      }
    },
    required: ["notes"]
  }
}
```

### Files Changed
- `crates/matric-core/src/traits.rs`
- `crates/matric-db/src/notes.rs`
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Issue #71 (P2): Collections/folders for hierarchy

### Current State
- `collection_id` exists in note schema
- `Collection` model exists in `models.rs`
- No CRUD operations implemented

### Implementation Plan

1. **Add CollectionRepository trait** (`matric-core/src/traits.rs`):
```rust
#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn create(&self, name: &str, description: Option<&str>, parent_id: Option<Uuid>) -> Result<Uuid>;
    async fn get(&self, id: Uuid) -> Result<Option<Collection>>;
    async fn list(&self, parent_id: Option<Uuid>) -> Result<Vec<Collection>>;
    async fn update(&self, id: Uuid, name: &str, description: Option<&str>) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
    async fn get_notes(&self, id: Uuid) -> Result<Vec<NoteSummary>>;
    async fn move_note(&self, note_id: Uuid, collection_id: Option<Uuid>) -> Result<()>;
}
```

2. **Extend Collection model** (`matric-core/src/models.rs`):
```rust
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,  // For nested hierarchy
    pub created_at_utc: DateTime<Utc>,
    pub note_count: i64,          // Computed field
}
```

3. **Implement PgCollectionRepository** (`matric-db/src/collections.rs` - new file)

4. **Add API endpoints** (`matric-api/src/main.rs`):
   - `GET /api/v1/collections` - List collections
   - `POST /api/v1/collections` - Create collection
   - `GET /api/v1/collections/:id` - Get collection
   - `PATCH /api/v1/collections/:id` - Update collection
   - `DELETE /api/v1/collections/:id` - Delete collection
   - `GET /api/v1/collections/:id/notes` - List notes in collection
   - `POST /api/v1/notes/:id/move` - Move note to collection

5. **Add MCP tools** (`mcp-server/index.js`):
   - `list_collections`, `create_collection`, `get_collection`, `delete_collection`, `move_note_to_collection`

### Files Changed
- `crates/matric-core/src/traits.rs`
- `crates/matric-core/src/models.rs`
- `crates/matric-db/src/lib.rs` (add mod collections)
- `crates/matric-db/src/collections.rs` (new)
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Issue #72 (P2): Export to markdown

### Implementation Plan

1. **Add export functions** (`matric-api/src/main.rs` or new `export.rs`):
```rust
// Export single note
async fn export_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<impl IntoResponse, ApiError>

// Bulk export
async fn export_notes(
    State(state): State<AppState>,
    Query(query): Query<BulkExportQuery>,
) -> Result<impl IntoResponse, ApiError>
```

2. **Export format**:
```markdown
---
id: {uuid}
title: {title}
created: {timestamp}
updated: {timestamp}
tags: [tag1, tag2]
---

{content}
```

3. **API endpoints**:
   - `GET /api/v1/notes/:id/export` - Export single note (returns markdown)
   - `GET /api/v1/notes/:id/export?format=json` - Export as JSON
   - `POST /api/v1/export` - Bulk export (returns zip or concatenated)

4. **Add MCP tools** (`mcp-server/index.js`):
```javascript
{
  name: "export_note",
  description: "Export a note to markdown format with frontmatter metadata.",
  inputSchema: {
    type: "object",
    properties: {
      id: { type: "string" },
      format: { type: "string", enum: ["markdown", "json"], default: "markdown" },
      include_links: { type: "boolean", default: false }
    },
    required: ["id"]
  }
}
```

### Files Changed
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Issue #73 (P3): Graph query for exploration

### Implementation Plan

1. **Add graph query types** (`matric-core/src/models.rs`):
```rust
pub struct GraphNode {
    pub id: Uuid,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub tags: Vec<String>,
    pub link_count: i64,
}

pub struct GraphEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub kind: String,
    pub score: f32,
}

pub struct GraphResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
```

2. **Add graph query method** (`matric-db/src/links.rs`):
```rust
async fn get_graph(&self, center_id: Option<Uuid>, depth: i32, limit: i64) -> Result<GraphResponse>;
```

3. **API endpoint** (`matric-api/src/main.rs`):
   - `GET /api/v1/graph?center={id}&depth=2&limit=50`

4. **MCP tool** (`mcp-server/index.js`):
```javascript
{
  name: "get_knowledge_graph",
  description: "Get the knowledge graph for visualization and exploration.",
  inputSchema: {
    type: "object",
    properties: {
      center: { type: "string", description: "Optional note ID to center graph on" },
      depth: { type: "number", default: 2, description: "Link traversal depth" },
      limit: { type: "number", default: 50, description: "Max nodes to return" }
    }
  }
}
```

### Files Changed
- `crates/matric-core/src/models.rs`
- `crates/matric-db/src/links.rs`
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Issue #74 (P3): Note templates

### Implementation Plan

1. **Add Template model** (`matric-core/src/models.rs`):
```rust
pub struct NoteTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub default_tags: Vec<String>,
    pub created_at_utc: DateTime<Utc>,
}
```

2. **Add TemplateRepository trait** (`matric-core/src/traits.rs`)

3. **Implement PgTemplateRepository** (`matric-db/src/templates.rs` - new file)

4. **API endpoints**:
   - `GET /api/v1/templates` - List templates
   - `POST /api/v1/templates` - Create template
   - `GET /api/v1/templates/:id` - Get template
   - `PATCH /api/v1/templates/:id` - Update template
   - `DELETE /api/v1/templates/:id` - Delete template
   - `POST /api/v1/templates/:id/apply` - Create note from template

5. **MCP tools**:
   - `list_templates`, `get_template`, `create_from_template`

### Files Changed
- `crates/matric-core/src/models.rs`
- `crates/matric-core/src/traits.rs`
- `crates/matric-db/src/lib.rs`
- `crates/matric-db/src/templates.rs` (new)
- `crates/matric-api/src/main.rs`
- `mcp-server/index.js`

---

## Database Schema Changes

```sql
-- Collections enhancement (if not exists)
ALTER TABLE collection ADD COLUMN IF NOT EXISTS parent_id UUID REFERENCES collection(id);
ALTER TABLE collection ADD COLUMN IF NOT EXISTS description TEXT;

-- Templates table (new)
CREATE TABLE IF NOT EXISTS note_template (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    content TEXT NOT NULL,
    default_tags TEXT[] DEFAULT '{}',
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## Implementation Order

| Phase | Issue | Priority | Effort | Dependencies |
|-------|-------|----------|--------|--------------|
| 1 | #67 | P0 | Low | None |
| 2 | #68 | P1 | Low | None |
| 3 | #69 | P1 | Medium | None |
| 4 | #70 | P2 | Medium | None |
| 5 | #71 | P2 | High | Schema migration |
| 6 | #72 | P2 | Medium | None |
| 7 | #73 | P3 | Medium | #68 (links) |
| 8 | #74 | P3 | Medium | Schema migration |

---

## Test Strategy

### Unit Tests (matric-core, matric-db)
- Model serialization/deserialization
- Repository method tests with mock data
- Date parsing and filtering logic

### Integration Tests
- API endpoint tests with test database
- MCP tool response validation

### Target Coverage: 70%+
- Focus on core business logic
- Test happy paths and error cases
- Test date edge cases (timezone handling)

---

## Verification Commands

```bash
# Build all crates
cargo build --all

# Run unit tests
cargo test --all

# Check test coverage
cargo tarpaulin --all --out Html

# Run MCP server (requires API running)
cd mcp-server && npm test
```

---

## Completion Criteria

- [ ] All 8 issues implemented
- [ ] API endpoints documented (OpenAPI)
- [ ] MCP tools updated with descriptions
- [ ] Unit tests for new functions
- [ ] Integration tests for API endpoints
- [ ] Test coverage >= 70%
- [ ] All tests passing
- [ ] `cargo build --release` succeeds
- [ ] `cargo test --all` passes

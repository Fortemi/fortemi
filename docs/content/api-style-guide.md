# API Style Guide

Comprehensive documentation of Fortémi API conventions, patterns, and best practices.

## Table of Contents

- [Audit Summary](#audit-summary)
- [Error Responses](#error-responses)
- [Limits and Pagination](#limits-and-pagination)
- [Parameter Patterns](#parameter-patterns)
- [Response Structures](#response-structures)
- [Boolean Naming Conventions](#boolean-naming-conventions)
- [ID Parameter Naming](#id-parameter-naming)
- [Best Practices](#best-practices)

## Audit Summary

This guide consolidates findings from the API consistency audit (Issue #307) and addresses all identified documentation gaps:

### Issues Addressed

| Issue | Topic | Status |
|-------|-------|--------|
| #307 | API Consistency Audit | Documented |
| #295 | Error Format Documentation | Documented |
| #282 | Bulk Operation Limits | Documented |
| #278 | Required vs Optional Parameters | Documented |
| #262 | Return Value Structures | Documented |
| #246 | Boolean Parameter Naming | Documented |
| #240 | Pagination Inconsistencies | Documented |
| #232 | ID Parameter Naming | Documented |

### Key Findings

1. **Error Responses**: Consistent `{"error": "message"}` format across all endpoints
2. **Pagination**: Most operations use `offset` + `limit`, search uses `limit` only
3. **ID Naming**: Simple `id` for single resources, qualified names for multi-resource ops
4. **Boolean Patterns**: Prefix-based conventions (`include_*`, `exclude_*`, `skip_*`, `*_only`)
5. **Limits**: Operation-specific defaults (list: 50, search: 20, autocomplete: 10)

## Error Responses

All API errors return JSON with a consistent structure.

### Error Format

```json
{
  "error": "Human-readable error message"
}
```

### HTTP Status Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| 400 | Bad Request | Invalid parameters, validation errors, parameter bounds errors |
| 401 | Unauthorized | Missing or invalid authentication token |
| 403 | Forbidden | Valid authentication but insufficient permissions |
| 404 | Not Found | Requested resource doesn't exist |
| 409 | Conflict | Duplicate key violation, constraint violation, state conflict |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Unexpected server error (report to maintainers) |

### Common Error Patterns

The error message field contains these patterns:

| Pattern | Status Code | Meaning |
|---------|-------------|---------|
| "not found" | 404 | Resource doesn't exist |
| "duplicate key" | 409 | Unique constraint violation |
| "validation error" | 400 | Invalid input data |
| "limit must be" | 400 | Parameter outside allowed range |
| "required parameter" | 400 | Missing required parameter |
| "invalid format" | 400 | Malformed UUID, date, or other typed value |

### Examples

**404 Not Found:**
```json
{
  "error": "Note not found"
}
```

**400 Bad Request:**
```json
{
  "error": "limit must be between 1 and 1000"
}
```

**409 Conflict:**
```json
{
  "error": "duplicate key value violates unique constraint"
}
```

## Limits and Pagination

### Pagination Parameters

All list operations support these parameters:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | varies | Maximum results to return |
| `offset` | integer | 0 | Number of results to skip |

### Default Limits by Operation Type

| Operation Type | Default Limit | Max Limit |
|---------------|---------------|-----------|
| List operations | 50 | 1000 |
| Search operations | 20 | 1000 |
| Autocomplete | 10 | 100 |

### Bulk Operation Limits

| Operation | Limit | Notes |
|-----------|-------|-------|
| `bulk_create_notes` | 100 | Maximum notes per batch |
| `purge_notes` | 100 | Maximum notes to delete per batch |
| `list_notes` | 1000 | Maximum `limit` parameter value |
| `search_notes` | 1000 | Maximum `limit` parameter value |
| `autocomplete_tags` | 100 | Maximum suggestions |

### Special Cases

**Unlimited Results:**
- `limit=0` returns all results (no pagination)
- Use with caution on large datasets (may cause timeout or OOM)
- Recommended: Use pagination for user-facing queries

**Error Conditions:**
- Negative `limit` values return 400 error
- Negative `offset` values return 400 error
- `limit` exceeding maximum returns 400 error

**Search Pagination:**
- Search operations use `limit` only (no `offset`)
- Results are not stable across repeated queries (scores may vary)
- For stable pagination, use `list_notes` with filters instead

## Parameter Patterns

### Required Parameters

Always required, no default value:

| Parameter | Used In | Description |
|-----------|---------|-------------|
| `id` | get/update/delete single resource | UUID of the resource |
| `query` | search operations | Search query string |
| `content` | create_note | Note content (markdown) |
| `title` | create_collection | Collection name |
| `pref_label` | create_concept | Preferred label for concept |

### Optional Parameters with Defaults

| Parameter | Default | Used In | Description |
|-----------|---------|---------|-------------|
| `limit` | 50 or 20 | list/search operations | Max results |
| `offset` | 0 | list operations | Skip N results |
| `revision_mode` | "full" | create_note, update_note | AI revision behavior |
| `include_archived` | false | list_notes | Include archived notes |
| `include_content` | false | list_notes | Include full content |
| `include_frontmatter` | false | export_note | Include YAML frontmatter |
| `hybrid_weight` | 0.5 | search_notes | Semantic vs FTS balance (0.0-1.0) |

### Optional Parameters without Defaults

These parameters are optional and have no default value (null/empty):

| Parameter | Used In | Description |
|-----------|---------|-------------|
| `tags` | create_note, update_note | Array of tag IDs |
| `template_id` | create_note | Template to use for new note |
| `parent_id` | create_collection | Parent collection ID |
| `collection_id` | list_notes | Filter by collection |
| `filter_tags` | search_notes | Strict tag filtering |

### Parameter Validation

**Type Validation:**
- UUIDs must be valid v4 format
- Integers must be within allowed range
- Booleans must be `true` or `false`
- Arrays must contain valid elements

**Range Validation:**
- `hybrid_weight`: 0.0 to 1.0
- `limit`: 1 to operation-specific maximum
- `offset`: 0 to 2^31-1

**String Validation:**
- Non-empty strings for required text fields
- URL-safe characters for slugs
- Valid markdown for content fields

## Response Structures

### Single Item Response

Operations that return a single resource (get, create, update):

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "title": "Example Note",
  "content": "Note content...",
  "created_at": "2026-02-02T10:00:00Z",
  "updated_at": "2026-02-02T10:30:00Z"
}
```

**Endpoints:**
- `get_note(id)` → Note object
- `create_note(...)` → Note object
- `update_note(id, ...)` → Note object
- `get_collection(id)` → Collection object
- `get_concept(id)` → Concept object

### List Response

Operations that return multiple resources:

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Note 1",
    "created_at": "2026-02-02T10:00:00Z"
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "title": "Note 2",
    "created_at": "2026-02-02T11:00:00Z"
  }
]
```

**Endpoints:**
- `list_notes(...)` → Array of Note objects
- `list_collections(...)` → Array of Collection objects
- `list_concepts(...)` → Array of Concept objects
- `list_embedding_sets()` → Array of EmbeddingSet objects

### Search Response

Search operations return structured results with metadata:

```json
{
  "notes": [
    {
      "note_id": "550e8400-e29b-41d4-a716-446655440000",
      "title": "Matching Note",
      "score": 0.95,
      "snippet": "...highlighted text...",
      "tags": ["tag-uuid-1", "tag-uuid-2"]
    }
  ],
  "semantic_available": true,
  "warnings": []
}
```

**Fields:**
- `notes`: Array of search results
- `semantic_available`: Boolean, true if semantic embeddings are available
- `warnings`: Array of warning messages (e.g., "No embeddings available")

**Note Fields:**
- `note_id`: UUID of the matching note
- `title`: Note title
- `score`: Relevance score (0.0 to 1.0)
- `snippet`: Highlighted excerpt showing match context
- `tags`: Array of tag UUIDs associated with the note

**Endpoints:**
- `search_notes(...)` → SearchResponse
- `hybrid_search(...)` → SearchResponse

### Deletion Response

Delete operations return null/empty on success:

```json
null
```

**Endpoints:**
- `delete_note(id)` → null
- `delete_collection(id)` → null
- `delete_concept(id)` → null
- `purge_notes(ids)` → null

### Bulk Operation Response

Bulk create operations return an array of created resources:

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Bulk Note 1",
    "created_at": "2026-02-02T10:00:00Z"
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "title": "Bulk Note 2",
    "created_at": "2026-02-02T10:00:00Z"
  }
]
```

**Endpoints:**
- `bulk_create_notes(notes)` → Array of Note objects

### Export Response

Export operations return formatted strings:

```json
"---\ntitle: Note Title\ntags: [tag1, tag2]\n---\n\nNote content..."
```

**Endpoints:**
- `export_note(id, ...)` → Markdown string with optional YAML frontmatter

### Count/Stat Response

Operations returning counts or statistics:

```json
{
  "count": 42,
  "active": 40,
  "archived": 2
}
```

**Endpoints:**
- `get_note_stats()` → Statistics object

## Boolean Naming Conventions

Boolean parameters follow prefix-based conventions to indicate their behavior.

### `include_*` Pattern

Opt-in behavior for additional data or resources.

| Parameter | Default | Used In | Effect When `true` |
|-----------|---------|---------|-------------------|
| `include_archived` | false | list_notes | Include archived notes in results |
| `include_content` | false | list_notes | Include full note content |
| `include_frontmatter` | true | export_note | Add YAML frontmatter to export |
| `include_links` | false | get_note | Include linked notes |
| `include_backlinks` | false | get_note | Include notes linking to this one |

**Convention**: Use `include_*` when the parameter adds optional data to the response.

### `exclude_*` Pattern

Opt-out behavior for filtering results.

| Parameter | Default | Used In | Effect When `true` |
|-----------|---------|---------|-------------------|
| `exclude_archived` | false | search_notes | Exclude archived notes from results |
| `exclude_empty` | false | list_collections | Exclude collections with no notes |

**Convention**: Use `exclude_*` when the parameter removes items from the result set.

### `skip_*` Pattern

Skip automatic processing steps.

| Parameter | Default | Used In | Effect When `true` |
|-----------|---------|---------|-------------------|
| `skip_snapshot` | false | update_note | Skip creating version snapshot |
| `skip_embedding_regen` | false | update_note | Skip regenerating embeddings |
| `skip_linking` | false | create_note | Skip automatic link detection |
| `skip_revision` | false | create_note | Skip AI revision |

**Convention**: Use `skip_*` when the parameter bypasses an automatic operation.

### `*_only` Pattern

Filter to a specific subset of results.

| Parameter | Default | Used In | Effect When `true` |
|-----------|---------|---------|-------------------|
| `starred_only` | false | list_notes | Show only starred notes |
| `top_only` | false | list_collections | Show only top-level collections |
| `orphaned_only` | false | list_notes | Show only notes without collections |

**Convention**: Use `*_only` when the parameter restricts results to a specific category.

### `dry_run` Pattern

Preview mode for destructive operations.

| Parameter | Default | Used In | Effect When `true` |
|-----------|---------|---------|-------------------|
| `dry_run` | false | backup_notes | Preview backup without executing |
| `dry_run` | false | purge_notes | Preview deletion without executing |

**Convention**: Use `dry_run` for operations that would modify or delete data.

### General Boolean Guidelines

**Default Values:**
- Additive operations (`include_*`, `*_only`): default `false`
- Subtractive operations (`exclude_*`, `skip_*`): default `false`
- Destructive operations (`dry_run`): default `false`

**Naming Rules:**
- Use descriptive verbs/nouns (not abbreviations)
- Be explicit about the effect (not ambiguous)
- Avoid double negatives (`skip_no_linking` ❌, use `skip_linking` ✅)

## ID Parameter Naming

### Single-Resource Operations

Use simple `id` for operations on a single resource:

| Endpoint | Parameter | Resource Type |
|----------|-----------|---------------|
| `get_note(id)` | `id` | Note UUID |
| `update_note(id, ...)` | `id` | Note UUID |
| `delete_note(id)` | `id` | Note UUID |
| `get_collection(id)` | `id` | Collection UUID |
| `delete_collection(id)` | `id` | Collection UUID |
| `get_concept(id)` | `id` | Concept UUID |
| `delete_concept(id)` | `id` | Concept UUID |

**Convention**: When an endpoint operates on a single primary resource, use `id`.

### Multi-Resource Operations

Use qualified names when operating on multiple resources:

| Endpoint | Parameters | Description |
|----------|------------|-------------|
| `tag_note_concept(note_id, concept_id)` | `note_id`, `concept_id` | Link note to concept |
| `untag_note_concept(note_id, concept_id)` | `note_id`, `concept_id` | Unlink note from concept |
| `move_note_to_collection(note_id, collection_id)` | `note_id`, `collection_id` | Move note to collection |
| `link_notes(source_id, target_id)` | `source_id`, `target_id` | Create bidirectional link |

**Convention**: Qualify IDs with resource type when multiple resources are involved.

### Filter Parameters

Use qualified names for filter/query parameters:

| Endpoint | Parameter | Description |
|----------|-----------|-------------|
| `list_notes(collection_id=...)` | `collection_id` | Filter notes by collection |
| `list_notes(template_id=...)` | `template_id` | Filter notes by template |
| `search_notes(filter_tags=...)` | `filter_tags` | Strict tag filtering |

**Convention**: Use qualified names for filter parameters even if they're optional.

### Special Cases

**Embedding Sets:**
- Use `slug` instead of `id` (human-readable identifier)
- Example: `get_embedding_set(slug="default")`
- Slugs are URL-safe strings (e.g., "default", "wiki-2024")

**Bulk Operations:**
- Use plural forms for arrays
- Example: `purge_notes(note_ids=[...])`
- Parameter is `note_ids` (plural), not `ids`

**Parent-Child Relationships:**
- Use `parent_id` for hierarchical resources
- Example: `create_collection(parent_id=...)`
- `parent_id=null` creates top-level resource

### ID Format

All IDs are UUIDs (version 4) unless otherwise specified:

```
550e8400-e29b-41d4-a716-446655440000
```

**Validation:**
- Must match UUID v4 format (8-4-4-4-12 hex digits)
- Case-insensitive (normalized to lowercase)
- Reject invalid formats with 400 Bad Request

**Special Identifiers:**
- Slugs: URL-safe strings (alphanumeric + hyphen/underscore)
- Tag URIs: Full URIs for SKOS concepts (e.g., `http://example.org/concept/1`)

## Best Practices

### API Design

1. **Consistency**: Follow established patterns for similar operations
2. **Predictability**: Use conventional status codes and error formats
3. **Documentation**: Document all parameters, defaults, and edge cases
4. **Validation**: Validate inputs early and return clear error messages
5. **Versioning**: Use URL versioning for breaking changes (`/v2/notes`)

### Parameter Design

1. **Required vs Optional**: Make common parameters optional with sensible defaults
2. **Boolean Naming**: Use prefix conventions (`include_`, `exclude_`, `skip_`, `*_only`)
3. **ID Naming**: Use `id` for single resource, qualified names for multiple
4. **Defaults**: Choose safe defaults (opt-in for expensive operations)
5. **Limits**: Enforce reasonable limits to prevent abuse/timeouts

### Response Design

1. **Single Format**: Use consistent JSON structure across endpoints
2. **Error Format**: Always use `{"error": "message"}` format
3. **Null Handling**: Use `null` for missing optional fields, omit or use empty array/object for collections
4. **Timestamps**: Always ISO 8601 format with timezone (UTC)
5. **Metadata**: Include metadata for search/list responses (scores, warnings, pagination info)

### Error Handling

1. **User-Friendly**: Write error messages for developers (be specific)
2. **Actionable**: Include what went wrong and how to fix it
3. **Status Codes**: Use correct HTTP status codes
4. **No Leaks**: Don't expose internal stack traces or database errors
5. **Logging**: Log errors server-side for debugging

### Documentation

1. **Examples**: Provide real examples for all endpoints
2. **Edge Cases**: Document special cases (limit=0, dry_run, etc.)
3. **Migrations**: Document breaking changes and migration paths
4. **Changelog**: Maintain changelog for API changes
5. **OpenAPI**: Generate OpenAPI/Swagger spec from code

### Testing

1. **Happy Path**: Test successful operations
2. **Error Cases**: Test validation errors, not found, conflicts
3. **Edge Cases**: Test limits, empty arrays, null values
4. **Regression**: Add tests for bug fixes
5. **Integration**: Test end-to-end workflows

## Reference

### Related Documentation

- [MCP Tools Reference](../mcp-server/README.md) - MCP server tool documentation
- [API Routes](../../crates/matric-api/src/main.rs) - HTTP endpoint implementations
- [Search Documentation](./search-operators.md) - Search query syntax and operators
- [Embedding Pipeline](./embedding-pipeline.md) - Embedding lifecycle and configuration

### Issues Resolved

This guide resolves the following documentation issues:

- #307: API Consistency Audit Summary
- #295: Error Format Documentation
- #282: Bulk Operation Limits
- #278: Required vs Optional Parameters
- #262: Return Value Structures
- #246: Boolean Parameter Naming
- #240: Pagination Inconsistencies
- #232: ID Parameter Naming

### Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-02-02 | Initial comprehensive API style guide |

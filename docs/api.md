# API Reference

Matric Memory provides a RESTful API for AI-enhanced note management with semantic search capabilities.

**Base URL**: `https://memory.integrolabs.net`

**OpenAPI Spec**: [openapi.yaml](../crates/matric-api/src/openapi.yaml)

## Authentication

### OAuth2 (Recommended)

The API supports full OAuth2 with Dynamic Client Registration (RFC 7591).

```bash
# 1. Discover endpoints
curl https://memory.integrolabs.net/.well-known/oauth-authorization-server

# 2. Register client
curl -X POST https://memory.integrolabs.net/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name": "My App", "grant_types": ["client_credentials"]}'

# 3. Get token
curl -X POST https://memory.integrolabs.net/oauth/token \
  -d "grant_type=client_credentials&client_id=xxx&client_secret=yyy"
```

### API Keys (Simple)

For trusted integrations, use API key authentication:

```bash
curl -H "Authorization: Bearer mm_key_xxx" \
  https://memory.integrolabs.net/api/v1/notes
```

Create API keys via POST `/api/v1/api-keys`.

## Notes

### Create Note

```http
POST /api/v1/notes
Content-Type: application/json
Authorization: Bearer <token>

{
  "content": "# My Note\n\nNote content in markdown...",
  "tags": ["project", "ideas"],
  "revision_mode": "full"
}
```

**Parameters:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| content | string | Yes | Markdown content |
| tags | string[] | No | Tags to apply |
| revision_mode | string | No | `full` (default), `light`, or `none` |

**Response (201 Created):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "title": "AI-generated title",
  "content_original": "# My Note\n\n...",
  "content_revised": "# My Note\n\nEnhanced content...",
  "tags": ["project", "ideas"],
  "created_at_utc": "2026-01-24T12:00:00Z",
  "updated_at_utc": "2026-01-24T12:00:00Z"
}
```

### Get Note

```http
GET /api/v1/notes/{id}
```

Returns the full note with original and revised content, tags, and semantic links.

### Update Note

```http
PUT /api/v1/notes/{id}
Content-Type: application/json

{
  "content": "Updated content...",
  "starred": true,
  "archived": false
}
```

### Delete Note

```http
DELETE /api/v1/notes/{id}
```

Soft-deletes the note. Can be restored later.

### List Notes

```http
GET /api/v1/notes?limit=50&offset=0&filter=starred
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| limit | int | Max results (default: 50) |
| offset | int | Pagination offset |
| filter | string | `starred` or `archived` |
| tags | string | Comma-separated tag filter |
| created_after | ISO8601 | Date filter |
| created_before | ISO8601 | Date filter |

## Search

### Hybrid Search

```http
GET /api/v1/search?query=machine+learning&mode=hybrid&limit=20
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| query | string | Search query (required) |
| mode | string | `hybrid` (default), `fts`, or `semantic` |
| limit | int | Max results (default: 20) |
| strict_filter | object | Strict tag filter (see below) |

**Response:**

```json
{
  "results": [
    {
      "note_id": "550e8400-...",
      "score": 0.85,
      "snippet": "...machine learning algorithms...",
      "title": "ML Research Notes",
      "tags": ["ml", "research"]
    }
  ],
  "total": 42
}
```

**Search Modes:**

- `hybrid`: Combines FTS + semantic (best for most queries)
- `fts`: Full-text search only (exact keyword matching)
- `semantic`: Vector similarity only (conceptual matching)

### Strict Tag Filtering

Apply guaranteed tag-based filtering **before** fuzzy search. Unlike query string filters, strict filters guarantee exact matches.

```http
POST /api/v1/search
Content-Type: application/json

{
  "query": "authentication",
  "mode": "hybrid",
  "strict_filter": {
    "required_tags": ["project:matric"],
    "any_tags": ["priority:high", "priority:critical"],
    "excluded_tags": ["status:archived"],
    "required_schemes": ["client-acme"]
  }
}
```

**Strict Filter Parameters:**

| Field | Type | Logic | Description |
|-------|------|-------|-------------|
| required_tags | string[] | AND | Notes MUST have ALL these tags |
| any_tags | string[] | OR | Notes MUST have AT LEAST ONE of these |
| excluded_tags | string[] | NOT | Notes MUST NOT have ANY of these |
| required_schemes | string[] | Isolation | Notes ONLY from these vocabulary schemes |
| excluded_schemes | string[] | Exclusion | Notes NOT from these schemes |
| min_tag_count | int | - | Minimum number of tags required |
| include_untagged | bool | - | Include notes with no tags (default: true) |

**Use Cases:**

- **Client isolation**: `"required_schemes": ["client-acme"]`
- **Project search**: `"required_tags": ["project:matric"]`
- **Priority filter**: `"any_tags": ["priority:high", "priority:critical"]`
- **Exclude drafts**: `"excluded_tags": ["draft", "wip", "internal"]`

### Advanced Filters (Query String)

```http
GET /api/v1/search?query=api&tag:backend&created_after:2026-01-01
```

Filter syntax in query string (soft filtering, combined with fuzzy search):
- `tag:name` - Filter by tag
- `collection:uuid` - Filter by collection
- `created_after:ISO8601` - Date range
- `created_before:ISO8601` - Date range

## Tags

### List Tags

```http
GET /api/v1/tags
```

Returns all tags with usage counts.

### Set Note Tags

```http
PUT /api/v1/notes/{id}/tags
Content-Type: application/json

{
  "tags": ["updated", "tags"]
}
```

## Collections

### List Collections

```http
GET /api/v1/collections?parent_id=<uuid>
```

### Create Collection

```http
POST /api/v1/collections
Content-Type: application/json

{
  "name": "Work Projects",
  "description": "Work-related notes",
  "parent_id": null
}
```

### Move Note to Collection

```http
PUT /api/v1/notes/{note_id}/collection
Content-Type: application/json

{
  "collection_id": "550e8400-..."
}
```

## Links

### Get Note Links

```http
GET /api/v1/notes/{id}/links
```

Returns bidirectional semantic links:

```json
{
  "outgoing": [
    {"to_note_id": "...", "score": 0.82, "kind": "semantic"}
  ],
  "incoming": [
    {"from_note_id": "...", "score": 0.78, "kind": "semantic"}
  ]
}
```

### Explore Graph

```http
GET /api/v1/notes/{id}/graph?depth=2&max_nodes=50
```

Traverses semantic links to discover connected notes.

## Templates

### List Templates

```http
GET /api/v1/templates
```

### Create Template

```http
POST /api/v1/templates
Content-Type: application/json

{
  "name": "Meeting Notes",
  "content": "# Meeting: {{topic}}\n\nDate: {{date}}\n\n## Attendees\n{{attendees}}",
  "default_tags": ["meeting"]
}
```

### Instantiate Template

```http
POST /api/v1/templates/{id}/instantiate
Content-Type: application/json

{
  "variables": {
    "topic": "Sprint Planning",
    "date": "2026-01-24",
    "attendees": "Alice, Bob"
  }
}
```

## Jobs

Background processing status for AI operations.

### List Jobs

```http
GET /api/v1/jobs?status=pending&job_type=ai_revision
```

### Queue Stats

```http
GET /api/v1/jobs/stats
```

Returns queue health metrics:

```json
{
  "pending": 5,
  "processing": 2,
  "completed_last_hour": 150,
  "failed_last_hour": 0
}
```

## Export

### Export Note as Markdown

```http
GET /api/v1/notes/{id}/export?content=revised&include_frontmatter=true
```

Returns markdown with YAML frontmatter suitable for Obsidian/Notion import.

## Health

### Health Check

```http
GET /health
```

Returns `200 OK` if the service is healthy.

## Error Responses

All errors follow a consistent format:

```json
{
  "error": "not_found",
  "message": "Note not found",
  "details": {
    "note_id": "550e8400-..."
  }
}
```

**Common Error Codes:**

| Status | Error | Description |
|--------|-------|-------------|
| 400 | bad_request | Invalid request parameters |
| 401 | unauthorized | Missing or invalid authentication |
| 403 | forbidden | Insufficient permissions |
| 404 | not_found | Resource not found |
| 429 | rate_limited | Too many requests |
| 500 | internal_error | Server error |

## Rate Limiting

- Default: 100 requests/minute per API key
- Search: 30 requests/minute
- AI operations: 10 requests/minute

Rate limit headers:
- `X-RateLimit-Limit`: Request limit
- `X-RateLimit-Remaining`: Remaining requests
- `X-RateLimit-Reset`: Reset timestamp

## Versioning

The API is versioned via URL path (`/api/v1/`). Breaking changes will increment the version number.

## See Also

- [MCP Server Documentation](./mcp.md) - Claude integration
- [Authentication Guide](./authentication.md) - OAuth2 flows
- [Integration Guide](./integration.md) - Client examples

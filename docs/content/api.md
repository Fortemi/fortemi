# API Reference

Fortémi provides a RESTful API for AI-enhanced note management with semantic search capabilities.

**Base URL**: `http://localhost:3000`

**OpenAPI Spec**: [openapi.yaml](../../crates/matric-api/src/openapi.yaml)

## Authentication

### OAuth2 (Recommended)

The API supports full OAuth2 with Dynamic Client Registration (RFC 7591).

```bash
# 1. Discover endpoints
curl http://localhost:3000/.well-known/oauth-authorization-server

# 2. Register client
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name": "My App", "grant_types": ["client_credentials"]}'

# 3. Get token
curl -X POST http://localhost:3000/oauth/token \
  -d "grant_type=client_credentials&client_id=xxx&client_secret=yyy"
```

**OAuth2 Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/.well-known/oauth-authorization-server` | GET | OAuth2 discovery metadata |
| `/.well-known/oauth-protected-resource` | GET | Protected resource metadata |
| `/oauth/authorize` | GET, POST | Authorization endpoint |
| `/oauth/register` | POST | Dynamic client registration (RFC 7591) |
| `/oauth/token` | POST | Token endpoint |
| `/oauth/introspect` | POST | Token introspection (RFC 7662) |
| `/oauth/revoke` | POST | Token revocation (RFC 7009) |

### API Keys (Simple)

For trusted integrations, use API key authentication:

```bash
curl -H "Authorization: Bearer mm_key_xxx" \
  http://localhost:3000/api/v1/notes
```

**API Key Management:**

```http
# List API keys
GET /api/v1/api-keys

# Create API key
POST /api/v1/api-keys
Content-Type: application/json

{
  "name": "My Integration Key",
  "expires_at": "2027-01-01T00:00:00Z"
}

# Revoke API key
DELETE /api/v1/api-keys/{id}
```

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
PATCH /api/v1/notes/{id}
Content-Type: application/json

{
  "content": "Updated content...",
  "starred": true,
  "archived": false
}
```

### Update Note Status

Quick endpoint for status-only updates:

```http
PATCH /api/v1/notes/{id}/status
Content-Type: application/json

{
  "starred": true,
  "archived": false
}
```

### Delete Note

```http
DELETE /api/v1/notes/{id}
```

Soft-deletes the note. Can be restored later.

### Restore Note

```http
POST /api/v1/notes/{id}/restore
```

Restores a soft-deleted note.

### Purge Note

```http
POST /api/v1/notes/{id}/purge
```

Permanently deletes a note and all associated data.

### Reprocess Note

```http
POST /api/v1/notes/{id}/reprocess
```

Queues a note for AI reprocessing (re-embedding, re-revision).

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

### Bulk Create Notes

```http
POST /api/v1/notes/bulk
Content-Type: application/json

{
  "notes": [
    {
      "content": "# Note 1",
      "tags": ["batch"]
    },
    {
      "content": "# Note 2",
      "tags": ["batch"]
    }
  ]
}
```

## Note Versioning

Fortémi maintains dual-track versioning: **original** (user-written) and **revised** (AI-enhanced) histories.

### List Note Versions

```http
GET /api/v1/notes/{id}/versions
```

Returns all versions of a note with metadata.

**Response:**

```json
{
  "versions": [
    {
      "version": 3,
      "created_at": "2026-01-24T15:30:00Z",
      "change_summary": "Updated section on authentication",
      "content_hash": "sha256:abc123..."
    },
    {
      "version": 2,
      "created_at": "2026-01-24T12:00:00Z",
      "change_summary": "Initial revision",
      "content_hash": "sha256:def456..."
    }
  ]
}
```

### Get Specific Version

```http
GET /api/v1/notes/{id}/versions/{version}
```

Returns the full content of a specific version.

### Restore Version

```http
POST /api/v1/notes/{id}/versions/{version}/restore
```

Restores a note to a previous version, creating a new version in the process.

### Delete Version

```http
DELETE /api/v1/notes/{id}/versions/{version}
```

Deletes a specific version (cannot delete current version).

### Diff Versions

```http
GET /api/v1/notes/{id}/versions/diff?from=2&to=3
```

Returns a unified diff between two versions.

**Response:**

```json
{
  "from_version": 2,
  "to_version": 3,
  "diff": "--- Version 2\n+++ Version 3\n@@ -10,3 +10,4 @@\n-Old line\n+New line"
}
```

## Note Provenance

### Get Provenance Chain

```http
GET /api/v1/notes/{id}/provenance
```

Returns the W3C PROV provenance chain showing the full AI processing history.

**Response:**

```json
{
  "note_id": "550e8400-...",
  "provenance": [
    {
      "activity": "ai_revision",
      "agent": "ollama:llama3.2",
      "timestamp": "2026-01-24T12:00:00Z",
      "inputs": ["original_content"],
      "outputs": ["revised_content"],
      "parameters": {
        "model": "llama3.2",
        "temperature": 0.7
      }
    },
    {
      "activity": "embedding_generation",
      "agent": "ollama:mxbai-embed-large",
      "timestamp": "2026-01-24T12:01:00Z"
    }
  ]
}
```

## File Attachments

### Upload File Attachment

```http
POST /api/v1/notes/{id}/attachments
Content-Type: multipart/form-data
Authorization: Bearer <token>

file=@photo.jpg
```

Upload a file attachment to a note. Supported file types include images (JPEG, PNG, GIF, WebP), documents (PDF, DOCX, TXT), and more.

**Response (201 Created):**

```json
{
  "id": "660e8400-e29b-41d4-a716-446655440000",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "filename": "photo.jpg",
  "content_type": "image/jpeg",
  "size_bytes": 2457600,
  "created_at": "2026-01-24T12:00:00Z",
  "storage_path": "attachments/660e8400-e29b-41d4-a716-446655440000.jpg"
}
```

**Example:**

```bash
curl -X POST http://localhost:3000/api/v1/notes/550e8400-e29b-41d4-a716-446655440000/attachments \
  -H "Authorization: Bearer mm_key_xxx" \
  -F "file=@vacation-photo.jpg"
```

### List Note Attachments

```http
GET /api/v1/notes/{id}/attachments
```

Returns all attachments for a specific note.

**Response:**

```json
{
  "attachments": [
    {
      "id": "660e8400-...",
      "filename": "photo.jpg",
      "content_type": "image/jpeg",
      "size_bytes": 2457600,
      "created_at": "2026-01-24T12:00:00Z",
      "has_exif": true,
      "has_location": true
    },
    {
      "id": "770e8400-...",
      "filename": "document.pdf",
      "content_type": "application/pdf",
      "size_bytes": 524288,
      "created_at": "2026-01-24T13:00:00Z",
      "has_exif": false,
      "has_location": false
    }
  ]
}
```

**Example:**

```bash
curl http://localhost:3000/api/v1/notes/550e8400-e29b-41d4-a716-446655440000/attachments \
  -H "Authorization: Bearer mm_key_xxx"
```

### Download Attachment

```http
GET /api/v1/attachments/{id}
```

Downloads the file content with appropriate Content-Type and Content-Disposition headers.

**Response Headers:**

- `Content-Type`: Original file MIME type (e.g., `image/jpeg`)
- `Content-Disposition`: `attachment; filename="photo.jpg"`
- `Content-Length`: File size in bytes

**Example:**

```bash
curl -O http://localhost:3000/api/v1/attachments/660e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer mm_key_xxx"
```

### Get Attachment Metadata

```http
GET /api/v1/attachments/{id}/metadata
```

Returns comprehensive metadata including EXIF data, location provenance, and processing status.

**Response:**

```json
{
  "id": "660e8400-...",
  "filename": "photo.jpg",
  "content_type": "image/jpeg",
  "size_bytes": 2457600,
  "created_at": "2026-01-24T12:00:00Z",
  "exif": {
    "camera_make": "Apple",
    "camera_model": "iPhone 14 Pro",
    "capture_time": "2026-01-24T10:30:45Z",
    "gps_latitude": 37.7749,
    "gps_longitude": -122.4194,
    "gps_altitude": 15.5,
    "orientation": 1,
    "iso": 100,
    "focal_length": "6.86 mm",
    "exposure_time": "1/120",
    "f_number": 1.78
  },
  "provenance": {
    "device_id": "iPhone-12345",
    "device_name": "John's iPhone",
    "software": "iOS 17.2",
    "location": {
      "latitude": 37.7749,
      "longitude": -122.4194,
      "altitude": 15.5,
      "accuracy": 5.0
    }
  },
  "processing": {
    "ocr_completed": true,
    "thumbnail_generated": true,
    "embedding_generated": false
  }
}
```

**Example:**

```bash
curl http://localhost:3000/api/v1/attachments/660e8400-e29b-41d4-a716-446655440000/metadata \
  -H "Authorization: Bearer mm_key_xxx"
```

### Delete Attachment

```http
DELETE /api/v1/attachments/{id}
```

Permanently deletes an attachment and its associated file from storage.

**Response (204 No Content)**

**Example:**

```bash
curl -X DELETE http://localhost:3000/api/v1/attachments/660e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer mm_key_xxx"
```

## Memory Search

Memory search enables temporal-spatial queries on file attachments based on when and where they were captured. Uses a single unified endpoint with parameter-based mode selection.

For comprehensive documentation, see [Memory Search Guide](memory-search.md).

### Search Memories

```http
GET /api/v1/memories/search
```

A single endpoint that switches between location, temporal, and combined modes based on which query parameters are provided.

**Query Parameters:**

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `lat` | float | Conditional | Latitude in decimal degrees (-90 to 90). Required for location/combined mode. |
| `lon` | float | Conditional | Longitude in decimal degrees (-180 to 180). Required for location/combined mode. |
| `radius` | float | No | Search radius in meters (default: 1000) |
| `start` | datetime | Conditional | Start of time range (ISO 8601 or flexible format). Required for time/combined mode. |
| `end` | datetime | Conditional | End of time range (ISO 8601 or flexible format). Required for time/combined mode. |

At least one search dimension is required: `lat`+`lon` for location, `start`+`end` for temporal, or all five for combined.

**Mode Selection:**

| Parameters Provided | Mode | Description |
|---------------------|------|-------------|
| `lat` + `lon` (+ optional `radius`) | `location` | Spatial search, nearest memories |
| `start` + `end` | `time` | Temporal search, memories in time range |
| All five | `combined` | Intersection of spatial + temporal |
| None | 400 error | At least one dimension required |

**Response:**

```json
{
  "mode": "location",
  "results": [
    {
      "provenance_id": "uuid",
      "attachment_id": "uuid",
      "note_id": "uuid",
      "filename": "IMG_1234.jpg",
      "content_type": "image/jpeg",
      "distance_m": 245.7,
      "capture_time_start": "2026-01-15T14:30:00Z",
      "capture_time_end": "2026-01-15T14:30:00Z",
      "location_name": "Eiffel Tower",
      "event_type": "photo"
    }
  ],
  "count": 1
}
```

**Examples:**

```bash
# Location search: memories within 1km of a point
curl "http://localhost:3000/api/v1/memories/search?lat=37.7749&lon=-122.4194&radius=1000" \
  -H "Authorization: Bearer mm_key_xxx"

# Temporal search: memories from January 2026
curl "http://localhost:3000/api/v1/memories/search?start=2026-01-01&end=2026-02-01" \
  -H "Authorization: Bearer mm_key_xxx"

# Combined search: near a location during a specific week
curl "http://localhost:3000/api/v1/memories/search?lat=37.7749&lon=-122.4194&radius=5000&start=2026-01-15&end=2026-01-20" \
  -H "Authorization: Bearer mm_key_xxx"
```

### Get Memory Provenance

```http
GET /api/v1/notes/{id}/memory-provenance
```

Returns the complete file provenance chain for a note's attachments, including location, device, and capture time information.

**Response (when provenance exists):**

```json
{
  "note_id": "550e8400-...",
  "files": [
    {
      "attachment_id": "660e8400-...",
      "filename": "photo.jpg",
      "capture_time_start": "2026-01-24T10:30:45Z",
      "location": {
        "latitude": 37.7749,
        "longitude": -122.4194
      },
      "device_name": "iPhone 14 Pro",
      "event_type": "photo"
    }
  ]
}
```

**Response (no provenance):**

```json
{
  "note_id": "550e8400-...",
  "files": []
}
```

**Example:**

```bash
curl http://localhost:3000/api/v1/notes/550e8400-e29b-41d4-a716-446655440000/memory-provenance \
  -H "Authorization: Bearer mm_key_xxx"
```

## Full Document Reconstruction

### Get Full Document

```http
GET /api/v1/notes/{id}/full
```

Reconstructs the full document from chunks, useful for notes split across multiple database records.

**Response:**

```json
{
  "note_id": "550e8400-...",
  "full_content": "# Complete Document\n\n...",
  "chunk_count": 3,
  "total_length": 15234
}
```

## Temporal Queries

Fortémi uses UUIDv7 for temporal ordering.

### Timeline View

```http
GET /api/v1/notes/timeline?limit=50&before=2026-01-24T12:00:00Z
```

Returns notes in temporal order based on UUIDv7 creation time.

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| limit | int | Max results (default: 50) |
| before | ISO8601 | Notes created before this time |
| after | ISO8601 | Notes created after this time |

### Activity View

```http
GET /api/v1/notes/activity?days=7
```

Returns note activity statistics over a time period.

**Response:**

```json
{
  "period_days": 7,
  "notes_created": 42,
  "notes_updated": 18,
  "notes_deleted": 3,
  "daily_breakdown": [
    {
      "date": "2026-01-24",
      "created": 8,
      "updated": 4,
      "deleted": 1
    }
  ]
}
```

## Knowledge Health Dashboard

Monitor the health and quality of your knowledge base.

### Overall Knowledge Health

```http
GET /api/v1/health/knowledge
```

Returns comprehensive knowledge base health metrics.

**Response:**

```json
{
  "total_notes": 1523,
  "orphan_notes": 42,
  "stale_notes": 18,
  "unlinked_notes": 95,
  "avg_links_per_note": 3.2,
  "tag_coverage": 0.87,
  "last_activity": "2026-01-24T15:30:00Z"
}
```

### Orphan Tags

```http
GET /api/v1/health/orphan-tags
```

Lists tags that are defined but not used by any notes.

### Stale Notes

```http
GET /api/v1/health/stale-notes?days=180
```

Returns notes that haven't been updated in N days.

### Unlinked Notes

```http
GET /api/v1/health/unlinked-notes
```

Returns notes with no semantic links to other notes.

### Tag Co-occurrence

```http
GET /api/v1/health/tag-cooccurrence?min_count=5
```

Returns tag co-occurrence statistics for discovering tag relationships.

**Response:**

```json
{
  "pairs": [
    {
      "tag_a": "machine-learning",
      "tag_b": "python",
      "count": 42,
      "correlation": 0.78
    }
  ]
}
```

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

### Get Note Tags

```http
GET /api/v1/notes/{id}/tags
```

Returns all tags applied to a specific note.

### Set Note Tags

```http
PUT /api/v1/notes/{id}/tags
Content-Type: application/json

{
  "tags": ["updated", "tags"]
}
```

Replaces all tags for a note.

## SKOS Concepts

Fortémi implements W3C SKOS (Simple Knowledge Organization System) for controlled vocabularies and semantic tagging.

### Concept Schemes

Concept schemes are top-level vocabularies that organize related concepts.

#### List Concept Schemes

```http
GET /api/v1/concepts/schemes
```

#### Create Concept Scheme

```http
POST /api/v1/concepts/schemes
Content-Type: application/json

{
  "title": "Project Taxonomy",
  "description": "Controlled vocabulary for project classification",
  "namespace": "https://example.org/projects/"
}
```

#### Get Concept Scheme

```http
GET /api/v1/concepts/schemes/{id}
```

#### Update Concept Scheme

```http
PATCH /api/v1/concepts/schemes/{id}
Content-Type: application/json

{
  "title": "Updated Project Taxonomy",
  "description": "Updated description"
}
```

#### Get Top Concepts

```http
GET /api/v1/concepts/schemes/{id}/top-concepts
```

Returns the top-level concepts in a scheme (concepts with no broader concepts).

### Concepts

#### List/Search Concepts

```http
GET /api/v1/concepts?scheme_id={scheme_id}&search=machine
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| scheme_id | UUID | Filter by concept scheme |
| search | string | Search in labels and definitions |
| limit | int | Max results |

#### Autocomplete Concepts

```http
GET /api/v1/concepts/autocomplete?q=mach&scheme_id={scheme_id}
```

Fast autocomplete endpoint for UI type-ahead.

#### Create Concept

```http
POST /api/v1/concepts
Content-Type: application/json

{
  "scheme_id": "550e8400-...",
  "pref_label": "Machine Learning",
  "alt_labels": ["ML", "Statistical Learning"],
  "definition": "A field of AI focused on learning from data",
  "notation": "ML-001"
}
```

#### Get Concept

```http
GET /api/v1/concepts/{id}
```

#### Get Full Concept

```http
GET /api/v1/concepts/{id}/full
```

Returns concept with all relationships (broader, narrower, related) and usage statistics.

#### Update Concept

```http
PATCH /api/v1/concepts/{id}
Content-Type: application/json

{
  "pref_label": "Machine Learning (Updated)",
  "definition": "Updated definition"
}
```

#### Delete Concept

```http
DELETE /api/v1/concepts/{id}
```

Deletes a concept. Fails if the concept is in use by notes.

### Concept Relationships

#### Get Ancestors

```http
GET /api/v1/concepts/{id}/ancestors
```

Returns all ancestor concepts in the hierarchy.

#### Get Descendants

```http
GET /api/v1/concepts/{id}/descendants?depth=2
```

Returns all descendant concepts up to a specified depth.

#### Get Broader Concepts

```http
GET /api/v1/concepts/{id}/broader
```

Returns immediate parent concepts.

#### Add Broader Concept

```http
POST /api/v1/concepts/{id}/broader
Content-Type: application/json

{
  "broader_id": "550e8400-..."
}
```

Establishes a broader/narrower relationship.

#### Get Narrower Concepts

```http
GET /api/v1/concepts/{id}/narrower
```

Returns immediate child concepts.

#### Add Narrower Concept

```http
POST /api/v1/concepts/{id}/narrower
Content-Type: application/json

{
  "narrower_id": "550e8400-..."
}
```

#### Get Related Concepts

```http
GET /api/v1/concepts/{id}/related
```

Returns associatively related concepts (not hierarchical).

#### Add Related Concept

```http
POST /api/v1/concepts/{id}/related
Content-Type: application/json

{
  "related_id": "550e8400-..."
}
```

### Note Tagging with Concepts

#### Get Note Concepts

```http
GET /api/v1/notes/{id}/concepts
```

Returns all SKOS concepts applied to a note.

#### Tag Note with Concept

```http
POST /api/v1/notes/{id}/concepts
Content-Type: application/json

{
  "concept_id": "550e8400-..."
}
```

#### Untag Note Concept

```http
DELETE /api/v1/notes/{id}/concepts/{concept_id}
```

### Governance

#### Get Governance Stats

```http
GET /api/v1/concepts/governance
```

Returns governance and quality metrics for the concept system.

**Response:**

```json
{
  "total_schemes": 5,
  "total_concepts": 342,
  "concepts_with_definitions": 298,
  "concepts_in_use": 215,
  "avg_hierarchy_depth": 3.2,
  "orphan_concepts": 12
}
```

### Export

#### Export Scheme as Turtle

```http
GET /api/v1/concepts/schemes/{id}/export/turtle
```

Exports a concept scheme in RDF Turtle format (W3C SKOS-compatible).

### SKOS Collections

SKOS Collections group related concepts for convenience (W3C SKOS Section 9).

#### List Collections

```http
GET /api/v1/concepts/collections?scheme_id={scheme_id}
```

#### Create Collection

```http
POST /api/v1/concepts/collections
Content-Type: application/json

{
  "scheme_id": "550e8400-...",
  "label": "Core ML Concepts",
  "description": "Essential machine learning concepts"
}
```

#### Get Collection

```http
GET /api/v1/concepts/collections/{id}
```

#### Update Collection

```http
PATCH /api/v1/concepts/collections/{id}
Content-Type: application/json

{
  "label": "Updated Collection Name"
}
```

#### Delete Collection

```http
DELETE /api/v1/concepts/collections/{id}
```

#### Replace Collection Members

```http
PUT /api/v1/concepts/collections/{id}/members
Content-Type: application/json

{
  "concept_ids": ["550e8400-...", "660e8400-..."]
}
```

Replaces all members of a collection.

#### Add Collection Member

```http
POST /api/v1/concepts/collections/{id}/members/{concept_id}
```

#### Remove Collection Member

```http
DELETE /api/v1/concepts/collections/{id}/members/{concept_id}
```


## Document Types

### List Document Types

```http
GET /api/v1/document-types?category={category}
```

Returns all document types, optionally filtered by category.

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| category | string | Filter by category (code, prose, config, markup, data, api-spec, iac, etc.) |

**Response:**

```json
{
  "document_types": [
    {
      "name": "rust",
      "display_name": "Rust",
      "category": "code",
      "file_extensions": [".rs"],
      "filename_patterns": ["Cargo.toml", "Cargo.lock"],
      "chunking_strategy": "syntactic",
      "is_system": true
    }
  ]
}
```

### Get Document Type

```http
GET /api/v1/document-types/:name
```

Returns details for a specific document type.

**Response:**

```json
{
  "name": "rust",
  "display_name": "Rust",
  "category": "code",
  "description": "Rust programming language",
  "file_extensions": [".rs"],
  "filename_patterns": ["Cargo.toml", "Cargo.lock"],
  "content_magic": [],
  "chunking_strategy": "syntactic",
  "syntax_language": "rust",
  "embedding_model_hint": null,
  "is_system": true,
  "created_at": "2026-01-15T10:00:00Z"
}
```

### Create Document Type

```http
POST /api/v1/document-types
Content-Type: application/json

{
  "name": "my-custom-type",
  "display_name": "My Custom Type",
  "category": "custom",
  "description": "Custom document type for specialized content",
  "file_extensions": [".mytype"],
  "filename_patterns": ["*.mytype"],
  "content_magic": ["^MYTYPE:"],
  "chunking_strategy": "semantic",
  "syntax_language": null,
  "embedding_model_hint": null
}
```

Creates a custom document type.

**Parameters:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| name | string | Yes | Unique identifier (lowercase, hyphens) |
| display_name | string | Yes | Human-readable name |
| category | string | Yes | Category: code, prose, config, markup, data, api-spec, iac, database, shell, docs, package, observability, legal, communication, research, creative, media, personal, custom |
| description | string | No | Description of the document type |
| file_extensions | string[] | No | File extensions (e.g., [".rs", ".rust"]) |
| filename_patterns | string[] | No | Exact filename patterns (e.g., ["Cargo.toml"]) |
| content_magic | string[] | No | Regex patterns for content detection |
| chunking_strategy | string | Yes | semantic, syntactic, fixed, per_section, whole |
| syntax_language | string | No | Language for syntactic chunking |
| embedding_model_hint | string | No | Recommended embedding model |

**Response (201 Created):**

```json
{
  "name": "my-custom-type",
  "display_name": "My Custom Type",
  "category": "custom",
  "is_system": false,
  ...
}
```

### Update Document Type

```http
PATCH /api/v1/document-types/:name
Content-Type: application/json

{
  "display_name": "Updated Display Name",
  "description": "Updated description",
  "file_extensions": [".mytype", ".mt"]
}
```

Updates a custom document type. System types cannot be updated.

### Delete Document Type

```http
DELETE /api/v1/document-types/:name
```

Deletes a custom document type. System types cannot be deleted.

### Detect Document Type

```http
POST /api/v1/document-types/detect
Content-Type: application/json

{
  "filename": "docker-compose.yml",
  "content": "version: '3.8'\nservices:"
}
```

Auto-detects document type from filename and/or content.

**Parameters:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| filename | string | No | Filename to analyze |
| content | string | No | Content to analyze (first 1KB sufficient) |

At least one of filename or content must be provided.

**Response:**

```json
{
  "document_type": "docker-compose",
  "confidence": 0.9,
  "detection_method": "filename_pattern",
  "category": "iac",
  "chunking_strategy": "per_section",
  "alternatives": [
    {
      "document_type": "yaml",
      "confidence": 0.5,
      "detection_method": "extension"
    }
  ]
}
```

**Detection Methods:**

| Method | Confidence | Description |
|--------|------------|-------------|
| filename_pattern | 1.0 | Exact pattern match (e.g., `Dockerfile`, `docker-compose.yml`) |
| extension | 0.9 | File extension match (e.g., `.rs` → rust) |
| content_magic | 0.7 | Content pattern recognition (e.g., `openapi:` → OpenAPI) |
| default | 0.1 | Fallback to generic type |

## Collections

Note collections organize notes into folders with hierarchy support.

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

### Get Collection

```http
GET /api/v1/collections/{id}
```

### Update Collection

```http
PATCH /api/v1/collections/{id}
Content-Type: application/json

{
  "name": "Updated Collection Name",
  "description": "Updated description"
}
```

### Delete Collection

```http
DELETE /api/v1/collections/{id}
```

### Get Collection Notes

```http
GET /api/v1/collections/{id}/notes
```

Returns all notes in a collection.

### Move Note to Collection

```http
POST /api/v1/notes/{note_id}/move
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

### Get Note Backlinks

```http
GET /api/v1/notes/{id}/backlinks
```

Returns only incoming links to a note.

## Graph Exploration

### Explore Graph

```http
GET /api/v1/graph/{id}?depth=2&max_nodes=50
```

Traverses semantic links to discover connected notes using recursive CTEs.

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| depth | int | Maximum traversal depth (default: 2) |
| max_nodes | int | Maximum nodes to return (default: 50) |
| min_score | float | Minimum link score threshold |

**Response:**

```json
{
  "nodes": [
    {
      "id": "550e8400-...",
      "title": "Root Note",
      "depth": 0
    },
    {
      "id": "660e8400-...",
      "title": "Connected Note",
      "depth": 1
    }
  ],
  "edges": [
    {
      "from": "550e8400-...",
      "to": "660e8400-...",
      "score": 0.82
    }
  ]
}
```

## Embedding Sets

Embedding sets allow creating isolated embedding spaces for multi-tenant or specialized use cases.

### List Embedding Sets

```http
GET /api/v1/embedding-sets
```

### Create Embedding Set

```http
POST /api/v1/embedding-sets
Content-Type: application/json

{
  "slug": "client-acme",
  "name": "ACME Corp Knowledge",
  "embedding_config_id": "550e8400-..."
}
```

### Get Embedding Set

```http
GET /api/v1/embedding-sets/{slug}
```

### Update Embedding Set

```http
PATCH /api/v1/embedding-sets/{slug}
Content-Type: application/json

{
  "name": "Updated Name"
}
```

### Delete Embedding Set

```http
DELETE /api/v1/embedding-sets/{slug}
```

### List Embedding Set Members

```http
GET /api/v1/embedding-sets/{slug}/members
```

Returns all notes in an embedding set.

### Add Embedding Set Members

```http
POST /api/v1/embedding-sets/{slug}/members
Content-Type: application/json

{
  "note_ids": ["550e8400-...", "660e8400-..."]
}
```

### Remove Embedding Set Member

```http
DELETE /api/v1/embedding-sets/{slug}/members/{note_id}
```

### Refresh Embedding Set

```http
POST /api/v1/embedding-sets/{slug}/refresh
```

Regenerates embeddings for all notes in the set.

### List Embedding Configs

```http
GET /api/v1/embedding-configs
```

Returns available embedding model configurations.

### Get Default Embedding Config

```http
GET /api/v1/embedding-configs/default
```

### Get Embedding Config

```http
GET /api/v1/embedding-configs/{id}
```

Returns details for a specific embedding configuration.

### Create Embedding Config

```http
POST /api/v1/embedding-configs
Content-Type: application/json

{
  "name": "Custom Config",
  "model": "mxbai-embed-large",
  "dimension": 1024,
  "provider": "ollama",
  "is_default": false
}
```

### Update Embedding Config

```http
PATCH /api/v1/embedding-configs/{id}
Content-Type: application/json

{
  "name": "Updated Config",
  "is_default": true
}
```

### Delete Embedding Config

```http
DELETE /api/v1/embedding-configs/{id}
```

Deletes a non-default embedding configuration.

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

### Get Template

```http
GET /api/v1/templates/{id}
```

### Update Template

```http
PATCH /api/v1/templates/{id}
Content-Type: application/json

{
  "name": "Updated Template Name",
  "content": "Updated template content"
}
```

### Delete Template

```http
DELETE /api/v1/templates/{id}
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

Creates a new note from the template with variables substituted.

## Jobs

Background processing status for AI operations.

### List Jobs

```http
GET /api/v1/jobs?status=pending&job_type=ai_revision
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| status | string | Filter by status: `pending`, `processing`, `completed`, `failed` |
| job_type | string | Filter by type: `ai_revision`, `embedding`, etc. |
| limit | int | Max results |

### Create Job

```http
POST /api/v1/jobs
Content-Type: application/json

{
  "job_type": "ai_revision",
  "target_id": "550e8400-...",
  "parameters": {
    "mode": "full"
  }
}
```

### Get Job

```http
GET /api/v1/jobs/{id}
```

### Pending Jobs Count

```http
GET /api/v1/jobs/pending
```

Returns count of pending jobs.

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
  "failed_last_hour": 0,
  "avg_processing_time_ms": 2341
}
```

## Backup & Export

Fortémi provides multiple backup strategies for different use cases.

### JSON Export/Import (Legacy)

#### Export Backup

```http
GET /api/v1/backup/export
```

Exports all notes and metadata as JSON.

#### Download Backup

```http
GET /api/v1/backup/download
```

Downloads the most recent export as a file.

#### Import Backup

```http
POST /api/v1/backup/import
Content-Type: multipart/form-data

file=@backup.json
```

Imports notes from a JSON export.

#### Trigger Backup

```http
POST /api/v1/backup/trigger
```

Manually triggers a backup job.

#### Backup Status

```http
GET /api/v1/backup/status
```

Returns status of the most recent backup operation.

### Knowledge Shards (Portable Exports)

Knowledge shards are application-level exports that include notes, concepts, and metadata but exclude embeddings.

#### Export Knowledge Shard

```http
GET /api/v1/backup/knowledge-shard?format=json
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| format | string | Export format: `json` or `yaml` |
| include_deleted | bool | Include soft-deleted notes |

#### Import Knowledge Shard

```http
POST /api/v1/backup/knowledge-shard/import
Content-Type: multipart/form-data

file=@knowledge-shard.json
```

### Database Backups (Full pg_dump)

Full PostgreSQL backups including embeddings and all data.

#### Download Database Backup

```http
GET /api/v1/backup/database
```

Downloads a full `pg_dump` of the database.

#### Create Database Snapshot

```http
POST /api/v1/backup/database/snapshot
Content-Type: application/json

{
  "label": "pre-migration-backup"
}
```

Creates a named database snapshot.

#### Upload Database Backup

```http
POST /api/v1/backup/database/upload
Content-Type: multipart/form-data

file=@backup.sql
```

Uploads a database backup file for later restoration.

#### Restore Database Backup

```http
POST /api/v1/backup/database/restore
Content-Type: application/json

{
  "filename": "backup_20260124_120000.sql"
}
```

Restores the database from a backup file. **WARNING: This will overwrite all current data.**

### Knowledge Archives

Knowledge archives bundle a knowledge shard with metadata in a single `.archive` file.

#### Download Knowledge Archive

```http
GET /api/v1/backup/knowledge-archive/{filename}
```

#### Upload Knowledge Archive

```http
POST /api/v1/backup/knowledge-archive
Content-Type: multipart/form-data

file=@knowledge-archive.archive
```

### Backup Browser

#### List Backups

```http
GET /api/v1/backup/list
```

Returns all available backup files.

**Response:**

```json
{
  "backups": [
    {
      "filename": "backup_20260124_120000.sql",
      "size_bytes": 15234567,
      "created_at": "2026-01-24T12:00:00Z",
      "type": "database",
      "label": "pre-migration-backup"
    }
  ]
}
```

#### Get Backup Info

```http
GET /api/v1/backup/list/{filename}
```

Returns detailed information about a specific backup file.

#### Swap Backup

```http
POST /api/v1/backup/swap
Content-Type: application/json

{
  "backup_filename": "backup_20260124_120000.sql"
}
```

Swaps the current database with a backup (creates a backup of current state first).

### Backup Metadata

#### Get Backup Metadata

```http
GET /api/v1/backup/metadata/{filename}
```

Returns metadata for a backup file.

#### Update Backup Metadata

```http
PUT /api/v1/backup/metadata/{filename}
Content-Type: application/json

{
  "label": "Updated label",
  "description": "Updated description",
  "tags": ["important", "pre-migration"]
}
```

## Export

### Export Note as Markdown

```http
GET /api/v1/notes/{id}/export?content=revised&include_frontmatter=true
```

Returns markdown with YAML frontmatter suitable for Obsidian/Notion import.

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| content | string | `original` or `revised` (default: `revised`) |
| include_frontmatter | bool | Include YAML frontmatter (default: true) |

## Real-Time Events

Fortémi provides real-time event streaming through three channels. For comprehensive documentation, see [Real-Time Events](./real-time-events.md).

### SSE (Server-Sent Events)

```http
GET /api/v1/events
```

Streams all server events as `text/event-stream`. Each event includes an `event:` type field and `data:` JSON payload. Keep-alive sent every 15 seconds.

### WebSocket

```http
GET /api/v1/ws
```

Full-duplex WebSocket connection receiving JSON-encoded events. Send `"refresh"` to trigger an immediate `QueueStatus` response.

### Webhooks

Full CRUD for webhook subscriptions with event filtering and HMAC-SHA256 signing.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/webhooks` | POST | Create webhook subscription |
| `/api/v1/webhooks` | GET | List all webhooks |
| `/api/v1/webhooks/{id}` | GET | Get webhook details |
| `/api/v1/webhooks/{id}` | PATCH | Update webhook |
| `/api/v1/webhooks/{id}` | DELETE | Delete webhook |
| `/api/v1/webhooks/{id}/deliveries` | GET | List delivery logs |
| `/api/v1/webhooks/{id}/test` | POST | Send test delivery |

**Create Webhook:**

```http
POST /api/v1/webhooks
Content-Type: application/json

{
  "url": "https://example.com/webhook",
  "events": ["NoteUpdated", "JobCompleted", "JobFailed"],
  "secret": "optional-hmac-secret"
}
```

**Event Types:** `QueueStatus`, `JobQueued`, `JobStarted`, `JobProgress`, `JobCompleted`, `JobFailed`, `NoteUpdated`

Webhook deliveries include `X-Fortemi-Event` header and optional `X-Fortemi-Signature` (HMAC-SHA256) when a secret is configured.

## System

### Memory Info

```http
GET /api/v1/memory/info
```

Returns system memory usage information.

**Response:**

```json
{
  "total_bytes": 16777216000,
  "used_bytes": 8388608000,
  "available_bytes": 8388608000,
  "percent_used": 50.0
}
```

### Rate Limit Status

```http
GET /api/v1/rate-limit/status
```

Returns current rate limit status for the authenticated client.

**Response:**

```json
{
  "limit": 100,
  "remaining": 87,
  "reset_at": "2026-01-24T12:01:00Z",
  "retry_after_seconds": 45
}
```

## Health

### Health Check

```http
GET /health
```

Returns `200 OK` if the service is healthy.

**Response:**

```json
{
  "status": "ok",
  "version": "2026.1.0",
  "database": "connected",
  "ollama": "connected"
}
```

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
- [Real-Time Events](./real-time-events.md) - SSE, WebSocket, and webhook event streaming
- [Authentication Guide](./authentication.md) - OAuth2 flows
- [Integration Guide](./integration.md) - Client examples

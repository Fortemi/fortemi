# ADR-031: Intelligent Attachment Processing with Document Type Integration

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-025 (Document Type Registry), Epic #430 (File Attachments)

## Context

Matric-memory has two powerful systems that should work together:

1. **Document Types (Doctypes)**: 131+ pre-configured types with chunking strategies, AI templates, and extraction rules
2. **File Attachments**: Storage, processing, and embedding of attached files

Currently, these systems are designed independently. Users uploading files must manually specify document types, and the attachment system doesn't leverage doctype templates for AI-enhanced content generation.

**Key insight**: When a user uploads a PDF or video with minimal guidance, the system should be intelligent enough to:
1. Auto-detect the appropriate document type
2. Extract and process content according to doctype rules
3. Auto-generate an AI-enhanced note using doctype templates
4. Make the content searchable with appropriate embeddings

### Use Case: Personal Memory System

Users record personal videos on their phones to remember places, people, and events. The system should:
- Extract temporal/spatial metadata (when/where)
- Transcribe audio, analyze video frames
- Use AI to reconstruct detailed memories
- Create searchable, contextual notes automatically

## Decision

Implement **intelligent attachment processing** that deeply integrates with the document type system:

### 1. Doctype-Driven Processing

Each document type defines an `extraction_strategy` that controls how attached files are processed:

```sql
ALTER TABLE document_type ADD COLUMN extraction_strategy TEXT DEFAULT 'text_native';
ALTER TABLE document_type ADD COLUMN extraction_config JSONB DEFAULT '{}';
ALTER TABLE document_type ADD COLUMN requires_attachment BOOLEAN DEFAULT FALSE;
ALTER TABLE document_type ADD COLUMN attachment_generates_content BOOLEAN DEFAULT FALSE;
```

**Extraction strategies:**
- `text_native`: Direct text extraction (plaintext, markdown)
- `pdf_text`: PDF text extraction with OCR fallback
- `pdf_scanned`: Force OCR for scanned documents
- `vision`: Image analysis via vision models (LLaVA)
- `audio_transcribe`: Audio transcription (Whisper)
- `video_multimodal`: Video = frames + audio + temporal indexing
- `code_ast`: Code parsing with tree-sitter + LLM summary
- `office_convert`: Office document conversion (pandoc)
- `structured_extract`: JSON/YAML/XML data extraction

### 2. Auto-Document Creation

When file content IS the document (not supplementary), the attachment becomes the canonical content source:

```sql
ALTER TABLE file_attachment ADD COLUMN is_canonical_content BOOLEAN DEFAULT FALSE;
ALTER TABLE file_attachment ADD COLUMN generated_note_id UUID REFERENCES note(id);
```

**Flow:**
1. User uploads `vacation-beach-2026.mp4` with optional title
2. System detects MIME type → maps to `personal-memory` doctype
3. Processing job extracts content per doctype strategy
4. AI generates note content using doctype's `agentic_config.generation_prompt`
5. Note is created with extracted text as content, attachment as canonical source
6. Embeddings generated per doctype rules

### 3. Template Variable System

Doctype templates receive attachment-derived variables:

```handlebars
## {{title}}

**Captured:** {{capture_time}} at {{location_name}}

### Memory
{{ai_description}}

### What Happened
{{transcript}}

### Key Moments
{{#each scene_descriptions}}
- **{{timestamp}}**: {{description}}
{{/each}}

### People & Places
{{#if people}}People: {{join people ", "}}{{/if}}
{{#if places}}Places: {{join places ", "}}{{/if}}

---
*Auto-generated from {{filename}} ({{content_type}})*
```

**Available variables:**
- `{{ai_description}}`: Vision/multimodal AI description
- `{{transcript}}`: Audio transcription
- `{{extracted_text}}`: Raw extracted text
- `{{scene_descriptions}}`: Timestamped scene analysis
- `{{capture_time}}`, `{{location_name}}`: Temporal/spatial provenance
- `{{filename}}`, `{{content_type}}`, `{{file_size}}`: File metadata
- `{{people}}`, `{{places}}`, `{{objects}}`: Detected entities

### 4. Dual Search: Notes + Attachments

Attachments are searchable independently AND as part of notes:

```sql
-- Search attachments exclusively
SELECT * FROM file_attachment
WHERE to_tsvector('english', extracted_text) @@ websearch_to_tsquery($1)
  AND processing_status = 'complete';

-- Search with attachment type filter
SELECT n.* FROM note n
LEFT JOIN file_attachment fa ON fa.note_id = n.id
WHERE
  to_tsvector('english', n.content || COALESCE(fa.extracted_text, '')) @@ websearch_to_tsquery($1)
  AND ($2::text IS NULL OR fa.content_type LIKE $2);  -- 'image/%', 'video/%', etc.
```

**Search modifiers:**
- `attachment:any` - Notes with any attachment
- `attachment:image` - Notes with image attachments
- `attachment:video` - Notes with video attachments
- `attachment:none` - Notes without attachments
- `attachment:canonical` - Notes where attachment is primary content

### 5. Full Provenance Tracking

Every attachment tracks complete provenance:

```sql
CREATE TABLE file_provenance (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID REFERENCES file_attachment(id),

    -- Origin
    upload_source TEXT,           -- 'api', 'mcp', 'mobile', 'import'
    original_filename TEXT,
    original_path TEXT,           -- If from local filesystem

    -- Temporal
    capture_time TIMESTAMPTZ,     -- When content was created (EXIF)
    capture_timezone TEXT,
    event_duration_seconds REAL,

    -- Spatial
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    altitude_m REAL,
    location_accuracy_m REAL,
    location_name TEXT,           -- Reverse geocoded or user-specified

    -- Device
    device_make TEXT,
    device_model TEXT,
    software TEXT,

    -- Processing history
    processing_log JSONB DEFAULT '[]',  -- Array of processing events

    -- Raw metadata
    exif_data JSONB,
    media_info JSONB,

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Processing log entry example:
-- {"step": "text_extraction", "model": "tesseract", "started": "...", "completed": "...", "tokens": 1234}
```

### 6. Temporal-Spatial Search

Enable "memory queries" based on time and location:

```sql
-- PostGIS extension for spatial queries
CREATE EXTENSION IF NOT EXISTS postgis;

-- Add geography column to provenance
ALTER TABLE file_provenance
    ADD COLUMN point geography(Point, 4326);

CREATE INDEX idx_provenance_point ON file_provenance USING GIST (point);
CREATE INDEX idx_provenance_time ON file_provenance USING BRIN (capture_time);
```

**Query patterns:**
- "Show memories from Paris in December 2025"
- "What happened near here?" (current GPS)
- "Timeline of this location" (all captures at a place)

### 7. Processing Pipeline

```
┌─────────────┐
│   Upload    │
│  (API/MCP)  │
└──────┬──────┘
       │
       v
┌─────────────┐     ┌──────────────┐
│   Detect    │────>│   Doctype    │
│   Doctype   │     │   Registry   │
└──────┬──────┘     └──────────────┘
       │
       v
┌─────────────┐     ┌──────────────┐
│   Extract   │────>│  Provenance  │
│   Metadata  │     │   (EXIF/GPS) │
└──────┬──────┘     └──────────────┘
       │
       v
┌─────────────┐     ┌──────────────┐
│   Process   │────>│  Strategy    │
│   Content   │     │  (per type)  │
└──────┬──────┘     └──────────────┘
       │
       v
┌─────────────┐     ┌──────────────┐
│   AI        │────>│  Template    │
│   Enhance   │     │  Generation  │
└──────┬──────┘     └──────────────┘
       │
       v
┌─────────────┐     ┌──────────────┐
│   Create/   │────>│  Note + Link │
│   Update    │     │  Attachment  │
└──────┬──────┘     └──────────────┘
       │
       v
┌─────────────┐
│   Embed     │
│  (text/CLIP)│
└─────────────┘
```

## Consequences

### Positive

- (+) **Zero-configuration uploads**: Drop a file, get an intelligent note
- (+) **Leverage existing doctypes**: 131 types already define extraction rules
- (+) **Personal memory system**: Time/place-aware content retrieval
- (+) **Search flexibility**: Filter by attachment type, time, location
- (+) **Full audit trail**: Complete provenance for every file
- (+) **MCP agent friendly**: Structured responses for automated workflows

### Negative

- (-) **Processing complexity**: Multi-step async pipeline
- (-) **Model dependencies**: Requires LLaVA, Whisper, etc.
- (-) **Storage growth**: Provenance and extracted text add overhead
- (-) **Template maintenance**: Doctypes need templates maintained

## Implementation

### Schema Changes

1. Extend `document_type` with extraction config
2. Add `file_provenance` table
3. Add `is_canonical_content` to attachments
4. Add PostGIS for spatial queries

### New Job Types

- `attachment_metadata_extraction`: EXIF/media info parsing
- `attachment_content_processing`: Per-strategy extraction
- `attachment_ai_enhancement`: AI description generation
- `attachment_note_creation`: Auto-generate note from attachment

### API Changes

```
POST /notes/{id}/attachments
  ?auto_process=true           # Enable full pipeline (default: true)
  &create_note=true            # Create new note from attachment
  &doctype=personal-memory     # Override detected doctype

GET /attachments/search
  ?q=beach+vacation
  &type=video
  &from=2025-12-01
  &to=2025-12-31
  &near=48.8566,2.3522
  &radius=5000

GET /memories/timeline
  ?lat=48.8566&lon=2.3522
  &radius=1000
```

### MCP Tools

```javascript
// Upload and auto-process
attach_file(note_id?, filename, data, {
  auto_process: true,
  create_note: true,
  doctype: 'personal-memory'
})

// Search memories by location
search_memories({
  query: 'beach vacation',
  near: { lat: 48.8566, lon: 2.3522, radius: 5000 },
  from: '2025-12-01',
  to: '2025-12-31'
})

// Get attachment with provenance
get_attachment(id, { include_provenance: true })
```

## References

- ADR-025: Document Type Registry
- Epic #430: File Attachments with Intelligent Processing
- docs/research/file-attachments-research.md
- docs/research/attachment-provenance-tracking.md

# Architecture Design: File Attachment + Document Type Integration

**Status:** Proposed
**Date:** 2026-02-01
**Author:** Architecture Designer
**Related Issues:** #425 (proposed)

---

## Architecture Overview

This design integrates the existing document type system (131 pre-configured doctypes) with file attachments to enable intelligent, content-aware processing of uploaded files. The architecture transforms uploaded files into semantically-rich notes with AI-enhanced content.

```
+------------------+     +-------------------+     +--------------------+
|   File Upload    |---->| Doctype Detection |---->| Content Extraction |
| (binary + meta)  |     | (MIME/magic/ext)  |     | (per strategy)     |
+------------------+     +-------------------+     +--------------------+
                                  |                         |
                                  v                         v
                         +----------------+     +------------------------+
                         | Processing Job |<--->| AI Enhancement         |
                         | (matric-jobs)  |     | (LLaVA/Whisper/Ollama) |
                         +----------------+     +------------------------+
                                  |
                                  v
                         +-------------------+     +------------------+
                         | Auto-Create Note  |---->| Apply Template   |
                         | (with attachment) |     | (doctype-based)  |
                         +-------------------+     +------------------+
                                  |
                                  v
                         +-------------------+
                         | Embed + Index     |
                         | (per embed rules) |
                         +-------------------+
```

---

## 1. Schema Additions

### 1.1 Attachment Table with Doctype Link

```sql
-- Migration: 20260203000000_attachment_doctype_integration.sql

-- Extraction strategy enum (how to extract text/meaning from file)
CREATE TYPE extraction_strategy AS ENUM (
    'text_native',     -- Native text (Markdown, code, config)
    'pdf_text',        -- PDF text extraction (pdftotext)
    'pdf_ocr',         -- PDF with OCR (scanned documents)
    'pandoc',          -- Office documents (DOCX, PPTX, etc.)
    'vision',          -- Image description (LLaVA)
    'audio_transcribe',-- Audio transcription (Whisper)
    'video_multimodal',-- Video: frames + audio
    'structured_data', -- CSV, Excel, JSON data
    'code_analysis',   -- Code with AST analysis
    'none'             -- No extraction (binary blob)
);

-- Processing status for attachments
CREATE TYPE attachment_status AS ENUM (
    'uploaded',        -- File received, not processed
    'queued',          -- Processing job queued
    'processing',      -- Currently being processed
    'completed',       -- Processing complete
    'failed',          -- Processing failed
    'quarantined'      -- Security concern (virus, malicious)
);

-- Content-addressable blob storage
CREATE TABLE attachment_blob (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    content_hash TEXT NOT NULL UNIQUE,  -- BLAKE3 hash
    content_type TEXT NOT NULL,         -- MIME type
    size_bytes BIGINT NOT NULL,
    storage_type TEXT NOT NULL DEFAULT 'database',  -- 'database' | 'object_storage'

    -- Database storage (files <10MB)
    data BYTEA,

    -- Object storage (files >=10MB)
    object_key TEXT,
    object_bucket TEXT,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reference_count INTEGER NOT NULL DEFAULT 0
);

-- Use EXTERNAL storage for BYTEA (no compression, fast access)
ALTER TABLE attachment_blob ALTER COLUMN data SET STORAGE EXTERNAL;

CREATE INDEX idx_attachment_blob_hash ON attachment_blob(content_hash);
CREATE INDEX idx_attachment_blob_orphan ON attachment_blob(reference_count)
    WHERE reference_count = 0;

-- Attachment records (many-to-one with blob, linked to note)
CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES attachment_blob(id) ON DELETE RESTRICT,

    -- File metadata
    filename TEXT NOT NULL,
    original_filename TEXT,           -- Pre-sanitization name
    display_order INTEGER DEFAULT 0,

    -- Document type integration
    document_type_id UUID REFERENCES document_type(id),
    detected_document_type_id UUID REFERENCES document_type(id),
    detection_confidence FLOAT,
    detection_method TEXT,            -- 'mime', 'extension', 'magic', 'content'

    -- Processing state
    status attachment_status NOT NULL DEFAULT 'uploaded',
    processing_error TEXT,
    processing_job_id UUID,           -- Link to job_queue

    -- Extraction configuration
    extraction_strategy extraction_strategy,
    extraction_config JSONB DEFAULT '{}',

    -- Extracted content
    extracted_text TEXT,              -- For FTS indexing
    extracted_metadata JSONB,         -- EXIF, PDF metadata, etc.
    ai_description TEXT,              -- Vision/audio AI description

    -- Preview/thumbnail
    has_preview BOOLEAN DEFAULT FALSE,
    preview_content_type TEXT,
    preview_blob_id UUID REFERENCES attachment_blob(id),

    -- Security
    virus_scan_status TEXT,           -- 'pending', 'clean', 'infected', 'error'
    virus_scan_at TIMESTAMPTZ,

    -- Flags
    is_canonical_content BOOLEAN DEFAULT FALSE,  -- File IS the note content
    is_ai_generated BOOLEAN DEFAULT FALSE,       -- AI generated the note from file

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT
);

-- Indexes
CREATE INDEX idx_attachment_note ON attachment(note_id);
CREATE INDEX idx_attachment_doctype ON attachment(document_type_id);
CREATE INDEX idx_attachment_status ON attachment(status);
CREATE INDEX idx_attachment_blob ON attachment(blob_id);

-- FTS on extracted text
CREATE INDEX idx_attachment_extracted_fts ON attachment
    USING GIN (to_tsvector('english', COALESCE(extracted_text, '')));

-- Reference counting trigger for blob cleanup
CREATE OR REPLACE FUNCTION update_blob_refcount()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE attachment_blob
        SET reference_count = reference_count + 1
        WHERE id = NEW.blob_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE attachment_blob
        SET reference_count = reference_count - 1
        WHERE id = OLD.blob_id;
    ELSIF TG_OP = 'UPDATE' AND NEW.blob_id != OLD.blob_id THEN
        UPDATE attachment_blob
        SET reference_count = reference_count - 1
        WHERE id = OLD.blob_id;
        UPDATE attachment_blob
        SET reference_count = reference_count + 1
        WHERE id = NEW.blob_id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachment_refcount_update
AFTER INSERT OR DELETE OR UPDATE OF blob_id ON attachment
FOR EACH ROW EXECUTE FUNCTION update_blob_refcount();

-- Update timestamp trigger
CREATE TRIGGER attachment_updated
    BEFORE UPDATE ON attachment
    FOR EACH ROW
    EXECUTE FUNCTION update_document_type_timestamp();
```

### 1.2 Document Type Extensions for Attachments

```sql
-- Migration: 20260203100000_doctype_attachment_extensions.sql

-- Add attachment-specific fields to document_type
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    requires_file_attachment BOOLEAN DEFAULT FALSE;

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    extraction_strategy extraction_strategy DEFAULT 'text_native';

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    auto_create_note BOOLEAN DEFAULT FALSE;

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    note_template TEXT;  -- Template for auto-generated notes

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    embedding_model_override TEXT;  -- e.g., 'clip-vit-b-32' for images

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    processing_config JSONB DEFAULT '{}';

COMMENT ON COLUMN document_type.requires_file_attachment IS
    'When true, notes of this type must have a file attachment';
COMMENT ON COLUMN document_type.extraction_strategy IS
    'How to extract searchable content from attached files';
COMMENT ON COLUMN document_type.auto_create_note IS
    'Automatically create a note when file is uploaded';
COMMENT ON COLUMN document_type.note_template IS
    'Markdown template for auto-generated notes with {{variables}}';
COMMENT ON COLUMN document_type.embedding_model_override IS
    'Use specific embedding model (e.g., CLIP for images)';

-- Update existing media doctypes
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'vision',
    auto_create_note = TRUE,
    note_template = E'# {{filename}}\n\n{{ai_description}}\n\n## Details\n- **Type:** {{content_type}}\n- **Size:** {{file_size}}\n- **Captured:** {{capture_date}}\n\n## Tags\n{{suggested_tags}}'
WHERE name IN ('image', 'screenshot', 'diagram', 'image-with-text');

UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'video_multimodal',
    auto_create_note = TRUE,
    note_template = E'# {{filename}}\n\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Key Moments\n{{scene_descriptions}}\n\n## Metadata\n- **Duration:** {{duration}}\n- **Resolution:** {{resolution}}'
WHERE name IN ('video');

UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'audio_transcribe',
    auto_create_note = TRUE,
    note_template = E'# {{filename}}\n\n## Summary\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Key Topics\n{{topics}}'
WHERE name IN ('audio', 'podcast');

UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE
WHERE name IN ('pdf', 'academic-paper', 'arxiv', 'thesis', 'contract', 'policy');

UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pandoc'
WHERE name IN ('excel', 'presentation');
```

### 1.3 Attachment Embeddings Table

```sql
-- Migration: 20260203200000_attachment_embeddings.sql

-- Store embeddings for attachment content
CREATE TABLE attachment_embedding (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES attachment(id) ON DELETE CASCADE,
    embedding_set_id UUID REFERENCES embedding_set(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL DEFAULT 0,

    -- Text chunk that was embedded
    text TEXT NOT NULL,

    -- Standard text embedding
    vector vector(768),

    -- CLIP embedding for images (optional)
    clip_vector vector(512),

    -- Metadata
    model TEXT NOT NULL,
    embedding_type TEXT NOT NULL DEFAULT 'text',  -- 'text', 'clip', 'multimodal'

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT attachment_embedding_unique
        UNIQUE (attachment_id, embedding_set_id, chunk_index)
);

-- HNSW index for text embeddings
CREATE INDEX idx_attachment_embedding_hnsw ON attachment_embedding
    USING hnsw (vector vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- HNSW index for CLIP embeddings
CREATE INDEX idx_attachment_embedding_clip_hnsw ON attachment_embedding
    USING hnsw (clip_vector vector_cosine_ops)
    WITH (m = 16, ef_construction = 64)
    WHERE clip_vector IS NOT NULL;

CREATE INDEX idx_attachment_embedding_attachment ON attachment_embedding(attachment_id);
CREATE INDEX idx_attachment_embedding_set ON attachment_embedding(embedding_set_id);
```

---

## 2. Processing Flow Diagram

```
                                FILE UPLOAD
                                     |
                                     v
+--------------------------------------------------------------------+
|                        UPLOAD HANDLER                               |
|  1. Receive file (multipart/form-data or streaming)                |
|  2. Validate file size (per-type limits)                           |
|  3. Compute BLAKE3 hash                                            |
|  4. Check for existing blob (deduplication)                        |
|  5. Validate magic bytes vs. claimed type                          |
|  6. Store blob (database <10MB, object storage >=10MB)             |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                      DOCTYPE DETECTION                              |
|  1. Match by MIME type (highest confidence)                        |
|  2. Match by file extension                                        |
|  3. Match by filename pattern                                      |
|  4. Match by magic bytes / content patterns                        |
|  5. Return: document_type_id, confidence, method                   |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                    CREATE ATTACHMENT RECORD                         |
|  1. Link to note (existing) or create new note                     |
|  2. Set document_type_id (detected or user-specified)              |
|  3. Set extraction_strategy from doctype                           |
|  4. Status = 'queued'                                              |
|  5. Return 202 Accepted with attachment_id                         |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                   ENQUEUE PROCESSING JOB                            |
|  job_type: 'attachment_processing'                                 |
|  payload: { attachment_id, steps: [...], priority }                |
+--------------------------------------------------------------------+
                                     |
                                     v
          +-------------------------|-------------------------+
          |                         |                         |
          v                         v                         v
+------------------+    +--------------------+    +-------------------+
| VIRUS SCAN       |    | CONTENT EXTRACTION |    | PREVIEW GENERATION|
| (ClamAV/async)   |    | (per strategy)     |    | (thumbnails)      |
+------------------+    +--------------------+    +-------------------+
          |                         |                         |
          v                         v                         v
+------------------+    +--------------------+    +-------------------+
| If infected:     |    | EXTRACTION STEPS:  |    | Store preview     |
| quarantine,      |    | - vision (LLaVA)   |    | blob, update      |
| notify user      |    | - audio (Whisper)  |    | has_preview=true  |
+------------------+    | - pdf (pdftotext)  |    +-------------------+
                        | - docx (pandoc)    |
                        | - data (parsing)   |
                        +--------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                       AI ENHANCEMENT                                |
|  1. Use extraction_strategy to get raw content                     |
|  2. Apply doctype's agentic_config.generation_prompt               |
|  3. Generate description/summary via LLM                           |
|  4. Suggest tags based on content                                  |
|  5. Determine sub-doctype if applicable (PDF -> contract/paper)    |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                    NOTE CREATION / UPDATE                           |
|  If auto_create_note = true AND no note_id provided:               |
|    1. Apply note_template with extracted data                      |
|    2. Create note with content, tags, document_type_id             |
|    3. Mark attachment.is_ai_generated = true                       |
|    4. Mark attachment.is_canonical_content = true                  |
|  Else:                                                             |
|    1. Update attachment with extracted_text, ai_description        |
|    2. Append to existing note if configured                        |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                      EMBEDDING GENERATION                           |
|  Based on doctype's auto_embed_rules + embedding_model_override:   |
|  1. Chunk extracted_text per doctype's chunking_strategy           |
|  2. Generate text embeddings (nomic-embed-text)                    |
|  3. If image: generate CLIP embedding                              |
|  4. Store in attachment_embedding table                            |
|  5. Update note embeddings if is_canonical_content                 |
+--------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------+
|                         COMPLETE                                    |
|  1. Update attachment.status = 'completed'                         |
|  2. Update note.updated_at                                         |
|  3. Notify user (websocket/callback)                               |
+--------------------------------------------------------------------+
```

---

## 3. Doctype Templates for Auto-Generated Notes

### 3.1 Template Variable System

Templates use Handlebars-style `{{variable}}` syntax with the following available variables:

| Variable | Description | Example |
|----------|-------------|---------|
| `{{filename}}` | Original filename | `vacation-video.mp4` |
| `{{content_type}}` | MIME type | `video/mp4` |
| `{{file_size}}` | Human-readable size | `125.4 MB` |
| `{{capture_date}}` | EXIF/metadata date | `2026-01-15` |
| `{{ai_description}}` | AI-generated description | `A beach sunset...` |
| `{{ai_summary}}` | AI summary of content | `This video shows...` |
| `{{transcript}}` | Audio/video transcript | `Speaker 1: Hello...` |
| `{{extracted_text}}` | Raw extracted text | `Document content...` |
| `{{suggested_tags}}` | AI-suggested tags | `vacation, beach, sunset` |
| `{{scene_descriptions}}` | Video scene list | `0:00 - Beach view...` |
| `{{topics}}` | Extracted topics | `travel, relaxation` |
| `{{duration}}` | Media duration | `3:45` |
| `{{resolution}}` | Video resolution | `1920x1080` |
| `{{page_count}}` | PDF page count | `24` |
| `{{author}}` | Document author | `John Smith` |
| `{{creation_date}}` | Document creation date | `2026-01-01` |

### 3.2 Personal Memory Video Template

For the `video` doctype in `personal` category, create a new `personal-memory` doctype:

```sql
INSERT INTO document_type (
    name, display_name, category, description,
    file_extensions, mime_types,
    chunking_strategy,
    requires_file_attachment, extraction_strategy, auto_create_note,
    note_template,
    agentic_config,
    is_system
) VALUES (
    'personal-memory',
    'Personal Memory',
    'personal',
    'Personal video memories with AI-enhanced descriptions and transcripts',
    ARRAY['.mp4', '.mov', '.avi', '.mkv', '.webm'],
    ARRAY['video/mp4', 'video/quicktime', 'video/x-msvideo', 'video/x-matroska', 'video/webm'],
    'whole',
    TRUE,
    'video_multimodal',
    TRUE,
    E'# {{title}}\n\n## Memory\n{{ai_description}}\n\n## What Happened\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Key Moments\n{{#each scene_descriptions}}\n- **{{this.timestamp}}**: {{this.description}}\n{{/each}}\n\n## People & Places\n{{#if people}}- **People:** {{people}}{{/if}}\n{{#if location}}- **Location:** {{location}}{{/if}}\n{{#if event}}- **Event:** {{event}}{{/if}}\n\n## Details\n- **Recorded:** {{capture_date}}\n- **Duration:** {{duration}}\n- **File:** {{filename}} ({{file_size}})\n\n---\n*Tags: {{suggested_tags}}*',
    '{
        "generation_prompt": "Describe this personal video memory with emotional context. Focus on who is present, what activity is happening, and the mood/atmosphere. Generate a nostalgic, personal narrative.",
        "required_sections": ["Memory", "What Happened"],
        "optional_sections": ["People & Places", "Key Moments", "Transcript"],
        "context_requirements": {"needs_vision_model": true, "needs_audio_model": true},
        "agent_hints": {
            "tone": "personal_nostalgic",
            "extract_people": true,
            "extract_location": true,
            "extract_event_type": true,
            "suggest_related_memories": true
        }
    }'::jsonb,
    TRUE
);
```

### 3.3 Standard Doctype Templates

#### Image Template

```markdown
# {{filename}}

{{ai_description}}

## Visual Details
{{#if objects}}**Objects:** {{objects}}{{/if}}
{{#if text_content}}**Text in Image:** {{text_content}}{{/if}}
{{#if colors}}**Dominant Colors:** {{colors}}{{/if}}

## Metadata
- **Dimensions:** {{dimensions}}
- **Taken:** {{capture_date}}
- **Camera:** {{camera_model}}
- **Location:** {{gps_location}}

---
*Tags: {{suggested_tags}}*
```

#### Audio/Podcast Template

```markdown
# {{title}}

## Summary
{{ai_summary}}

## Transcript
{{transcript}}

## Topics Discussed
{{#each topics}}
- {{this}}
{{/each}}

## Speakers
{{#each speakers}}
- **{{this.name}}**: {{this.speaking_time}}
{{/each}}

## Details
- **Duration:** {{duration}}
- **Recorded:** {{capture_date}}
- **File:** {{filename}}

---
*Tags: {{suggested_tags}}*
```

#### PDF Document Template

```markdown
# {{title}}

## Summary
{{ai_summary}}

## Key Points
{{key_points}}

## Content
{{extracted_text}}

## Document Info
- **Author:** {{author}}
- **Pages:** {{page_count}}
- **Created:** {{creation_date}}
- **Modified:** {{modification_date}}

---
*Tags: {{suggested_tags}}*
```

---

## 4. Example: Video Upload to Personal-Memory Note

### 4.1 Upload Request

```http
POST /api/v1/attachments
Content-Type: multipart/form-data

file: <binary: vacation-2026-01-beach.mp4>
title: Beach Vacation January 2026
tags: vacation, beach, family
```

### 4.2 Processing Steps

```yaml
# Step 1: Upload Handler
received:
  filename: vacation-2026-01-beach.mp4
  size: 125,432,000 bytes
  content_type: video/mp4

computed:
  blake3_hash: "abc123def456..."
  storage_type: object_storage  # >10MB

# Step 2: Doctype Detection
detected:
  document_type: personal-memory
  confidence: 0.95
  method: mime_type

# Step 3: Create Records
attachment:
  id: "01JQWX-attachment-001"
  blob_id: "01JQWX-blob-001"
  document_type_id: "01JQWX-doctype-personal-memory"
  status: queued
  extraction_strategy: video_multimodal

note:
  id: "01JQWX-note-001"
  title: "Beach Vacation January 2026"
  document_type_id: "01JQWX-doctype-personal-memory"
  content: ""  # Will be populated after processing

# Step 4: Processing Job
job:
  type: attachment_processing
  payload:
    attachment_id: "01JQWX-attachment-001"
    steps:
      - virus_scan
      - extract_audio
      - transcribe_audio
      - extract_keyframes
      - analyze_frames_vision
      - generate_description
      - generate_embeddings
      - apply_template
```

### 4.3 Extraction Results

```json
{
  "transcript": {
    "segments": [
      {"time": "0:00", "speaker": "Alice", "text": "Look at this view!"},
      {"time": "0:05", "speaker": "Bob", "text": "Amazing sunset."},
      {"time": "0:12", "speaker": "Alice", "text": "I wish we could stay here forever."}
    ],
    "full_text": "Look at this view! Amazing sunset. I wish we could stay here forever."
  },
  "scene_descriptions": [
    {"timestamp": "0:00-0:10", "description": "Wide shot of ocean beach at sunset with golden light"},
    {"timestamp": "0:10-0:25", "description": "Two people walking along shoreline, waves lapping"},
    {"timestamp": "0:25-0:45", "description": "Close-up of seashells in sand"},
    {"timestamp": "0:45-1:00", "description": "Panoramic view of sun setting over water"}
  ],
  "ai_description": "A warm, intimate video capturing a peaceful beach sunset. Two people, likely a couple, enjoy a romantic walk along the shoreline as the golden sun dips toward the horizon. The scene evokes tranquility and connection, with the rhythmic sound of waves providing a natural soundtrack.",
  "ai_summary": "This personal memory captures a beach vacation moment during sunset. Alice and Bob share an appreciation for the natural beauty around them, with Alice expressing a wish to stay longer. The video showcases the ocean, sunset, and a leisurely walk along the beach.",
  "metadata": {
    "duration": "1:00",
    "resolution": "1920x1080",
    "capture_date": "2026-01-15T18:30:00Z",
    "gps_location": "Malibu Beach, California"
  },
  "entities": {
    "people": ["Alice", "Bob"],
    "location": "Beach (Malibu, California)",
    "event": "Vacation",
    "time_of_day": "Sunset"
  },
  "suggested_tags": ["vacation", "beach", "sunset", "romantic", "ocean", "california", "2026", "family"]
}
```

### 4.4 Generated Note Content

```markdown
# Beach Vacation January 2026

## Memory
A warm, intimate video capturing a peaceful beach sunset. Two people, likely a couple, enjoy a romantic walk along the shoreline as the golden sun dips toward the horizon. The scene evokes tranquility and connection, with the rhythmic sound of waves providing a natural soundtrack.

## What Happened
This personal memory captures a beach vacation moment during sunset. Alice and Bob share an appreciation for the natural beauty around them, with Alice expressing a wish to stay longer. The video showcases the ocean, sunset, and a leisurely walk along the beach.

## Transcript
**0:00 - Alice:** Look at this view!
**0:05 - Bob:** Amazing sunset.
**0:12 - Alice:** I wish we could stay here forever.

## Key Moments
- **0:00-0:10**: Wide shot of ocean beach at sunset with golden light
- **0:10-0:25**: Two people walking along shoreline, waves lapping
- **0:25-0:45**: Close-up of seashells in sand
- **0:45-1:00**: Panoramic view of sun setting over water

## People & Places
- **People:** Alice, Bob
- **Location:** Malibu Beach, California
- **Event:** Vacation

## Details
- **Recorded:** January 15, 2026 at 6:30 PM
- **Duration:** 1:00
- **File:** vacation-2026-01-beach.mp4 (125.4 MB)

---
*Tags: vacation, beach, sunset, romantic, ocean, california, 2026, family*
```

### 4.5 Database State After Processing

```sql
-- attachment record
SELECT * FROM attachment WHERE id = '01JQWX-attachment-001';
-- id: 01JQWX-attachment-001
-- note_id: 01JQWX-note-001
-- blob_id: 01JQWX-blob-001
-- filename: vacation-2026-01-beach.mp4
-- document_type_id: <personal-memory-uuid>
-- status: completed
-- extraction_strategy: video_multimodal
-- extracted_text: "Look at this view! Amazing sunset..."
-- ai_description: "A warm, intimate video capturing..."
-- is_canonical_content: true
-- is_ai_generated: true
-- virus_scan_status: clean

-- note record
SELECT * FROM note WHERE id = '01JQWX-note-001';
-- id: 01JQWX-note-001
-- document_type_id: <personal-memory-uuid>
-- title: Beach Vacation January 2026

-- note_revised_current (generated content)
SELECT content FROM note_revised_current WHERE note_id = '01JQWX-note-001';
-- content: "# Beach Vacation January 2026\n\n## Memory\n..."

-- embeddings
SELECT COUNT(*) FROM attachment_embedding
WHERE attachment_id = '01JQWX-attachment-001';
-- count: 3 (transcript chunks)

SELECT COUNT(*) FROM embedding
WHERE note_id = '01JQWX-note-001';
-- count: 4 (note content chunks)
```

---

## 5. Auto-Embed Rules Per Doctype

### 5.1 Extended AgenticConfig Schema

```rust
/// Extended AgenticConfig for attachment processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticConfig {
    // Existing fields...

    /// Embedding configuration for this doctype
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_config: Option<DoctypeEmbedConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctypeEmbedConfig {
    /// Always generate embeddings on attachment upload
    #[serde(default)]
    pub auto_embed: bool,

    /// Embedding model override (e.g., "clip-vit-b-32" for images)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,

    /// Use CLIP embedding in addition to text embedding
    #[serde(default)]
    pub use_clip: bool,

    /// MRL truncation dimension (for storage optimization)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_dim: Option<i32>,

    /// Embedding priority (affects job queue ordering)
    #[serde(default = "default_priority")]
    pub priority: String,  // "low", "normal", "high"

    /// Custom chunking for embeddings (overrides doctype default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_config: Option<ChunkConfig>,
}

fn default_priority() -> String { "normal".to_string() }
```

### 5.2 Doctype Embedding Configurations

```sql
-- Images: CLIP + text embeddings
UPDATE document_type SET agentic_config = agentic_config || '{
    "embed_config": {
        "auto_embed": true,
        "use_clip": true,
        "model_override": "clip-vit-b-32",
        "priority": "normal"
    }
}'::jsonb
WHERE name IN ('image', 'screenshot', 'diagram', 'image-with-text');

-- Videos: Text embeddings from transcript
UPDATE document_type SET agentic_config = agentic_config || '{
    "embed_config": {
        "auto_embed": true,
        "use_clip": true,
        "priority": "low",
        "chunk_config": {
            "strategy": "semantic",
            "chunk_size": 1000,
            "overlap": 100
        }
    }
}'::jsonb
WHERE name IN ('video', 'personal-memory');

-- Audio: Text embeddings from transcript
UPDATE document_type SET agentic_config = agentic_config || '{
    "embed_config": {
        "auto_embed": true,
        "priority": "normal",
        "chunk_config": {
            "strategy": "semantic",
            "chunk_size": 1500,
            "overlap": 150
        }
    }
}'::jsonb
WHERE name IN ('audio', 'podcast', 'transcript');

-- PDFs: Dense text embeddings
UPDATE document_type SET agentic_config = agentic_config || '{
    "embed_config": {
        "auto_embed": true,
        "truncate_dim": 256,
        "priority": "high"
    }
}'::jsonb
WHERE name IN ('academic-paper', 'arxiv', 'thesis', 'contract');

-- Code: Syntactic chunking
UPDATE document_type SET agentic_config = agentic_config || '{
    "embed_config": {
        "auto_embed": false,
        "model_override": "nomic-embed-text:code",
        "priority": "low",
        "chunk_config": {
            "strategy": "syntactic",
            "preserve_boundaries": true
        }
    }
}'::jsonb
WHERE category = 'code';
```

---

## 6. Processing Job Types

### 6.1 New Job Type Enum Values

```sql
-- Add attachment processing job types
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'attachment_processing'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'attachment_processing';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'attachment_extraction'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'attachment_extraction';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'attachment_preview'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'attachment_preview';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'attachment_virus_scan'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'attachment_virus_scan';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'attachment_embed'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'attachment_embed';
    END IF;
END$$;
```

### 6.2 Job Payloads

```rust
/// Payload for attachment processing orchestration job
#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentProcessingPayload {
    pub attachment_id: Uuid,
    pub steps: Vec<ProcessingStep>,
    pub priority: JobPriority,
    pub callback_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingStep {
    VirusScan,
    ExtractContent,
    GeneratePreview,
    AIEnhance,
    GenerateEmbeddings,
    ApplyTemplate,
    UpdateNote,
}

/// Payload for content extraction job
#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentExtractionPayload {
    pub attachment_id: Uuid,
    pub extraction_strategy: ExtractionStrategy,
    pub extraction_config: serde_json::Value,
}

/// Payload for embedding generation job
#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentEmbedPayload {
    pub attachment_id: Uuid,
    pub embed_config: DoctypeEmbedConfig,
    pub text: String,  // Text to embed
}
```

---

## 7. API Endpoints

### 7.1 Upload Attachment

```http
POST /api/v1/attachments
Content-Type: multipart/form-data

# With existing note:
file: <binary>
note_id: 01JQWX-note-001

# Auto-create note:
file: <binary>
title: "My Video Title"
tags: ["personal", "memory"]
auto_create_note: true

Response 202 Accepted:
{
  "attachment_id": "01JQWX-attachment-001",
  "note_id": "01JQWX-note-001",
  "status": "queued",
  "document_type": {
    "name": "personal-memory",
    "display_name": "Personal Memory",
    "confidence": 0.95
  },
  "processing_job_id": "01JQWX-job-001",
  "estimated_completion_seconds": 45
}
```

### 7.2 Check Processing Status

```http
GET /api/v1/attachments/01JQWX-attachment-001/status

Response 200 OK:
{
  "attachment_id": "01JQWX-attachment-001",
  "status": "processing",
  "current_step": "ExtractContent",
  "progress_percent": 35,
  "steps_completed": ["VirusScan"],
  "steps_remaining": ["GeneratePreview", "AIEnhance", "GenerateEmbeddings"]
}
```

### 7.3 Get Attachment with Extracted Data

```http
GET /api/v1/attachments/01JQWX-attachment-001

Response 200 OK:
{
  "id": "01JQWX-attachment-001",
  "note_id": "01JQWX-note-001",
  "filename": "vacation-2026-01-beach.mp4",
  "content_type": "video/mp4",
  "size_bytes": 125432000,
  "status": "completed",
  "document_type": {
    "name": "personal-memory",
    "display_name": "Personal Memory"
  },
  "extracted_text": "Look at this view! Amazing sunset...",
  "ai_description": "A warm, intimate video capturing...",
  "extracted_metadata": {
    "duration": "1:00",
    "resolution": "1920x1080",
    "capture_date": "2026-01-15T18:30:00Z"
  },
  "preview_url": "/api/v1/attachments/01JQWX-attachment-001/preview",
  "download_url": "/api/v1/attachments/01JQWX-attachment-001/download"
}
```

---

## 8. Implementation Roadmap

### Phase 1: Core Infrastructure (3-4 days)
- [ ] Create attachment and attachment_blob tables
- [ ] Implement BLAKE3 content-addressable storage
- [ ] Add upload endpoint with deduplication
- [ ] Implement doctype detection from MIME/extension
- [ ] Basic download endpoint

### Phase 2: Processing Pipeline (4-5 days)
- [ ] Add job types for attachment processing
- [ ] Implement extraction strategies:
  - [ ] pdf_text (pdftotext)
  - [ ] pandoc (DOCX, PPTX)
  - [ ] structured_data (CSV, Excel)
- [ ] Preview generation (thumbnails)
- [ ] Update FTS index with extracted text

### Phase 3: AI Enhancement (5-7 days)
- [ ] Vision model integration (LLaVA)
- [ ] Audio transcription (Whisper)
- [ ] Video processing (FFmpeg + vision + audio)
- [ ] Template variable substitution
- [ ] Auto-generate notes from templates

### Phase 4: Embeddings & Search (3-4 days)
- [ ] Attachment embedding table
- [ ] CLIP embeddings for images
- [ ] Integrate with hybrid search
- [ ] Per-doctype embedding configuration

### Phase 5: Polish & Security (2-3 days)
- [ ] Virus scanning integration (ClamAV)
- [ ] File type validation (magic bytes)
- [ ] Size limits per doctype
- [ ] Quarantine workflow

---

## 9. Architectural Decision Records

### ADR-025: Attachment Storage Strategy

**Decision:** Hybrid storage with PostgreSQL BYTEA (<10MB) and object storage (>=10MB)

**Rationale:**
- PostgreSQL BYTEA with EXTERNAL storage provides ACID transactions and single-backup simplicity
- Object storage (MinIO/S3) handles large files efficiently
- BLAKE3 content-addressing enables deduplication across both storage types
- Reference counting allows safe garbage collection

### ADR-026: File-as-Canonical-Content

**Decision:** Introduce `is_canonical_content` flag to indicate the attachment IS the note content

**Rationale:**
- Some doctypes (video, image) have the file as the primary artifact
- Extracted text is derivative, stored in `extracted_text` for search
- AI-generated note content references the file but isn't the source of truth
- Enables "regenerate description" without losing the original file

### ADR-027: Doctype-Driven Processing

**Decision:** Use document_type's `extraction_strategy` and `agentic_config` to drive processing

**Rationale:**
- Leverages existing 131+ doctype definitions
- Consistent processing rules per file type
- Extensible: add new doctypes for specialized processing
- AI hints in agentic_config guide content generation

### ADR-028: Two-Phase Embedding (Text + CLIP)

**Decision:** Support both text embeddings and CLIP embeddings for media doctypes

**Rationale:**
- Text embeddings enable semantic search on extracted/described content
- CLIP embeddings enable "search images by description" use case
- Both stored in attachment_embedding with `embedding_type` discriminator
- Per-doctype configuration via `agentic_config.embed_config`

---

## 10. Security Considerations

### 10.1 File Validation

```rust
// Magic byte validation using `infer` crate
fn validate_file_magic(data: &[u8], claimed_mime: &str) -> Result<()> {
    let detected = infer::get(data)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    if !mime_compatible(claimed_mime, detected) {
        return Err(Error::FileTypeMismatch { claimed, detected });
    }

    if !ALLOWED_MIME_TYPES.contains(&detected) {
        return Err(Error::FileTypeNotAllowed(detected));
    }

    Ok(())
}
```

### 10.2 Size Limits

```rust
const LIMITS: AttachmentLimits = AttachmentLimits {
    max_file_size: 500 * MB,        // Global max
    max_image_size: 25 * MB,        // Images
    max_video_size: 500 * MB,       // Videos
    max_audio_size: 100 * MB,       // Audio
    max_document_size: 50 * MB,     // PDFs, Office docs
    max_per_note: 1 * GB,           // Total per note
    max_per_user: 10 * GB,          // Total per user
};
```

### 10.3 Virus Scanning

- Integrate ClamAV daemon for async scanning
- Quarantine infected files before processing
- Notify user of infection detection
- Do not serve quarantined files

---

## 11. References

- [Existing Document Type System](/path/to/fortemi/migrations/20260202000000_document_types.sql)
- [File Attachments Research](/path/to/fortemi/docs/research/file-attachments-research.md)
- [Auto-Embed Rules ADR](/path/to/fortemi/.aiwg/architecture/ADR-024-auto-embed-rules.md)
- [Embedding Sets Documentation](/path/to/fortemi/docs/content/embedding-sets.md)

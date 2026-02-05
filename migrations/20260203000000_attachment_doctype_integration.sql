-- ============================================================================
-- File Attachment + Document Type Integration Migration
-- Issue: #425 (proposed)
-- ============================================================================
-- This migration integrates the file attachment system with the existing
-- document type registry, enabling intelligent content-aware processing.
--
-- Features:
-- 1. Content-addressable blob storage with BLAKE3 hashing
-- 2. Doctype detection and auto-creation of notes
-- 3. Extraction strategy per file type
-- 4. AI-enhanced content generation from attachments
-- 5. Multi-modal embeddings (text + CLIP)
-- ============================================================================

-- ============================================================================
-- PART 1: ENUM TYPES
-- ============================================================================

-- Extraction strategy: how to extract searchable content from file
CREATE TYPE extraction_strategy AS ENUM (
    'text_native',      -- Native text (Markdown, code, config)
    'pdf_text',         -- PDF text extraction (pdftotext)
    'pdf_ocr',          -- PDF with OCR (scanned documents)
    'pandoc',           -- Office documents (DOCX, PPTX, etc.)
    'vision',           -- Image description (LLaVA vision model)
    'audio_transcribe', -- Audio transcription (Whisper)
    'video_multimodal', -- Video: frame extraction + audio transcription
    'structured_data',  -- CSV, Excel, JSON parsing
    'code_analysis',    -- Code with AST analysis (tree-sitter)
    'none'              -- No extraction (binary blob only)
);

-- Processing status for attachments
CREATE TYPE attachment_status AS ENUM (
    'uploaded',         -- File received, not yet processed
    'queued',           -- Processing job queued
    'processing',       -- Currently being processed
    'completed',        -- Processing complete
    'failed',           -- Processing failed (retryable)
    'quarantined'       -- Security concern (virus, malicious file)
);

-- ============================================================================
-- PART 2: ATTACHMENT BLOB TABLE (Content-Addressable Storage)
-- ============================================================================

CREATE TABLE attachment_blob (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    content_hash TEXT NOT NULL UNIQUE,      -- BLAKE3 hash for deduplication
    content_type TEXT NOT NULL,             -- MIME type
    size_bytes BIGINT NOT NULL,
    storage_type TEXT NOT NULL DEFAULT 'database',  -- 'database' | 'object_storage'

    -- Database storage (files <10MB, stored inline)
    data BYTEA,

    -- Object storage (files >=10MB, stored externally)
    object_key TEXT,                        -- S3/MinIO key
    object_bucket TEXT,                     -- S3/MinIO bucket

    -- Reference counting for garbage collection
    reference_count INTEGER NOT NULL DEFAULT 0,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Use EXTERNAL storage for BYTEA (no compression, optimized for binary access)
ALTER TABLE attachment_blob ALTER COLUMN data SET STORAGE EXTERNAL;

-- Indexes
CREATE INDEX idx_attachment_blob_hash ON attachment_blob(content_hash);
CREATE INDEX idx_attachment_blob_orphan ON attachment_blob(reference_count)
    WHERE reference_count = 0;

COMMENT ON TABLE attachment_blob IS 'Content-addressable blob storage with BLAKE3 deduplication';
COMMENT ON COLUMN attachment_blob.content_hash IS 'BLAKE3 hash of file content for deduplication';
COMMENT ON COLUMN attachment_blob.storage_type IS 'Where blob is stored: database (inline) or object_storage (external)';
COMMENT ON COLUMN attachment_blob.reference_count IS 'Number of attachments referencing this blob';

-- ============================================================================
-- PART 3: ATTACHMENT TABLE
-- ============================================================================

CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES attachment_blob(id) ON DELETE RESTRICT,

    -- File metadata
    filename TEXT NOT NULL,                 -- Sanitized filename
    original_filename TEXT,                 -- Pre-sanitization name
    display_order INTEGER DEFAULT 0,        -- Order within note

    -- Document type integration
    document_type_id UUID REFERENCES document_type(id),
    detected_document_type_id UUID REFERENCES document_type(id),
    detection_confidence FLOAT,
    detection_method TEXT,                  -- 'mime', 'extension', 'magic', 'content'

    -- Processing state
    status attachment_status NOT NULL DEFAULT 'uploaded',
    processing_error TEXT,
    processing_job_id UUID,                 -- Link to job_queue.id

    -- Extraction configuration (from doctype or override)
    extraction_strategy extraction_strategy,
    extraction_config JSONB DEFAULT '{}',

    -- Extracted content (result of processing)
    extracted_text TEXT,                    -- For FTS indexing
    extracted_metadata JSONB,               -- EXIF, PDF metadata, dimensions, etc.
    ai_description TEXT,                    -- AI-generated description (vision/audio)

    -- Preview/thumbnail
    has_preview BOOLEAN DEFAULT FALSE,
    preview_content_type TEXT,
    preview_blob_id UUID REFERENCES attachment_blob(id),

    -- Security
    virus_scan_status TEXT,                 -- 'pending', 'clean', 'infected', 'error'
    virus_scan_at TIMESTAMPTZ,

    -- Content relationship flags
    is_canonical_content BOOLEAN DEFAULT FALSE,  -- File IS the note content (not supplementary)
    is_ai_generated BOOLEAN DEFAULT FALSE,       -- Note content was AI-generated from this file

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
CREATE INDEX idx_attachment_created ON attachment(created_at);

-- Full-text search on extracted content
CREATE INDEX idx_attachment_extracted_fts ON attachment
    USING GIN (to_tsvector('english', COALESCE(extracted_text, '')));

COMMENT ON TABLE attachment IS 'File attachments linked to notes with doctype-aware processing';
COMMENT ON COLUMN attachment.is_canonical_content IS 'When true, the file IS the note content (e.g., video = memory)';
COMMENT ON COLUMN attachment.is_ai_generated IS 'Note content was AI-generated from this attachment';

-- ============================================================================
-- PART 4: REFERENCE COUNTING TRIGGER
-- ============================================================================

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
    ELSIF TG_OP = 'UPDATE' AND NEW.blob_id IS DISTINCT FROM OLD.blob_id THEN
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

-- ============================================================================
-- PART 5: ATTACHMENT TIMESTAMP TRIGGER
-- ============================================================================

CREATE OR REPLACE FUNCTION update_attachment_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachment_updated
    BEFORE UPDATE ON attachment
    FOR EACH ROW
    EXECUTE FUNCTION update_attachment_timestamp();

-- ============================================================================
-- PART 6: EXTEND DOCUMENT_TYPE FOR ATTACHMENTS
-- ============================================================================

-- Flag indicating doctype requires a file attachment
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    requires_file_attachment BOOLEAN DEFAULT FALSE;

-- Default extraction strategy for this doctype
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    extraction_strategy extraction_strategy DEFAULT 'text_native';

-- Automatically create a note when file is uploaded
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    auto_create_note BOOLEAN DEFAULT FALSE;

-- Template for auto-generated note content (Handlebars-style)
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    note_template TEXT;

-- Override embedding model for this doctype (e.g., CLIP for images)
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    embedding_model_override TEXT;

-- Additional processing configuration
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    processing_config JSONB DEFAULT '{}';

COMMENT ON COLUMN document_type.requires_file_attachment IS
    'When true, notes of this type must have a file attachment';
COMMENT ON COLUMN document_type.extraction_strategy IS
    'How to extract searchable content from attached files';
COMMENT ON COLUMN document_type.auto_create_note IS
    'Automatically create a note when file of this type is uploaded';
COMMENT ON COLUMN document_type.note_template IS
    'Handlebars template for auto-generated note content with {{variables}}';
COMMENT ON COLUMN document_type.embedding_model_override IS
    'Use specific embedding model (e.g., clip-vit-b-32 for images)';

-- ============================================================================
-- PART 7: ATTACHMENT EMBEDDINGS TABLE
-- ============================================================================

CREATE TABLE attachment_embedding (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES attachment(id) ON DELETE CASCADE,
    embedding_set_id UUID REFERENCES embedding_set(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL DEFAULT 0,

    -- Text chunk that was embedded
    text TEXT NOT NULL,

    -- Standard text embedding (nomic-embed-text, etc.)
    vector vector(768),

    -- CLIP embedding for images/visual content (optional)
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

COMMENT ON TABLE attachment_embedding IS 'Embeddings for attachment content (text and/or CLIP)';
COMMENT ON COLUMN attachment_embedding.clip_vector IS 'CLIP embedding for image/visual content search';

-- ============================================================================
-- PART 8: JOB TYPES FOR ATTACHMENT PROCESSING
-- ============================================================================

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

-- ============================================================================
-- PART 9: HELPER FUNCTIONS
-- ============================================================================

-- Get searchable text including attachment content for a note
CREATE OR REPLACE FUNCTION get_note_searchable_text(p_note_id UUID)
RETURNS TEXT AS $$
    SELECT
        COALESCE(nr.content, '') || E'\n\n' ||
        COALESCE(
            (SELECT string_agg(a.extracted_text, E'\n\n')
             FROM attachment a
             WHERE a.note_id = p_note_id
             AND a.extracted_text IS NOT NULL
             AND a.status = 'completed'),
            ''
        )
    FROM note_revised_current nr
    WHERE nr.note_id = p_note_id;
$$ LANGUAGE sql STABLE;

-- Cleanup orphaned blobs (call periodically via cron/job)
CREATE OR REPLACE FUNCTION cleanup_orphaned_blobs(
    min_age_hours INTEGER DEFAULT 24
)
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- Delete orphaned blobs older than specified age
    WITH deleted AS (
        DELETE FROM attachment_blob
        WHERE reference_count = 0
        AND created_at < NOW() - (min_age_hours || ' hours')::interval
        RETURNING id
    )
    SELECT COUNT(*) INTO deleted_count FROM deleted;

    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_note_searchable_text IS
    'Returns combined note content and attachment extracted text for FTS';
COMMENT ON FUNCTION cleanup_orphaned_blobs IS
    'Deletes unreferenced blobs older than specified hours';

-- ============================================================================
-- PART 10: ATTACHMENT SUMMARY VIEW
-- ============================================================================

CREATE OR REPLACE VIEW attachment_summary AS
SELECT
    a.id,
    a.note_id,
    a.filename,
    ab.content_type,
    ab.size_bytes,
    a.status::text as status,
    a.document_type_id,
    dt.name as document_type_name,
    dt.display_name as document_type_display_name,
    a.detection_confidence,
    a.has_preview,
    a.is_canonical_content,
    a.is_ai_generated,
    a.virus_scan_status,
    a.created_at,
    a.updated_at
FROM attachment a
JOIN attachment_blob ab ON a.blob_id = ab.id
LEFT JOIN document_type dt ON a.document_type_id = dt.id;

COMMENT ON VIEW attachment_summary IS 'Joined view of attachments with blob and doctype info';

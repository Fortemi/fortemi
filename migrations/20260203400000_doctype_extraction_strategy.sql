-- ============================================================================
-- Document Type Extraction Strategy Enhancement
-- Issue: #436
-- ============================================================================
-- This migration adds extraction configuration fields to document_type for
-- intelligent file attachment processing. It enhances the existing attachment
-- system with more granular control over content extraction and generation.
--
-- Changes:
-- 1. Add extraction_config JSONB column for strategy-specific configuration
-- 2. Add requires_attachment flag (rename from requires_file_attachment)
-- 3. Add attachment_generates_content flag for AI content generation
-- 4. Update existing media types with extraction strategies
-- 5. Add new doctypes for personal memory use cases
-- ============================================================================

-- ============================================================================
-- PART 1: ADD NEW COLUMNS
-- ============================================================================

-- Add extraction_config for strategy-specific settings
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    extraction_config JSONB DEFAULT '{}';

-- Add requires_attachment flag (semantic alias for requires_file_attachment)
-- Keep both columns for backward compatibility during transition
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    requires_attachment BOOLEAN DEFAULT FALSE;

-- Add attachment_generates_content flag
-- When true, the attachment's extracted/AI-generated content becomes the note content
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS
    attachment_generates_content BOOLEAN DEFAULT FALSE;

-- Add comments
COMMENT ON COLUMN document_type.extraction_config IS
    'Strategy-specific configuration (e.g., {"model": "llava:13b", "include_ocr": true})';
COMMENT ON COLUMN document_type.requires_attachment IS
    'When true, notes of this type must have a file attachment';
COMMENT ON COLUMN document_type.attachment_generates_content IS
    'When true, the attachment content becomes the primary note content';

-- ============================================================================
-- PART 2: UPDATE EXISTING MEDIA TYPES WITH EXTRACTION STRATEGIES
-- ============================================================================

-- Update image types to use vision extraction
UPDATE document_type
SET
    extraction_strategy = 'vision',
    extraction_config = '{"model": "llava:13b", "include_ocr": true}'::jsonb,
    requires_attachment = true,
    attachment_generates_content = true
WHERE category = 'media'
AND name LIKE 'image%';

-- Update PDF types with OCR fallback
UPDATE document_type
SET
    extraction_strategy = 'pdf_text',
    extraction_config = '{"ocr_fallback": true}'::jsonb
WHERE name LIKE 'pdf%'
OR 'application/pdf' = ANY(mime_types);

-- ============================================================================
-- PART 3: ADD NEW DOCTYPES FOR PERSONAL MEMORY
-- ============================================================================

INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    extraction_strategy,
    extraction_config,
    requires_attachment,
    attachment_generates_content,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    is_system,
    is_active
)
VALUES
(
    'personal-memory',
    'Personal Memory',
    'media',
    'Video recording for memory reconstruction and life logging',
    'video_multimodal',
    '{"keyframe_interval": 10, "audio_transcribe": true, "scene_detection": true}'::jsonb,
    true,
    true,
    'semantic',
    512,
    50,
    true,
    true,
    true
),
(
    'voice-memo',
    'Voice Memo',
    'media',
    'Audio recording for transcription and note-taking',
    'audio_transcribe',
    '{"model": "whisper-base", "language": "auto", "timestamps": true}'::jsonb,
    true,
    true,
    'semantic',
    512,
    50,
    true,
    true,
    true
),
(
    'scanned-document',
    'Scanned Document',
    'docs',
    'Scanned PDF or image requiring OCR processing',
    'pdf_ocr',
    '{"ocr_engine": "tesseract", "language": "eng", "deskew": true}'::jsonb,
    true,
    false,
    'semantic',
    512,
    50,
    true,
    true,
    true
)
ON CONFLICT (name) DO UPDATE SET
    extraction_strategy = EXCLUDED.extraction_strategy,
    extraction_config = EXCLUDED.extraction_config,
    requires_attachment = EXCLUDED.requires_attachment,
    attachment_generates_content = EXCLUDED.attachment_generates_content,
    description = EXCLUDED.description,
    updated_at = NOW();

-- ============================================================================
-- PART 4: DATA MIGRATION
-- ============================================================================

-- Copy values from requires_file_attachment to requires_attachment
-- for any existing rows that have requires_file_attachment set
UPDATE document_type
SET requires_attachment = requires_file_attachment
WHERE requires_file_attachment = true
AND requires_attachment = false;

-- Sync processing_config to extraction_config for existing rows
-- that have processing_config but not extraction_config
UPDATE document_type
SET extraction_config = processing_config
WHERE processing_config IS NOT NULL
AND processing_config != '{}'::jsonb
AND (extraction_config IS NULL OR extraction_config = '{}'::jsonb);

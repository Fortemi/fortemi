-- Seed: Extraction strategy configs and new document types
-- Related migration: 20260203400000_doctype_extraction_strategy.sql

-- ============================================================================
-- PART 1: UPDATE EXISTING MEDIA TYPES WITH EXTRACTION STRATEGIES
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
-- PART 2: ADD NEW DOCTYPES FOR PERSONAL MEMORY
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
-- PART 3: DATA MIGRATION
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

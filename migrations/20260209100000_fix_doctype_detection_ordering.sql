-- ============================================================================
-- Fix document type detection ordering (#251, #256)
--
-- Problem: get_by_extension returns non-deterministic results when multiple
-- types claim the same extension. Specific types like scanned-document
-- (missing filename_patterns) win over generic types like image.
--
-- Fix:
-- 1. Add filename_patterns to scanned-document so it only matches via
--    filename pattern detection, not generic extension match
-- 2. Create a generic pdf-document type for .pdf extension match
-- ============================================================================

-- 1. Add filename_patterns to scanned-document so it's treated as a
--    specific type (only matches filenames containing scan/scanned/ocr)
UPDATE document_type
SET filename_patterns = ARRAY['%scan%', '%scanned%', '%ocr%']
WHERE name = 'scanned-document';

-- 2. Create a generic pdf-document type for .pdf files without specific
--    filename patterns. This becomes the default for any PDF file.
INSERT INTO document_type (
    name, display_name, category, description,
    file_extensions, mime_types,
    chunking_strategy,
    is_system
) VALUES (
    'pdf-document',
    'PDF Document',
    'prose',
    'Generic PDF documents',
    ARRAY['.pdf'],
    ARRAY['application/pdf'],
    'per_section',
    TRUE
) ON CONFLICT (name) DO NOTHING;

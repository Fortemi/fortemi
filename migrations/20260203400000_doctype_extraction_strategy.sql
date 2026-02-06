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

-- Seed data moved to: 20260203400000_seed_extraction_strategies.sql

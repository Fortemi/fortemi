-- Add ai_description column to attachment table for storing AI-generated
-- descriptions from extraction adapters (Vision, Glb3DModel, etc.).
-- Previously, ExtractionResult.ai_description was discarded by the
-- ExtractionHandler (Issue #492, Bug 1).

ALTER TABLE attachment ADD COLUMN IF NOT EXISTS ai_description TEXT;
ALTER TABLE attachment ADD COLUMN IF NOT EXISTS ai_model TEXT;

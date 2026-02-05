-- Add missing extraction_strategy enum values
-- These values are used by later migrations but weren't in the original enum definition
-- Issue: CI migration failures

-- Add new extraction strategies for specialized media types
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'video_multimodal';  -- Video with keyframes + audio
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'audio_transcribe';   -- Audio/speech transcription
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'structured_extract'; -- Structured formats (SVG, MIDI, diagrams)

COMMENT ON TYPE extraction_strategy IS 'Content extraction method: text_native, pdf_text, pdf_ocr, pandoc, vision, video_multimodal, audio_transcribe, structured_extract';

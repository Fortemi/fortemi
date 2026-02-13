-- Fix: Audio MIME types should route to audio_transcribe, not text_native
-- Issue: #336 - Generic 'audio' document type had NULL extraction_strategy,
-- which parse_extraction_strategy() defaults to TextNative.
-- Specific audio types (voice-memo, tracker-module) already use audio_transcribe,
-- but the generic fallback 'audio' type was missing it.

UPDATE document_type
SET extraction_strategy = 'audio_transcribe'
WHERE name = 'audio'
  AND extraction_strategy IS NULL;

-- Also fix 'video' and 'podcast' types which have the same NULL issue
UPDATE document_type
SET extraction_strategy = 'video_multimodal'
WHERE name = 'video'
  AND extraction_strategy IS NULL;

UPDATE document_type
SET extraction_strategy = 'audio_transcribe'
WHERE name = 'podcast'
  AND extraction_strategy IS NULL;

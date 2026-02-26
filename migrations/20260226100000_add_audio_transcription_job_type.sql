-- Add audio_transcription job type for atomic audio transcription jobs (#542)
-- Used by AudioTranscriptionHandler: processes audio from derived attachments,
-- calls Whisper, stores transcript segments, persists caption files (VTT/SRT/TXT),
-- and coordinates fan-in with KeyframeAssembly for video content.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'audio_transcription';

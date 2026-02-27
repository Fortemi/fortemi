-- Add audio_chunk_transcription job type for atomic per-chunk audio transcription.
-- Each chunk job transcribes a single audio segment via Whisper; the last to
-- complete merges results and triggers downstream work (diarization, assembly).
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'audio_chunk_transcription';

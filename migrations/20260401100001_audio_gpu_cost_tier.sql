-- Assign AUDIO_GPU cost tier (5) to audio job types (#576).
-- Previously these were tier-agnostic (NULL), which meant they ran in Phase 1
-- alongside CPU jobs. With dedicated audio tier, the worker can manage sidecar
-- lifecycle at tier boundaries.

UPDATE job_queue
SET cost_tier = 5
WHERE job_type IN ('audio_transcription', 'audio_chunk_transcription', 'speaker_diarization')
  AND status = 'pending'
  AND cost_tier IS NULL;

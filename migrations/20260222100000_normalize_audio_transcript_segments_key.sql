-- Normalize audio extraction metadata: rename "segments" key to "transcript_segments"
-- for consistency with video pipeline. The UI reads "transcript_segments" from
-- extracted_metadata; audio files previously stored under "segments".
UPDATE attachment
SET extracted_metadata = (
    extracted_metadata - 'segments'
) || jsonb_build_object('transcript_segments', extracted_metadata->'segments')
WHERE extracted_metadata ? 'segments'
  AND NOT extracted_metadata ? 'transcript_segments'
  AND extraction_strategy = 'audio_transcribe';

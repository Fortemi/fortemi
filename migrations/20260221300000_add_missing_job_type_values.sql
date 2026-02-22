-- Add missing job_type enum values (#506)
-- Several Rust JobType variants were added without corresponding DB migrations.
-- This migration adds them all, plus the new media_optimize type.

ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'ai_revision_contextual';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'document_type_inference';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'speaker_diarization';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'speaker_relabel';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'media_optimize';

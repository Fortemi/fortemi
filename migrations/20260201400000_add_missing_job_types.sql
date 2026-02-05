-- Add missing job_type enum values
-- These job types are defined in Rust but were missing from the initial schema

-- Note: ALTER TYPE ADD VALUE cannot run inside a transaction block in older PostgreSQL versions
-- but works in PostgreSQL 12+ with IF NOT EXISTS

ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'create_embedding_set';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'refresh_embedding_set';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'build_set_index';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'purge_note';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'concept_tagging';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 're_embed_all';

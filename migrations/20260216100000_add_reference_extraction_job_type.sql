-- Add reference_extraction job type for extracting named entity references
-- (companies, people, tools, datasets, venues, etc.) from note content.
-- Runs in Phase 1 alongside ConceptTagging.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'reference_extraction';

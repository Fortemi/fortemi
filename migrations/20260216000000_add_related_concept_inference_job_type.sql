-- Add related_concept_inference job type for the SKOS related concept inference pipeline step.
-- This step runs between ConceptTagging and Embedding to infer associative (related)
-- relationships between concepts tagged on a note.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'related_concept_inference';

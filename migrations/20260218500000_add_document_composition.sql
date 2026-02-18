-- Add document composition config to embedding_config (#485).
-- Controls which note properties are included in embedding text per config.
-- Default '{}' means DocumentComposition::default() (title+content, no tags).

ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS document_composition JSONB NOT NULL DEFAULT '{}'::jsonb;

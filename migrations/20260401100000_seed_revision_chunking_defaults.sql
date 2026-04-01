-- Seed revision_chunking defaults in agentic_config for video document types (#573).
-- These control per-document-type chunk sizing for AI revision.
-- Resolution: per-call > document type > env var > auto-computed from model.

-- Movie/documentary: longer form, larger chunks for coherent scene-level revision
UPDATE document_type
SET agentic_config = jsonb_set(
    COALESCE(agentic_config, '{}'::jsonb),
    '{revision_chunking}',
    '{"max_chars": 80000, "overlap": 0}'::jsonb
)
WHERE name IN ('movie', 'documentary');

-- Meeting recordings and lectures: moderate chunks for structured content
UPDATE document_type
SET agentic_config = jsonb_set(
    COALESCE(agentic_config, '{}'::jsonb),
    '{revision_chunking}',
    '{"max_chars": 60000, "overlap": 0}'::jsonb
)
WHERE name IN ('meeting-recording', 'lecture');

-- Short video: small content, no chunking needed (null means use system default)
-- No update needed — absence of revision_chunking means auto-computed.

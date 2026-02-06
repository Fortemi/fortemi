-- Seed: Default embedding configuration and set

-- Create default embedding config
INSERT INTO embedding_config (
    id,
    name,
    description,
    model,
    dimension,
    chunk_size,
    chunk_overlap,
    is_default
) VALUES (
    gen_uuid_v7(),
    'default',
    'Default embedding configuration using nomic-embed-text (768 dimensions)',
    'nomic-embed-text',
    768,
    1500,
    200,
    TRUE
);

-- Create the "default" embedding set
INSERT INTO embedding_set (
    id,
    name,
    slug,
    description,
    purpose,
    usage_hints,
    keywords,
    mode,
    criteria,
    embedding_config_id,
    is_system,
    is_active,
    index_status
) VALUES (
    gen_uuid_v7(),
    'Default',
    'default',
    'Primary embedding set containing all notes. Used for general semantic search.',
    'Provides semantic search across the entire knowledge base.',
    'Use this set for general queries when you want to search all content. This is the default set used when no specific set is specified.',
    ARRAY['all', 'general', 'default', 'everything', 'global'],
    'auto',
    '{"include_all": true, "exclude_archived": true}'::jsonb,
    (SELECT id FROM embedding_config WHERE is_default = TRUE),
    TRUE,
    TRUE,
    'ready'
);

-- Migrate existing embeddings to default set
UPDATE embedding
SET embedding_set_id = (SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default')
WHERE embedding_set_id IS NULL;

-- Add existing notes with embeddings to default set membership
INSERT INTO embedding_set_member (embedding_set_id, note_id, membership_type)
SELECT DISTINCT
    (SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default'),
    note_id,
    'auto'
FROM embedding
WHERE embedding_set_id = (SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default')
ON CONFLICT DO NOTHING;

-- Update stats for default set
SELECT update_embedding_set_stats((SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default'));

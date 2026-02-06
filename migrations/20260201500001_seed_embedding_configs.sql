-- Seed: Advanced embedding model configurations (mxbai, bge, multilingual-e5)

-- mxbai-embed-large (MRL-enabled)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'mxbai-embed-large',
    'MixedBread mxbai-embed-large-v1 with Matryoshka support (1024 dimensions, MRL to 64)',
    'mxbai-embed-large-v1',
    1024,
    1500,
    200,
    TRUE,
    ARRAY[1024, 512, 256, 128, 64],
    256,
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    supports_mrl = EXCLUDED.supports_mrl,
    matryoshka_dims = EXCLUDED.matryoshka_dims,
    default_truncate_dim = EXCLUDED.default_truncate_dim;

-- bge-large (no MRL)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'bge-large-en',
    'BGE Large English v1.5 (1024 dimensions, no MRL support)',
    'bge-large-en-v1.5',
    1024,
    1500,
    200,
    FALSE,
    NULL,
    NULL,
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    supports_mrl = EXCLUDED.supports_mrl;

-- multilingual-e5-large (no MRL, multilingual)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'multilingual-e5-large',
    'Multilingual E5 Large (1024 dimensions, 100+ languages)',
    'multilingual-e5-large',
    1024,
    1500,
    200,
    FALSE,
    NULL,
    NULL,
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    supports_mrl = EXCLUDED.supports_mrl;

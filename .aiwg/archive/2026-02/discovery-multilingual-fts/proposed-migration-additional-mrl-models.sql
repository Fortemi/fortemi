-- ============================================================================
-- Additional MRL Embedding Models Migration
-- ============================================================================
-- Adds research-validated MRL-enabled embedding models:
-- 1. jina-embeddings-v3: Best multilingual MRL (1024->32 dim)
-- 2. gte-Qwen2-1.5B: Commercial-friendly multilingual MRL (Apache 2.0)
-- 3. all-MiniLM-L6-v2: Fast model optimized for LLM re-ranking (REF-068)
-- 4. stella_en_1.5B_v5: High-quality English MRL (optional)
-- 5. jina-embeddings-v2-base-code: Code search (no MRL)
--
-- Research: .aiwg/working/discovery/multilingual-fts/mrl-embedding-models-research.md
-- Date: 2026-02-02
-- ============================================================================

-- ============================================================================
-- HIGH PRIORITY: Production-Ready Models
-- ============================================================================

-- jina-embeddings-v3: Best-in-class multilingual MRL
-- - 89+ languages with excellent MTEB scores
-- - MRL range: 1024 down to 32 dimensions
-- - 8k context window support
-- - Caveat: CC-BY-NC-4.0 license (non-commercial use)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'jina-embeddings-v3',
    'Jina AI v3 (1024 dims, 89+ languages, MRL 32-1024, 8k context)',
    'jinaai/jina-embeddings-v3',
    1024,
    8192,  -- Supports 8k context with ALiBi
    200,
    TRUE,
    ARRAY[1024, 512, 256, 128, 64, 32],  -- Full MRL range
    512,  -- Balanced default: 2× storage savings, ~1% quality loss
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    supports_mrl = EXCLUDED.supports_mrl,
    matryoshka_dims = EXCLUDED.matryoshka_dims,
    default_truncate_dim = EXCLUDED.default_truncate_dim,
    chunk_size = EXCLUDED.chunk_size;

COMMENT ON TABLE embedding_config IS 'Note: jina-v3 requires CC-BY-NC-4.0 compliance (non-commercial)';

-- gte-Qwen2-1.5B-instruct: Commercial-friendly multilingual MRL
-- - Apache 2.0 license (commercial use allowed)
-- - Strong MTEB performance (NDCG@10: 70+)
-- - Multilingual support with 1.5B params
-- - Requires ~3GB GPU memory
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'gte-qwen2-1.5b-instruct',
    'Alibaba GTE-Qwen2 1.5B (1536 dims, multilingual, Apache 2.0, MRL 128-1536)',
    'Alibaba-NLP/gte-Qwen2-1.5B-instruct',
    1536,
    8192,
    200,
    TRUE,
    ARRAY[1536, 1024, 512, 256, 128],  -- Standard MRL dimensions
    512,  -- Balanced 3× reduction
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    supports_mrl = EXCLUDED.supports_mrl,
    matryoshka_dims = EXCLUDED.matryoshka_dims,
    default_truncate_dim = EXCLUDED.default_truncate_dim;

-- all-MiniLM-L6-v2: Fast model optimized for LLM re-ranking
-- - Per REF-068: Outperforms larger models when combined with LLM re-ranking
-- - 2.4× faster than nomic-embed
-- - Best for RAG with Claude/GPT/Gemini re-ranking stage
-- - No MRL needed (already 384-dim)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'all-minilm-l6-v2',
    'MiniLM-v6 (384 dims, fast, outperforms larger models with LLM re-ranking)',
    'sentence-transformers/all-MiniLM-L6-v2',
    384,
    512,   -- Smaller context optimized for speed
    100,   -- Less overlap for faster chunking
    FALSE,
    NULL,
    NULL,
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description;

COMMENT ON TABLE embedding_config IS 'MiniLM-v6: Per Rao et al. 2025, beats BGE-Large 335M with LLM re-ranking';

-- ============================================================================
-- MEDIUM PRIORITY: Optional Models (Pending Verification)
-- ============================================================================

-- stella_en_1.5B_v5: High-quality English MRL
-- - Excellent MTEB scores (Banking77: 89.8%, ArguAna: 65.3 NDCG@10)
-- - English-optimized with 1.5B params
-- - Caveat: License needs verification before production use
-- - Requires ~3GB GPU memory
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'stella-en-1.5b-v5',
    'Stella 1.5B v5 (1024 dims, English-optimized, MRL 64-1024, verify license)',
    'NovaSearch/stella_en_1.5B_v5',
    1024,
    8192,
    200,
    TRUE,
    ARRAY[1024, 512, 256, 128, 64],
    256,  -- 4× storage savings
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    supports_mrl = EXCLUDED.supports_mrl,
    matryoshka_dims = EXCLUDED.matryoshka_dims;

COMMENT ON TABLE embedding_config IS 'Stella: Verify license on HuggingFace before commercial use';

-- ============================================================================
-- CODE-SPECIFIC MODELS
-- ============================================================================

-- jina-embeddings-v2-base-code: Code search without MRL
-- - 30+ programming languages (Python, JavaScript, Java, C++, Go, Rust, etc.)
-- - 8k context window with ALiBi
-- - Trained on 150M+ code Q&A pairs from github-code dataset
-- - No MRL support (v2 predates Matryoshka training)
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'jina-code-v2',
    'Jina v2 Code (768 dims, 30+ languages, 8k context, no MRL)',
    'jinaai/jina-embeddings-v2-base-code',
    768,
    8192,  -- Long context for code files
    200,
    FALSE,
    NULL,
    NULL,
    FALSE
) ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    chunk_size = EXCLUDED.chunk_size;

-- ============================================================================
-- HELPER VIEW: Show All MRL Models
-- ============================================================================

CREATE OR REPLACE VIEW embedding_config_mrl AS
SELECT
    name,
    model,
    dimension,
    supports_mrl,
    matryoshka_dims,
    default_truncate_dim,
    CASE
        WHEN supports_mrl AND matryoshka_dims IS NOT NULL THEN
            dimension / matryoshka_dims[array_length(matryoshka_dims, 1)]
        ELSE 1
    END as max_compression_ratio,
    is_default
FROM embedding_config
WHERE supports_mrl = TRUE
ORDER BY dimension DESC;

COMMENT ON VIEW embedding_config_mrl IS 'All MRL-enabled embedding configs with compression ratios';

-- ============================================================================
-- VALIDATION: Check MRL Dimensions Are Valid
-- ============================================================================

DO $$
DECLARE
    config_rec RECORD;
    dim INTEGER;
BEGIN
    FOR config_rec IN
        SELECT name, matryoshka_dims
        FROM embedding_config
        WHERE supports_mrl = TRUE AND matryoshka_dims IS NOT NULL
    LOOP
        -- Verify dimensions are in descending order
        FOR i IN 1..(array_length(config_rec.matryoshka_dims, 1) - 1) LOOP
            IF config_rec.matryoshka_dims[i] <= config_rec.matryoshka_dims[i + 1] THEN
                RAISE WARNING 'Model % has non-descending MRL dimensions: %',
                    config_rec.name, config_rec.matryoshka_dims;
            END IF;
        END LOOP;
    END LOOP;
END$$;

-- ============================================================================
-- USAGE EXAMPLES (Documentation)
-- ============================================================================

COMMENT ON TABLE embedding_config IS '
Usage Examples:

-- Create multilingual Full Set with jina-v3 @ 256-dim (4× compression)
INSERT INTO embedding_set (name, slug, set_type, embedding_config_id, truncate_dim)
SELECT ''Multilingual Docs'', ''multilingual'', ''full'', id, 256
FROM embedding_config WHERE name = ''jina-embeddings-v3'';

-- Create fast RAG set with MiniLM for LLM re-ranking
INSERT INTO embedding_set (name, slug, set_type, embedding_config_id)
SELECT ''Fast RAG'', ''fast-rag'', ''full'', id
FROM embedding_config WHERE name = ''all-minilm-l6-v2'';

-- Create code search set with jina-code-v2
INSERT INTO embedding_set (name, slug, set_type, embedding_config_id)
SELECT ''Code Search'', ''code-search'', ''full'', id
FROM embedding_config WHERE name = ''jina-code-v2'';

-- Validate MRL truncation
SELECT validate_mrl_truncation(
    (SELECT id FROM embedding_config WHERE name = ''jina-embeddings-v3''),
    256  -- Requested dimension
);  -- Returns TRUE if valid

-- Query available MRL models
SELECT * FROM embedding_config_mrl;
';

-- ============================================================================
-- PERFORMANCE NOTES
-- ============================================================================

COMMENT ON COLUMN embedding_config.matryoshka_dims IS '
Validated MRL dimensions from research:

jina-v3:           [1024, 512, 256, 128, 64, 32] - Widest range
gte-Qwen2-1.5B:    [1536, 1024, 512, 256, 128]   - Large baseline
stella-1.5B:       [1024, 512, 256, 128, 64]     - English-optimized
nomic-v1.5:        [768, 512, 256, 128, 64]      - Current default
mxbai-large:       [1024, 512, 256, 128, 64]     - Current alternate

Quality retention by dimension (approximate):
- 512-dim: 98-99% of full quality, 2× storage savings
- 256-dim: 95-97% of full quality, 4× storage savings
- 128-dim: 92-95% of full quality, 8× storage savings
- 64-dim:  88-92% of full quality, 16× storage savings
- 32-dim:  80-85% of full quality, 32× storage savings (jina-v3 only)

Inference speed estimates (relative to nomic-embed baseline):
- all-MiniLM:   2.4× faster (22M params)
- nomic-v1.5:   1.0× (baseline, 137M params)
- jina-v3:      0.5× (570M params, ~2GB GPU)
- stella-1.5B:  0.3× (1.5B params, ~3GB GPU)
- gte-Qwen2:    0.3× (1.5B params, ~3GB GPU)

Storage per 1M documents @ 512-dim truncation: ~2GB
';

-- ============================================================================
-- MIGRATION VERIFICATION
-- ============================================================================

-- Verify all configs were inserted
DO $$
DECLARE
    expected_models TEXT[] := ARRAY[
        'jina-embeddings-v3',
        'gte-qwen2-1.5b-instruct',
        'all-minilm-l6-v2',
        'stella-en-1.5b-v5',
        'jina-code-v2'
    ];
    model TEXT;
    missing_count INTEGER := 0;
BEGIN
    FOREACH model IN ARRAY expected_models LOOP
        IF NOT EXISTS (SELECT 1 FROM embedding_config WHERE name = model) THEN
            RAISE WARNING 'Missing embedding config: %', model;
            missing_count := missing_count + 1;
        END IF;
    END LOOP;

    IF missing_count = 0 THEN
        RAISE NOTICE 'All % new embedding configs inserted successfully', array_length(expected_models, 1);
    ELSE
        RAISE WARNING '% embedding configs missing', missing_count;
    END IF;
END$$;

-- Show summary of MRL-enabled configs
DO $$
DECLARE
    mrl_count INTEGER;
    total_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO mrl_count FROM embedding_config WHERE supports_mrl = TRUE;
    SELECT COUNT(*) INTO total_count FROM embedding_config;

    RAISE NOTICE 'MRL-enabled configs: % of % total', mrl_count, total_count;
END$$;

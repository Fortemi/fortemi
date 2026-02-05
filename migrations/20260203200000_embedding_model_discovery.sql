-- ============================================================================
-- Embedding Model Discovery and Selection API (#447)
-- ============================================================================
-- Adds model metadata columns to embedding_config for discovery and
-- recommendation functionality.
-- ============================================================================

-- ============================================================================
-- PART 1: ADD MODEL METADATA COLUMNS
-- ============================================================================

-- Add strengths (e.g., "code", "multilingual", "long-context")
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS strengths TEXT[] DEFAULT '{}';

-- Add limitations (e.g., "English-only", "512 token limit")
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS limitations TEXT[] DEFAULT '{}';

-- Add recommended use cases (e.g., "code-search", "semantic-dedup")
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS recommended_for TEXT[] DEFAULT '{}';

-- Add benchmark scores (JSONB for flexibility: MTEB, BEIR, etc.)
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS benchmark_scores JSONB DEFAULT '{}';

-- Add availability flag (for models not yet installed locally)
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS is_available BOOLEAN DEFAULT true;

-- ============================================================================
-- PART 2: INDICES FOR DISCOVERY QUERIES
-- ============================================================================

-- Index for strengths queries (e.g., find models with "multilingual")
CREATE INDEX IF NOT EXISTS idx_embedding_config_strengths
    ON embedding_config USING GIN(strengths);

-- Index for recommended_for queries
CREATE INDEX IF NOT EXISTS idx_embedding_config_recommended_for
    ON embedding_config USING GIN(recommended_for);

-- Index for availability queries
CREATE INDEX IF NOT EXISTS idx_embedding_config_is_available
    ON embedding_config(is_available);

-- ============================================================================
-- PART 3: SEED MODEL METADATA
-- ============================================================================

-- nomic-embed-text (768d, symmetric, long-context)
UPDATE embedding_config
SET
    strengths = ARRAY['long-context', 'symmetric', 'general-purpose'],
    limitations = ARRAY['Large model size (~1GB)'],
    recommended_for = ARRAY['general-search', 'long-documents', 'semantic-dedup'],
    benchmark_scores = '{
        "mteb_retrieval": 0.537,
        "mteb_clustering": 0.491,
        "mteb_classification": 0.723,
        "context_length": 8192
    }'::jsonb,
    is_available = true
WHERE model = 'nomic-embed-text';

-- snowflake-arctic-embed (if exists)
UPDATE embedding_config
SET
    strengths = ARRAY['retrieval', 'symmetric', 'balanced'],
    limitations = ARRAY['512 token limit'],
    recommended_for = ARRAY['general-search', 'qa-systems', 'semantic-search'],
    benchmark_scores = '{
        "mteb_retrieval": 0.547,
        "mteb_clustering": 0.503,
        "context_length": 512
    }'::jsonb,
    is_available = true
WHERE model LIKE 'snowflake-arctic-embed%';

-- all-minilm (384d, fast, lightweight)
UPDATE embedding_config
SET
    strengths = ARRAY['fast', 'lightweight', 'low-latency'],
    limitations = ARRAY['Lower accuracy', '256 token limit', 'English-only'],
    recommended_for = ARRAY['real-time-search', 'resource-constrained', 'demo'],
    benchmark_scores = '{
        "mteb_retrieval": 0.420,
        "mteb_clustering": 0.392,
        "context_length": 256,
        "inference_speed_ms": 10
    }'::jsonb,
    is_available = true
WHERE model = 'all-minilm';

-- e5 models (asymmetric)
UPDATE embedding_config
SET
    strengths = ARRAY['asymmetric', 'high-quality', 'retrieval'],
    limitations = ARRAY['Requires query/passage prefixes', '512 token limit'],
    recommended_for = ARRAY['retrieval', 'qa-systems', 'document-search'],
    benchmark_scores = '{
        "mteb_retrieval": 0.510,
        "context_length": 512
    }'::jsonb,
    is_available = true
WHERE model LIKE 'e5-%';

-- multilingual-e5 models
UPDATE embedding_config
SET
    strengths = ARRAY['multilingual', 'asymmetric', '100-languages'],
    limitations = ARRAY['Requires query/passage prefixes', '512 token limit'],
    recommended_for = ARRAY['multilingual-search', 'cross-lingual-retrieval'],
    benchmark_scores = '{
        "mteb_retrieval": 0.495,
        "context_length": 512,
        "languages": 100
    }'::jsonb,
    is_available = true
WHERE model LIKE 'multilingual-e5%';

-- bge models (asymmetric, high-quality)
UPDATE embedding_config
SET
    strengths = ARRAY['high-quality', 'asymmetric', 'general-purpose'],
    limitations = ARRAY['Query prefix required', '512 token limit'],
    recommended_for = ARRAY['retrieval', 'semantic-search', 'qa-systems'],
    benchmark_scores = '{
        "mteb_retrieval": 0.530,
        "context_length": 512
    }'::jsonb,
    is_available = true
WHERE model LIKE 'bge-%';

-- mxbai-embed-large (1024d, symmetric)
UPDATE embedding_config
SET
    strengths = ARRAY['high-dimension', 'symmetric', 'high-quality'],
    limitations = ARRAY['512 token limit', 'Large model size'],
    recommended_for = ARRAY['semantic-search', 'clustering', 'deduplication'],
    benchmark_scores = '{
        "mteb_retrieval": 0.548,
        "mteb_clustering": 0.512,
        "context_length": 512
    }'::jsonb,
    is_available = true
WHERE model = 'mxbai-embed-large';

-- ============================================================================
-- PART 4: COMMENTS
-- ============================================================================

COMMENT ON COLUMN embedding_config.strengths IS 'Model strengths (e.g., code, multilingual, long-context)';
COMMENT ON COLUMN embedding_config.limitations IS 'Model limitations (e.g., token limits, language constraints)';
COMMENT ON COLUMN embedding_config.recommended_for IS 'Recommended use cases (e.g., code-search, semantic-dedup)';
COMMENT ON COLUMN embedding_config.benchmark_scores IS 'Benchmark scores (MTEB, BEIR, etc.) as JSON';
COMMENT ON COLUMN embedding_config.is_available IS 'Whether the model is available locally (vs. requires download)';

-- ============================================================================
-- ColBERT Late Interaction Re-ranking Support
-- ============================================================================
-- Adds support for ColBERT-style token-level embeddings for re-ranking.
--
-- ColBERT (Contextualized Late Interaction over BERT) stores per-token
-- embeddings to enable fine-grained semantic matching at query time.
--
-- Architecture:
-- - Store 128-dim token embeddings for each document token
-- - Query-time: encode query tokens, compute MaxSim score
-- - MaxSim = Σ max(qi · dj) for all query tokens i and doc tokens j
-- - Use as re-ranking stage after initial hybrid search retrieval
--
-- Version: 0.1.0
-- Generated: 2026-02-05
-- Issue: #173
-- ============================================================================

-- ============================================================================
-- TABLES
-- ============================================================================

-- Token embeddings for ColBERT late interaction re-ranking
CREATE TABLE note_token_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,

    -- Optional chunk reference for multi-chunk documents
    chunk_id UUID REFERENCES embedding(id) ON DELETE CASCADE,

    -- Token metadata
    token_position INTEGER NOT NULL,
    token_text TEXT NOT NULL,

    -- 128-dim token embedding for efficient similarity computation
    embedding VECTOR(128),

    -- Model information
    model TEXT NOT NULL DEFAULT 'colbert-v2',

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Ensure unique token positions per note/chunk
    UNIQUE(note_id, chunk_id, token_position)
);

-- ============================================================================
-- INDICES
-- ============================================================================

-- Primary lookup: retrieve all tokens for a note
CREATE INDEX idx_token_embeddings_note ON note_token_embeddings(note_id);

-- Chunk-level lookup for multi-chunk documents
CREATE INDEX idx_token_embeddings_chunk ON note_token_embeddings(chunk_id)
    WHERE chunk_id IS NOT NULL;

-- Model tracking for future migration support
CREATE INDEX idx_token_embeddings_model ON note_token_embeddings(model);

-- Vector similarity search (HNSW index for efficient nearest neighbor)
-- This enables fast retrieval of similar tokens during re-ranking
CREATE INDEX idx_token_embeddings_vector ON note_token_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- ============================================================================
-- ADD JOB TYPE FOR COLBERT EMBEDDING GENERATION
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'generate_colbert_embeddings'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'generate_colbert_embeddings';
    END IF;
END$$;

-- ============================================================================
-- STATISTICS AND MONITORING
-- ============================================================================

-- View for monitoring ColBERT embedding coverage
CREATE VIEW colbert_embedding_stats AS
SELECT
    (SELECT COUNT(DISTINCT note_id) FROM note_token_embeddings) AS notes_with_tokens,
    (SELECT COUNT(*) FROM note_token_embeddings) AS total_tokens,
    AVG(token_count) AS avg_tokens_per_note,
    MAX(token_count) AS max_tokens_per_note,
    (SELECT COUNT(DISTINCT model) FROM note_token_embeddings) AS model_count,
    pg_size_pretty(pg_total_relation_size('note_token_embeddings')) AS total_size
FROM (
    SELECT COUNT(*) AS token_count
    FROM note_token_embeddings
    GROUP BY note_id
) AS note_stats;

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Check if a note has ColBERT token embeddings
CREATE OR REPLACE FUNCTION has_colbert_embeddings(p_note_id UUID)
RETURNS BOOLEAN AS $$
    SELECT EXISTS(
        SELECT 1 FROM note_token_embeddings
        WHERE note_id = p_note_id
    );
$$ LANGUAGE SQL STABLE;

-- Get token count for a note
CREATE OR REPLACE FUNCTION get_token_count(p_note_id UUID)
RETURNS INTEGER AS $$
    SELECT COUNT(*)::INTEGER
    FROM note_token_embeddings
    WHERE note_id = p_note_id;
$$ LANGUAGE SQL STABLE;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE note_token_embeddings IS
'Token-level embeddings for ColBERT late interaction re-ranking. Each row represents a single token with its 128-dim contextualized embedding.';

COMMENT ON COLUMN note_token_embeddings.note_id IS
'Reference to the note this token belongs to';

COMMENT ON COLUMN note_token_embeddings.chunk_id IS
'Optional reference to specific embedding chunk for multi-chunk documents';

COMMENT ON COLUMN note_token_embeddings.token_position IS
'Zero-based position of this token in the document';

COMMENT ON COLUMN note_token_embeddings.token_text IS
'Original token text for debugging and analysis';

COMMENT ON COLUMN note_token_embeddings.embedding IS
'128-dimensional contextualized token embedding for late interaction scoring';

COMMENT ON COLUMN note_token_embeddings.model IS
'Model used to generate this token embedding (e.g., "colbert-v2")';

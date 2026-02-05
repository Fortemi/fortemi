-- ============================================================================
-- Full Embedding Sets Migration (#384, #385, #386, #387, #388)
-- ============================================================================
-- Adds support for:
-- 1. Full embedding sets with their own embeddings (vs filter sets)
-- 2. Auto-embedding rules for automatic embedding lifecycle
-- 3. Matryoshka (MRL) dimension support for storage optimization
-- 4. Entity extraction for tri-modal search (#386)
-- 5. Synthetic data generation for fine-tuning (#387)
-- 6. Coarse embeddings for two-stage retrieval (#388)
-- ============================================================================

-- ============================================================================
-- PART 1: EMBEDDING SET TYPE & AUTO-EMBED RULES (#384)
-- ============================================================================

-- Embedding set type: filter (shares default embeddings) vs full (own embeddings)
CREATE TYPE embedding_set_type AS ENUM (
    'filter',    -- Logical view filtering shared embeddings from default
    'full'       -- Independent set with own embeddings and model
);

-- Add set_type column to embedding_set
ALTER TABLE embedding_set
ADD COLUMN IF NOT EXISTS set_type embedding_set_type NOT NULL DEFAULT 'filter';

-- Auto-embedding rules for Full sets
ALTER TABLE embedding_set
ADD COLUMN IF NOT EXISTS auto_embed_rules JSONB DEFAULT '{}'::jsonb;

-- Tracking whether embeddings are current for Full sets
ALTER TABLE embedding_set
ADD COLUMN IF NOT EXISTS embeddings_current BOOLEAN DEFAULT TRUE;

-- Truncation dimension for MRL (NULL = use full dimension)
ALTER TABLE embedding_set
ADD COLUMN IF NOT EXISTS truncate_dim INTEGER DEFAULT NULL;

-- ============================================================================
-- PART 2: MATRYOSHKA (MRL) SUPPORT (#385)
-- ============================================================================

-- Add MRL fields to embedding_config
ALTER TABLE embedding_config
ADD COLUMN IF NOT EXISTS supports_mrl BOOLEAN DEFAULT FALSE;

ALTER TABLE embedding_config
ADD COLUMN IF NOT EXISTS matryoshka_dims INTEGER[] DEFAULT NULL;

ALTER TABLE embedding_config
ADD COLUMN IF NOT EXISTS default_truncate_dim INTEGER DEFAULT NULL;

-- Update default config with MRL info for nomic-embed-text
UPDATE embedding_config
SET supports_mrl = TRUE,
    matryoshka_dims = ARRAY[768, 512, 256, 128, 64],
    default_truncate_dim = 256
WHERE model = 'nomic-embed-text';

-- ============================================================================
-- PART 3: MULTI-SET EMBEDDING SUPPORT (#384)
-- ============================================================================

-- Modify embedding unique constraint to support same note in multiple sets
-- First drop the old constraint
ALTER TABLE embedding
DROP CONSTRAINT IF EXISTS embedding_note_id_chunk_index_key;

-- Add new constraint including embedding_set_id
ALTER TABLE embedding
ADD CONSTRAINT embedding_note_set_chunk_unique
UNIQUE (note_id, embedding_set_id, chunk_index);

-- ============================================================================
-- PART 4: ENTITY EXTRACTION FOR TRI-MODAL SEARCH (#386)
-- ============================================================================

-- Entity types for NER
CREATE TYPE entity_type AS ENUM (
    'person',
    'organization',
    'location',
    'product',
    'event',
    'date',
    'money',
    'percent',
    'work_of_art',
    'language',
    'other'
);

-- Extracted entities per note
CREATE TABLE IF NOT EXISTS note_entity (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    entity_text TEXT NOT NULL,
    entity_type entity_type NOT NULL,
    start_offset INTEGER,
    end_offset INTEGER,
    confidence FLOAT,
    normalized_text TEXT,  -- Canonical form for deduplication
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_note_entity_note ON note_entity(note_id);
CREATE INDEX idx_note_entity_text ON note_entity(entity_text);
CREATE INDEX idx_note_entity_type ON note_entity(entity_type);
CREATE INDEX idx_note_entity_normalized ON note_entity(normalized_text) WHERE normalized_text IS NOT NULL;

-- Entity IDF statistics (corpus-level frequency for weighting)
CREATE TABLE IF NOT EXISTS entity_stats (
    entity_text TEXT PRIMARY KEY,
    doc_frequency INTEGER NOT NULL DEFAULT 1,
    idf_score FLOAT,
    last_updated TIMESTAMPTZ DEFAULT NOW()
);

-- Graph embedding per note (aggregated entity representation)
CREATE TABLE IF NOT EXISTS note_graph_embedding (
    note_id UUID PRIMARY KEY REFERENCES note(id) ON DELETE CASCADE,
    vector vector(384) NOT NULL,  -- Smaller dimension for graph modality
    entity_count INTEGER NOT NULL DEFAULT 0,
    entity_types TEXT[],  -- Which entity types are represented
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- HNSW index for graph embeddings
CREATE INDEX idx_note_graph_embedding_hnsw
ON note_graph_embedding USING hnsw (vector vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- ============================================================================
-- PART 5: FINE-TUNING DATA GENERATION (#387)
-- ============================================================================

-- Training data generation jobs
CREATE TABLE IF NOT EXISTS fine_tuning_dataset (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    source_type TEXT NOT NULL,  -- 'embedding_set', 'tag', 'collection'
    source_id TEXT NOT NULL,    -- slug, tag name, or collection id
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, generating, completed, failed
    sample_count INTEGER DEFAULT 0,
    training_count INTEGER DEFAULT 0,
    validation_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error_message TEXT
);

CREATE INDEX idx_fine_tuning_dataset_status ON fine_tuning_dataset(status);

-- Generated query-document pairs for training
CREATE TABLE IF NOT EXISTS fine_tuning_sample (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    dataset_id UUID NOT NULL REFERENCES fine_tuning_dataset(id) ON DELETE CASCADE,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    query TEXT NOT NULL,
    query_type TEXT,  -- factoid, conceptual, comparative
    quality_score FLOAT,
    is_validation BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fine_tuning_sample_dataset ON fine_tuning_sample(dataset_id);
CREATE INDEX idx_fine_tuning_sample_note ON fine_tuning_sample(note_id);
CREATE INDEX idx_fine_tuning_sample_validation ON fine_tuning_sample(dataset_id, is_validation);

-- ============================================================================
-- PART 6: COARSE EMBEDDINGS FOR TWO-STAGE RETRIEVAL (#388)
-- ============================================================================

-- Coarse embeddings (small dimension) for fast initial filtering
CREATE TABLE IF NOT EXISTS embedding_coarse (
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    embedding_set_id UUID REFERENCES embedding_set(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL DEFAULT 0,
    vector vector(64) NOT NULL,  -- Small dimension for fast search
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (note_id, embedding_set_id, chunk_index)
);

-- HNSW index optimized for small vectors
CREATE INDEX idx_embedding_coarse_hnsw
ON embedding_coarse USING hnsw (vector vector_cosine_ops)
WITH (m = 32, ef_construction = 128);

-- Index by set for filtered searches
CREATE INDEX idx_embedding_coarse_set ON embedding_coarse(embedding_set_id);

-- ============================================================================
-- PART 7: NEW JOB TYPES
-- ============================================================================

-- Add new job types for entity extraction and fine-tuning
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'entity_extraction'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'entity_extraction';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'generate_fine_tuning_data'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'generate_fine_tuning_data';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'embed_for_set'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'embed_for_set';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'generate_graph_embedding'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'generate_graph_embedding';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'generate_coarse_embedding'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'generate_coarse_embedding';
    END IF;
END$$;

-- ============================================================================
-- PART 8: SEED MRL-AWARE EMBEDDING CONFIGS
-- ============================================================================

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

-- ============================================================================
-- PART 9: UPDATE VIEW TO INCLUDE NEW FIELDS
-- ============================================================================

-- Drop and recreate the summary view to include new fields
DROP VIEW IF EXISTS embedding_set_summary;

CREATE VIEW embedding_set_summary AS
SELECT
    es.id,
    es.name,
    es.slug,
    es.description,
    es.purpose,
    es.usage_hints,
    es.keywords,
    es.mode::text as mode,
    es.set_type::text as set_type,
    es.document_count,
    es.embedding_count,
    es.index_status::text as index_status,
    es.is_system,
    es.is_active,
    es.index_size_bytes,
    es.last_indexed_at,
    es.agent_metadata,
    es.criteria,
    es.auto_embed_rules,
    es.truncate_dim,
    es.embeddings_current,
    ec.model,
    ec.dimension,
    ec.supports_mrl,
    ec.matryoshka_dims,
    es.created_at,
    es.updated_at
FROM embedding_set es
LEFT JOIN embedding_config ec ON es.embedding_config_id = ec.id
WHERE es.is_active = TRUE
ORDER BY es.is_system DESC, es.document_count DESC;

-- ============================================================================
-- PART 10: HELPER FUNCTIONS
-- ============================================================================

-- Validate MRL truncation dimension
CREATE OR REPLACE FUNCTION validate_mrl_truncation(
    config_id UUID,
    requested_dim INTEGER
) RETURNS BOOLEAN AS $$
DECLARE
    config_rec RECORD;
BEGIN
    SELECT supports_mrl, matryoshka_dims INTO config_rec
    FROM embedding_config WHERE id = config_id;

    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    IF NOT config_rec.supports_mrl THEN
        RETURN FALSE;
    END IF;

    IF config_rec.matryoshka_dims IS NULL THEN
        RETURN FALSE;
    END IF;

    RETURN requested_dim = ANY(config_rec.matryoshka_dims);
END;
$$ LANGUAGE plpgsql STABLE;

-- Update entity IDF scores
CREATE OR REPLACE FUNCTION update_entity_idf_scores() RETURNS VOID AS $$
DECLARE
    total_docs INTEGER;
BEGIN
    SELECT COUNT(DISTINCT note_id) INTO total_docs FROM note_entity;

    IF total_docs > 0 THEN
        UPDATE entity_stats es SET
            idf_score = ln(total_docs::float / GREATEST(es.doc_frequency, 1)),
            last_updated = NOW();
    END IF;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TYPE embedding_set_type IS 'Filter: shares embeddings from default. Full: has own embeddings';
COMMENT ON COLUMN embedding_set.set_type IS 'Whether this set stores its own embeddings (full) or filters shared ones (filter)';
COMMENT ON COLUMN embedding_set.auto_embed_rules IS 'JSON rules for automatic embedding: on_create, on_update, priority, rate_limit';
COMMENT ON COLUMN embedding_set.truncate_dim IS 'MRL truncation dimension (NULL = use full dimension from config)';
COMMENT ON COLUMN embedding_config.supports_mrl IS 'Whether model supports Matryoshka dimension truncation';
COMMENT ON COLUMN embedding_config.matryoshka_dims IS 'Valid truncation dimensions for MRL models (descending)';
COMMENT ON COLUMN embedding_config.default_truncate_dim IS 'Default dimension when MRL truncation enabled';
COMMENT ON TABLE note_entity IS 'Named entities extracted from notes for tri-modal search';
COMMENT ON TABLE entity_stats IS 'Corpus-level entity frequency for IDF weighting';
COMMENT ON TABLE note_graph_embedding IS 'Graph-based embeddings aggregating entity information';
COMMENT ON TABLE fine_tuning_dataset IS 'Configuration for synthetic training data generation';
COMMENT ON TABLE fine_tuning_sample IS 'Query-document pairs for embedding model fine-tuning';
COMMENT ON TABLE embedding_coarse IS 'Small-dimension embeddings for fast two-stage retrieval';

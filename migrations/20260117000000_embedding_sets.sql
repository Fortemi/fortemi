-- ============================================================================
-- Embedding Sets Migration
-- ============================================================================
-- Adds support for multiple embedding sets with different configurations,
-- enabling tiered semantic search with focused embedding collections.
--
-- Default behavior is unchanged - a "default" set contains all notes.
-- Power users can create focused sets for specific use cases.
-- ============================================================================

-- ============================================================================
-- ENUMS
-- ============================================================================

-- Membership mode for embedding sets
CREATE TYPE embedding_set_mode AS ENUM (
    'auto',      -- Automatically include notes matching criteria
    'manual',    -- Only explicitly added notes
    'mixed'      -- Auto criteria + manual additions/exclusions
);

-- Index build status
CREATE TYPE embedding_index_status AS ENUM (
    'pending',   -- Needs initial build
    'building',  -- Currently building
    'ready',     -- Index is current
    'stale',     -- Index needs rebuild (new members)
    'disabled'   -- No index (for very small sets)
);

-- Add new job types for embedding set management
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'create_embedding_set'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'create_embedding_set';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'refresh_embedding_set'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'refresh_embedding_set';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'build_set_index'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'build_set_index';
    END IF;
END$$;

-- ============================================================================
-- TABLES
-- ============================================================================

-- Embedding configuration profiles
CREATE TABLE embedding_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT,

    -- Model settings
    model TEXT NOT NULL DEFAULT 'nomic-embed-text',
    dimension INTEGER NOT NULL DEFAULT 768,

    -- Chunking settings
    chunk_size INTEGER NOT NULL DEFAULT 1500,
    chunk_overlap INTEGER NOT NULL DEFAULT 200,

    -- Index settings
    hnsw_m INTEGER DEFAULT 16,
    hnsw_ef_construction INTEGER DEFAULT 64,
    ivfflat_lists INTEGER DEFAULT 100,

    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- An embedding set groups documents for semantic search
CREATE TABLE embedding_set (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Identity
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,

    -- Agent-friendly metadata
    description TEXT,
    purpose TEXT,
    usage_hints TEXT,
    keywords TEXT[] DEFAULT '{}',

    -- Membership
    mode embedding_set_mode NOT NULL DEFAULT 'auto',

    -- Auto-membership criteria (JSON for flexibility)
    criteria JSONB DEFAULT '{}',

    -- Embedding configuration reference
    embedding_config_id UUID REFERENCES embedding_config(id) ON DELETE SET NULL,

    -- Index management
    index_status embedding_index_status NOT NULL DEFAULT 'pending',
    index_type TEXT DEFAULT 'hnsw',
    last_indexed_at TIMESTAMPTZ,

    -- Stats (denormalized for quick access)
    document_count INTEGER DEFAULT 0,
    embedding_count INTEGER DEFAULT 0,
    index_size_bytes BIGINT DEFAULT 0,

    -- Lifecycle
    is_system BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    auto_refresh BOOLEAN DEFAULT TRUE,
    refresh_interval INTERVAL DEFAULT '1 day',
    last_refresh_at TIMESTAMPTZ,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT,

    -- Agent notes (structured)
    agent_metadata JSONB DEFAULT '{}'
);

-- Set membership (which notes are in which sets)
CREATE TABLE embedding_set_member (
    embedding_set_id UUID REFERENCES embedding_set(id) ON DELETE CASCADE,
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,

    -- How this note joined the set
    membership_type TEXT DEFAULT 'auto',
    added_at TIMESTAMPTZ DEFAULT NOW(),
    added_by TEXT,

    PRIMARY KEY (embedding_set_id, note_id)
);

-- ============================================================================
-- EXTEND EXISTING EMBEDDING TABLE
-- ============================================================================

-- Add set reference to existing embedding table
ALTER TABLE embedding
    ADD COLUMN IF NOT EXISTS embedding_set_id UUID REFERENCES embedding_set(id) ON DELETE CASCADE;

-- ============================================================================
-- INDICES
-- ============================================================================

-- Embedding set indices
CREATE INDEX idx_embedding_set_slug ON embedding_set(slug);
CREATE INDEX idx_embedding_set_active ON embedding_set(is_active) WHERE is_active = TRUE;
CREATE INDEX idx_embedding_set_system ON embedding_set(is_system);

-- Membership indices
CREATE INDEX idx_embedding_set_member_set ON embedding_set_member(embedding_set_id);
CREATE INDEX idx_embedding_set_member_note ON embedding_set_member(note_id);

-- Embedding set_id index
CREATE INDEX idx_embedding_set_id ON embedding(embedding_set_id);

-- Config indices
CREATE INDEX idx_embedding_config_default ON embedding_config(is_default) WHERE is_default = TRUE;

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Get the active default embedding set ID
CREATE OR REPLACE FUNCTION get_default_embedding_set_id()
RETURNS UUID AS $$
    SELECT id FROM embedding_set
    WHERE slug = 'default' AND is_active = TRUE
    LIMIT 1;
$$ LANGUAGE SQL STABLE;

-- Get the default embedding config ID
CREATE OR REPLACE FUNCTION get_default_embedding_config_id()
RETURNS UUID AS $$
    SELECT id FROM embedding_config
    WHERE is_default = TRUE
    LIMIT 1;
$$ LANGUAGE SQL STABLE;

-- Update set stats (call after membership changes)
CREATE OR REPLACE FUNCTION update_embedding_set_stats(set_id UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE embedding_set SET
        document_count = (
            SELECT COUNT(DISTINCT note_id)
            FROM embedding_set_member
            WHERE embedding_set_id = set_id
        ),
        embedding_count = (
            SELECT COUNT(*)
            FROM embedding
            WHERE embedding_set_id = set_id
        ),
        updated_at = NOW()
    WHERE id = set_id;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update set stats on member changes
CREATE OR REPLACE FUNCTION trigger_update_set_stats()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM update_embedding_set_stats(OLD.embedding_set_id);
        RETURN OLD;
    ELSE
        PERFORM update_embedding_set_stats(NEW.embedding_set_id);
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER embedding_set_member_stats_trigger
AFTER INSERT OR UPDATE OR DELETE ON embedding_set_member
FOR EACH ROW EXECUTE FUNCTION trigger_update_set_stats();

-- Trigger to update set stats on embedding changes
CREATE OR REPLACE FUNCTION trigger_update_embedding_set_stats()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' AND OLD.embedding_set_id IS NOT NULL THEN
        PERFORM update_embedding_set_stats(OLD.embedding_set_id);
        RETURN OLD;
    ELSIF NEW.embedding_set_id IS NOT NULL THEN
        PERFORM update_embedding_set_stats(NEW.embedding_set_id);
        RETURN NEW;
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER embedding_stats_trigger
AFTER INSERT OR UPDATE OR DELETE ON embedding
FOR EACH ROW EXECUTE FUNCTION trigger_update_embedding_set_stats();

-- ============================================================================
-- VIEWS
-- ============================================================================

-- Embedding sets with stats for discovery
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
    es.document_count,
    es.embedding_count,
    es.index_status::text as index_status,
    es.is_system,
    es.is_active,
    es.index_size_bytes,
    es.last_indexed_at,
    es.agent_metadata,
    es.criteria,
    ec.model,
    ec.dimension,
    es.created_at,
    es.updated_at
FROM embedding_set es
LEFT JOIN embedding_config ec ON es.embedding_config_id = ec.id
WHERE es.is_active = TRUE
ORDER BY es.is_system DESC, es.document_count DESC;

-- ============================================================================
-- DEFAULT DATA SETUP
-- ============================================================================

-- Seed data moved to 20260117000000_seed_default_embedding_set.sql

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE embedding_config IS 'Embedding configuration profiles with model and chunking settings';
COMMENT ON TABLE embedding_set IS 'Named embedding sets for focused semantic search';
COMMENT ON TABLE embedding_set_member IS 'Many-to-many relationship between embedding sets and notes';
COMMENT ON COLUMN embedding_set.criteria IS 'JSON criteria for auto-membership: include_all, tags, collections, fts_query, created_after, exclude_archived';
COMMENT ON COLUMN embedding_set.agent_metadata IS 'Agent-provided metadata for discovery: created_by_agent, rationale, performance_notes, suggested_queries';

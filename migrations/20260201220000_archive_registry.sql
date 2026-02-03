-- Archive registry for parallel memory archives
-- Part of Epic #441: Parallel Memory Archives
-- This table manages isolated data namespaces using PostgreSQL schemas

-- Archive registry table
CREATE TABLE IF NOT EXISTS archive_registry (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    schema_name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed TIMESTAMPTZ,
    note_count INTEGER DEFAULT 0,
    size_bytes BIGINT DEFAULT 0,
    is_default BOOLEAN DEFAULT FALSE
);

-- Index for lookups by name
CREATE INDEX IF NOT EXISTS idx_archive_registry_name ON archive_registry(name);

-- Only one default archive is allowed
CREATE UNIQUE INDEX IF NOT EXISTS idx_archive_registry_default
    ON archive_registry(is_default) WHERE is_default = TRUE;

-- Index for statistics queries
CREATE INDEX IF NOT EXISTS idx_archive_registry_last_accessed
    ON archive_registry(last_accessed) WHERE last_accessed IS NOT NULL;

-- Comments for documentation
COMMENT ON TABLE archive_registry IS 'Registry of archive schemas for parallel memory namespaces (Epic #441)';
COMMENT ON COLUMN archive_registry.name IS 'Human-readable name for the archive';
COMMENT ON COLUMN archive_registry.schema_name IS 'PostgreSQL schema name (must be valid identifier)';
COMMENT ON COLUMN archive_registry.note_count IS 'Cached count of notes in this archive';
COMMENT ON COLUMN archive_registry.size_bytes IS 'Estimated storage size in bytes';
COMMENT ON COLUMN archive_registry.is_default IS 'Whether this is the default archive (only one allowed)';

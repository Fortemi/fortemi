-- ============================================================================
-- Temporal & Positional Document Types
-- Issue: #431 - Subject-matter temporal search
-- ============================================================================
--
-- Adds document types for content with inherent temporal or spatial properties:
-- - Events, meetings, deadlines with start/end times
-- - Location notes, travel logs with GPS coordinates
-- - Combined temporal-spatial (itineraries, conference sessions)
-- ============================================================================

-- ============================================================================
-- PART 1: Add temporal_metadata and positional_metadata columns to document_type
-- ============================================================================

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS temporal_metadata JSONB DEFAULT '{}';
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS positional_metadata JSONB DEFAULT '{}';

COMMENT ON COLUMN document_type.temporal_metadata IS
'Schema for temporal fields: {"primary_field": "starts_at", "fields": [{"name": "starts_at", "type": "datetime", "required": true}]}';
COMMENT ON COLUMN document_type.positional_metadata IS
'Schema for positional fields: {"fields": [{"name": "latitude", "type": "float"}, {"name": "longitude", "type": "float"}]}';

-- Seed data moved to: 20260204400000_seed_temporal_positional_types.sql

-- ============================================================================
-- PART 2: Indexes for temporal queries on note.metadata
-- ============================================================================

-- NOTE: Temporal fields are stored as ISO 8601 text (e.g., "2026-01-15T09:00:00Z").
-- ISO 8601 strings sort lexicographically in chronological order, so we can use
-- text-based BTREE indexes for range queries. This avoids IMMUTABLE function issues
-- with timezone-dependent casts.

-- Index for temporal.starts_at queries (text comparison for ISO 8601)
CREATE INDEX IF NOT EXISTS idx_note_temporal_starts ON note
    USING BTREE ((metadata->'temporal'->>'starts_at'))
    WHERE metadata->'temporal'->>'starts_at' IS NOT NULL;

-- Index for temporal.ends_at queries
CREATE INDEX IF NOT EXISTS idx_note_temporal_ends ON note
    USING BTREE ((metadata->'temporal'->>'ends_at'))
    WHERE metadata->'temporal'->>'ends_at' IS NOT NULL;

-- Index for temporal.due_at (deadlines)
CREATE INDEX IF NOT EXISTS idx_note_temporal_due ON note
    USING BTREE ((metadata->'temporal'->>'due_at'))
    WHERE metadata->'temporal'->>'due_at' IS NOT NULL;

-- GIN index for complex temporal/positional queries
CREATE INDEX IF NOT EXISTS idx_note_temporal_gin ON note
    USING GIN ((metadata->'temporal'))
    WHERE metadata->'temporal' IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_note_positional_gin ON note
    USING GIN ((metadata->'positional'))
    WHERE metadata->'positional' IS NOT NULL;

COMMENT ON INDEX idx_note_temporal_starts IS 'Index for filtering notes by subject-matter start time (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_ends IS 'Index for filtering notes by subject-matter end time (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_due IS 'Index for filtering notes by subject-matter due date (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_gin IS 'GIN index for complex temporal metadata queries';
COMMENT ON INDEX idx_note_positional_gin IS 'GIN index for complex positional metadata queries';

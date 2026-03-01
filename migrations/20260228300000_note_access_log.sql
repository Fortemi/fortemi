-- Note Access Log for frequency analytics (Issue #562)
-- Records individual access events for pattern analysis, cold/hot spot detection,
-- and novelty-weighted retrieval.

-- Access type enum
DO $$ BEGIN
    CREATE TYPE note_access_type AS ENUM (
        'direct_get',        -- GET /api/v1/notes/{id}
        'search_result',     -- Appeared in search results
        'graph_traversal',   -- Reached via link/graph exploration
        'related_notes',     -- Fetched as a related note
        'mcp_tool'           -- Accessed via MCP tool
    );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- Access log table (per-memory-archive, lives in schema)
CREATE TABLE IF NOT EXISTS note_access_log (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    access_type note_access_type NOT NULL DEFAULT 'direct_get',
    source TEXT              -- e.g., "api", "mcp", "hotm", "web"
);

-- Index for per-note access queries (most recent first)
CREATE INDEX idx_note_access_log_note_id ON note_access_log(note_id, accessed_at DESC);

-- Index for time-range aggregate queries
CREATE INDEX idx_note_access_log_accessed_at ON note_access_log(accessed_at DESC);

-- View for aggregate access statistics
CREATE OR REPLACE VIEW note_access_stats AS
SELECT
    note_id,
    COUNT(*) AS total_accesses,
    COUNT(*) FILTER (WHERE accessed_at > NOW() - INTERVAL '7 days') AS recent_7d,
    COUNT(*) FILTER (WHERE accessed_at > NOW() - INTERVAL '30 days') AS recent_30d,
    MAX(accessed_at) AS last_accessed,
    MIN(accessed_at) AS first_accessed
FROM note_access_log
GROUP BY note_id;

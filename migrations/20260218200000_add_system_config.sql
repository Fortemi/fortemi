-- System configuration key-value store for persisting runtime state.
-- Used by the job worker to persist pause state across container restarts.
-- Issue #466: pause/resume job processing globally and per-archive.

CREATE TABLE IF NOT EXISTS system_config (
    key   TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

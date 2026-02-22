-- Add tus resumable upload tracking table (#528)
--
-- Tracks in-progress tus protocol uploads. Each row represents a single
-- upload session with a staging file on disk. When the final chunk arrives,
-- the file is finalized into the attachment pipeline and the row is deleted.
-- Expired rows are cleaned up by a periodic maintenance query.

CREATE TABLE IF NOT EXISTS tus_upload (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    note_id         UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL DEFAULT 'application/octet-stream',
    total_size      BIGINT NOT NULL,
    current_offset  BIGINT NOT NULL DEFAULT 0,
    storage_path    TEXT NOT NULL,
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL
);

-- Index for cleanup queries (only incomplete uploads can expire)
CREATE INDEX idx_tus_upload_expires ON tus_upload (expires_at);

-- Index for note lookup (cascading deletes, listing uploads per note)
CREATE INDEX idx_tus_upload_note ON tus_upload (note_id);

-- Auto-update updated_at on modification
CREATE OR REPLACE FUNCTION tus_upload_updated() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tus_upload_updated
    BEFORE UPDATE ON tus_upload
    FOR EACH ROW
    EXECUTE FUNCTION tus_upload_updated();

-- Cleanup function: deletes expired uploads, returns count of deleted rows.
-- Called periodically by the API server (e.g., on each new upload creation).
-- The caller is responsible for deleting the staging files from disk.
CREATE OR REPLACE FUNCTION cleanup_expired_tus_uploads()
RETURNS TABLE(id UUID, storage_path TEXT) AS $$
    DELETE FROM tus_upload
    WHERE expires_at < NOW()
    RETURNING id, storage_path;
$$ LANGUAGE sql;

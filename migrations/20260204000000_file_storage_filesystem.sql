-- ============================================================================
-- File Storage Filesystem Enhancement Migration
-- Issue: #432 - UUIDv7-Based File Storage Architecture
-- ============================================================================
-- This migration enhances the attachment_blob table to support filesystem-based
-- storage using UUIDv7 blob IDs for deterministic, time-ordered paths.
--
-- Storage Backends:
-- 1. 'database' - Inline BYTEA storage (existing, files <10MB)
-- 2. 'filesystem' - Local filesystem with UUIDv7 paths (NEW)
-- 3. 's3' - Object storage (future)
--
-- Path Structure (filesystem backend):
--   blobs/{first-2-hex}/{next-2-hex}/{uuid}.bin
--   Example: blobs/01/94/01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f.bin
--
-- Benefits:
-- - UUIDv7 provides time-ordered directory structure
-- - First 4 hex chars distribute files across 65,536 subdirectories
-- - No database size limits (PG TOAST max 1GB per row)
-- - Faster backups (filesystem snapshots)
-- - Content verification via checksums
-- ============================================================================

-- ============================================================================
-- PART 1: ALTER ATTACHMENT_BLOB TABLE
-- ============================================================================

-- Add filesystem storage path
ALTER TABLE attachment_blob
    ADD COLUMN IF NOT EXISTS storage_path TEXT;

-- Rename storage_type to storage_backend for clarity
ALTER TABLE attachment_blob
    ADD COLUMN IF NOT EXISTS storage_backend TEXT DEFAULT 'database';

-- Migrate existing storage_type values to storage_backend
UPDATE attachment_blob
SET storage_backend = storage_type
WHERE storage_backend = 'database';

-- Add verification tracking for data integrity
ALTER TABLE attachment_blob
    ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ;

ALTER TABLE attachment_blob
    ADD COLUMN IF NOT EXISTS verification_status TEXT;

-- ============================================================================
-- PART 2: STORAGE PATH GENERATION FUNCTION
-- ============================================================================

-- Function to generate storage path from UUIDv7
-- Path format: blobs/{first 2 hex}/{next 2 hex}/{uuid}.bin
-- Example: blobs/01/94/01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f.bin
CREATE OR REPLACE FUNCTION generate_blob_storage_path(blob_id UUID)
RETURNS TEXT AS $$
BEGIN
    RETURN 'blobs/' ||
           substring(replace(blob_id::text, '-', ''), 1, 2) || '/' ||
           substring(replace(blob_id::text, '-', ''), 3, 2) || '/' ||
           blob_id::text || '.bin';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION generate_blob_storage_path IS
    'Generates deterministic filesystem path from UUIDv7 blob ID';

-- ============================================================================
-- PART 3: AUTO-POPULATE STORAGE PATH TRIGGER
-- ============================================================================

-- Update storage_path on insert for filesystem storage
CREATE OR REPLACE FUNCTION set_blob_storage_path()
RETURNS TRIGGER AS $$
BEGIN
    -- Only set path if storage_backend is 'filesystem' and path is not manually provided
    IF NEW.storage_backend = 'filesystem' AND NEW.storage_path IS NULL THEN
        NEW.storage_path := generate_blob_storage_path(NEW.id);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER blob_storage_path_trigger
    BEFORE INSERT ON attachment_blob
    FOR EACH ROW
    EXECUTE FUNCTION set_blob_storage_path();

COMMENT ON TRIGGER blob_storage_path_trigger ON attachment_blob IS
    'Auto-generates storage_path for filesystem backend on insert';

-- ============================================================================
-- PART 4: INDEXES AND CONSTRAINTS
-- ============================================================================

-- Index for filesystem storage queries
CREATE INDEX IF NOT EXISTS idx_attachment_blob_storage_backend
    ON attachment_blob(storage_backend);

-- Index for unverified blobs (data integrity checks)
CREATE INDEX IF NOT EXISTS idx_attachment_blob_unverified
    ON attachment_blob(verification_status)
    WHERE verification_status IS NULL OR verification_status = 'failed';

-- Partial index for filesystem blobs
CREATE INDEX IF NOT EXISTS idx_attachment_blob_filesystem
    ON attachment_blob(storage_path)
    WHERE storage_backend = 'filesystem';

-- ============================================================================
-- PART 5: HELPER FUNCTIONS
-- ============================================================================

-- Function to get filesystem directory path from blob ID (for batch operations)
CREATE OR REPLACE FUNCTION get_blob_directory(blob_id UUID)
RETURNS TEXT AS $$
BEGIN
    RETURN 'blobs/' ||
           substring(replace(blob_id::text, '-', ''), 1, 2) || '/' ||
           substring(replace(blob_id::text, '-', ''), 3, 2);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION get_blob_directory IS
    'Returns directory path for blob (e.g., blobs/01/94)';

-- Function to verify blob integrity (checksum match)
CREATE OR REPLACE FUNCTION verify_blob_integrity(
    p_blob_id UUID,
    p_filesystem_hash TEXT
)
RETURNS BOOLEAN AS $$
DECLARE
    v_db_hash TEXT;
BEGIN
    SELECT content_hash INTO v_db_hash
    FROM attachment_blob
    WHERE id = p_blob_id;

    IF v_db_hash IS NULL THEN
        RETURN FALSE;
    END IF;

    -- Compare hashes (case-insensitive)
    IF LOWER(v_db_hash) = LOWER(p_filesystem_hash) THEN
        -- Update verification status
        UPDATE attachment_blob
        SET verified_at = NOW(),
            verification_status = 'verified'
        WHERE id = p_blob_id;
        RETURN TRUE;
    ELSE
        -- Hash mismatch
        UPDATE attachment_blob
        SET verified_at = NOW(),
            verification_status = 'failed'
        WHERE id = p_blob_id;
        RETURN FALSE;
    END IF;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION verify_blob_integrity IS
    'Verifies blob integrity by comparing database hash with filesystem hash';

-- Function to migrate blob from database to filesystem
-- NOTE: This only updates metadata. Actual file write must be done externally.
CREATE OR REPLACE FUNCTION mark_blob_migrated_to_filesystem(
    p_blob_id UUID,
    p_storage_path TEXT DEFAULT NULL
)
RETURNS BOOLEAN AS $$
DECLARE
    v_storage_path TEXT;
BEGIN
    -- Use provided path or generate default
    v_storage_path := COALESCE(p_storage_path, generate_blob_storage_path(p_blob_id));

    -- Update storage backend and path
    UPDATE attachment_blob
    SET storage_backend = 'filesystem',
        storage_path = v_storage_path,
        verification_status = 'pending'
    WHERE id = p_blob_id
      AND storage_backend = 'database';

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION mark_blob_migrated_to_filesystem IS
    'Marks blob as migrated to filesystem storage (metadata only, file write done externally)';

-- ============================================================================
-- PART 6: STATISTICS VIEW
-- ============================================================================

CREATE OR REPLACE VIEW storage_backend_stats AS
SELECT
    storage_backend,
    COUNT(*) as blob_count,
    SUM(size_bytes) as total_bytes,
    pg_size_pretty(SUM(size_bytes)) as total_size,
    AVG(size_bytes) as avg_size_bytes,
    pg_size_pretty(AVG(size_bytes)::bigint) as avg_size,
    MIN(created_at) as oldest_blob,
    MAX(created_at) as newest_blob
FROM attachment_blob
GROUP BY storage_backend
ORDER BY total_bytes DESC;

COMMENT ON VIEW storage_backend_stats IS
    'Storage backend usage statistics by blob count and size';

-- ============================================================================
-- PART 7: COMMENTS
-- ============================================================================

COMMENT ON COLUMN attachment_blob.storage_path IS
    'Filesystem path for blob (e.g., blobs/01/94/{uuid}.bin) - auto-generated from UUIDv7';

COMMENT ON COLUMN attachment_blob.storage_backend IS
    'Storage backend: database (inline BYTEA), filesystem (local disk), or s3 (object storage)';

COMMENT ON COLUMN attachment_blob.verified_at IS
    'Timestamp of last content verification (checksum match)';

COMMENT ON COLUMN attachment_blob.verification_status IS
    'Verification status: pending, verified, failed';

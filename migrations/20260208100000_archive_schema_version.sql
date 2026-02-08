-- Add schema version tracking to archive_registry.
-- Allows detecting when an archive's schema is outdated compared to public,
-- triggering auto-migration of missing tables on next access.

ALTER TABLE archive_registry
    ADD COLUMN IF NOT EXISTS schema_version INTEGER NOT NULL DEFAULT 0;

-- Update all existing archives to version 0 (forces migration check on next access).
-- After migration completes, the version is bumped to match the current table count.

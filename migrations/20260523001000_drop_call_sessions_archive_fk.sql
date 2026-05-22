-- Keep realtime call session metadata provider/archive-associated without
-- participating in archive_registry delete/drop lock cycles.

ALTER TABLE call_sessions
    DROP CONSTRAINT IF EXISTS call_sessions_archive_id_fkey;

-- Fix job_queue for multi-memory archives (Issues #413, #414)
--
-- Problem: job_queue.note_id has FK to public.note(id), but notes in
-- non-default archives exist in archive_X.note. INSERT fails with FK violation,
-- silently preventing any AI pipeline jobs from being queued.
--
-- Solution: Drop the FK constraint so job_queue can reference notes in any schema.
-- Schema context is already passed via the payload JSON field.

ALTER TABLE job_queue DROP CONSTRAINT IF EXISTS job_queue_note_id_fkey;

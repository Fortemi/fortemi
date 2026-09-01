-- Database authority for idempotent note-to-note links (GitHub #62).
--
-- Create the authority index in public and every existing archive schema.
-- New archives inherit it through LIKE public.link INCLUDING ALL. The index
-- build itself is the complete-row duplicate audit and is not weakened by
-- forced tenant RLS. Duplicate values are collapsed to a constant error so
-- migration output never prints note identifiers.
DO $link_identity$
DECLARE
    schema_row record;
BEGIN
    FOR schema_row IN
        SELECT DISTINCT n.nspname
        FROM pg_namespace n
        JOIN pg_class c ON c.relnamespace = n.oid
        WHERE c.relname = 'link'
          AND c.relkind IN ('r', 'p')
          AND n.nspname NOT LIKE 'pg_%'
          AND n.nspname <> 'information_schema'
    LOOP
        BEGIN
            EXECUTE format(
                'CREATE UNIQUE INDEX IF NOT EXISTS ux_link_note_identity ON %I.link (from_note_id, to_note_id, kind) WHERE to_note_id IS NOT NULL',
                schema_row.nspname
            );
            EXECUTE format(
                'COMMENT ON INDEX %I.ux_link_note_identity IS %L',
                schema_row.nspname,
                'Canonical note-link identity; URL-only links are outside the manual-link-v1 contract.'
            );
        EXCEPTION WHEN unique_violation THEN
            RAISE EXCEPTION USING
                ERRCODE = '23505',
                MESSAGE = 'note-link identity contains duplicate rows; migration refused';
        END;
    END LOOP;
END
$link_identity$;

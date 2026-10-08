-- Fortemi #1157 follow-up: take tombstone maintenance off the insert path.
--
-- 20261008000000 cleared a key's tombstone on every INSERT with a dynamic
-- DELETE that PL/pgSQL re-plans per row. That roughly doubled insert cost on
-- the six exported tables and pushed AL-PERF01 archive import throughput under
-- its approved floor on a loaded CI runner.
--
-- Tombstones are now written only when a key disappears (DELETE, or an UPDATE
-- that changes the key) and are never cleared on insert. The export query
-- emits a tombstone only while no live row holds its key, so a tombstone left
-- behind by a later re-insert is ignored; the re-inserted row itself carries a
-- newer stamp and reaches the consumer as an upsert.
--
-- Both trigger functions also skip all work when an UPDATE leaves the row
-- byte-for-byte unchanged (for example a shard re-import of identical rows),
-- and serialize each row at most once.

CREATE OR REPLACE FUNCTION public.memory_export_stamp_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- TG_ARGV lists columns that never reach an export (access counters,
    -- generated search vectors, the stamp itself). Updates that only touch
    -- those columns keep the previous stamp, so reads do not churn exports.
    IF TG_OP = 'UPDATE'
       AND (NEW IS NOT DISTINCT FROM OLD
            OR (to_jsonb(NEW) - TG_ARGV) = (to_jsonb(OLD) - TG_ARGV)) THEN
        NEW.export_change_xid := OLD.export_change_xid;
        RETURN NEW;
    END IF;
    NEW.export_change_xid := pg_current_xact_id();
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.memory_export_track_key()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    -- TG_ARGV[0] is the export entity type; TG_ARGV[1..] are key columns.
    key_column TEXT;
    old_row JSONB;
    new_row JSONB;
    old_key JSONB := '[]'::jsonb;
    new_key JSONB := '[]'::jsonb;
BEGIN
    IF TG_OP = 'INSERT' THEN
        RETURN NULL;
    END IF;
    IF TG_OP = 'UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN
        RETURN NULL;
    END IF;

    old_row := to_jsonb(OLD);
    FOREACH key_column IN ARRAY TG_ARGV[1:] LOOP
        old_key := old_key || jsonb_build_array(old_row -> key_column);
    END LOOP;

    IF TG_OP = 'UPDATE' THEN
        new_row := to_jsonb(NEW);
        FOREACH key_column IN ARRAY TG_ARGV[1:] LOOP
            new_key := new_key || jsonb_build_array(new_row -> key_column);
        END LOOP;
        IF old_key = new_key AND OLD.tenant_id = NEW.tenant_id THEN
            RETURN NULL;
        END IF;
    END IF;

    -- Tombstones live beside the changed row (TG_TABLE_SCHEMA), never wherever
    -- the session search_path happens to point.
    EXECUTE format(
        'INSERT INTO %I.export_tombstone (tenant_id, entity_type, entity_key, change_xid) '
        'VALUES ($1, $2, $3, pg_current_xact_id()) '
        'ON CONFLICT (tenant_id, entity_type, entity_key) '
        'DO UPDATE SET change_xid = EXCLUDED.change_xid',
        TG_TABLE_SCHEMA
    ) USING OLD.tenant_id, TG_ARGV[0], old_key;
    RETURN NULL;
END;
$$;

COMMENT ON TABLE export_tombstone IS
    'Content-free hard-delete markers for incremental memory export (Fortemi #1157, ADR-109). '
    'A marker whose key is live again is stale and ignored by export.';

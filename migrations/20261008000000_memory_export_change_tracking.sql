-- Fortemi #1157: point-in-time memory export with a transaction-safe
-- high-water mark (ADR-109).
--
-- Every exported per-memory row carries the 64-bit transaction id (xid8) of
-- the transaction that last changed an exported column. Hard deletes and key
-- changes leave a per-memory tombstone stamped the same way; re-inserting a key
-- clears its tombstone in the same transaction, so a tombstone exists exactly
-- when the key is absent.
--
-- A reader derives its high-water mark from the snapshot it read with:
-- every transaction older than pg_snapshot_xmin() has finished, so no later
-- commit can introduce a change stamped below that bound. Existing rows are
-- stamped '0' (older than any mark) without rewriting the tables.

CREATE OR REPLACE FUNCTION public.memory_export_stamp_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- TG_ARGV lists columns that never reach an export (access counters,
    -- generated search vectors, the stamp itself). Updates that only touch
    -- those columns keep the previous stamp, so reads do not churn exports.
    IF TG_OP = 'UPDATE'
       AND (to_jsonb(NEW) - TG_ARGV) = (to_jsonb(OLD) - TG_ARGV) THEN
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
    old_key JSONB := '[]'::jsonb;
    new_key JSONB := '[]'::jsonb;
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        FOREACH key_column IN ARRAY TG_ARGV[1:] LOOP
            old_key := old_key || jsonb_build_array(to_jsonb(OLD) -> key_column);
        END LOOP;
    END IF;
    IF TG_OP IN ('UPDATE', 'INSERT') THEN
        FOREACH key_column IN ARRAY TG_ARGV[1:] LOOP
            new_key := new_key || jsonb_build_array(to_jsonb(NEW) -> key_column);
        END LOOP;
    END IF;

    IF TG_OP = 'UPDATE' AND old_key = new_key AND OLD.tenant_id = NEW.tenant_id THEN
        RETURN NULL;
    END IF;

    -- Tombstones live beside the changed row (TG_TABLE_SCHEMA), never wherever
    -- the session search_path happens to point.
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        EXECUTE format(
            'INSERT INTO %I.export_tombstone (tenant_id, entity_type, entity_key, change_xid) '
            'VALUES ($1, $2, $3, pg_current_xact_id()) '
            'ON CONFLICT (tenant_id, entity_type, entity_key) '
            'DO UPDATE SET change_xid = EXCLUDED.change_xid',
            TG_TABLE_SCHEMA
        ) USING OLD.tenant_id, TG_ARGV[0], old_key;
    END IF;
    IF TG_OP IN ('UPDATE', 'INSERT') THEN
        EXECUTE format(
            'DELETE FROM %I.export_tombstone '
            'WHERE tenant_id = $1 AND entity_type = $2 AND entity_key = $3',
            TG_TABLE_SCHEMA
        ) USING NEW.tenant_id, TG_ARGV[0], new_key;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TABLE export_tombstone (
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    entity_type TEXT NOT NULL CHECK (entity_type IN (
        'note', 'note_original', 'note_revised_current', 'note_tag', 'link', 'collection'
    )),
    entity_key JSONB NOT NULL CHECK (jsonb_typeof(entity_key) = 'array'),
    change_xid XID8 NOT NULL,
    PRIMARY KEY (tenant_id, entity_type, entity_key),
    -- Tombstones are derived markers with no meaning outside their tenant, so
    -- they leave with the tenant instead of blocking its removal.
    CONSTRAINT export_tombstone_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE CASCADE
);

CREATE INDEX export_tombstone_change ON export_tombstone (tenant_id, change_xid);

COMMENT ON TABLE export_tombstone IS
    'Content-free hard-delete markers for incremental memory export (Fortemi #1157, ADR-109).';

-- Install the stamp column, index, and triggers on one schema. Runs for public
-- and every registered archive; archives created later clone the triggers.
CREATE OR REPLACE FUNCTION pg_temp.memory_export_install(target_schema TEXT)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    spec RECORD;
BEGIN
    FOR spec IN
        SELECT *
          FROM (VALUES
            ('note', 'note', ARRAY['id'],
             ARRAY['export_change_xid', 'access_count', 'last_accessed_at']),
            ('note_original', 'note_original', ARRAY['note_id'],
             ARRAY['export_change_xid']),
            ('note_revised_current', 'note_revised_current', ARRAY['note_id'],
             ARRAY['export_change_xid', 'tsv']),
            ('note_tag', 'note_tag', ARRAY['note_id', 'tag_name'],
             ARRAY['export_change_xid']),
            ('link', 'link', ARRAY['id'],
             ARRAY['export_change_xid']),
            ('collection', 'collection', ARRAY['id'],
             ARRAY['export_change_xid', 'shard_note_count'])
          ) AS v(table_name, entity_type, key_columns, ignored_columns)
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS export_change_xid XID8 NOT NULL DEFAULT ''0''',
            target_schema, spec.table_name
        );
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS %I ON %I.%I (tenant_id, export_change_xid)',
            'idx_' || spec.table_name || '_export_change', target_schema, spec.table_name
        );
        -- "zz_" sorts after existing BEFORE triggers so the comparison sees the
        -- final NEW row (e.g. after update_original_edited adjusts timestamps).
        EXECUTE format(
            'CREATE TRIGGER zz_memory_export_stamp BEFORE INSERT OR UPDATE ON %I.%I '
            'FOR EACH ROW EXECUTE FUNCTION public.memory_export_stamp_change(%s)',
            target_schema, spec.table_name,
            (SELECT string_agg(quote_literal(c), ', ') FROM unnest(spec.ignored_columns) c)
        );
        EXECUTE format(
            'CREATE TRIGGER zz_memory_export_key AFTER INSERT OR UPDATE OR DELETE ON %I.%I '
            'FOR EACH ROW EXECUTE FUNCTION public.memory_export_track_key(%s)',
            target_schema, spec.table_name,
            (SELECT string_agg(quote_literal(c), ', ')
               FROM unnest(ARRAY[spec.entity_type] || spec.key_columns) c)
        );
    END LOOP;
END;
$$;

SELECT pg_temp.memory_export_install('public');

ALTER TABLE public.export_tombstone ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.export_tombstone FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.export_tombstone
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

DO $memory_export_archives$
DECLARE
    archive_row RECORD;
BEGIN
    FOR archive_row IN
        SELECT schema_name
          FROM archive_registry
         WHERE schema_name <> 'public'
         ORDER BY schema_name
    LOOP
        IF archive_row.schema_name !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;
        IF to_regclass(format('%I.note', archive_row.schema_name)) IS NULL THEN
            CONTINUE;
        END IF;

        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I.export_tombstone (LIKE public.export_tombstone INCLUDING ALL)',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.export_tombstone ADD CONSTRAINT export_tombstone_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name
        );
        EXECUTE format('ALTER TABLE %I.export_tombstone ENABLE ROW LEVEL SECURITY', archive_row.schema_name);
        EXECUTE format('ALTER TABLE %I.export_tombstone FORCE ROW LEVEL SECURITY', archive_row.schema_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.export_tombstone USING (tenant_id = current_setting(''app.current_tenant'')::uuid) WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)',
            archive_row.schema_name
        );

        PERFORM pg_temp.memory_export_install(archive_row.schema_name);
    END LOOP;
END
$memory_export_archives$;

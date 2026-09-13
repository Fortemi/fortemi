-- #1091: bounded, monotone index keys followed by exact query rechecks.
-- Keys never change author metadata or the portable record shape. Versioned
-- functions must not change semantics in place after these indexes are built.

CREATE FUNCTION public.metadata_search_order_key_v1(value jsonb)
RETURNS numeric LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
SET search_path = pg_catalog
AS $function$
    SELECT CASE jsonb_typeof(value)
        WHEN 'number' THEN trunc(greatest(-1e16::numeric,
            least(1e16::numeric, (value #>> '{}')::numeric)), 18)
        WHEN 'boolean' THEN CASE WHEN value = 'true'::jsonb THEN 1::numeric ELSE 0::numeric END
        ELSE NULL::numeric
    END
$function$;

CREATE FUNCTION public.metadata_search_text_key_v1(value jsonb)
RETURNS text LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
SET search_path = pg_catalog
AS $function$
    SELECT CASE WHEN jsonb_typeof(value) = 'string'
        THEN left(value #>> '{}', 256) ELSE NULL::text END
$function$;

DO $metadata_indexes$
DECLARE
    target_schema text;
    metadata_path text;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF target_schema <> 'public' AND target_schema !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;
        FOREACH metadata_path IN ARRAY ARRAY['provider', 'model', 'role', 'event_kind', 'sensitivity']
        LOOP
            EXECUTE format(
                'CREATE INDEX %I ON %I.note (
                    (jsonb_typeof(metadata -> %L)),
                    (public.metadata_search_order_key_v1(metadata -> %L)),
                    (public.metadata_search_text_key_v1(metadata -> %L) COLLATE "C")
                )',
                'idx_note_metadata_' || metadata_path || '_v1', target_schema,
                metadata_path, metadata_path, metadata_path
            );
        END LOOP;
        EXECUTE format(
            'CREATE INDEX idx_source_identity_metadata_run_v1 ON %I.source_identity
                (tenant_id, import_run_id COLLATE "C", note_id)', target_schema
        );
        EXECUTE format(
            'CREATE INDEX idx_source_identity_metadata_note_v1 ON %I.source_identity
                (tenant_id, note_id)', target_schema
        );
    END LOOP;
END
$metadata_indexes$;

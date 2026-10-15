-- Default selection must remain tenant-qualified on owner/superuser pools too.
CREATE OR REPLACE FUNCTION public.get_default_embedding_set_id()
RETURNS UUID AS $$
    SELECT id FROM embedding_set
    WHERE tenant_id = current_setting('app.current_tenant')::uuid
      AND slug = 'default' AND is_active = TRUE LIMIT 1;
$$ LANGUAGE SQL STABLE;

CREATE OR REPLACE FUNCTION public.get_default_embedding_config_id()
RETURNS UUID AS $$
    SELECT id FROM public.embedding_config
    WHERE tenant_id = current_setting('app.current_tenant')::uuid
      AND is_default = TRUE ORDER BY created_at,id LIMIT 1;
$$ LANGUAGE SQL STABLE;

DO $archive_defaults$
DECLARE
    archive RECORD;
BEGIN
    FOR archive IN SELECT schema_name FROM public.archive_registry WHERE schema_name <> 'public'
    LOOP
        IF to_regclass(format('%I.embedding_set', archive.schema_name)) IS NOT NULL THEN
            EXECUTE format($function$
                CREATE OR REPLACE FUNCTION %I.get_default_embedding_set_id()
                RETURNS UUID AS $body$
                    SELECT id FROM %I.embedding_set
                    WHERE tenant_id = current_setting('app.current_tenant')::uuid
                      AND slug = 'default' AND is_active = TRUE LIMIT 1;
                $body$ LANGUAGE SQL STABLE
            $function$, archive.schema_name, archive.schema_name);
        END IF;
    END LOOP;
END
$archive_defaults$;

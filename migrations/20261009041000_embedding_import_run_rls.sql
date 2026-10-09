-- Fortemi #1177 R1 part E: tenant RLS for embedding import run journals.

ALTER TABLE public.embedding_import_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.embedding_import_run FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON public.embedding_import_run;
CREATE POLICY tenant_isolation ON public.embedding_import_run
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

DO $embedding_import_run_rls_archives$
DECLARE
    archive_row RECORD;
BEGIN
    FOR archive_row IN
        SELECT table_schema AS schema_name
          FROM information_schema.tables
         WHERE table_name = 'embedding_import_run'
           AND table_schema LIKE 'archive_%'
         ORDER BY table_schema
    LOOP
        IF archive_row.schema_name !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;

        EXECUTE format(
            'ALTER TABLE %I.embedding_import_run ENABLE ROW LEVEL SECURITY',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.embedding_import_run FORCE ROW LEVEL SECURITY',
            archive_row.schema_name
        );
        EXECUTE format(
            'DROP POLICY IF EXISTS tenant_isolation ON %I.embedding_import_run',
            archive_row.schema_name
        );
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.embedding_import_run USING (tenant_id = current_setting(''app.current_tenant'')::uuid) WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)',
            archive_row.schema_name
        );
    END LOOP;
END
$embedding_import_run_rls_archives$;

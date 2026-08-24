-- Archive schemas predate hosted tenancy. Backfill each legacy archive to the
-- tenant that owns its registry row, then apply the same fail-closed policy as
-- public tenant tables. Future archives are hardened by PgArchiveRepository.

SELECT set_config(
    'app.current_tenant',
    '00000000-0000-0000-0000-000000000000',
    true
);

DO $archive_tenant_rls$
DECLARE
    archive_row RECORD;
    table_row RECORD;
BEGIN
    FOR archive_row IN
        SELECT schema_name, tenant_id
          FROM archive_registry
         WHERE schema_name <> 'public'
         ORDER BY schema_name
    LOOP
        IF archive_row.schema_name !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;

        IF NOT EXISTS (
            SELECT 1
              FROM pg_namespace
             WHERE nspname = archive_row.schema_name
        ) THEN
            RAISE EXCEPTION 'registered archive schema is missing';
        END IF;

        FOR table_row IN
            SELECT c.relname AS table_name
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = archive_row.schema_name
               AND c.relkind = 'r'
             ORDER BY c.relname
        LOOP
            EXECUTE format(
                'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS tenant_id UUID',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'UPDATE %I.%I SET tenant_id = $1 WHERE tenant_id IS NULL',
                archive_row.schema_name,
                table_row.table_name
            ) USING archive_row.tenant_id;
            EXECUTE format(
                'ALTER TABLE %I.%I ALTER COLUMN tenant_id SET NOT NULL',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'ALTER TABLE %I.%I ALTER COLUMN tenant_id SET DEFAULT current_setting(''app.current_tenant'')::uuid',
                archive_row.schema_name,
                table_row.table_name
            );

            IF NOT EXISTS (
                SELECT 1
                  FROM pg_constraint fk
                  JOIN pg_attribute source_col
                    ON source_col.attrelid = fk.conrelid
                   AND source_col.attnum = ANY(fk.conkey)
                 WHERE fk.conrelid = format('%I.%I', archive_row.schema_name, table_row.table_name)::regclass
                   AND fk.contype = 'f'
                   AND fk.confrelid = 'public.tenant_registry'::regclass
                   AND source_col.attname = 'tenant_id'
            ) THEN
                EXECUTE format(
                    'ALTER TABLE %I.%I ADD CONSTRAINT tenant_registry_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id)',
                    archive_row.schema_name,
                    table_row.table_name
                );
            END IF;

            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS %I ON %I.%I (tenant_id)',
                table_row.table_name || '_tenant_id_idx',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'DROP POLICY IF EXISTS tenant_isolation ON %I.%I',
                archive_row.schema_name,
                table_row.table_name
            );
            EXECUTE format(
                'CREATE POLICY tenant_isolation ON %I.%I USING (tenant_id = current_setting(''app.current_tenant'')::uuid) WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)',
                archive_row.schema_name,
                table_row.table_name
            );
        END LOOP;
    END LOOP;
END
$archive_tenant_rls$;

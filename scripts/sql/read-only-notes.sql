-- Invoke with psql -X -v ON_ERROR_STOP=1 -v tenant_id=... -v archive_name=... -f this-file
\set ON_ERROR_STOP on
BEGIN READ ONLY;
SELECT set_config('app.current_tenant', :'tenant_id'::uuid::text, true);

-- The registry is tenant-scoped too. A missing or invisible archive must fail
-- before search_path can fall back to public. \gset requires exactly one row.
SELECT schema_name AS selected_schema
FROM public.archive_registry
WHERE name = :'archive_name'
  AND tenant_id = current_setting('app.current_tenant')::uuid
\gset
SELECT set_config('search_path', format('%I, public', :'selected_schema'), true);

-- Show and check the actual connection and relation scope before the read.
SELECT current_user AS role, current_setting('app.current_tenant') AS tenant_id,
       current_schema() AS archive_schema, current_setting('transaction_read_only') AS read_only;
SELECT current_schema() = :'selected_schema'
       AND NOT r.rolsuper AND NOT r.rolbypassrls
       AND c.relrowsecurity AND c.relforcerowsecurity
       AND n.nspname = :'selected_schema' AS scope_ok
FROM pg_roles r, pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE r.rolname = current_user AND c.oid = to_regclass('note')
\gset
\if :scope_ok
SELECT count(*) AS live_notes FROM note WHERE deleted_at IS NULL;
\else
\echo 'Scope verification failed: use a role subject to RLS and a migrated archive.'
DO $$ BEGIN RAISE EXCEPTION 'read-only recipe scope verification failed'; END $$;
\endif
COMMIT;

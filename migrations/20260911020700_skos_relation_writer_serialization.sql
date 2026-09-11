-- Runtime coordination is local to each archive/tenant, not portable shard data.
-- A physical no-op update serializes writers and forces stale snapshot writers
-- to fail with a serialization error without changing logical record values.
CREATE TABLE IF NOT EXISTS public.skos_relation_write_guard (
    tenant_id UUID PRIMARY KEY DEFAULT current_setting('app.current_tenant')::uuid
        REFERENCES public.tenant_registry(id)
);

CREATE OR REPLACE FUNCTION public.skos_serialize_relation_writes()
RETURNS TRIGGER LANGUAGE plpgsql AS $serialize_relations$
BEGIN
    EXECUTE format('INSERT INTO %I.skos_relation_write_guard(tenant_id)
        VALUES (current_setting(''app.current_tenant'')::uuid)
        ON CONFLICT (tenant_id) DO UPDATE SET tenant_id=EXCLUDED.tenant_id', TG_TABLE_SCHEMA);
    RETURN NULL;
END
$serialize_relations$;

DO $relation_writer_guards$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_semantic_relation_edge', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        IF target_schema <> 'public' THEN
            EXECUTE format('CREATE TABLE IF NOT EXISTS %I.skos_relation_write_guard
                (LIKE public.skos_relation_write_guard INCLUDING ALL,
                 FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id))', target_schema);
        END IF;
        EXECUTE format('ALTER TABLE %I.skos_relation_write_guard ENABLE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('ALTER TABLE %I.skos_relation_write_guard FORCE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('DROP POLICY IF EXISTS tenant_isolation ON %I.skos_relation_write_guard', target_schema);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I.skos_relation_write_guard
            USING (tenant_id=current_setting(''app.current_tenant'')::uuid)
            WITH CHECK (tenant_id=current_setting(''app.current_tenant'')::uuid)', target_schema);
        EXECUTE format('CREATE OR REPLACE TRIGGER aaa_skos_serialize_relation_writes
            BEFORE INSERT OR UPDATE OR DELETE ON %I.skos_semantic_relation_edge
            FOR EACH STATEMENT EXECUTE FUNCTION public.skos_serialize_relation_writes()', target_schema);
    END LOOP;
END
$relation_writer_guards$;

COMMENT ON TABLE public.skos_relation_write_guard IS
    'Local per-tenant relation-writer coordination. Physical updates retain identical logical values; not part of Knowledge Shard state transfer.';

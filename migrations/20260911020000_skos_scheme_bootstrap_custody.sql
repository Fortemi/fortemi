-- Only initializers may record creation custody. Never infer it for existing
-- default-looking rows: unused and system flags do not imply disposability.
CREATE TABLE public.shard_skos_scheme_bootstrap (
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    scheme_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, scheme_id),
    FOREIGN KEY (tenant_id, scheme_id)
        REFERENCES public.skos_concept_scheme (tenant_id, id) ON DELETE CASCADE
);

CREATE FUNCTION public.adopt_native_skos_scheme() RETURNS TRIGGER
LANGUAGE plpgsql AS $adopt_scheme$
BEGIN
    EXECUTE format('DELETE FROM %I.shard_skos_scheme_bootstrap
        WHERE tenant_id = $1 AND scheme_id = $2', TG_TABLE_SCHEMA)
        USING OLD.tenant_id, OLD.id;
    RETURN NEW;
END
$adopt_scheme$;

CREATE FUNCTION public.adopt_native_skos_scheme_reference() RETURNS TRIGGER
LANGUAGE plpgsql AS $adopt_reference$
DECLARE
    parent RECORD;
BEGIN
    -- Imported references also make scaffolding live. Both ends of a moved
    -- reference are adopted; deleting the last reference never restores custody.
    FOR parent IN
        SELECT DISTINCT tenant_id, scheme_id FROM (VALUES
            ((to_jsonb(OLD)->>'tenant_id')::uuid, (to_jsonb(OLD)->>TG_ARGV[0])::uuid),
            ((to_jsonb(NEW)->>'tenant_id')::uuid, (to_jsonb(NEW)->>TG_ARGV[0])::uuid)
        ) AS referenced(tenant_id, scheme_id)
        WHERE tenant_id IS NOT NULL AND scheme_id IS NOT NULL
        ORDER BY tenant_id, scheme_id
    LOOP
        EXECUTE format('SELECT id FROM %I.skos_concept_scheme
            WHERE tenant_id = $1 AND id = $2 FOR UPDATE', TG_TABLE_SCHEMA)
            USING parent.tenant_id, parent.scheme_id;
        EXECUTE format('DELETE FROM %I.shard_skos_scheme_bootstrap
            WHERE tenant_id = $1 AND scheme_id = $2', TG_TABLE_SCHEMA)
            USING parent.tenant_id, parent.scheme_id;
    END LOOP;
    RETURN COALESCE(NEW, OLD);
END
$adopt_reference$;

DO $bootstrap_custody$
DECLARE
    target_schema TEXT;
    child RECORD;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_concept_scheme', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        IF target_schema <> 'public' THEN
            EXECUTE format('CREATE TABLE %I.shard_skos_scheme_bootstrap
                (LIKE public.shard_skos_scheme_bootstrap INCLUDING ALL)', target_schema);
            EXECUTE format('ALTER TABLE %I.shard_skos_scheme_bootstrap
                ADD FOREIGN KEY (tenant_id, scheme_id)
                REFERENCES %I.skos_concept_scheme (tenant_id, id) ON DELETE CASCADE',
                target_schema, target_schema);
        END IF;
        EXECUTE format('ALTER TABLE %I.shard_skos_scheme_bootstrap ENABLE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('ALTER TABLE %I.shard_skos_scheme_bootstrap FORCE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I.shard_skos_scheme_bootstrap
            USING (tenant_id = current_setting(''app.current_tenant'')::uuid)
            WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)', target_schema);
        EXECUTE format('CREATE TRIGGER adopt_native_skos_scheme BEFORE UPDATE
            ON %I.skos_concept_scheme FOR EACH ROW EXECUTE FUNCTION public.adopt_native_skos_scheme()', target_schema);
        FOR child IN SELECT * FROM (VALUES
            ('skos_concept', 'primary_scheme_id'),
            ('skos_concept_in_scheme', 'scheme_id'),
            ('skos_collection', 'scheme_id')
        ) AS children(table_name, column_name)
        LOOP
            IF to_regclass(format('%I.%I', target_schema, child.table_name)) IS NOT NULL THEN
                EXECUTE format('CREATE TRIGGER adopt_native_skos_scheme_reference
                    BEFORE INSERT OR UPDATE OR DELETE ON %I.%I
                    FOR EACH ROW EXECUTE FUNCTION public.adopt_native_skos_scheme_reference(%L)',
                    target_schema, child.table_name, child.column_name);
            END IF;
        END LOOP;
    END LOOP;
END
$bootstrap_custody$;

COMMENT ON TABLE public.shard_skos_scheme_bootstrap IS
    'Identity-only custody for newly created archive SKOS scaffolding. Edits and references adopt live ownership transactionally; existing rows are never inferred to be disposable.';

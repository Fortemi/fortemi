-- Creation custody, not a serialized copy of live set state. Existing rows
-- are deliberately not inferred to be disposable from name or system flags.
CREATE TABLE public.shard_embedding_set_bootstrap (
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    set_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, set_id),
    FOREIGN KEY (tenant_id, set_id)
        REFERENCES public.embedding_set (tenant_id, id) ON DELETE CASCADE
);

CREATE FUNCTION public.adopt_native_embedding_set() RETURNS TRIGGER
LANGUAGE plpgsql AS $adopt_set$
BEGIN
    IF COALESCE(current_setting('app.shard_import', true), '') <> 'on' THEN
        EXECUTE format('DELETE FROM %I.shard_embedding_set_bootstrap
            WHERE tenant_id = $1 AND set_id = $2', TG_TABLE_SCHEMA)
            USING OLD.tenant_id, OLD.id;
        NEW.shard_export_present := TRUE;
    END IF;
    RETURN NEW;
END
$adopt_set$;

CREATE FUNCTION public.adopt_native_embedding_set_reference() RETURNS TRIGGER
LANGUAGE plpgsql AS $adopt_reference$
DECLARE
    parent_id UUID;
    parent_tenant UUID;
    adopted_id UUID;
BEGIN
    IF COALESCE(current_setting('app.shard_import', true), '') <> 'on' THEN
        IF TG_OP = 'DELETE' THEN
            parent_id := OLD.embedding_set_id;
            parent_tenant := OLD.tenant_id;
        ELSE
            parent_id := NEW.embedding_set_id;
            parent_tenant := NEW.tenant_id;
        END IF;
        -- Lock the parent before custody, matching import lock order.
        EXECUTE format('SELECT id FROM %I.embedding_set
            WHERE tenant_id = $1 AND id = $2 FOR UPDATE', TG_TABLE_SCHEMA)
            USING parent_tenant, parent_id;
        EXECUTE format('DELETE FROM %I.shard_embedding_set_bootstrap
            WHERE tenant_id = $1 AND set_id = $2 RETURNING set_id', TG_TABLE_SCHEMA)
            INTO adopted_id USING parent_tenant, parent_id;
        IF adopted_id IS NOT NULL THEN
            EXECUTE format('UPDATE %I.embedding_set SET shard_export_present = TRUE
                WHERE tenant_id = $1 AND id = $2', TG_TABLE_SCHEMA)
                USING parent_tenant, adopted_id;
        END IF;
    END IF;
    RETURN COALESCE(NEW, OLD);
END
$adopt_reference$;

DO $bootstrap_custody$
DECLARE
    target_schema TEXT;
    child_table TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.embedding_set', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        IF target_schema <> 'public' THEN
            EXECUTE format('CREATE TABLE %I.shard_embedding_set_bootstrap
                (LIKE public.shard_embedding_set_bootstrap INCLUDING ALL)', target_schema);
            EXECUTE format('ALTER TABLE %I.shard_embedding_set_bootstrap
                ADD FOREIGN KEY (tenant_id, set_id)
                REFERENCES %I.embedding_set (tenant_id, id) ON DELETE CASCADE',
                target_schema, target_schema);
        END IF;
        EXECUTE format('ALTER TABLE %I.shard_embedding_set_bootstrap ENABLE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('ALTER TABLE %I.shard_embedding_set_bootstrap FORCE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I.shard_embedding_set_bootstrap
            USING (tenant_id = current_setting(''app.current_tenant'')::uuid)
            WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)', target_schema);
        EXECUTE format('CREATE TRIGGER adopt_native_embedding_set BEFORE UPDATE
            ON %I.embedding_set FOR EACH ROW EXECUTE FUNCTION public.adopt_native_embedding_set()', target_schema);
        IF to_regclass(format('%I.note', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_auto_add_note_to_embedding_sets
                AFTER INSERT OR UPDATE ON %I.note FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.auto_add_note_to_embedding_sets()', target_schema);
        END IF;
        FOREACH child_table IN ARRAY ARRAY['embedding_set_member', 'embedding', 'embedding_coarse', 'attachment_embedding']
        LOOP
            IF to_regclass(format('%I.%I', target_schema, child_table)) IS NOT NULL THEN
                EXECUTE format('CREATE TRIGGER adopt_native_embedding_set_reference
                    AFTER INSERT OR UPDATE OR DELETE ON %I.%I
                    FOR EACH ROW EXECUTE FUNCTION public.adopt_native_embedding_set_reference()',
                    target_schema, child_table);
            END IF;
        END LOOP;
    END LOOP;
END
$bootstrap_custody$;

COMMENT ON TABLE public.shard_embedding_set_bootstrap IS
    'Identity-only custody for newly created bootstrap sets. Native changes adopt live ownership; imported declarations do not grant cleanup authority over existing rows.';

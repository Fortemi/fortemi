-- Shard declarations are archive-local; embedding_config remains a shared
-- live registry. This table stores identities, never copied configuration data.
CREATE TABLE public.shard_embedding_config_declaration (
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    config_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, config_id),
    FOREIGN KEY (tenant_id, config_id)
        REFERENCES public.embedding_config (tenant_id, id) ON DELETE CASCADE
);

INSERT INTO public.shard_embedding_config_declaration (tenant_id, config_id)
SELECT tenant_id, id FROM public.embedding_config WHERE shard_export_present;

-- Preserve the pre-upgrade visible roots in every existing archive. Future
-- archives start without explicit declarations and export referenced configs.
DO $archive_declarations$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT DISTINCT schema_name FROM public.archive_registry WHERE schema_name <> 'public'
    LOOP
        IF to_regclass(format('%I.note', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        EXECUTE format('CREATE TABLE %I.shard_embedding_config_declaration
            (LIKE public.shard_embedding_config_declaration INCLUDING ALL)', target_schema);
        EXECUTE format('ALTER TABLE %I.shard_embedding_config_declaration
            ADD FOREIGN KEY (tenant_id, config_id)
            REFERENCES public.embedding_config (tenant_id, id) ON DELETE CASCADE', target_schema);
        EXECUTE format('INSERT INTO %I.shard_embedding_config_declaration (tenant_id, config_id)
            SELECT tenant_id, id FROM public.embedding_config WHERE shard_export_present', target_schema);
    END LOOP;
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.shard_embedding_config_declaration', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        EXECUTE format('ALTER TABLE %I.shard_embedding_config_declaration ENABLE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('ALTER TABLE %I.shard_embedding_config_declaration FORCE ROW LEVEL SECURITY', target_schema);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I.shard_embedding_config_declaration
            USING (tenant_id = current_setting(''app.current_tenant'')::uuid)
            WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)', target_schema);
    END LOOP;
END
$archive_declarations$;

CREATE FUNCTION public.declare_native_embedding_config() RETURNS TRIGGER
LANGUAGE plpgsql AS $declare_config$
BEGIN
    EXECUTE format('INSERT INTO %I.shard_embedding_config_declaration (tenant_id, config_id)
        VALUES ($1, $2) ON CONFLICT (tenant_id, config_id) DO NOTHING', current_schema())
        USING NEW.tenant_id, NEW.id;
    RETURN NEW;
END
$declare_config$;

CREATE TRIGGER declare_native_embedding_config
AFTER INSERT OR UPDATE ON public.embedding_config
FOR EACH ROW EXECUTE FUNCTION public.declare_native_embedding_config();

COMMENT ON TABLE public.shard_embedding_config_declaration IS
    'Archive-local schema-2 configuration root declarations. Referenced live configs are additionally included by dependency closure.';

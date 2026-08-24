-- ADR-090 construction: exhaustive public-table tenant inventory, local-data
-- backfill, tenant-qualified FK guards, and fail-closed forced RLS.

CREATE TABLE tenant_registry (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'suspended', 'soft_deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    suspended_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

COMMENT ON TABLE tenant_registry IS
    'System-scoped active-tenant authority. It is queried before request tenant scope exists.';

INSERT INTO tenant_registry (id, slug, display_name, status)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'local',
    'Local personal server',
    'active'
)
ON CONFLICT (id) DO NOTHING;

DO $tenant_inventory$
DECLARE
    local_tenant CONSTANT UUID := '00000000-0000-0000-0000-000000000000';
    tenant_tables CONSTANT TEXT[] := ARRAY[
        'activity_log',
        'api_key',
        'archive_inference_override',
        'archive_registry',
        'attachment',
        'attachment_blob',
        'attachment_embedding',
        'call_sessions',
        'collection',
        'community',
        'community_assignment',
        'community_set',
        'document_type',
        'embedding',
        'embedding_coarse',
        'embedding_config',
        'embedding_set',
        'embedding_set_member',
        'entity_stats',
        'event_outbox',
        'file_upload_audit',
        'fine_tuning_dataset',
        'fine_tuning_sample',
        'graph_diagnostics_history',
        'graph_edge_artifact',
        'graph_source',
        'inbound_dlq',
        'inbound_source',
        'incoming_webhook_receiver',
        'inference_config_audit',
        'job_attempt',
        'job_history',
        'job_queue',
        'link',
        'model_3d_metadata',
        'named_location',
        'note',
        'note_access_log',
        'note_entity',
        'note_graph_embedding',
        'note_original',
        'note_original_history',
        'note_revised_current',
        'note_revision',
        'note_share_grant',
        'note_skos_concept',
        'note_tag',
        'note_template',
        'note_token_embeddings',
        'oauth_authorization_code',
        'oauth_client',
        'oauth_token',
        'pke_active_keyset',
        'pke_keysets',
        'pke_public_keys',
        'prov_agent_device',
        'prov_location',
        'provenance',
        'provenance_activity',
        'provenance_edge',
        'realtime_media_stream_attempt',
        'skos_audit_log',
        'skos_collection',
        'skos_collection_member',
        'skos_concept',
        'skos_concept_in_scheme',
        'skos_concept_label',
        'skos_concept_merge',
        'skos_concept_note',
        'skos_concept_scheme',
        'skos_mapping_relation_edge',
        'skos_semantic_relation_edge',
        'structured_media_metadata',
        'tag',
        'transcript_segments',
        'tus_upload',
        'usage_delivery_attempt',
        'usage_event_conflict',
        'usage_event_delivery',
        'usage_event_ledger',
        'user_config',
        'user_metadata_label',
        'webhook',
        'webhook_delivery'
    ];
    exempt_tables CONSTANT TEXT[] := ARRAY[
        'spatial_ref_sys',
        'system_config',
        'tenant_registry',
        'usage_sink'
    ];
    table_name TEXT;
    object_name TEXT;
    unclassified TEXT[];
BEGIN
    SELECT array_agg(c.relname ORDER BY c.relname)
      INTO unclassified
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE n.nspname = 'public'
       AND c.relkind = 'r'
       AND c.relname NOT LIKE '_sqlx_%'
       AND NOT (c.relname = ANY(tenant_tables))
       AND NOT (c.relname = ANY(exempt_tables));

    IF unclassified IS NOT NULL THEN
        RAISE EXCEPTION 'ADR-090 table inventory is incomplete: %', unclassified;
    END IF;

    FOREACH table_name IN ARRAY tenant_tables LOOP
        IF to_regclass(format('public.%I', table_name)) IS NULL THEN
            RAISE EXCEPTION 'ADR-090 tenant table is missing: %', table_name;
        END IF;

        EXECUTE format(
            'ALTER TABLE public.%I ADD COLUMN IF NOT EXISTS tenant_id UUID',
            table_name
        );
        EXECUTE format(
            'UPDATE public.%I SET tenant_id = $1 WHERE tenant_id IS NULL',
            table_name
        ) USING local_tenant;
        EXECUTE format(
            'ALTER TABLE public.%I ALTER COLUMN tenant_id SET NOT NULL',
            table_name
        );
        EXECUTE format(
            'ALTER TABLE public.%I ALTER COLUMN tenant_id SET DEFAULT (current_setting(''app.current_tenant''))::uuid',
            table_name
        );

        object_name := format(
            'idx_tenant_%s_%s',
            left(table_name, 35),
            left(md5(table_name), 8)
        );
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS %I ON public.%I (tenant_id)',
            object_name,
            table_name
        );

        object_name := format(
            'fk_tenant_registry_%s_%s',
            left(table_name, 24),
            left(md5(table_name), 8)
        );
        IF NOT EXISTS (
            SELECT 1
              FROM pg_constraint
             WHERE conrelid = format('public.%I', table_name)::regclass
               AND conname = object_name
        ) THEN
            EXECUTE format(
                'ALTER TABLE public.%I ADD CONSTRAINT %I FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT NOT VALID',
                table_name,
                object_name
            );
            EXECUTE format(
                'ALTER TABLE public.%I VALIDATE CONSTRAINT %I',
                table_name,
                object_name
            );
        END IF;
    END LOOP;
END
$tenant_inventory$;

-- Add tenant-qualified guards alongside existing foreign keys. Existing keys
-- retain their ON DELETE/UPDATE behavior; these additional constraints prevent
-- a child row from associating with a parent in another tenant.
DO $tenant_foreign_keys$
DECLARE
    tenant_tables CONSTANT TEXT[] := ARRAY[
        'activity_log','api_key','archive_inference_override','archive_registry',
        'attachment','attachment_blob','attachment_embedding','call_sessions',
        'collection','community','community_assignment','community_set','document_type',
        'embedding','embedding_coarse','embedding_config','embedding_set',
        'embedding_set_member','entity_stats','event_outbox','file_upload_audit',
        'fine_tuning_dataset','fine_tuning_sample','graph_diagnostics_history',
        'graph_edge_artifact','graph_source','inbound_dlq','inbound_source',
        'incoming_webhook_receiver','inference_config_audit','job_attempt','job_history',
        'job_queue','link','model_3d_metadata','named_location','note','note_access_log',
        'note_entity','note_graph_embedding','note_original','note_original_history',
        'note_revised_current','note_revision','note_share_grant','note_skos_concept',
        'note_tag','note_template','note_token_embeddings','oauth_authorization_code',
        'oauth_client','oauth_token','pke_active_keyset','pke_keysets','pke_public_keys',
        'prov_agent_device','prov_location','provenance','provenance_activity',
        'provenance_edge','realtime_media_stream_attempt','skos_audit_log','skos_collection',
        'skos_collection_member','skos_concept','skos_concept_in_scheme',
        'skos_concept_label','skos_concept_merge','skos_concept_note','skos_concept_scheme',
        'skos_mapping_relation_edge','skos_semantic_relation_edge','structured_media_metadata',
        'tag','transcript_segments','tus_upload','usage_delivery_attempt',
        'usage_event_conflict','usage_event_delivery','usage_event_ledger','user_config',
        'user_metadata_label','webhook','webhook_delivery'
    ];
    fk RECORD;
    child_columns TEXT;
    parent_columns TEXT;
    unique_index_name TEXT;
    guard_name TEXT;
BEGIN
    FOR fk IN
        SELECT con.oid,
               con.conname,
               child.relname AS child_table,
               parent.relname AS parent_table,
               con.conkey,
               con.confkey
          FROM pg_constraint con
          JOIN pg_class child ON child.oid = con.conrelid
          JOIN pg_class parent ON parent.oid = con.confrelid
          JOIN pg_namespace child_ns ON child_ns.oid = child.relnamespace
          JOIN pg_namespace parent_ns ON parent_ns.oid = parent.relnamespace
         WHERE con.contype = 'f'
           AND child_ns.nspname = 'public'
           AND parent_ns.nspname = 'public'
           AND child.relname = ANY(tenant_tables)
           AND parent.relname = ANY(tenant_tables)
           AND NOT EXISTS (
               SELECT 1
                 FROM unnest(con.conkey) key(attnum)
                 JOIN pg_attribute a
                   ON a.attrelid = con.conrelid
                  AND a.attnum = key.attnum
                WHERE a.attname = 'tenant_id'
           )
    LOOP
        SELECT string_agg(format('%I', a.attname), ', ' ORDER BY key.ordinality)
          INTO child_columns
          FROM unnest(fk.conkey) WITH ORDINALITY key(attnum, ordinality)
          JOIN pg_attribute a
            ON a.attrelid = format('public.%I', fk.child_table)::regclass
           AND a.attnum = key.attnum;

        SELECT string_agg(format('%I', a.attname), ', ' ORDER BY key.ordinality)
          INTO parent_columns
          FROM unnest(fk.confkey) WITH ORDINALITY key(attnum, ordinality)
          JOIN pg_attribute a
            ON a.attrelid = format('public.%I', fk.parent_table)::regclass
           AND a.attnum = key.attnum;

        unique_index_name := format(
            'uq_tenant_ref_%s_%s',
            left(fk.parent_table, 24),
            left(md5(fk.parent_table || ':' || parent_columns), 10)
        );
        EXECUTE format(
            'CREATE UNIQUE INDEX IF NOT EXISTS %I ON public.%I (tenant_id, %s)',
            unique_index_name,
            fk.parent_table,
            parent_columns
        );

        guard_name := format(
            'fk_tenant_guard_%s_%s',
            left(fk.child_table, 20),
            left(md5(fk.child_table || ':' || fk.conname), 10)
        );
        IF NOT EXISTS (
            SELECT 1
              FROM pg_constraint
             WHERE conrelid = format('public.%I', fk.child_table)::regclass
               AND conname = guard_name
        ) THEN
            EXECUTE format(
                'ALTER TABLE public.%I ADD CONSTRAINT %I FOREIGN KEY (tenant_id, %s) REFERENCES public.%I (tenant_id, %s) NOT VALID',
                fk.child_table,
                guard_name,
                child_columns,
                fk.parent_table,
                parent_columns
            );
            EXECUTE format(
                'ALTER TABLE public.%I VALIDATE CONSTRAINT %I',
                fk.child_table,
                guard_name
            );
        END IF;
    END LOOP;
END
$tenant_foreign_keys$;

DO $tenant_policies$
DECLARE
    tenant_tables CONSTANT TEXT[] := ARRAY[
        'activity_log','api_key','archive_inference_override','archive_registry',
        'attachment','attachment_blob','attachment_embedding','call_sessions',
        'collection','community','community_assignment','community_set','document_type',
        'embedding','embedding_coarse','embedding_config','embedding_set',
        'embedding_set_member','entity_stats','event_outbox','file_upload_audit',
        'fine_tuning_dataset','fine_tuning_sample','graph_diagnostics_history',
        'graph_edge_artifact','graph_source','inbound_dlq','inbound_source',
        'incoming_webhook_receiver','inference_config_audit','job_attempt','job_history',
        'job_queue','link','model_3d_metadata','named_location','note','note_access_log',
        'note_entity','note_graph_embedding','note_original','note_original_history',
        'note_revised_current','note_revision','note_share_grant','note_skos_concept',
        'note_tag','note_template','note_token_embeddings','oauth_authorization_code',
        'oauth_client','oauth_token','pke_active_keyset','pke_keysets','pke_public_keys',
        'prov_agent_device','prov_location','provenance','provenance_activity',
        'provenance_edge','realtime_media_stream_attempt','skos_audit_log','skos_collection',
        'skos_collection_member','skos_concept','skos_concept_in_scheme',
        'skos_concept_label','skos_concept_merge','skos_concept_note','skos_concept_scheme',
        'skos_mapping_relation_edge','skos_semantic_relation_edge','structured_media_metadata',
        'tag','transcript_segments','tus_upload','usage_delivery_attempt',
        'usage_event_conflict','usage_event_delivery','usage_event_ledger','user_config',
        'user_metadata_label','webhook','webhook_delivery'
    ];
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY tenant_tables LOOP
        EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE public.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format('DROP POLICY IF EXISTS tenant_isolation ON public.%I', table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON public.%I FOR ALL USING (tenant_id = (current_setting(''app.current_tenant''))::uuid) WITH CHECK (tenant_id = (current_setting(''app.current_tenant''))::uuid)',
            table_name
        );
    END LOOP;
END
$tenant_policies$;

COMMENT ON COLUMN note.tenant_id IS
    'ADR-090 tenant boundary; NOT NULL and enforced by FORCE RLS.';

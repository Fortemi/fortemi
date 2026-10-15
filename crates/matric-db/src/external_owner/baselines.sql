DO $baseline$
DECLARE a RECORD; config UUID; usable BOOLEAN; owner UUID := current_setting('app.current_tenant')::uuid;
BEGIN
    SELECT id INTO config FROM public.embedding_config WHERE tenant_id=owner AND is_default
        ORDER BY created_at,id LIMIT 1;
    IF config IS NULL THEN RAISE EXCEPTION 'Canonical owner has no default embedding configuration'; END IF;
    FOR a IN SELECT schema_name FROM public.archive_registry WHERE tenant_id=owner
        UNION SELECT 'public' LOOP
        EXECUTE format('WITH seeded AS (
            INSERT INTO %I.embedding_set(tenant_id,id,name,slug,mode,criteria,embedding_config_id,is_system,is_active,index_status)
            SELECT $1,uuidv7(),''Default'',''default'',''auto'',
                ''{"include_all":true,"exclude_archived":true}''::jsonb,$2,true,true,''ready''
            WHERE NOT EXISTS(SELECT 1 FROM %I.embedding_set WHERE tenant_id=$1 AND slug=''default'')
            RETURNING tenant_id,id)
            INSERT INTO %I.shard_embedding_set_bootstrap(tenant_id,set_id) SELECT tenant_id,id FROM seeded',
            a.schema_name,a.schema_name,a.schema_name) USING owner,config;
        EXECUTE format('WITH seeded AS (
            INSERT INTO %I.skos_concept_scheme(tenant_id,id,notation,title,description,is_system)
            SELECT $1,uuidv7(),''default'',''Default Tags'',''Default concept scheme for general-purpose tagging'',true
            WHERE NOT EXISTS(SELECT 1 FROM %I.skos_concept_scheme WHERE tenant_id=$1 AND notation=''default'')
            RETURNING tenant_id,id)
            INSERT INTO %I.shard_skos_scheme_bootstrap(tenant_id,scheme_id) SELECT tenant_id,id FROM seeded',
            a.schema_name,a.schema_name,a.schema_name) USING owner;
        EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I.embedding_set s
            JOIN public.embedding_config c ON c.id=s.embedding_config_id AND c.tenant_id=s.tenant_id
            WHERE s.tenant_id=$1 AND s.slug=''default'' AND s.is_active)',a.schema_name)
            INTO usable USING owner;
        IF NOT usable THEN RAISE EXCEPTION 'Unusable default embedding set in %',a.schema_name; END IF;
        EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I.skos_concept_scheme
            WHERE tenant_id=$1 AND notation=''default'' AND is_active AND is_system)',a.schema_name)
            INTO usable USING owner;
        IF NOT usable THEN RAISE EXCEPTION 'Unusable default SKOS scheme in %',a.schema_name; END IF;
        EXECUTE format('INSERT INTO %I.embedding_set_member(tenant_id,embedding_set_id,note_id,membership_type)
            SELECT $1,s.id,n.id,''auto'' FROM %I.embedding_set s JOIN %I.note n ON n.tenant_id=s.tenant_id
            WHERE s.tenant_id=$1 AND s.slug=''default'' AND n.deleted_at IS NULL AND NOT n.archived
            ON CONFLICT(embedding_set_id,note_id) DO NOTHING',
            a.schema_name,a.schema_name,a.schema_name) USING owner;
    END LOOP;
END $baseline$;

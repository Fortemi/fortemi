-- Reference UUIDs and names may differ between bootstraps. Compare the actual
-- configuration values, including model, dimensions, chunking and index tuning.
-- Default selection is reconciled separately below.
CREATE FUNCTION pg_temp.owner_config_semantics(config UUID) RETURNS JSONB
LANGUAGE sql STABLE AS $config$
    SELECT to_jsonb(c)-ARRAY['id','tenant_id','name','is_default','created_at','updated_at']
    FROM public.embedding_config c WHERE c.id=config
$config$;
-- Preserve both UUIDs when round-2 bootstrap duplicated an equivalent baseline.
-- L's public name remains canonical; E's equivalent row gets a stable alternate
-- name. References keep their original UUIDs. Customized collisions abort.
DO $baselines$
DECLARE
    t RECORD; pair RECORD; key_column TEXT; ignored TEXT[]; eligible TEXT; references_equal TEXT;
    owner UUID := current_setting('app.external_owner')::uuid;
    local UUID := '00000000-0000-0000-0000-000000000000';
    conflicts BOOLEAN;
BEGIN
    IF owner=local THEN RETURN; END IF;
    -- Local archive identity clones must agree exactly with public identities.
    FOR t IN SELECT * FROM owner_tables WHERE schema_name<>'public'
        AND table_name IN ('app_user','personal_access_token') ORDER BY table_name LOOP
        EXECUTE format('SELECT EXISTS(SELECT 1 FROM %s a JOIN public.%I p ON a.id=p.id
            WHERE a.tenant_id IN ($1,$2) AND to_jsonb(a)<>to_jsonb(p))',t.relation,t.table_name)
            INTO conflicts USING local,owner;
        IF conflicts THEN RAISE EXCEPTION 'Conflicting archive identity in %',t.relation; END IF;
        EXECUTE format('INSERT INTO public.%I SELECT a.* FROM %s a
            WHERE a.tenant_id IN ($1,$2) AND NOT EXISTS(SELECT 1 FROM public.%I p WHERE p.id=a.id)',
            t.table_name,t.relation,t.table_name) USING local,owner;
    END LOOP;
    -- The relation-writer guard contains only a tenant discriminator, no content.
    FOR t IN SELECT * FROM owner_tables WHERE table_name='skos_relation_write_guard' LOOP
        EXECUTE format('DELETE FROM %s WHERE tenant_id=$1 AND EXISTS(SELECT 1 FROM %s WHERE tenant_id=$2)',t.relation,t.relation)
            USING local,owner;
    END LOOP;
    FOR t IN SELECT * FROM owner_tables WHERE table_name IN
        ('document_type','embedding_config','embedding_set','skos_concept_scheme') ORDER BY relation LOOP
        key_column := CASE t.table_name WHEN 'embedding_set' THEN 'slug'
            WHEN 'skos_concept_scheme' THEN 'notation' ELSE 'name' END;
        ignored := ARRAY['id','tenant_id','created_at','updated_at','created_at_utc','updated_at_utc'];
        eligible := CASE t.table_name WHEN 'embedding_config' THEN 'l.is_default AND e.is_default'
            WHEN 'document_type' THEN '(l.is_system OR l.name IN (''graphviz-diagram'',''mermaid-diagram'',''plantuml-diagram'',''svg-graphic'',''midi-music'',''tracker-module'',''model-3d-cad'',''model-3d-generic'',''model-3d-printable''))'
            ELSE 'l.is_system AND e.is_system' END;
        references_equal := 'true';
        IF t.table_name='document_type' THEN
            ignored := ignored || ARRAY['recommended_config_id'];
            references_equal := '(pg_temp.owner_config_semantics(l.recommended_config_id) IS NOT DISTINCT FROM pg_temp.owner_config_semantics(e.recommended_config_id))';
        ELSIF t.table_name='embedding_set' THEN
            ignored := ignored || ARRAY['embedding_config_id','document_count','embedding_count','index_status','index_built_at','last_refresh_at','last_indexed_at','index_size_bytes','embeddings_current'];
            references_equal := '(pg_temp.owner_config_semantics(l.embedding_config_id) IS NOT DISTINCT FROM pg_temp.owner_config_semantics(e.embedding_config_id))';
        ELSIF t.table_name='skos_concept_scheme' THEN
            ignored := ignored || ARRAY['uri'];
        END IF;
        FOR pair IN EXECUTE format('SELECT e.id,e.%I AS name,
            (to_jsonb(l)-$3)=(to_jsonb(e)-$3) AND (%s) AND (%s) AS equivalent
            FROM %s l JOIN %s e ON l.%I=e.%I WHERE l.tenant_id=$1 AND e.tenant_id=$2',
            key_column,eligible,references_equal,t.relation,t.relation,key_column,key_column) USING local,owner,ignored LOOP
            IF NOT pair.equivalent THEN
                RAISE EXCEPTION 'Conflicting customized baseline in % (%)',t.relation,pair.name;
            END IF;
            IF t.table_name='embedding_set' THEN
                EXECUTE format('UPDATE %s SET slug=$1,name=$1 WHERE id=$2 AND tenant_id=$3',t.relation)
                    USING 'external-previous-'||pair.id,pair.id,owner;
            ELSE
                EXECUTE format('UPDATE %s SET %I=$1 WHERE id=$2 AND tenant_id=$3',t.relation,key_column)
                    USING 'external-previous-'||pair.id,pair.id,owner;
            END IF;
        END LOOP;
    END LOOP;
    IF EXISTS(SELECT 1 FROM public.embedding_config WHERE tenant_id IN(local,owner) AND is_default
        GROUP BY tenant_id HAVING count(*)>1) THEN
        RAISE EXCEPTION 'Multiple default embedding configurations for one owner';
    END IF;
    -- Two differently customized defaults are ambiguous even with distinct names.
    IF EXISTS(SELECT 1 FROM public.embedding_config l JOIN public.embedding_config e
        ON l.is_default AND e.is_default WHERE l.tenant_id=local AND e.tenant_id=owner
        AND (to_jsonb(l)-ARRAY['id','tenant_id','name','created_at','updated_at'])
          <> (to_jsonb(e)-ARRAY['id','tenant_id','name','created_at','updated_at'])) THEN
        RAISE EXCEPTION 'Conflicting default embedding configurations';
    END IF;
    UPDATE public.embedding_config SET is_default=false WHERE tenant_id=owner AND is_default
        AND EXISTS(SELECT 1 FROM public.embedding_config WHERE tenant_id=local AND is_default);
    IF (SELECT count(*) FROM public.archive_registry WHERE tenant_id IN (local,owner) AND is_default)>1 THEN
        RAISE EXCEPTION 'Conflicting archive defaults';
    END IF;
END $baselines$;

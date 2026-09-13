-- Existing public/archive triggers share this function. Resolve content from
-- the triggering relation and queue from the explicit shared repository.
CREATE OR REPLACE FUNCTION public.queue_embedding_jobs_for_set_member()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY INVOKER AS $member_jobs$
DECLARE
    v_set RECORD;
    v_note_exists BOOLEAN;
    v_job_exists BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN RETURN NEW; END IF;
    EXECUTE format('SELECT id, set_type, is_active, auto_embed_rules
        FROM %I.embedding_set WHERE id=$1 AND tenant_id=$2', TG_TABLE_SCHEMA)
        INTO v_set USING NEW.embedding_set_id, NEW.tenant_id;
    IF v_set.id IS NULL OR v_set.set_type IS DISTINCT FROM 'full'
        OR v_set.is_active IS DISTINCT FROM TRUE
        OR (v_set.auto_embed_rules->>'on_create')::boolean IS FALSE THEN
        RETURN NEW;
    END IF;
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM %I.note
        WHERE id=$1 AND tenant_id=$2 AND deleted_at IS NULL)', TG_TABLE_SCHEMA)
        INTO v_note_exists USING NEW.note_id, NEW.tenant_id;
    IF NOT v_note_exists THEN RETURN NEW; END IF;
    SELECT EXISTS (
        SELECT 1 FROM public.job_queue
        WHERE tenant_id=NEW.tenant_id AND note_id=NEW.note_id
          AND job_type='embedding' AND status IN ('pending','running')
          AND COALESCE(payload->>'schema','public')=TG_TABLE_SCHEMA
          AND (payload->>'embedding_set_id' IS NULL
               OR payload->>'embedding_set_id'=v_set.id::text)
    ) INTO v_job_exists;
    IF NOT v_job_exists THEN
        INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,status,priority,payload,created_at)
        VALUES(pg_catalog.uuidv7(),NEW.tenant_id,NEW.note_id,'embedding','pending',
            COALESCE((v_set.auto_embed_rules->>'priority')::integer,5),
            jsonb_build_object('embedding_set_id',v_set.id,'schema',TG_TABLE_SCHEMA),NOW());
    END IF;
    RETURN NEW;
END
$member_jobs$;

-- Membership statistics are another native side effect of the same insert.
-- Do not call the legacy unqualified helper from an archive-only search_path.
CREATE OR REPLACE FUNCTION public.trigger_update_set_stats()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY INVOKER AS $member_stats$
DECLARE
    parent_ids UUID[];
    parent_tenants UUID[];
    parent RECORD;
BEGIN
    IF TG_OP = 'DELETE' THEN
        parent_ids := ARRAY[OLD.embedding_set_id];
        parent_tenants := ARRAY[OLD.tenant_id];
    ELSIF TG_OP = 'UPDATE' THEN
        parent_ids := ARRAY[OLD.embedding_set_id, NEW.embedding_set_id];
        parent_tenants := ARRAY[OLD.tenant_id, NEW.tenant_id];
    ELSE
        parent_ids := ARRAY[NEW.embedding_set_id];
        parent_tenants := ARRAY[NEW.tenant_id];
    END IF;
    FOR parent IN SELECT DISTINCT id, tenant
        FROM unnest(parent_ids,parent_tenants) AS parents(id,tenant)
        WHERE id IS NOT NULL ORDER BY tenant,id
    LOOP
        EXECUTE format('UPDATE %1$I.embedding_set AS s SET
            document_count=counts.documents, embedding_count=counts.embeddings,
            embeddings_current=CASE
                WHEN s.embedding_count=0 AND counts.embeddings=0 THEN TRUE
                WHEN counts.embeddings>=counts.documents THEN TRUE ELSE FALSE END,
            updated_at=NOW()
            FROM (SELECT
                (SELECT count(DISTINCT note_id) FROM %1$I.embedding_set_member
                    WHERE embedding_set_id=$1 AND tenant_id=$2) AS documents,
                (SELECT count(*) FROM %1$I.embedding
                    WHERE embedding_set_id=$1 AND tenant_id=$2) AS embeddings) AS counts
            WHERE s.id=$1 AND s.tenant_id=$2', TG_TABLE_SCHEMA)
            USING parent.id,parent.tenant;
    END LOOP;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END
$member_stats$;

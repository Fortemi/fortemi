-- External embedding sets and chunk identity foundation (#1177 part D).
--
-- Decisions for automatic embedding triggers:
-- * queue_embedding_jobs_for_set_member can directly target an embedding set
--   through NEW.embedding_set_id. It now skips vector_source='external' sets
--   and shard-import sessions.
-- * queue_reembed_for_skos_changes queues unscoped default embedding jobs after
--   SKOS semantic changes. It cannot target individual external sets because
--   its payload has no embedding_set_id. It now queues only when the tenant's
--   default set is internal, and also skips shard-import sessions.
-- * SKOS trigger definitions are left in place; redefining the function updates
--   trg_reembed_on_skos_concept_update, trg_reembed_on_skos_concept_delete, and
--   trg_reembed_on_skos_relation_change without touching their pgvector-safe
--   trigger predicates.

ALTER TABLE embedding_set
    ADD COLUMN IF NOT EXISTS vector_source TEXT NOT NULL DEFAULT 'internal';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'embedding_set'::regclass
          AND conname = 'embedding_set_vector_source_check'
    ) THEN
        ALTER TABLE embedding_set
            ADD CONSTRAINT embedding_set_vector_source_check
            CHECK (vector_source IN ('internal', 'external')) NOT VALID;
    END IF;
END $$;

ALTER TABLE embedding_set
    VALIDATE CONSTRAINT embedding_set_vector_source_check;

COMMENT ON COLUMN embedding_set.vector_source IS
    'internal = Fortemi owns automatic embedding generation; external = vectors are supplied externally and automatic writes are suppressed';

ALTER TABLE embedding
    ADD COLUMN IF NOT EXISTS chunk_hash TEXT,
    ADD COLUMN IF NOT EXISTS doc_hash TEXT;

COMMENT ON COLUMN embedding.chunk_hash IS
    'sha256:<hex> digest of the exact chunk text; NULL for legacy rows';
COMMENT ON COLUMN embedding.doc_hash IS
    'sha256:<hex> digest of the exact text passed to the chunker; NULL for legacy rows';

CREATE UNIQUE INDEX IF NOT EXISTS embedding_note_set_chunk_hash_unique
    ON embedding (tenant_id, note_id, embedding_set_id, chunk_hash)
    WHERE chunk_hash IS NOT NULL;

CREATE OR REPLACE FUNCTION public.queue_embedding_jobs_for_set_member()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY INVOKER AS $member_jobs$
DECLARE
    v_set RECORD;
    v_note_exists BOOLEAN;
    v_job_exists BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN RETURN NEW; END IF;
    IF current_setting('app.shard_import', true) = 'on' THEN RETURN NEW; END IF;

    EXECUTE format('SELECT id, set_type, is_active, auto_embed_rules, vector_source
        FROM %I.embedding_set WHERE id=$1 AND tenant_id=$2', TG_TABLE_SCHEMA)
        INTO v_set USING NEW.embedding_set_id, NEW.tenant_id;

    IF v_set.id IS NULL OR v_set.set_type IS DISTINCT FROM 'full'
        OR v_set.is_active IS DISTINCT FROM TRUE
        OR v_set.vector_source IS DISTINCT FROM 'internal'
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

CREATE OR REPLACE FUNCTION public.queue_reembed_for_skos_changes()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY INVOKER AS $skos_reembed$
DECLARE
    v_note_id UUID;
    v_concept_ids UUID[];
    v_tenant_id UUID;
    v_has_internal_default BOOLEAN;
BEGIN
    IF current_setting('app.shard_import', true) = 'on' THEN
        IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
        RETURN NEW;
    END IF;

    IF TG_TABLE_NAME = 'skos_semantic_relation_edge' THEN
        IF TG_OP = 'DELETE' THEN
            v_concept_ids := ARRAY[OLD.subject_id, OLD.object_id];
            v_tenant_id := OLD.tenant_id;
        ELSE
            v_concept_ids := ARRAY[NEW.subject_id, NEW.object_id];
            v_tenant_id := NEW.tenant_id;
        END IF;
    ELSIF TG_OP = 'DELETE' THEN
        v_concept_ids := ARRAY[OLD.id];
        v_tenant_id := OLD.tenant_id;
    ELSE
        v_concept_ids := ARRAY[NEW.id];
        v_tenant_id := NEW.tenant_id;
    END IF;

    EXECUTE format('SELECT EXISTS (
        SELECT 1 FROM %I.embedding_set
        WHERE tenant_id=$1 AND slug=''default'' AND is_system IS TRUE
          AND is_active IS TRUE AND vector_source=''internal''
    )', TG_TABLE_SCHEMA)
        INTO v_has_internal_default USING v_tenant_id;

    IF NOT v_has_internal_default THEN
        IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
        RETURN NEW;
    END IF;

    FOR v_note_id IN EXECUTE format(
        'SELECT DISTINCT nsc.note_id
           FROM %1$I.note_skos_concept nsc
          WHERE nsc.tenant_id=$1
            AND (nsc.concept_id=ANY($2)
                 OR nsc.concept_id IN (
                    WITH RECURSIVE narrower_tree AS (
                        SELECT subject_id, object_id
                          FROM %1$I.skos_semantic_relation_edge
                         WHERE tenant_id=$1 AND subject_id=ANY($2)
                           AND relation_type=''narrower''
                        UNION
                        SELECT sre.subject_id, sre.object_id
                          FROM %1$I.skos_semantic_relation_edge sre
                          JOIN narrower_tree nt ON sre.subject_id=nt.object_id
                         WHERE sre.tenant_id=$1 AND sre.relation_type=''narrower''
                    )
                    SELECT object_id FROM narrower_tree
                 ))',
        TG_TABLE_SCHEMA
    ) USING v_tenant_id, v_concept_ids
    LOOP
        INSERT INTO public.job_queue (
            id, tenant_id, note_id, job_type, status, priority, payload, created_at
        ) VALUES (
            pg_catalog.uuidv7(), v_tenant_id, v_note_id, 'embedding', 'pending', 7,
            jsonb_build_object('schema', TG_TABLE_SCHEMA), NOW()
        )
        ON CONFLICT DO NOTHING;
    END LOOP;

    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END
$skos_reembed$;

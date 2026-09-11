-- Auto membership must not depend on RLS hiding other tenants from an admin.
-- Explicit relation ownership also supports archive triggers with a public search_path.
CREATE FUNCTION public.evaluate_note_for_embedding_set_scoped(
    p_schema TEXT, p_tenant UUID, p_note_id UUID, p_set_id UUID
) RETURNS BOOLEAN LANGUAGE plpgsql STABLE SECURITY INVOKER AS $criteria$
DECLARE
    v_set RECORD;
    v_note RECORD;
    v_matches BOOLEAN;
BEGIN
    EXECUTE format('SELECT id, mode, criteria FROM %I.embedding_set
        WHERE id=$1 AND tenant_id=$2 AND is_active', p_schema)
        INTO v_set USING p_set_id, p_tenant;
    IF v_set.id IS NULL OR v_set.mode = 'manual' THEN
        RETURN FALSE;
    END IF;
    EXECUTE format('SELECT id, collection_id, created_at_utc, archived FROM %I.note
        WHERE id=$1 AND tenant_id=$2 AND deleted_at IS NULL', p_schema)
        INTO v_note USING p_note_id, p_tenant;
    IF v_note.id IS NULL THEN
        RETURN FALSE;
    END IF;

    IF (v_set.criteria->>'include_all')::boolean IS DISTINCT FROM TRUE THEN
        IF jsonb_array_length(COALESCE(v_set.criteria->'tags', '[]'::jsonb)) > 0 THEN
            EXECUTE format('SELECT EXISTS (
                SELECT 1 FROM %I.note_tag nt
                WHERE nt.note_id=$1 AND nt.tenant_id=$2 AND EXISTS (
                    SELECT 1 FROM jsonb_array_elements_text($3) AS criterion
                    WHERE LOWER(nt.tag_name)=LOWER(criterion)
                        OR LOWER(nt.tag_name) LIKE LOWER(criterion) || ''/%%''))', p_schema)
                INTO v_matches USING p_note_id, p_tenant, v_set.criteria->'tags';
            IF NOT v_matches THEN RETURN FALSE; END IF;
        END IF;
        IF jsonb_array_length(COALESCE(v_set.criteria->'collections', '[]'::jsonb)) > 0 THEN
            IF (v_note.collection_id = ANY(ARRAY(
                SELECT jsonb_array_elements_text(v_set.criteria->'collections')::uuid
            ))) IS DISTINCT FROM TRUE THEN
                RETURN FALSE;
            END IF;
        END IF;
        IF COALESCE(v_set.criteria->>'fts_query', '') <> '' THEN
            EXECUTE format('SELECT EXISTS (
                SELECT 1 FROM %I.note_revised_current nrc
                WHERE nrc.note_id=$1 AND nrc.tenant_id=$2
                    AND nrc.tsv @@ websearch_to_tsquery(''public.matric_english'', $3))', p_schema)
                INTO v_matches USING p_note_id, p_tenant, v_set.criteria->>'fts_query';
            IF NOT v_matches THEN RETURN FALSE; END IF;
        END IF;
        IF v_set.criteria->>'created_after' IS NOT NULL
            AND v_note.created_at_utc <= (v_set.criteria->>'created_after')::timestamptz THEN
            RETURN FALSE;
        END IF;
        IF v_set.criteria->>'created_before' IS NOT NULL
            AND v_note.created_at_utc >= (v_set.criteria->>'created_before')::timestamptz THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF (v_set.criteria->>'exclude_archived')::boolean = TRUE
        AND COALESCE(v_note.archived, FALSE) THEN
        RETURN FALSE;
    END IF;
    RETURN TRUE;
END
$criteria$;

-- Retain the existing SQL entry point, resolving the note relation just as its
-- former unqualified queries did, but rejecting cross-tenant note/set pairs.
CREATE OR REPLACE FUNCTION public.evaluate_note_for_embedding_set(
    p_note_id UUID, p_set_id UUID
) RETURNS BOOLEAN LANGUAGE plpgsql STABLE SECURITY INVOKER AS $evaluate$
DECLARE
    v_schema TEXT;
    v_tenant UUID;
BEGIN
    SELECT n.nspname INTO v_schema FROM pg_class c
        JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.oid='note'::regclass;
    EXECUTE format('SELECT tenant_id FROM %I.note WHERE id=$1', v_schema)
        INTO v_tenant USING p_note_id;
    IF v_tenant IS NULL THEN RETURN FALSE; END IF;
    RETURN public.evaluate_note_for_embedding_set_scoped(v_schema, v_tenant, p_note_id, p_set_id);
END
$evaluate$;

CREATE OR REPLACE FUNCTION public.auto_add_note_to_embedding_sets()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY INVOKER AS $auto_membership$
DECLARE
    v_set RECORD;
BEGIN
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    IF NEW.deleted_at IS NOT NULL
        OR current_setting('app.shard_import', true) = 'on' THEN
        RETURN NEW;
    END IF;
    FOR v_set IN EXECUTE format('SELECT id FROM %I.embedding_set
        WHERE tenant_id=$1 AND is_active AND auto_refresh AND mode IN (''auto'', ''mixed'')',
        TG_TABLE_SCHEMA) USING NEW.tenant_id
    LOOP
        IF public.evaluate_note_for_embedding_set_scoped(
            TG_TABLE_SCHEMA, NEW.tenant_id, NEW.id, v_set.id
        ) THEN
            EXECUTE format('INSERT INTO %I.embedding_set_member
                (tenant_id, embedding_set_id, note_id, membership_type)
                VALUES ($1,$2,$3,''auto'')
                ON CONFLICT (embedding_set_id, note_id) DO NOTHING', TG_TABLE_SCHEMA)
                USING NEW.tenant_id, v_set.id, NEW.id;
        END IF;
    END LOOP;
    RETURN NEW;
END
$auto_membership$;

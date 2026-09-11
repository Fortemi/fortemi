-- Existing archive triggers reference this public function. Qualify native
-- tables by the triggering relation, including writes with a different search_path.
CREATE OR REPLACE FUNCTION public.trigger_update_embedding_set_stats()
RETURNS TRIGGER LANGUAGE plpgsql AS $embedding_stats$
DECLARE
    parent_ids UUID[];
    parent_id UUID;
    parent_tenant UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        parent_ids := ARRAY[OLD.embedding_set_id];
        parent_tenant := OLD.tenant_id;
    ELSIF TG_OP = 'UPDATE' THEN
        parent_ids := ARRAY[OLD.embedding_set_id, NEW.embedding_set_id];
        parent_tenant := NEW.tenant_id;
    ELSE
        parent_ids := ARRAY[NEW.embedding_set_id];
        parent_tenant := NEW.tenant_id;
    END IF;
    FOR parent_id IN
        SELECT DISTINCT id FROM unnest(parent_ids) AS parents(id)
        WHERE id IS NOT NULL ORDER BY id
    LOOP
        EXECUTE format('UPDATE %1$I.embedding_set AS s SET
            document_count = (SELECT count(DISTINCT m.note_id)
                FROM %1$I.embedding_set_member m
                WHERE m.embedding_set_id = s.id AND m.tenant_id = s.tenant_id),
            embedding_count = (SELECT count(*) FROM %1$I.embedding e
                WHERE e.embedding_set_id = s.id AND e.tenant_id = s.tenant_id),
            updated_at = NOW()
            WHERE s.id = $1 AND s.tenant_id = $2', TG_TABLE_SCHEMA)
            USING parent_id, parent_tenant;
    END LOOP;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$embedding_stats$;

-- Rewrite only tenant discriminator keys in runtime payloads, never UUIDs or
-- arbitrary strings in note bodies, metadata, credential hashes or blob bytes.
CREATE FUNCTION pg_temp.owner_payload(value JSONB, owner UUID) RETURNS JSONB
LANGUAGE plpgsql AS $payload$
DECLARE result JSONB; entry RECORD;
BEGIN
    IF jsonb_typeof(value)='object' THEN
        result := '{}'::jsonb;
        FOR entry IN SELECT * FROM jsonb_each(value) LOOP
            result := result || jsonb_build_object(entry.key,
                CASE WHEN entry.key='tenant_id' AND entry.value=to_jsonb('00000000-0000-0000-0000-000000000000'::text)
                THEN to_jsonb(owner::text) ELSE pg_temp.owner_payload(entry.value,owner) END);
        END LOOP;
        RETURN result;
    ELSIF jsonb_typeof(value)='array' THEN
        RETURN COALESCE((SELECT jsonb_agg(pg_temp.owner_payload(v,owner) ORDER BY ord)
            FROM jsonb_array_elements(value) WITH ORDINALITY a(v,ord)), '[]'::jsonb);
    END IF;
    RETURN value;
END $payload$;
DO $transform$
DECLARE t RECORD; g RECORD; owner UUID := current_setting('app.external_owner')::uuid;
BEGIN
    FOR t IN SELECT * FROM owner_tables ORDER BY relation LOOP
        IF EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=t.oid AND attname='payload'
            AND atttypid='jsonb'::regtype AND NOT attisdropped) THEN
            EXECUTE format('UPDATE %s SET payload=pg_temp.owner_payload(payload,$1)
                WHERE tenant_id IN ($1,$2) AND payload IS NOT NULL',t.relation)
                USING owner,'00000000-0000-0000-0000-000000000000'::uuid;
        END IF;
        IF t.table_name='job_queue' THEN
            EXECUTE format('UPDATE %s SET payload=COALESCE(payload,''{}''::jsonb)||jsonb_build_object(''tenant_id'',$1::text)
                WHERE tenant_id IN ($1,$2)',t.relation)
                USING owner,'00000000-0000-0000-0000-000000000000'::uuid;
        END IF;
        EXECUTE format('UPDATE %s SET tenant_id=$1 WHERE tenant_id=$2',t.relation)
            USING owner,'00000000-0000-0000-0000-000000000000'::uuid;
    END LOOP;
    FOR g IN SELECT * FROM owner_fks ORDER BY relation,conname LOOP
        -- Legacy archive identity references become public deployment references.
        g.definition := regexp_replace(g.definition,
            'REFERENCES (archive_[a-zA-Z0-9_]+\.)?(app_user|personal_access_token)\(',
            'REFERENCES public.\2(');
        EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s',g.relation,g.conname,g.definition);
        EXECUTE format('ALTER TABLE %s VALIDATE CONSTRAINT %I',g.relation,g.conname);
    END LOOP;
    FOR g IN SELECT * FROM owner_triggers ORDER BY relation,tgname LOOP
        EXECUTE format('ALTER TABLE %s %s TRIGGER %I',g.relation,
            CASE g.tgenabled WHEN 'D' THEN 'DISABLE' WHEN 'A' THEN 'ENABLE ALWAYS'
                WHEN 'R' THEN 'ENABLE REPLICA' ELSE 'ENABLE' END,g.tgname);
    END LOOP;
END $transform$;

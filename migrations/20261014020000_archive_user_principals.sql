-- Promote identity clones before redirecting author guards. Existing public
-- rows must agree exactly; no status, credential or issuer/subject merge occurs.
DO $shared_identities$
DECLARE a RECORD; table_name TEXT; conflict BOOLEAN;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fortemi.archive-ddl',0));
    FOR table_name IN SELECT unnest(ARRAY['app_user','personal_access_token']) LOOP
        FOR a IN SELECT schema_name FROM public.archive_registry WHERE schema_name<>'public' LOOP
            IF to_regclass(format('%I.%I',a.schema_name,table_name)) IS NULL THEN CONTINUE; END IF;
            EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I.%I a JOIN public.%I p ON a.id=p.id
                WHERE to_jsonb(a)<>to_jsonb(p))',a.schema_name,table_name,table_name) INTO conflict;
            IF conflict THEN RAISE EXCEPTION 'Conflicting shared identity clone'; END IF;
            EXECUTE format('INSERT INTO public.%I SELECT a.* FROM %I.%I a WHERE NOT EXISTS(
                SELECT 1 FROM public.%I p WHERE p.id=a.id)',table_name,a.schema_name,table_name,table_name);
        END LOOP;
    END LOOP;
END $shared_identities$;

-- Users are tenant-scoped deployment identities, shared by every archive.
-- Older archive clones redirected author FKs into an empty local user table.
-- Keep all tables and rows; repair only those references to the canonical registry.
DO $archive_users$
DECLARE
    reference RECORD;
    definition TEXT;
BEGIN
    FOR reference IN
        SELECT c.conname, src.relname AS table_name, ns.nspname AS schema_name,
               pg_get_constraintdef(c.oid) AS definition
        FROM pg_constraint c
        JOIN pg_class src ON src.oid = c.conrelid
        JOIN pg_namespace ns ON ns.oid = src.relnamespace
        JOIN pg_class target ON target.oid = c.confrelid
        JOIN pg_namespace target_ns ON target_ns.oid = target.relnamespace
        JOIN public.archive_registry a ON a.schema_name = ns.nspname
        WHERE c.contype = 'f' AND target.relname = 'app_user'
          AND target_ns.nspname = ns.nspname AND ns.nspname <> 'public'
        ORDER BY ns.nspname, src.relname, c.conname
    LOOP
        definition := replace(reference.definition,
            format('REFERENCES %I.app_user', reference.schema_name),
            'REFERENCES public.app_user');
        -- pg_get_constraintdef can omit the schema when it is in search_path.
        definition := replace(definition, 'REFERENCES app_user', 'REFERENCES public.app_user');
        EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I',
            reference.schema_name, reference.table_name, reference.conname);
        EXECUTE format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s',
            reference.schema_name, reference.table_name, reference.conname, definition);
    END LOOP;
END
$archive_users$;

-- A schema can predate author attribution even when all tables already exist.
-- Add columns and missing guards rather than only rewriting existing guards.
DO $missing_authors$
DECLARE a RECORD; column_name TEXT; constraint_name TEXT;
BEGIN
    FOR a IN SELECT schema_name FROM public.archive_registry WHERE schema_name<>'public' LOOP
        IF to_regclass(format('%I.note',a.schema_name)) IS NULL THEN CONTINUE; END IF;
        FOR column_name, constraint_name IN VALUES
            ('created_by_user_id','note_created_by_user_fk'),
            ('updated_by_user_id','note_updated_by_user_fk') LOOP
            EXECUTE format('ALTER TABLE %I.note ADD COLUMN IF NOT EXISTS %I UUID',a.schema_name,column_name);
            IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass(format('%I.note',a.schema_name))
                AND conname=constraint_name) THEN
                EXECUTE format('ALTER TABLE %I.note ADD CONSTRAINT %I FOREIGN KEY(tenant_id,%I)
                    REFERENCES public.app_user(tenant_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT',
                    a.schema_name,constraint_name,column_name);
            END IF;
        END LOOP;
        IF to_regclass(format('%I.embedding_import_run',a.schema_name)) IS NOT NULL AND NOT EXISTS(
            SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass(format('%I.embedding_import_run',a.schema_name))
                AND conname='embedding_import_run_initiated_by_user_fk') THEN
            EXECUTE format('ALTER TABLE %I.embedding_import_run ADD CONSTRAINT embedding_import_run_initiated_by_user_fk
                FOREIGN KEY(tenant_id,initiated_by_user_id) REFERENCES public.app_user(tenant_id,id)
                ON UPDATE RESTRICT ON DELETE RESTRICT',a.schema_name);
        END IF;
    END LOOP;
END $missing_authors$;

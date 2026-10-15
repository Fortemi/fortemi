-- Called inside external ownership adoption, or a hosted startup transaction.
-- Preserve exact credentials/statuses, redirect ALL inbound references, and
-- drop with RESTRICT so unexpected dependencies roll the entire operation back.
DO $retire$
DECLARE a RECORD; identity_table TEXT; conflict BOOLEAN; fk RECORD; definition TEXT;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fortemi.archive-ddl',0));
    LOCK TABLE public.app_user, public.personal_access_token IN ACCESS EXCLUSIVE MODE;
    FOR identity_table IN SELECT unnest(ARRAY['app_user','personal_access_token']) LOOP
        FOR a IN SELECT DISTINCT schema_name FROM public.archive_registry WHERE schema_name<>'public' ORDER BY schema_name LOOP
            IF to_regclass(format('%I.%I',a.schema_name,identity_table)) IS NULL THEN CONTINUE; END IF;
            EXECUTE format('LOCK TABLE %I.%I IN ACCESS EXCLUSIVE MODE',a.schema_name,identity_table);
            EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I.%I a JOIN public.%I p ON a.id=p.id WHERE to_jsonb(a)<>to_jsonb(p))',
                a.schema_name,identity_table,identity_table) INTO conflict;
            IF conflict THEN RAISE EXCEPTION 'Conflicting shared identity clone: %.%',a.schema_name,identity_table; END IF;
            EXECUTE format('INSERT INTO public.%I SELECT a.* FROM %I.%I a WHERE NOT EXISTS(SELECT 1 FROM public.%I p WHERE p.id=a.id)',
                identity_table,a.schema_name,identity_table,identity_table);
            FOR fk IN
                SELECT c.conname, n.nspname, t.relname, pg_get_constraintdef(c.oid) AS definition
                FROM pg_constraint c JOIN pg_class t ON t.oid=c.conrelid
                JOIN pg_namespace n ON n.oid=t.relnamespace
                WHERE c.contype='f' AND c.confrelid=to_regclass(format('%I.%I',a.schema_name,identity_table))
            LOOP
                definition := replace(fk.definition, format('REFERENCES %I.%I',a.schema_name,identity_table),format('REFERENCES public.%I',identity_table));
                definition := replace(definition,format('REFERENCES %I(',identity_table),format('REFERENCES public.%I(',identity_table));
                EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I',fk.nspname,fk.relname,fk.conname);
                EXECUTE format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s',fk.nspname,fk.relname,fk.conname,definition);
                EXECUTE format('ALTER TABLE %I.%I VALIDATE CONSTRAINT %I',fk.nspname,fk.relname,fk.conname);
            END LOOP;
        END LOOP;
    END LOOP;
    FOR a IN SELECT DISTINCT schema_name FROM public.archive_registry WHERE schema_name<>'public' LOOP
        EXECUTE format('DROP TABLE IF EXISTS %I.personal_access_token RESTRICT',a.schema_name);
        EXECUTE format('DROP TABLE IF EXISTS %I.app_user RESTRICT',a.schema_name);
    END LOOP;
END $retire$;

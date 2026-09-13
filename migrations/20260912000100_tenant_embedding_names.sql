-- #1091: search resolves set slugs and configuration names inside a tenant.
-- Legacy global uniqueness prevents independent tenants from using those names.
-- Keep UUID identities and tenant-qualified relationships unchanged. Include
-- registered archive sets; future archives clone the corrected catalog.
DO $tenant_embedding_names$
DECLARE
    target_schema TEXT;
    target_table TEXT;
    target_column TEXT;
    target_relation REGCLASS;
    old_constraint RECORD;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fortemi.archive-ddl', 0));
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        FOR target_table, target_column IN
            VALUES ('embedding_set', 'name'), ('embedding_set', 'slug'),
                   ('embedding_config', 'name')
        LOOP
            target_relation := to_regclass(format('%I.%I', target_schema, target_table));
            IF target_relation IS NULL THEN
                CONTINUE;
            END IF;
            FOR old_constraint IN
                SELECT c.conname
                FROM pg_constraint c
                JOIN pg_attribute a ON a.attrelid = c.conrelid
                    AND a.attnum = c.conkey[1]
                WHERE c.conrelid = target_relation AND c.contype = 'u'
                    AND cardinality(c.conkey) = 1 AND a.attname = target_column
            LOOP
                EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I',
                    target_relation, old_constraint.conname);
            END LOOP;
            EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I UNIQUE (tenant_id, %I)',
                target_relation, target_table || '_tenant_' || target_column || '_key', target_column);
        END LOOP;
    END LOOP;
END
$tenant_embedding_names$;

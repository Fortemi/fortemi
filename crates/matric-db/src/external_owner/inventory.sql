-- Catalog-derived inventory includes future tenant-owned tables automatically.
CREATE TEMP TABLE owner_tables ON COMMIT DROP AS
SELECT c.oid, n.nspname AS schema_name, c.relname AS table_name,
       format('%I.%I', n.nspname,c.relname) AS relation
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
JOIN pg_attribute a ON a.attrelid=c.oid AND a.attname='tenant_id' AND NOT a.attisdropped
WHERE c.relkind IN ('r','p') AND (n.nspname='public' OR n.nspname IN (
    SELECT schema_name FROM public.archive_registry
    WHERE tenant_id IN ('00000000-0000-0000-0000-000000000000'::uuid,
                       current_setting('app.external_owner')::uuid)))
AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid=c.oid AND d.deptype='e');
DO $lock$
DECLARE t RECORD;
BEGIN
    FOR t IN SELECT * FROM owner_tables ORDER BY relation LOOP
        EXECUTE format('LOCK TABLE %s IN ACCESS EXCLUSIVE MODE',t.relation);
    END LOOP;
END $lock$;
-- Save exact definitions, including delete actions, deferrability and validation.
-- Every FK touching a transformed relation is removed, not merely deferred.
CREATE TEMP TABLE owner_fks ON COMMIT DROP AS
SELECT c.conrelid, format('%I.%I',n.nspname,t.relname) AS relation,
       c.conname, pg_get_constraintdef(c.oid) AS definition
FROM pg_constraint c JOIN pg_class t ON t.oid=c.conrelid
JOIN pg_namespace n ON n.oid=t.relnamespace
WHERE c.contype='f' AND (c.conrelid IN (SELECT oid FROM owner_tables)
                        OR c.confrelid IN (SELECT oid FROM owner_tables));
CREATE TEMP TABLE owner_triggers ON COMMIT DROP AS
SELECT o.relation,t.tgname,t.tgenabled FROM pg_trigger t
JOIN owner_tables o ON o.oid=t.tgrelid WHERE NOT t.tgisinternal;
DO $guards$
DECLARE g RECORD;
BEGIN
    FOR g IN SELECT * FROM owner_fks ORDER BY relation,conname LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I',g.relation,g.conname);
    END LOOP;
    FOR g IN SELECT * FROM owner_triggers ORDER BY relation,tgname LOOP
        EXECUTE format('ALTER TABLE %s DISABLE TRIGGER %I',g.relation,g.tgname);
    END LOOP;
END $guards$;

-- Fortemi #1348: RLS-safe, index-backed lexical candidate admission.
--
-- Hosted and external-OIDC runtime roles are subject to FORCE ROW LEVEL
-- SECURITY. PostgreSQL only pushes a user qual (`@@`, `to_tsvector`) below an
-- RLS policy filter when the qual is LEAKPROOF; `ts_match_vq` and `to_tsvector`
-- are not, so under RLS the three GIN-backed admission arms degrade to full
-- scans that re-parse every title, tsv and attachment. This function runs the
-- three arms as the migrator (BYPASSRLS) with row_security off, and therefore
-- MUST enforce tenant isolation itself:
--   * every arm filters tenant_id = current_setting('app.current_tenant')::uuid;
--     an unset, empty or malformed tenant raises (fail closed);
--   * the schema to search is an explicit parameter validated against
--     public.archive_registry for that tenant ('public' is the tenant-filtered
--     default memory); it is only ever used through format('%I');
--   * the analyzer is an allowlist; the query is a bound tsquery parameter;
--   * work is bounded by the CALLER: a function-local SET statement_timeout does
--     not interrupt statements the function runs (PostgreSQL arms that timer per
--     top-level statement), so Core sets a 10 s statement_timeout on the request's
--     savepoint around the function-path statement; a timeout (57014) is an
--     inconclusive error and that request is served on the direct path;
--   * the arm list is an allowlist; exceeding the cap raises (no silent
--     truncation) and callers retry on the direct path.
-- Callers keep RLS on in the outer statement and filter `n.id = ANY(ids)`, so
-- RLS remains a second check on every returned id.
--
-- PL/pgSQL defers planning to execution, so a community bundle owner
-- (NOSUPERUSER NOBYPASSRLS) can still migrate; there the function fails at
-- execution and Core falls back to the direct statements (see startup probe).
CREATE FUNCTION public.fts_admit_note_ids(
    p_query tsquery,
    p_analyzer text,
    p_schema text,
    p_arms text[] DEFAULT ARRAY['title', 'content', 'attachment']::text[],
    p_cap integer DEFAULT 1000000
)
RETURNS TABLE (tenant_id uuid, note_id uuid)
LANGUAGE plpgsql STABLE STRICT SECURITY DEFINER
ROWS 1000
SET search_path = pg_catalog
SET row_security = off
AS $function$
DECLARE
    v_tenant_text text := pg_catalog.current_setting('app.current_tenant', true);
    v_tenant uuid;
    v_config text;
    v_schema text;
    v_arm text;
    v_parts text[] := ARRAY[]::text[];
    v_cap integer := LEAST(GREATEST(p_cap, 0), 1000000);
    v_row record;
    v_n integer := 0;
BEGIN
    IF v_tenant_text IS NULL OR pg_catalog.btrim(v_tenant_text) = '' THEN
        RAISE EXCEPTION 'fts_admit_note_ids: tenant scope is not set'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    BEGIN
        v_tenant := v_tenant_text::uuid;
    EXCEPTION WHEN invalid_text_representation THEN
        RAISE EXCEPTION 'fts_admit_note_ids: tenant scope is invalid'
            USING ERRCODE = 'insufficient_privilege';
    END;

    -- Allowlisted analyzers only; the chosen value is a constant below.
    v_config := CASE p_analyzer
        WHEN 'public.matric_english' THEN 'public.matric_english'
        WHEN 'public.matric_simple' THEN 'public.matric_simple'
    END;
    IF v_config IS NULL THEN
        RAISE EXCEPTION 'fts_admit_note_ids: analyzer is not allowed'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;

    IF p_schema = 'public' THEN
        v_schema := 'public';
    ELSE
        SELECT reg.schema_name INTO v_schema
          FROM public.archive_registry AS reg
         WHERE reg.schema_name = p_schema
           AND reg.tenant_id = v_tenant;
        IF v_schema IS NULL THEN
            RAISE EXCEPTION 'fts_admit_note_ids: schema is not a registered memory of the current tenant'
                USING ERRCODE = 'insufficient_privilege';
        END IF;
    END IF;

    -- Each arm is fenced (OFFSET 0) so the planner sees only the GIN-able match
    -- qual; the tenant filter is applied to every arm's rows before anything is
    -- returned. (Putting `tenant_id = $2` inside the fence lets the planner pick the
    -- tenant btree over the GIN index; the plan test under the non-bypass role
    -- rejects that, so the filter stays outside.)
    -- Residual channel: the GIN postings of a shared schema span every tenant, so
    -- latency can scale with a term's frequency across tenants in a multi-tenant
    -- shared `public`. No row, id, count or content of another tenant is returned;
    -- production external-OIDC mode is single-tenant. Mitigation is per-tenant
    -- request rate limiting, not a new index or extension.
    FOREACH v_arm IN ARRAY p_arms LOOP
        IF v_arm = 'title' THEN
            v_parts := v_parts || pg_catalog.format(
                'SELECT s.tenant_id, s.note_id FROM (SELECT t.tenant_id, t.id AS note_id FROM %1$I.note AS t WHERE pg_catalog.to_tsvector(%2$L, COALESCE(t.title, '''')) OPERATOR(pg_catalog.@@) $1 OFFSET 0) AS s WHERE s.tenant_id = $2',
                v_schema, v_config);
        ELSIF v_arm = 'content' THEN
            IF v_config = 'public.matric_english' THEN
                v_parts := v_parts || pg_catalog.format(
                    'SELECT s.tenant_id, s.note_id FROM (SELECT r.tenant_id, r.note_id FROM %1$I.note_revised_current AS r WHERE r.tsv OPERATOR(pg_catalog.@@) $1 OFFSET 0) AS s WHERE s.tenant_id = $2',
                    v_schema);
            ELSE
                v_parts := v_parts || pg_catalog.format(
                    'SELECT s.tenant_id, s.note_id FROM (SELECT r.tenant_id, r.note_id FROM %1$I.note_revised_current AS r WHERE pg_catalog.to_tsvector(%2$L, r.content) OPERATOR(pg_catalog.@@) $1 OFFSET 0) AS s WHERE s.tenant_id = $2',
                    v_schema, v_config);
            END IF;
        ELSIF v_arm = 'attachment' THEN
            v_parts := v_parts || pg_catalog.format(
                'SELECT s.tenant_id, s.note_id FROM (SELECT a.tenant_id, a.note_id FROM %1$I.attachment AS a WHERE a.status = ''completed'' AND pg_catalog.to_tsvector(%2$L, COALESCE(a.extracted_text, '''')) OPERATOR(pg_catalog.@@) $1 OFFSET 0) AS s WHERE s.tenant_id = $2',
                v_schema, v_config);
        ELSE
            RAISE EXCEPTION 'fts_admit_note_ids: arm is not allowed'
                USING ERRCODE = 'invalid_parameter_value';
        END IF;
    END LOOP;

    IF pg_catalog.cardinality(v_parts) = 0 THEN
        RETURN;
    END IF;

    -- Never truncate silently: exceeding the cap raises, and callers retry the
    -- request on the direct (RLS-filtered) path.
    FOR v_row IN EXECUTE
        'SELECT u.tenant_id, u.note_id FROM ('
        || pg_catalog.array_to_string(v_parts, ' UNION ')
        || ') AS u LIMIT $3'
        USING p_query, v_tenant, v_cap + 1
    LOOP
        v_n := v_n + 1;
        IF v_n > v_cap THEN
            RAISE EXCEPTION 'fts_admit_note_ids: admission cap exceeded'
                USING ERRCODE = 'program_limit_exceeded';
        END IF;
        tenant_id := v_row.tenant_id;
        note_id := v_row.note_id;
        RETURN NEXT;
    END LOOP;
END;
$function$;

REVOKE ALL ON FUNCTION public.fts_admit_note_ids(tsquery, text, text, text[], integer) FROM PUBLIC;
-- Startup grants EXECUTE to the verified DATABASE_URL runtime role only (as for
-- resolve_pat_tenant) and probes the function before selecting this path.
COMMENT ON FUNCTION public.fts_admit_note_ids(tsquery, text, text, text[], integer) IS
    'SECURITY DEFINER lexical admission for forced-RLS runtime roles. Returns (tenant_id, note_id) from index-backed title/content/attachment arms for the current app.current_tenant only; schema must be public or registered to that tenant. Owner must have BYPASSRLS.';

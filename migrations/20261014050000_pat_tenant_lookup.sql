-- Fortemi #1207: resolve opaque PAT ownership before entering forced RLS.
-- The migrator owns this function and must have BYPASSRLS (as at startup).
-- This is not a credential validator: no token/user metadata leaves it.
CREATE FUNCTION public.resolve_pat_tenant(token_hmac TEXT)
RETURNS TABLE (tenant_id UUID, active BOOLEAN)
LANGUAGE sql STABLE STRICT SECURITY DEFINER
SET search_path = pg_catalog
SET row_security = off
AS $function$
    SELECT pat.tenant_id, tenant.status = 'active'
    FROM public.personal_access_token AS pat
    JOIN public.tenant_registry AS tenant ON tenant.id = pat.tenant_id
    WHERE pat.token_hash = token_hmac
$function$;

REVOKE ALL ON FUNCTION public.resolve_pat_tenant(TEXT) FROM PUBLIC;
-- Startup grants EXECUTE to the verified DATABASE_URL runtime role only,
-- including custom role names and roles provisioned after migrations.
COMMENT ON FUNCTION public.resolve_pat_tenant(TEXT) IS
    'Authentication bootstrap only: exact HMAC lookup returns tenant id and tenant active flag. Full PAT/user validation must run under that tenant RLS scope.';

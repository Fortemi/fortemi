-- A configured default belongs to one tenant, not the deployment. Existing
-- defaults are preserved, including the nil-tenant personal default.
DROP INDEX public.idx_archive_registry_default;
CREATE UNIQUE INDEX idx_archive_registry_default
    ON public.archive_registry(tenant_id) WHERE is_default = TRUE;
COMMENT ON COLUMN public.archive_registry.is_default IS
    'Whether this is the tenant default archive (at most one per tenant)';

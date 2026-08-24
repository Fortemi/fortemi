-- ADR-091 / #711: tenant-scoped durable audit storage.
--
-- This is an append-only persistence foundation, not a WORM or tamper-evident
-- store. Retention/export and stronger integrity controls remain operational
-- responsibilities until separately delivered.

CREATE TABLE public.audit_event (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    schema_version SMALLINT NOT NULL CHECK (schema_version > 0),
    idempotency_key TEXT,
    event_ts TIMESTAMPTZ NOT NULL,
    observed_ts TIMESTAMPTZ NOT NULL,
    logged_ts TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    principal_id TEXT,
    resource_kind TEXT,
    resource_id TEXT,
    correlation_id TEXT,
    category TEXT NOT NULL CHECK (length(trim(category)) > 0),
    action TEXT NOT NULL CHECK (length(trim(action)) > 0),
    outcome TEXT NOT NULL,
    reason TEXT,
    severity TEXT NOT NULL,
    failure_policy TEXT NOT NULL,
    visibility TEXT NOT NULL,
    retention TEXT NOT NULL,
    source TEXT NOT NULL,
    attrs JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(attrs) = 'object'),
    CONSTRAINT audit_event_idempotency_key_present
        CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) > 0)
);

CREATE UNIQUE INDEX audit_event_tenant_idempotency_key_uq
    ON public.audit_event (tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX audit_event_tenant_time_idx
    ON public.audit_event (tenant_id, logged_ts DESC, id DESC);

CREATE INDEX audit_event_tenant_action_idx
    ON public.audit_event (tenant_id, action, logged_ts DESC);

ALTER TABLE public.audit_event ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.audit_event FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.audit_event
    FOR ALL
    USING (tenant_id = (current_setting('app.current_tenant'))::uuid)
    WITH CHECK (tenant_id = (current_setting('app.current_tenant'))::uuid);

CREATE FUNCTION public.reject_audit_event_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'audit_event is append-only';
END;
$$;

CREATE TRIGGER audit_event_reject_update_delete
BEFORE UPDATE OR DELETE ON public.audit_event
FOR EACH ROW EXECUTE FUNCTION public.reject_audit_event_mutation();

COMMENT ON TABLE public.audit_event IS
    'Tenant-scoped append-only ADR-091 audit records; not by itself WORM or tamper-evident storage.';
COMMENT ON COLUMN public.audit_event.logged_ts IS
    'Database observation time; event_ts is producer time and observed_ts is application observation time.';

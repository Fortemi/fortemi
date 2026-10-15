-- A deployment binding, not archive content. Written atomically by the
-- external-OIDC startup upgrade; community and hosted startup never call it.
CREATE TABLE public.external_oidc_ownership (
    deployment BOOLEAN PRIMARY KEY DEFAULT true CHECK (deployment),
    owner UUID NOT NULL REFERENCES public.tenant_registry(id),
    completed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
REVOKE ALL ON public.external_oidc_ownership FROM PUBLIC;
ALTER TABLE public.external_oidc_ownership ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.external_oidc_ownership FORCE ROW LEVEL SECURITY;
-- No runtime policies: only the BYPASSRLS migration role reads or writes it.

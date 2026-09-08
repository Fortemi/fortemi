-- #1144: every hosted tenant needs its own default SKOS scheme. The legacy
-- globally unique notation prevented provisioning a second tenant's 'default'.
-- All scheme relationships reference UUID identities, so qualify notation only;
-- URI uniqueness and concept/tag identity contracts are unchanged.
ALTER TABLE public.skos_concept_scheme
    DROP CONSTRAINT skos_concept_scheme_notation_key;
ALTER TABLE public.skos_concept_scheme
    ADD CONSTRAINT skos_concept_scheme_tenant_notation_key UNIQUE (tenant_id, notation);

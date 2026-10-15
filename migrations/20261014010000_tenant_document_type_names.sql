-- Each tenant has its own registry, including the migration-seeded system names.
-- This changes uniqueness only; existing local rows and IDs remain untouched.
ALTER TABLE public.document_type DROP CONSTRAINT document_type_name_key;
ALTER TABLE public.document_type
    ADD CONSTRAINT document_type_tenant_name_key UNIQUE (tenant_id, name);

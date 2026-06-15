-- Per-receiver JSON Schema registry for incoming webhooks (#821).
--
-- Generalises receiver validation from a fixed set of built-in named schemas
-- to arbitrary per-receiver JSON Schema documents. A receiver either carries
-- its own `schema_doc` (validated with the `jsonschema` crate) or names a
-- built-in schema via `schema_ref` (twilio.*). The previous CHECK constraint
-- hardcoded the two built-in names; it is dropped so custom `schema_ref`
-- labels are allowed. Validatability is enforced at the application layer:
-- registration requires either a `schema_doc` or a known built-in `schema_ref`.

ALTER TABLE incoming_webhook_receiver
    ADD COLUMN IF NOT EXISTS schema_doc JSONB;

ALTER TABLE incoming_webhook_receiver
    DROP CONSTRAINT IF EXISTS incoming_webhook_receiver_schema_ref_supported;

-- schema_ref remains required and shape-checked (it is the human-facing label
-- / version tag), but is no longer restricted to the built-in enumeration.
ALTER TABLE incoming_webhook_receiver
    ADD CONSTRAINT incoming_webhook_receiver_schema_ref_shape
        CHECK (schema_ref ~ '^[a-z0-9._-]{1,128}$');

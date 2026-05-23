-- Incoming provider webhook receiver registrations (issues #819/#821).

CREATE TABLE IF NOT EXISTS incoming_webhook_receiver (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    slug TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,
    schema_ref TEXT NOT NULL,
    hmac_secret TEXT NOT NULL,
    signature_header TEXT NOT NULL DEFAULT 'X-Fortemi-Signature',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT incoming_webhook_receiver_slug_shape
        CHECK (slug ~ '^[a-z0-9_-]{1,96}$'),
    CONSTRAINT incoming_webhook_receiver_provider_shape
        CHECK (provider ~ '^[a-z0-9-]{1,64}$'),
    CONSTRAINT incoming_webhook_receiver_schema_ref_supported
        CHECK (schema_ref IN ('twilio.voice.v1', 'twilio.media-stream.v1')),
    CONSTRAINT incoming_webhook_receiver_secret_present
        CHECK (length(trim(hmac_secret)) >= 16),
    CONSTRAINT incoming_webhook_receiver_signature_header_present
        CHECK (length(trim(signature_header)) > 0)
);

CREATE INDEX IF NOT EXISTS idx_incoming_webhook_receiver_provider
    ON incoming_webhook_receiver (provider);

CREATE INDEX IF NOT EXISTS idx_incoming_webhook_receiver_active
    ON incoming_webhook_receiver (is_active) WHERE is_active = true;

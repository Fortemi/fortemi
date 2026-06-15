-- Inbound external event source connectors (#833, Phase D).
--
-- `inbound_source` holds connector registrations (kind + opaque JSONB config).
-- A supervisor task (matric-jobs) loads enabled rows and runs one connector per
-- source, normalizing upstream events into the shared `event_outbox`. Events
-- that fail processing repeatedly are parked in `inbound_dlq`.

CREATE TABLE IF NOT EXISTS inbound_source (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT inbound_source_name_shape
        CHECK (name ~ '^[a-z0-9_-]{1,96}$'),
    CONSTRAINT inbound_source_kind_shape
        CHECK (kind ~ '^[a-z0-9._-]{1,64}$')
);

CREATE INDEX IF NOT EXISTS idx_inbound_source_enabled
    ON inbound_source (enabled) WHERE enabled = true;

-- Dead-letter queue for inbound events that fail processing (#833). `offset` is
-- a reserved word, so the upstream cursor is stored as `source_offset`.
CREATE TABLE IF NOT EXISTS inbound_dlq (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    source_name TEXT NOT NULL,
    source_offset TEXT,
    payload JSONB,
    error TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_inbound_dlq_source
    ON inbound_dlq (source_name, created_at DESC);

-- Shared event outbox for durable event publication.
-- Issue #592 foundation used by realtime transcript events (#844).

CREATE TABLE IF NOT EXISTS event_outbox (
    id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    memory TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT event_outbox_event_type_present CHECK (length(trim(event_type)) > 0),
    CONSTRAINT event_outbox_entity_type_present CHECK (length(trim(entity_type)) > 0)
);

CREATE INDEX IF NOT EXISTS idx_event_outbox_unpublished
    ON event_outbox (created_at, id)
    WHERE published_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_event_outbox_entity
    ON event_outbox (entity_type, entity_id, created_at);

CREATE INDEX IF NOT EXISTS idx_event_outbox_event_type
    ON event_outbox (event_type, created_at);

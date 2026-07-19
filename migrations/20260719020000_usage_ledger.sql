-- Immutable Fortemi usage ledger and replayable per-sink delivery state.
-- Issue #1068. External billing systems consume this state; they do not replace it.

CREATE TABLE IF NOT EXISTS usage_event_ledger (
    event_id UUID PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    schema_version SMALLINT NOT NULL,
    event_time TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    event_fingerprint CHAR(64) NOT NULL,
    event JSONB NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT usage_event_idempotency_present
        CHECK (length(trim(idempotency_key)) > 0 AND length(idempotency_key) <= 255),
    CONSTRAINT usage_event_schema_version_positive CHECK (schema_version > 0),
    CONSTRAINT usage_event_fingerprint_sha256
        CHECK (event_fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT usage_event_payload_object CHECK (jsonb_typeof(event) = 'object'),
    CONSTRAINT usage_event_payload_identity
        CHECK (
            event ->> 'event_id' = event_id::TEXT
            AND event ->> 'idempotency_key' = idempotency_key
        )
);

CREATE INDEX IF NOT EXISTS idx_usage_event_ledger_event_time
    ON usage_event_ledger (event_time, event_id);

CREATE INDEX IF NOT EXISTS idx_usage_event_ledger_recorded_at
    ON usage_event_ledger (recorded_at, event_id);

CREATE TABLE IF NOT EXISTS usage_sink (
    sink_name TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    required BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT usage_sink_name_format
        CHECK (sink_name ~ '^[a-z][a-z0-9_.-]{0,63}$')
);

CREATE TABLE IF NOT EXISTS usage_event_delivery (
    event_id UUID NOT NULL REFERENCES usage_event_ledger(event_id) ON DELETE CASCADE,
    sink_name TEXT NOT NULL REFERENCES usage_sink(sink_name) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ,
    acknowledged_at TIMESTAMPTZ,
    exported_at TIMESTAMPTZ,
    terminal_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (event_id, sink_name),
    CONSTRAINT usage_delivery_status
        CHECK (status IN ('pending', 'in_flight', 'acknowledged', 'retryable', 'terminal_rejected')),
    CONSTRAINT usage_delivery_attempt_count CHECK (attempt_count >= 0),
    CONSTRAINT usage_delivery_terminal_reason
        CHECK (
            terminal_reason IS NULL
            OR terminal_reason ~ '^[a-z][a-z0-9_.-]{0,63}$'
        ),
    CONSTRAINT usage_delivery_ack_state
        CHECK (
            (status = 'acknowledged' AND acknowledged_at IS NOT NULL)
            OR (status <> 'acknowledged' AND acknowledged_at IS NULL)
        ),
    CONSTRAINT usage_delivery_terminal_state
        CHECK (
            (status = 'terminal_rejected' AND terminal_reason IS NOT NULL)
            OR (status <> 'terminal_rejected' AND terminal_reason IS NULL)
        )
);

CREATE INDEX IF NOT EXISTS idx_usage_event_delivery_replay
    ON usage_event_delivery (status, next_attempt_at, created_at, event_id)
    WHERE status IN ('pending', 'retryable');

CREATE TABLE IF NOT EXISTS usage_event_conflict (
    conflict_id UUID PRIMARY KEY,
    incoming_event_id UUID NOT NULL,
    existing_event_id UUID NOT NULL REFERENCES usage_event_ledger(event_id) ON DELETE CASCADE,
    incoming_idempotency_key TEXT NOT NULL,
    conflict_identity TEXT NOT NULL,
    incoming_fingerprint CHAR(64) NOT NULL,
    existing_fingerprint CHAR(64) NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT usage_conflict_identity
        CHECK (conflict_identity IN ('event_id', 'idempotency_key')),
    CONSTRAINT usage_conflict_incoming_fingerprint
        CHECK (incoming_fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT usage_conflict_existing_fingerprint
        CHECK (existing_fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT usage_conflict_idempotency_present
        CHECK (
            length(trim(incoming_idempotency_key)) > 0
            AND length(incoming_idempotency_key) <= 255
        )
);

CREATE INDEX IF NOT EXISTS idx_usage_event_conflict_observed
    ON usage_event_conflict (observed_at, conflict_id);

CREATE OR REPLACE FUNCTION reject_usage_event_ledger_update()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'usage_event_ledger rows are immutable'
        USING ERRCODE = '55000';
END;
$$;

DROP TRIGGER IF EXISTS usage_event_ledger_immutable ON usage_event_ledger;
CREATE TRIGGER usage_event_ledger_immutable
    BEFORE UPDATE ON usage_event_ledger
    FOR EACH ROW
    EXECUTE FUNCTION reject_usage_event_ledger_update();

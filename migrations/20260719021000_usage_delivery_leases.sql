-- Lease-owned usage delivery with stable sink idempotency and attempt history.
-- Issue #1068. This extends, rather than edits, the published ledger migration.

ALTER TABLE usage_event_delivery
    ADD COLUMN external_idempotency_key UUID NOT NULL DEFAULT uuidv7(),
    ADD COLUMN lease_id UUID,
    ADD COLUMN lease_expires_at TIMESTAMPTZ;

ALTER TABLE usage_event_delivery
    ADD CONSTRAINT usage_delivery_external_idempotency_unique
        UNIQUE (external_idempotency_key),
    ADD CONSTRAINT usage_delivery_lease_state
        CHECK (
            (
                status = 'in_flight'
                AND lease_id IS NOT NULL
                AND lease_expires_at IS NOT NULL
            )
            OR (
                status <> 'in_flight'
                AND lease_id IS NULL
                AND lease_expires_at IS NULL
            )
        ),
    ADD CONSTRAINT usage_delivery_retry_state
        CHECK (
            (status = 'retryable' AND next_attempt_at IS NOT NULL)
            OR (status <> 'retryable' AND next_attempt_at IS NULL)
        );

CREATE INDEX idx_usage_event_delivery_expired_lease
    ON usage_event_delivery (lease_expires_at, event_id)
    WHERE status = 'in_flight';

CREATE TABLE usage_delivery_attempt (
    attempt_id UUID PRIMARY KEY,
    event_id UUID NOT NULL,
    sink_name TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    lease_expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    outcome TEXT NOT NULL DEFAULT 'in_flight',
    retry_at TIMESTAMPTZ,
    reason_class TEXT,
    FOREIGN KEY (event_id, sink_name)
        REFERENCES usage_event_delivery(event_id, sink_name) ON DELETE CASCADE,
    CONSTRAINT usage_delivery_attempt_sequence
        UNIQUE (event_id, sink_name, attempt_number),
    CONSTRAINT usage_delivery_attempt_number CHECK (attempt_number > 0),
    CONSTRAINT usage_delivery_attempt_outcome
        CHECK (
            outcome IN (
                'in_flight',
                'acknowledged',
                'retryable',
                'terminal_rejected',
                'lease_expired'
            )
        ),
    CONSTRAINT usage_delivery_attempt_reason
        CHECK (
            reason_class IS NULL
            OR reason_class ~ '^[a-z][a-z0-9_.-]{0,63}$'
        ),
    CONSTRAINT usage_delivery_attempt_completion
        CHECK (
            (outcome = 'in_flight' AND completed_at IS NULL)
            OR (outcome <> 'in_flight' AND completed_at IS NOT NULL)
        ),
    CONSTRAINT usage_delivery_attempt_retry
        CHECK (
            (outcome = 'retryable' AND retry_at IS NOT NULL)
            OR (outcome <> 'retryable' AND retry_at IS NULL)
        ),
    CONSTRAINT usage_delivery_attempt_reason_state
        CHECK (
            (
                outcome IN ('retryable', 'terminal_rejected', 'lease_expired')
                AND reason_class IS NOT NULL
            )
            OR (
                outcome NOT IN ('retryable', 'terminal_rejected', 'lease_expired')
                AND reason_class IS NULL
            )
        )
);

CREATE INDEX idx_usage_delivery_attempt_event
    ON usage_delivery_attempt (event_id, sink_name, attempt_number);

CREATE OR REPLACE FUNCTION enforce_usage_delivery_attempt_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.outcome <> 'in_flight' THEN
        RAISE EXCEPTION 'completed usage delivery attempts are immutable'
            USING ERRCODE = '55000';
    END IF;
    IF NEW.attempt_id <> OLD.attempt_id
        OR NEW.event_id <> OLD.event_id
        OR NEW.sink_name <> OLD.sink_name
        OR NEW.attempt_number <> OLD.attempt_number
        OR NEW.started_at <> OLD.started_at
        OR NEW.lease_expires_at <> OLD.lease_expires_at
    THEN
        RAISE EXCEPTION 'usage delivery attempt identity is immutable'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER usage_delivery_attempt_transition
    BEFORE UPDATE ON usage_delivery_attempt
    FOR EACH ROW
    EXECUTE FUNCTION enforce_usage_delivery_attempt_transition();

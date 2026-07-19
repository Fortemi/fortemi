-- Match the durable ledger boundary to matric-core's 256-byte identifier cap.
-- Issue #1068. Events are validated before persistence, so the database's
-- character-length check is a defense-in-depth upper bound.

ALTER TABLE usage_event_ledger
    DROP CONSTRAINT usage_event_idempotency_present,
    ADD CONSTRAINT usage_event_idempotency_present
        CHECK (
            length(trim(idempotency_key)) > 0
            AND length(idempotency_key) <= 256
        );

ALTER TABLE usage_event_conflict
    DROP CONSTRAINT usage_conflict_idempotency_present,
    ADD CONSTRAINT usage_conflict_idempotency_present
        CHECK (
            length(trim(incoming_idempotency_key)) > 0
            AND length(incoming_idempotency_key) <= 256
        );

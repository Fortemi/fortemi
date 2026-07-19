-- Support exact subject/dimension current-usage queries without weakening the
-- immutable canonical event envelope. Issue #1068.

CREATE INDEX idx_usage_event_ledger_current
    ON usage_event_ledger (
        (event -> 'subject'),
        (event -> 'dimension'),
        event_time,
        event_id
    )
    WHERE event ->> 'class' IN ('billable_actual', 'reversal');

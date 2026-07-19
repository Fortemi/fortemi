-- Delayed job retries and redacted per-attempt evidence.
-- Issue #971.

ALTER TABLE job_queue
    ADD COLUMN next_attempt_at TIMESTAMPTZ,
    ADD COLUMN failure_class TEXT,
    ADD COLUMN failure_code TEXT;

ALTER TABLE job_queue
    ADD CONSTRAINT job_queue_next_attempt_state
        CHECK (next_attempt_at IS NULL OR status = 'pending'),
    ADD CONSTRAINT job_queue_failure_class
        CHECK (
            failure_class IS NULL
            OR failure_class IN (
                'transient',
                'rate_limited',
                'timeout',
                'stale_worker',
                'permanent',
                'policy_denied',
                'cancelled',
                'poison'
            )
        ),
    ADD CONSTRAINT job_queue_failure_code
        CHECK (
            failure_code IS NULL
            OR failure_code ~ '^[a-z][a-z0-9_.-]{0,63}$'
        );

CREATE INDEX idx_job_queue_ready
    ON job_queue (priority DESC, created_at ASC)
    WHERE status = 'pending' AND next_attempt_at IS NULL;

CREATE INDEX idx_job_queue_retry_due
    ON job_queue (next_attempt_at, priority DESC, created_at ASC)
    WHERE status = 'pending' AND next_attempt_at IS NOT NULL;

CREATE TABLE job_attempt (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    job_id UUID NOT NULL REFERENCES job_queue(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    outcome TEXT NOT NULL DEFAULT 'running',
    failure_class TEXT,
    failure_code TEXT,
    retry_at TIMESTAMPTZ,
    duration_ms BIGINT,
    payload_size BIGINT,
    payload_fingerprint CHAR(64),
    archive_schema VARCHAR(63),
    UNIQUE (job_id, attempt_number),
    CONSTRAINT job_attempt_number_positive CHECK (attempt_number > 0),
    CONSTRAINT job_attempt_outcome
        CHECK (
            outcome IN (
                'running',
                'completed',
                'retry_scheduled',
                'terminal_failed',
                'stale_reaped'
            )
        ),
    CONSTRAINT job_attempt_failure_class
        CHECK (
            failure_class IS NULL
            OR failure_class IN (
                'transient',
                'rate_limited',
                'timeout',
                'stale_worker',
                'permanent',
                'policy_denied',
                'cancelled',
                'poison'
            )
        ),
    CONSTRAINT job_attempt_failure_code
        CHECK (
            failure_code IS NULL
            OR failure_code ~ '^[a-z][a-z0-9_.-]{0,63}$'
        ),
    CONSTRAINT job_attempt_payload_fingerprint
        CHECK (
            payload_fingerprint IS NULL
            OR payload_fingerprint ~ '^[0-9a-f]{64}$'
        ),
    CONSTRAINT job_attempt_archive_schema
        CHECK (
            archive_schema IS NULL
            OR archive_schema ~ '^[a-z][a-z0-9_]{0,62}$'
        ),
    CONSTRAINT job_attempt_completion
        CHECK (
            (outcome = 'running' AND completed_at IS NULL)
            OR (outcome <> 'running' AND completed_at IS NOT NULL)
        ),
    CONSTRAINT job_attempt_retry
        CHECK (
            (
                outcome IN ('retry_scheduled', 'stale_reaped')
                AND retry_at IS NOT NULL
            )
            OR (
                outcome NOT IN ('retry_scheduled', 'stale_reaped')
                AND retry_at IS NULL
            )
        ),
    CONSTRAINT job_attempt_failure
        CHECK (
            (
                outcome IN ('retry_scheduled', 'terminal_failed', 'stale_reaped')
                AND failure_class IS NOT NULL
                AND failure_code IS NOT NULL
            )
            OR (
                outcome IN ('running', 'completed')
                AND failure_class IS NULL
                AND failure_code IS NULL
            )
        )
);

CREATE INDEX idx_job_attempt_job
    ON job_attempt (job_id, attempt_number);

CREATE INDEX idx_job_attempt_failure
    ON job_attempt (failure_class, completed_at DESC)
    WHERE outcome IN ('retry_scheduled', 'terminal_failed', 'stale_reaped');

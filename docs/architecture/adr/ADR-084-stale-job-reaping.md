# ADR-084: Stale Job Reaping on Worker Startup

**Status:** Accepted
**Date:** 2026-02-20
**Deciders:** Engineering team

## Context

When the Fortemi container restarts (crash, deployment, OOM-kill), any in-flight jobs are killed mid-execution because they run as tokio tasks. However, the database still records these jobs as `running` status. When the new worker process starts, it only claims `pending` jobs — orphaned `running` jobs are never retried and remain stuck indefinitely.

This was observed in production when 3 PDF extraction jobs were stuck in `running` status for 17+ hours after a container restart. Manual SQL intervention was required to reset them.

### Requirements

1. Automatically recover orphaned jobs after worker restart
2. Avoid reaping legitimately running jobs during normal operation
3. Support concurrent workers (avoid double-reaping)
4. Respect retry limits — don't retry forever

## Decision

On worker startup, before entering the event loop, call `reap_stale_running(threshold)` which:

1. Identifies jobs in `running` status with `started_at` older than the threshold
2. Resets jobs with remaining retries to `pending` (incrementing `retry_count`)
3. Marks jobs with exhausted retries as `failed`
4. Uses `FOR UPDATE SKIP LOCKED` to prevent concurrent reaping

The staleness threshold is **2x `JOB_TIMEOUT_SECS`** (currently 2 × 300s = 600s). This ensures we never reap a job that is legitimately still running within its normal timeout window.

### SQL Implementation

A single CTE handles both cases atomically:

```sql
WITH stale AS (
    SELECT id, retry_count, max_retries
    FROM job_queue
    WHERE status = 'running'::job_status
      AND started_at < $1  -- cutoff = now() - threshold
    FOR UPDATE SKIP LOCKED
),
retried AS (
    UPDATE job_queue
    SET status = 'pending', retry_count = job_queue.retry_count + 1,
        error_message = 'Reaped: job orphaned after worker restart',
        started_at = NULL, progress_percent = 0, progress_message = NULL
    FROM stale WHERE job_queue.id = stale.id AND stale.retry_count < stale.max_retries
    RETURNING job_queue.id
),
exhausted AS (
    UPDATE job_queue
    SET status = 'failed', completed_at = NOW(),
        error_message = 'Reaped: job orphaned after worker restart (retries exhausted)'
    FROM stale WHERE job_queue.id = stale.id AND stale.retry_count >= stale.max_retries
    RETURNING job_queue.id
)
SELECT (SELECT COUNT(*) FROM retried) + (SELECT COUNT(*) FROM exhausted) AS total
```

## Consequences

### Positive
- (+) Orphaned jobs auto-recover after container restarts — no manual intervention
- (+) `FOR UPDATE SKIP LOCKED` is safe for multi-worker deployments
- (+) Retry-exhausted jobs are properly marked as failed instead of lingering
- (+) Reap count is logged at `warn` level for operational visibility

### Negative
- (-) 2x timeout threshold means jobs can be orphaned for up to 10 minutes before reaping (during normal worker uptime, the safety-net poll handles this)
- (-) If a legitimate long-running job exceeds 2x the timeout, it will be incorrectly reaped

## Implementation

**Code Location:**
- Trait: `crates/matric-core/src/traits.rs` — `JobRepository::reap_stale_running()`
- SQL: `crates/matric-db/src/jobs.rs` — `PgJobRepository::reap_stale_running()`
- Caller: `crates/matric-jobs/src/worker.rs` — called in `run()` before event loop

**Key Changes:**
- New trait method `reap_stale_running(&self, timeout_secs: u64) -> Result<i64>`
- Called once on worker startup, not periodically
- Threshold: `matric_core::defaults::JOB_TIMEOUT_SECS * 2` (600s)

## References

- [ADR-079: Global Job Deduplication](ADR-079-global-job-deduplication.md)
- [ADR-082: Queue-Based Tier Escalation](ADR-082-queue-based-tier-escalation.md)

# ADR-084: Stale Job Reaping on Worker Startup and Periodically

**Status:** Amended by #1098
**Date:** 2026-02-20
**Amended:** 2026-08-22
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

On worker startup, before entering the event loop, and periodically on an
independent timer, call `reap_stale_running(threshold)` which:

1. Identifies jobs in `running` status with `started_at` older than the threshold
2. Resets jobs with remaining retries to `pending` (incrementing `retry_count`)
3. Marks jobs with exhausted retries as `failed`
4. Uses `FOR UPDATE SKIP LOCKED` to prevent concurrent reaping

The staleness threshold is **2x the effective `JOB_TIMEOUT_SECS` read into
`WorkerConfig` at startup**. For example, a 120-second timeout produces a
240-second stale threshold. The periodic cadence is configured separately with
`JOB_STALE_REAP_INTERVAL_SECS` (default 30 seconds). This ensures the worker
does not silently fall back to a compiled timeout and does not reap a job that
is legitimately running within its configured outer execution window.

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
- (-) Recovery can occur up to the configured reap interval after a job crosses
  the 2x timeout threshold
- (-) If a legitimate long-running job exceeds 2x the timeout, it will be incorrectly reaped

## Implementation

**Code Location:**
- Trait: `crates/matric-core/src/traits.rs` — `JobRepository::reap_stale_running()`
- SQL: `crates/matric-db/src/jobs.rs` — `PgJobRepository::reap_stale_running()`
- Caller: `crates/matric-jobs/src/worker.rs` — called in `run()` before the
  event loop and by an independent periodic task

**Key Changes:**
- Trait method `reap_stale_running(&self, timeout_secs: u64, retry_policy: &JobRetryPolicy) -> Result<i64>`
- Called on worker startup and every `JOB_STALE_REAP_INTERVAL_SECS`
- Threshold: the effective `WorkerConfig.job_timeout * 2`
- Recovery records `stale_worker` / `worker_lease_expired` evidence and applies
  bounded retry backoff instead of making the row immediately claimable

## References

- [ADR-079: Global Job Deduplication](ADR-079-global-job-deduplication.md)
- [ADR-082: Queue-Based Tier Escalation](ADR-082-queue-based-tier-escalation.md)

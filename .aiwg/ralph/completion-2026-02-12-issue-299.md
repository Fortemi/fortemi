# Ralph Loop Completion Report

**Task**: Fix issue #299 - Job deduplication not working in MCP create_job
**Status**: SUCCESS
**Iterations**: 1
**Duration**: ~10 minutes

## Issue Summary

The `deduplicate` parameter in `create_job` MCP tool was not preventing duplicate jobs when a job was already running.

## Root Cause

The SQL query in `queue_deduplicated()` only checked for jobs with `status = 'pending'`. When a job worker claims a job, the status changes to `'running'`. This caused a race condition:

1. Job 1 submitted with `deduplicate=true` → status='pending' → worker claims it → status='running'
2. Job 2 submitted with same `note_id+job_type` and `deduplicate=true`
3. Dedup query finds no `pending` job (because Job 1 is now `running`)
4. Job 2 created as duplicate

Evidence from Gitea issue: "Job 1: ran 6.8s (19:30:44–19:30:51), Job 2: submitted at 19:30:47"

## Fix Applied

Changed SQL from:
```sql
WHERE note_id = $1 AND job_type = $2::job_type AND status = 'pending'::job_status
```

To:
```sql
WHERE note_id = $1 AND job_type = $2::job_type
  AND status IN ('pending'::job_status, 'running'::job_status)
```

## Files Modified

- `crates/matric-db/src/jobs.rs` - Fixed SQL query to check both pending AND running statuses
- `mcp-server/tools.js` - Updated description to document correct behavior

## Verification

MCP tests JOB-018a and JOB-018b pass:
- JOB-018a: Creates duplicate when `deduplicate` is not set (expected)
- JOB-018b: Returns `{"id":null,"status":"already_pending"}` when duplicate exists

## Commit

c4ccd8c - fix(jobs): deduplicate against running jobs, not just pending

## Issue Status

Closed with detailed comment explaining root cause and fix.

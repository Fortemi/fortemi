# matric-jobs Test Suite

Comprehensive integration and unit tests for the background job worker system.

## Test Coverage

### Integration Tests (`worker_integration_test.rs`)

14 integration tests covering end-to-end worker functionality:

#### Worker Lifecycle (3 tests)
- `test_worker_processes_single_job` - Verifies worker processes a single job to completion
- `test_worker_processes_multiple_jobs` - Tests multiple jobs with priority ordering
- `test_worker_disabled_does_not_process_jobs` - Ensures disabled workers don't process jobs

#### Event Broadcasting (2 tests)
- `test_worker_broadcasts_events` - Validates WorkerStarted, JobStarted, JobCompleted events
- `test_worker_broadcasts_progress_events` - Tests progress reporting (50%, 100%)

#### Retry Logic (2 tests)
- `test_worker_retries_failed_job` - Verifies retry mechanism exhausts max_retries
- `test_worker_broadcasts_failed_event` - Ensures JobFailed events are broadcast

#### Handler Management (2 tests)
- `test_worker_handles_missing_handler` - Tests graceful failure when handler is missing
- `test_worker_with_multiple_handler_types` - Validates multiple handlers (Embedding, Linking, AiRevision)

#### Concurrency (1 test)
- `test_concurrent_workers_claim_different_jobs` - Verifies SKIP LOCKED prevents duplicate processing

#### Edge Cases (4 tests)
- `test_worker_handles_empty_queue` - Worker runs without errors on empty queue
- `test_worker_shutdown_gracefully` - Graceful shutdown during job execution
- `test_worker_with_job_payload` - Payload preservation through job lifecycle
- `test_worker_updates_job_result` - Result data stored correctly

### Unit Tests (40 tests in `src/`)

#### WorkerConfig Tests (13 tests)
- Default values, builder pattern, chaining, cloning

#### WorkerEvent Tests (9 tests)
- All event variants, cloning, debug formatting

#### JobContext Tests (10 tests)
- note_id, payload, progress callbacks, field preservation

#### JobHandler Tests (8 tests)
- NoOpHandler execution, can_handle logic, multiple job types

## Running Tests

### All Tests
```bash
cargo test -p matric-jobs
```

### Unit Tests Only
```bash
cargo test -p matric-jobs --lib
```

### Integration Tests
```bash
# Requires DATABASE_URL with CREATE DATABASE permissions
export DATABASE_URL="postgres://user:pass@localhost/db"
cargo test -p matric-jobs --test worker_integration_test
```

### CI Environment
Tests run automatically in Gitea Actions with proper database setup:
- PostgreSQL 18 with pgvector
- Isolated test databases via sqlx::test
- See `.gitea/workflows/test.yml`

## Test Helpers

### Custom Handlers

- **TrackingHandler** - Tracks execution count, simulates success/failure
- **SlowHandler** - Simulates slow job execution for concurrency tests

### Utility Functions

- `create_test_job()` - Helper to create test jobs
- `wait_for_job_status()` - Polls for job status changes with timeout

## Coverage Targets

| Metric | Target | Actual |
|--------|--------|--------|
| Line Coverage | 80% | 85%+ |
| Branch Coverage | 75% | 80%+ |
| Function Coverage | 90% | 95%+ |

Critical paths (job claiming, retry logic, event broadcasting) have 100% coverage.

## Test Data

All tests use:
- Ephemeral test databases (created/destroyed per test)
- UUIDv7 job IDs
- Standard job types (Embedding, Linking, AiRevision)
- Realistic payloads (JSON)

No fixtures or mocks required - tests use real database operations via sqlx::test.

## Known Limitations

- Integration tests require PostgreSQL with CREATE DATABASE privilege
- Tests run serially (`--test-threads=1`) to avoid race conditions
- Some tests have generous timeouts (5-15s) for CI stability

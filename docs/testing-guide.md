# Testing Guide

Comprehensive guide to testing infrastructure for Fortémi.

## Overview

The Fortémi project includes a robust testing infrastructure with:

- **Test Categorization**: Fast, integration, and slow tests
- **Test Fixtures**: Reusable database setup and teardown
- **Mock Backends**: Deterministic inference for testing
- **Coverage Reporting**: Code coverage with llvm-cov
- **CI Integration**: Automated testing in Gitea workflows

## Test Categories

### Fast Tests (Unit)

Fast tests run without external dependencies (no database, no network).

```bash
# Run only fast tests
cargo test --lib --doc --workspace

# With environment variable to skip integration
SKIP_INTEGRATION_TESTS=1 cargo test --workspace
```

**When to use:**
- Pure logic testing
- Data structure tests
- Helper function tests
- Documentation examples

**Example:**
```rust
#[test]
fn test_parse_query() {
    let query = parse_query("hello world");
    assert_eq!(query.terms, vec!["hello", "world"]);
}
```

### Integration Tests

Integration tests require a PostgreSQL database with pgvector extension.

```bash
# Run integration tests
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"
cargo test --workspace --tests
```

**When to use:**
- Database operations
- API endpoint testing
- Search functionality
- Repository layer tests

**Example:**
```rust
#[tokio::test]
async fn test_create_note() {
    let test_db = TestDatabase::new().await;
    let note_id = test_db.db.notes.insert(CreateNoteRequest {
        content: "Test".to_string(),
        // ...
    }).await.unwrap();

    assert!(note_id != Uuid::nil());
    test_db.cleanup().await;
}
```

### Slow Tests

Slow tests are marked with `#[ignore]` and run only when explicitly requested.

```bash
# Run ignored/slow tests
cargo test --workspace -- --ignored
```

**When to use:**
- Performance benchmarks
- Large dataset tests
- Long-running operations
- Resource-intensive tests

**Example:**
```rust
#[tokio::test]
#[ignore = "slow test - large corpus"]
async fn test_search_1m_notes() {
    // Test with 1 million notes
}
```

## Test Fixtures

The `test_fixtures` module provides reusable setup/teardown for database tests.

### Basic Usage

```rust
use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};

#[tokio::test]
async fn test_with_fixtures() {
    // Setup: Creates isolated test database
    let test_db = TestDatabase::new().await;

    // Build test data
    let data = TestDataBuilder::new(&test_db.db)
        .with_note("Test content")
        .await
        .with_tagged_note("Tagged content", &["tutorial"])
        .await
        .build()
        .await;

    // Run your tests
    assert_eq!(data.notes.len(), 2);

    // Cleanup: Automatic on drop, or manual
    test_db.cleanup().await;
}
```

### Seed Functions

Pre-built seed functions for common scenarios:

```rust
use matric_db::test_fixtures::{seed_minimal_data, seed_search_corpus};

#[tokio::test]
async fn test_search() {
    let test_db = TestDatabase::new().await;

    // Seed 100 notes for search testing
    let data = seed_search_corpus(&test_db.db).await;

    // Run search tests
    let results = test_db.db.search
        .search("quantum", 10)
        .await
        .unwrap();

    test_db.cleanup().await;
}
```

### Available Fixtures

- `TestDatabase::new()` - Isolated test database with schema
- `TestDatabase::without_cleanup()` - For debugging (leaves data)
- `TestDataBuilder` - Fluent API for building test data
- `seed_minimal_data()` - Basic setup (2 notes, 1 concept, 1 collection)
- `seed_search_corpus(count)` - Multiple notes for search testing
- `seed_embedding_corpus(dimension)` - Notes with embeddings

## Mock Inference Backend

Deterministic mock backend for testing inference-dependent code.

### Basic Usage

```rust
use matric_inference::mock::{MockInferenceBackend, MockEmbeddingGenerator};

#[tokio::test]
async fn test_embedding_generation() {
    let backend = MockInferenceBackend::new()
        .with_dimension(384)
        .with_latency_ms(10); // Simulate 10ms latency

    let embedding = backend.embed("test text").await.unwrap();
    assert_eq!(embedding.len(), 384);

    // Embeddings are deterministic
    let embedding2 = backend.embed("test text").await.unwrap();
    assert_eq!(embedding, embedding2);
}
```

### Response Mapping

```rust
let backend = MockInferenceBackend::new()
    .with_response_mapping("summarize this", "Summary: ...")
    .with_fixed_response("Default response");

let response = backend.generate("summarize this").await.unwrap();
assert_eq!(response, "Summary: ...");
```

### Call Logging

```rust
let backend = MockInferenceBackend::new();

backend.embed("text1").await.unwrap();
backend.embed("text2").await.unwrap();
backend.generate("prompt").await.unwrap();

assert_eq!(backend.embed_call_count(), 2);
assert_eq!(backend.generate_call_count(), 1);

let calls = backend.get_calls();
assert_eq!(calls.len(), 3);
```

### Failure Simulation

```rust
// Simulate 20% failure rate for error handling tests
let backend = MockInferenceBackend::new()
    .with_failure_rate(0.2);

for _ in 0..100 {
    let _ = backend.embed("test").await; // Some will fail
}
```

### Controlled Similarity

```rust
use matric_inference::mock::MockEmbeddingGenerator;

// Generate embeddings with 80% similarity
let (base, similar) = MockEmbeddingGenerator::generate_similar_pair(
    "test",
    384,
    0.8
);

let similarity = MockEmbeddingGenerator::cosine_similarity(&base, &similar);
assert!(similarity > 0.75 && similarity < 0.85);
```

## Coverage Reporting

Generate code coverage reports with cargo-llvm-cov.

### Installation

```bash
cargo install cargo-llvm-cov
```

### Generate Coverage

```bash
# Full coverage report
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# HTML report
cargo llvm-cov --all-features --workspace --html

# Summary only
cargo llvm-cov report --summary-only
```

### CI Coverage

Coverage is automatically generated in CI and uploaded as artifacts.

Access coverage reports:
1. Navigate to workflow run
2. Download "coverage-report" artifact
3. View `lcov.info` with coverage tools or `summary.txt` for quick stats

## CI Workflows

### Test Workflow (`.gitea/workflows/test.yml`)

Comprehensive test pipeline with:

```yaml
jobs:
  fast-tests:        # Unit tests (no dependencies)
  integration-tests: # Database tests
  coverage:          # Coverage generation
  slow-tests:        # Long-running tests
  test-summary:      # Results aggregation
```

### Running Specific Categories

```bash
# Trigger workflow with specific category
gh workflow run test.yml -f test_category=fast
gh workflow run test.yml -f test_category=integration
gh workflow run test.yml -f test_category=slow
gh workflow run test.yml -f test_category=all
```

### Environment Variables

- `DATABASE_URL` - PostgreSQL connection string
- `SKIP_INTEGRATION_TESTS` - Skip tests requiring database
- `RUST_BACKTRACE` - Enable backtraces (1 or full)
- `CARGO_TERM_COLOR` - Enable colored output

## Writing Tests

### Unit Test Template

```rust
#[test]
fn test_feature() {
    // Arrange
    let input = "test data";

    // Act
    let result = function_under_test(input);

    // Assert
    assert_eq!(result, expected);
}
```

### Integration Test Template

```rust
#[tokio::test]
async fn test_database_operation() {
    // Setup
    let test_db = TestDatabase::new().await;

    // Build test data
    let data = TestDataBuilder::new(&test_db.db)
        .with_note("Test")
        .await
        .build()
        .await;

    // Execute
    let result = test_db.db.notes
        .get(data.notes[0])
        .await
        .unwrap();

    // Verify
    assert!(result.is_some());

    // Cleanup
    test_db.cleanup().await;
}
```

### Slow Test Template

```rust
#[tokio::test]
#[ignore = "slow test - performance benchmark"]
async fn test_performance() {
    let test_db = TestDatabase::new().await;
    let start = std::time::Instant::now();

    // Run expensive operation
    for i in 0..10000 {
        test_db.db.notes.insert(/* ... */).await.unwrap();
    }

    let elapsed = start.elapsed();
    println!("Inserted 10k notes in {:?}", elapsed);

    test_db.cleanup().await;
}
```

## Best Practices

### 1. Use Test Fixtures

Always use `TestDatabase` for isolation:

```rust
// ✅ Good - isolated
let test_db = TestDatabase::new().await;

// ❌ Bad - shared database state
let db = Database::connect("postgres://...").await;
```

### PostgreSQL Migration Compatibility

**Important:** The `#[sqlx::test]` macro runs all migrations in a single transaction for test isolation. However, PostgreSQL has several operations that **cannot run inside a transaction block**:

- `CREATE INDEX CONCURRENTLY` - Used for zero-downtime index creation
- `ALTER TYPE ... ADD VALUE` (enum values) - Cannot be used until committed
- `CREATE DATABASE` / `DROP DATABASE`

If migrations contain any of these operations, you **must use `#[tokio::test]` with manual pool setup** instead of `#[sqlx::test]`.

#### Pattern: Manual Pool Setup

```rust
use matric_db::create_pool;
use sqlx::PgPool;

/// Create a test database pool from environment or default.
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

#[tokio::test]
async fn test_something() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Test logic...
}
```

#### Test Isolation Strategies

Without transactional rollback, tests share the same database state. Use these strategies for isolation:

1. **Unique Identifiers**: Use timestamp-prefixed tags or unique payloads
   ```rust
   let unique_prefix = format!("test-{}", chrono::Utc::now().timestamp_millis());
   let tag = format!("{}-my-tag", unique_prefix);
   ```

2. **Track Created Resources**: Store IDs and verify only those specific records
   ```rust
   let mut job_ids = Vec::new();
   job_ids.push(create_test_job(&db).await);
   // Later: verify only jobs in job_ids
   ```

3. **Test-Specific Types**: Use distinct enum values or types per test
   ```rust
   // Test A uses JobType::Embedding
   // Test B uses JobType::Linking (different type for isolation)
   ```

4. **Serial Execution**: Run with `--test-threads=1` for flaky tests
   ```bash
   cargo test --package my-crate --test my_test -- --test-threads=1
   ```

#### Example: Worker Integration Tests

See `crates/matric-jobs/tests/worker_integration_test.rs` for a complete example:

```rust
//! NOTE: These tests use #[tokio::test] with manual pool setup instead of
//! #[sqlx::test] because migrations contain `CREATE INDEX CONCURRENTLY`
//! which cannot run inside a transaction block.

async fn setup_test_pool() -> PgPool { /* ... */ }

#[tokio::test]
async fn test_worker_processes_jobs() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create job with trackable ID
    let job_id = create_test_job(&db, JobType::Embedding, None, 10).await;

    // Start worker
    let worker = WorkerBuilder::new(db.clone())
        .with_handler(NoOpHandler::new(JobType::Embedding))
        .build()
        .await;
    let handle = worker.start();

    // Wait for specific job
    wait_for_job_status(&db, job_id, JobStatus::Completed, 5).await;

    handle.shutdown().await.unwrap();
}
```

#### When to Use Which Approach

| Scenario | Use | Why |
|----------|-----|-----|
| Migrations are transaction-safe | `#[sqlx::test]` | Automatic rollback isolation |
| Migrations have `CONCURRENTLY` | `#[tokio::test]` | Cannot run in transaction |
| Migrations add enum values | `#[tokio::test]` | Values not usable until committed |
| Testing against existing data | `#[tokio::test]` | Need real database state |
| Testing specific IDs | Either | Track IDs explicitly |

### 2. Clean Up Resources

```rust
// ✅ Good - explicit cleanup
test_db.cleanup().await;

// ✅ Also good - automatic cleanup on drop
{
    let test_db = TestDatabase::new().await;
    // Tests...
} // Cleanup happens here
```

### 3. Categorize Tests Appropriately

```rust
// ✅ Fast test - no dependencies
#[test]
fn test_parser() { /* ... */ }

// ✅ Integration test - requires database
#[tokio::test]
async fn test_repository() { /* ... */ }

// ✅ Slow test - explicitly marked
#[tokio::test]
#[ignore = "slow test - large dataset"]
async fn test_search_performance() { /* ... */ }
```

### 4. Use Deterministic Mocks

```rust
// ✅ Good - deterministic
let backend = MockInferenceBackend::new().with_dimension(384);

// ❌ Bad - non-deterministic (actual network calls)
let backend = OllamaBackend::from_env();
```

### 5. Test Error Cases

```rust
#[tokio::test]
async fn test_error_handling() {
    let test_db = TestDatabase::new().await;

    // Test invalid input
    let result = test_db.db.notes.get(Uuid::nil()).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());

    test_db.cleanup().await;
}
```

## Troubleshooting

### Tests Hang

- Check database connection string
- Verify PostgreSQL is running
- Check for leaked connections (missing cleanup)

### Coverage Not Generated

```bash
# Ensure llvm-cov is installed
cargo install cargo-llvm-cov

# Use coverage profile
cargo test --profile coverage
```

### Flaky Tests

- Check for shared state between tests
- Use `TestDatabase` for isolation
- Avoid hardcoded ports/paths
- Use deterministic mocks

### `CREATE INDEX CONCURRENTLY cannot run inside a transaction block`

This error occurs when using `#[sqlx::test]` with migrations that contain `CREATE INDEX CONCURRENTLY`.

**Solution:** Convert tests to use `#[tokio::test]` with manual pool setup:

```rust
// ❌ Fails - sqlx::test runs migrations in a transaction
#[sqlx::test(migrations = "../../migrations")]
async fn test_something(pool: PgPool) { /* ... */ }

// ✅ Works - manual pool setup, migrations already applied
#[tokio::test]
async fn test_something() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);
    // ...
}
```

See the "PostgreSQL Migration Compatibility" section in Best Practices above.

### Enum Value Not Found After Migration

If you get `invalid input value for enum` after adding a new enum value via migration, PostgreSQL requires enum values to be committed before use.

**Solution:** Split enum additions into a separate migration file:

```sql
-- Migration 1: Add enum value (must commit)
ALTER TYPE my_enum ADD VALUE 'new_value';

-- Migration 2 (separate file): Use the value
UPDATE table SET column = 'new_value' WHERE ...;
```

For tests using `#[sqlx::test]`, this is unsolvable - use `#[tokio::test]` instead.

### PostgreSQL Connection Errors

```bash
# Check PostgreSQL is running
pg_isready

# Check connection string
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"

# Verify pgvector extension
psql $DATABASE_URL -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

## Performance Tips

### Parallel Test Execution

```bash
# Run tests in parallel (default)
cargo test --workspace

# Limit parallelism
cargo test --workspace -- --test-threads=4

# Run serially (for debugging)
cargo test --workspace -- --test-threads=1
```

### Test Caching

```bash
# Cache cargo artifacts
export CARGO_INCREMENTAL=1

# Use sccache for distributed caching
cargo install sccache
export RUSTC_WRAPPER=sccache
```

### Database Performance

- Use small test datasets when possible
- Consider in-memory PostgreSQL for CI
- Use connection pooling (automatic in fixtures)
- Clean up test data after each test

## References

- [Test Fixtures Module](/path/to/fortemi/crates/matric-db/src/test_fixtures.rs)
- [Mock Inference Backend](/path/to/fortemi/crates/matric-inference/src/mock.rs)
- [Test Workflow](/path/to/fortemi/.gitea/workflows/test.yml)
- [Cargo Book - Tests](https://doc.rust-lang.org/cargo/guide/tests.html)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)

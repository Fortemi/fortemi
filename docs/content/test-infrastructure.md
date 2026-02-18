# Test Infrastructure

Comprehensive CI/CD test infrastructure for Fortémi with database fixtures, mock backends, and coverage reporting.

## Quick Start

### Prerequisites

- Rust 1.70+
- Docker (for PostgreSQL test containers)
- PostgreSQL client tools (optional, for debugging)

### Running Tests Locally

```bash
# Quick start - all tests
./scripts/test-quick-start.sh all

# Fast tests only (no database)
./scripts/test-quick-start.sh fast

# Integration tests with database
./scripts/test-quick-start.sh integration

# Generate coverage report
./scripts/test-quick-start.sh coverage
```

### Manual Test Execution

```bash
# Fast tests
cargo test --lib --doc --workspace

# Integration tests (requires PostgreSQL)
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"
cargo test --workspace --tests

# Slow/ignored tests
cargo test --workspace -- --ignored

# Single test
cargo test test_name --package matric-db
```

## Architecture

### Components

1. **Test Fixtures** (`crates/matric-db/src/test_fixtures.rs`)
   - Database setup/teardown with schema isolation
   - Test data builders with fluent API
   - Pre-built seed functions

2. **Mock Inference Backend** (`crates/matric-inference/src/mock.rs`)
   - Deterministic embeddings
   - Configurable responses
   - Call logging and failure simulation

3. **CI Workflows** (`.gitea/workflows/test.yml`)
   - Fast, integration, and slow test jobs
   - Coverage generation and reporting
   - Parallel execution with isolated databases

4. **Test Categorization**
   - Fast: `--lib --doc` (no dependencies)
   - Integration: `--tests` (requires database)
   - Slow: `-- --ignored` (expensive tests)

## Features

### ✅ Test Fixtures

Reusable database setup with automatic cleanup:

```rust
use matric_db::test_fixtures::TestDatabase;

#[tokio::test]
async fn test_example() {
    let test_db = TestDatabase::new().await;
    // Use test_db.db for all operations
    // Cleanup happens automatically
}
```

### ✅ Mock Inference Backend

Deterministic testing without network calls:

```rust
use matric_inference::mock::MockInferenceBackend;

let backend = MockInferenceBackend::new()
    .with_dimension(384)
    .with_latency_ms(10);

let embedding = backend.embed("test").await.unwrap();
```

### ✅ Coverage Reporting

Automatic code coverage with cargo-llvm-cov:

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

### ✅ Test Categorization

Organized test execution:

- **Fast**: Unit tests, no external dependencies
- **Integration**: Database-dependent tests
- **Slow**: Performance benchmarks, large datasets

### ✅ Graph Quality Unit Tests

20 unit tests covering the graph quality pipeline (added in issues #470-#484):

- **Normalization**: Score distribution correctness across gamma values
- **SNN**: Neighborhood overlap computation, hub-penalization behavior, seashell pattern detection
- **PFNET**: Edge pruning correctness, transitivity rule validation, connectivity preservation
- **Louvain**: Community assignment stability, resolution parameter effects
- **Diagnostics**: Snapshot capture, metric accuracy, compare endpoint delta computation
- **MRL coarse**: 64-dim community detection on mock embedding corpus

These tests run as fast unit tests (no database required) using mock graph structures.

### ✅ CI Integration

Automated testing in Gitea workflows:

- Parallel test execution
- PostgreSQL test containers
- Coverage artifact uploads
- Test result summaries

## Test Fixtures API

### TestDatabase

```rust
// Create isolated test database
let test_db = TestDatabase::new().await;

// Access repositories
test_db.db.notes
test_db.db.tags
test_db.db.search
// ... etc

// Manual cleanup (or automatic on drop)
test_db.cleanup().await;
```

### TestDataBuilder

```rust
let data = TestDataBuilder::new(&test_db.db)
    .with_note("Content 1")
    .await
    .with_tagged_note("Content 2", &["tag1", "tag2"])
    .await
    .with_concept("Concept", None)
    .await
    .with_collection("Collection", None)
    .await
    .build()
    .await;

// Access created IDs
data.notes       // Vec<Uuid>
data.tags        // Vec<Uuid>
data.concepts    // Vec<Uuid>
data.collections // Vec<Uuid>
```

### Seed Functions

```rust
// Minimal setup: 2 notes, 1 concept, 1 collection
let data = seed_minimal_data(&test_db.db).await;

// Search corpus: 100 notes with varied content
let data = seed_search_corpus(&test_db.db).await;

// Embedding corpus: 50 notes with embeddings
let data = seed_embedding_corpus(&test_db.db, 384).await?;
```

## Mock Inference API

### Basic Configuration

```rust
let backend = MockInferenceBackend::new()
    .with_dimension(384)           // Embedding dimension
    .with_latency_ms(10)           // Simulate latency
    .with_failure_rate(0.1)        // 10% failure rate
    .with_fixed_response("text");  // Default response
```

### Response Mapping

```rust
let backend = MockInferenceBackend::new()
    .with_response_mapping("prompt1", "response1")
    .with_response_mapping("prompt2", "response2");

let response = backend.generate("prompt1").await.unwrap();
assert_eq!(response, "response1");
```

### Call Logging

```rust
let backend = MockInferenceBackend::new();

backend.embed("text1").await.unwrap();
backend.generate("prompt").await.unwrap();

assert_eq!(backend.embed_call_count(), 1);
assert_eq!(backend.generate_call_count(), 1);

let calls = backend.get_calls();
for call in calls {
    println!("{}: {}", call.operation, call.input);
}
```

### Deterministic Embeddings

```rust
use matric_inference::mock::MockEmbeddingGenerator;

// Same input always produces same output
let e1 = MockEmbeddingGenerator::generate("test", 384);
let e2 = MockEmbeddingGenerator::generate("test", 384);
assert_eq!(e1, e2);

// Control similarity
let (base, similar) = MockEmbeddingGenerator::generate_similar_pair(
    "test", 384, 0.8 // 80% similarity
);
```

## CI Workflow

### Workflow Structure

```
test.yml
├── fast-tests        # Unit tests (no dependencies)
├── integration-tests # Database tests
│   ├── Setup PostgreSQL
│   ├── Run migrations
│   └── Execute tests
├── coverage          # Coverage generation
│   └── Upload artifacts
├── slow-tests        # Long-running tests
└── test-summary      # Results aggregation
```

### Triggering Tests

```bash
# Automatic on push/PR
git push origin feature-branch

# Manual trigger with category
gh workflow run test.yml -f test_category=fast
gh workflow run test.yml -f test_category=integration
```

### Coverage Artifacts

Coverage reports are uploaded as workflow artifacts:

1. Navigate to workflow run in Gitea
2. Download "coverage-report" artifact
3. Extract and view:
   - `lcov.info` - Machine-readable coverage data
   - `summary.txt` - Human-readable summary

## Environment Variables

### Test Configuration

- `DATABASE_URL` - PostgreSQL connection string
- `TEST_DB_PORT` - Custom port for test database (default: 15432)
- `SKIP_INTEGRATION_TESTS` - Skip database-dependent tests
- `RUST_BACKTRACE` - Enable backtraces (1 or full)
- `CARGO_TERM_COLOR` - Enable colored output (always)

### Coverage Configuration

- `RUSTFLAGS` - Compiler flags for coverage instrumentation
- `LLVM_PROFILE_FILE` - Profile data output path
- `CARGO_INCREMENTAL` - Disable for coverage builds

## Performance Considerations

### Test Isolation

Each test gets an isolated PostgreSQL schema to prevent interference:

```rust
// Schema created: test_<uuid>
let test_db = TestDatabase::new().await;

// All operations scoped to isolated schema
test_db.db.notes.insert(...).await;

// Schema dropped on cleanup
test_db.cleanup().await;
```

### Connection Pooling

Test fixtures use connection pooling for performance:

```rust
PoolConfig {
    max_connections: 5,
    min_connections: 1,
    acquire_timeout_secs: 30,
    idle_timeout_secs: 600,
    max_lifetime_secs: 1800,
}
```

### Parallel Execution

Tests run in parallel by default:

```bash
# Default: parallel execution
cargo test --workspace

# Limit parallelism
cargo test --workspace -- --test-threads=4

# Serial execution (for debugging)
cargo test --workspace -- --test-threads=1
```

## Troubleshooting

### Tests Fail with "Connection Refused"

**Problem**: Can't connect to PostgreSQL

**Solution**:
```bash
# Check PostgreSQL is running
docker ps | grep pgvector

# Start test database
./scripts/test-quick-start.sh integration
```

### Tests Hang Indefinitely

**Problem**: Waiting for database locks or connections

**Solution**:
- Check for leaked connections (missing cleanup)
- Verify database is not in use by other tests
- Use serial execution: `cargo test -- --test-threads=1`

### Coverage Not Generated

**Problem**: cargo-llvm-cov not installed or configured

**Solution**:
```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate coverage
./scripts/test-quick-start.sh coverage
```

### Flaky Tests

**Problem**: Tests pass sometimes, fail other times

**Solution**:
- Use `TestDatabase` for isolation
- Use deterministic mocks instead of real backends
- Avoid shared state between tests
- Check for race conditions

## Best Practices

### ✅ DO: Use Test Fixtures

```rust
// ✅ Good
let test_db = TestDatabase::new().await;
// Isolated, automatic cleanup

// ❌ Bad
let db = Database::connect("postgres://...").await?;
// Shared state, manual cleanup required
```

### ✅ DO: Clean Up Resources

```rust
// ✅ Good - explicit cleanup
test_db.cleanup().await;

// ✅ Also good - automatic cleanup
{
    let test_db = TestDatabase::new().await;
    // Tests...
} // Drops here, cleanup happens
```

### ✅ DO: Categorize Appropriately

```rust
// ✅ Fast test
#[test]
fn test_parser() { /* no dependencies */ }

// ✅ Integration test
#[tokio::test]
async fn test_database() { /* uses TestDatabase */ }

// ✅ Slow test
#[tokio::test]
#[ignore = "slow test - large dataset"]
async fn test_performance() { /* expensive operation */ }
```

### ✅ DO: Use Deterministic Mocks

```rust
// ✅ Good - deterministic
let backend = MockInferenceBackend::new();

// ❌ Bad - network calls, non-deterministic
let backend = OllamaBackend::from_env();
```

## Examples

### Complete Integration Test

```rust
use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};

#[tokio::test]
async fn test_search_with_tags() {
    // Setup
    let test_db = TestDatabase::new().await;

    // Create test data
    let data = TestDataBuilder::new(&test_db.db)
        .with_tagged_note("Quantum computing", &["science"])
        .await
        .with_tagged_note("Classical physics", &["science"])
        .await
        .with_tagged_note("Cooking recipes", &["food"])
        .await
        .build()
        .await;

    // Execute search
    let results = test_db.db.search
        .search("quantum", 10)
        .await
        .unwrap();

    // Verify
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, data.notes[0]);

    // Cleanup
    test_db.cleanup().await;
}
```

### Mock Inference Test

```rust
use matric_inference::mock::MockInferenceBackend;

#[tokio::test]
async fn test_embedding_similarity() {
    let backend = MockInferenceBackend::new().with_dimension(384);

    let e1 = backend.embed("quantum computing").await.unwrap();
    let e2 = backend.embed("quantum physics").await.unwrap();
    let e3 = backend.embed("cooking recipes").await.unwrap();

    // Similar texts have similar embeddings (deterministic)
    let sim12 = cosine_similarity(&e1, &e2);
    let sim13 = cosine_similarity(&e1, &e3);

    assert!(sim12 > sim13);
}
```

## Resources

- [Full Testing Guide](./testing-guide.md)
- [Test Fixtures Source](/path/to/fortemi/crates/matric-db/src/test_fixtures.rs)
- [Mock Backend Source](/path/to/fortemi/crates/matric-inference/src/mock.rs)
- [CI Workflow](/path/to/fortemi/.gitea/workflows/test.yml)
- [cargo-llvm-cov Documentation](https://github.com/taiki-e/cargo-llvm-cov)

## Contributing

When adding new tests:

1. Choose appropriate category (fast/integration/slow)
2. Use test fixtures for database operations
3. Use mock backends for inference
4. Add documentation for complex test scenarios
5. Verify tests pass in CI

## Support

For issues or questions:

1. Check troubleshooting section above
2. Review existing tests for examples
3. Consult full testing guide
4. Open issue with test failure details

# Quick Start: Testing

Get started with Fortémi's test infrastructure in 5 minutes.

## Prerequisites

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Docker (for PostgreSQL test containers)
# Follow: https://docs.docker.com/get-docker/

# Install cargo-llvm-cov (for coverage, optional)
cargo install cargo-llvm-cov
```

## Running Tests

### Option 1: Quick Start Script (Recommended)

```bash
# All tests
./scripts/test-quick-start.sh all

# Fast tests only (no database, ~30 seconds)
./scripts/test-quick-start.sh fast

# Integration tests (with database, ~5 minutes)
./scripts/test-quick-start.sh integration

# Generate coverage report
./scripts/test-quick-start.sh coverage
```

### Option 2: Manual Execution

```bash
# Fast tests
cargo test --lib --doc --workspace

# Integration tests (requires PostgreSQL)
# 1. Start PostgreSQL
docker run -d --name matric-test \
  -p 15432:5432 \
  -e POSTGRES_USER=matric \
  -e POSTGRES_PASSWORD=matric \
  -e POSTGRES_DB=matric_test \
  pgvector/pgvector:pg18

# 2. Run migrations
for f in migrations/*.sql; do
  docker exec -i matric-test psql -U matric -d matric_test < "$f"
done

# 3. Run tests
export DATABASE_URL="postgres://matric:matric@localhost:15432/matric_test"
cargo test --workspace --tests

# 4. Cleanup
docker stop matric-test && docker rm matric-test
```

## Writing Your First Test

### Fast Test (No Database)

```rust
// crates/matric-core/tests/example_test.rs

#[test]
fn test_simple_function() {
    // Arrange
    let input = "test";

    // Act
    let result = process(input);

    // Assert
    assert_eq!(result, "expected");
}
```

### Integration Test (With Database)

```rust
// crates/matric-db/tests/example_test.rs

use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};

#[tokio::test]
async fn test_database_operation() {
    // Setup test database
    let test_db = TestDatabase::new().await;

    // Create test data
    let data = TestDataBuilder::new(&test_db.db)
        .with_note("Test content")
        .await
        .build()
        .await;

    // Test your operation
    let note = test_db.db.notes
        .get(data.notes[0])
        .await
        .unwrap()
        .unwrap();

    // Verify
    assert_eq!(note.content, "Test content");

    // Cleanup (automatic on drop, or manual)
    test_db.cleanup().await;
}
```

### Test with Mock Inference

```rust
// crates/matric-inference/tests/example_test.rs

use matric_inference::mock::MockInferenceBackend;

#[tokio::test]
async fn test_inference() {
    // Create mock backend
    let backend = MockInferenceBackend::new()
        .with_dimension(384);

    // Generate embedding
    let embedding = backend.embed("test text").await.unwrap();

    // Verify
    assert_eq!(embedding.len(), 384);

    // Embeddings are deterministic
    let embedding2 = backend.embed("test text").await.unwrap();
    assert_eq!(embedding, embedding2);
}
```

## Common Commands

```bash
# Run single test
cargo test test_name

# Run tests in specific package
cargo test --package matric-db

# Run with output
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored

# Show test names without running
cargo test -- --list

# Run tests matching pattern
cargo test search
```

## CI Integration

Tests run automatically on push/PR:

```yaml
# .gitea/workflows/test.yml runs:
- fast-tests       # Unit tests
- integration-tests # Database tests
- coverage         # Coverage report
- slow-tests       # Optional, on main branch only
```

View results:
1. Go to Actions tab in Gitea
2. Click on your workflow run
3. View job results and logs

## Troubleshooting

### "Connection refused" Error

PostgreSQL is not running:

```bash
docker ps | grep pgvector
# If not running:
./scripts/test-quick-start.sh integration
```

### Tests Hang

Check for database lock or leaked connections:

```bash
# Run serially for debugging
cargo test -- --test-threads=1
```

### "cargo-llvm-cov not found"

Install coverage tool:

```bash
cargo install cargo-llvm-cov
```

## Next Steps

- Read the [full testing guide](testing-guide.md)
- Review [test infrastructure README](test-infrastructure-readme.md)
- Check out [existing tests](../crates/matric-db/tests/) for examples
- Explore [test fixtures API](../crates/matric-db/src/test_fixtures.rs)

## Quick Reference

| Command | Purpose | Time |
|---------|---------|------|
| `./scripts/test-quick-start.sh fast` | Unit tests | ~30s |
| `./scripts/test-quick-start.sh integration` | Database tests | ~5m |
| `./scripts/test-quick-start.sh coverage` | Coverage report | ~10m |
| `cargo test --lib` | Library tests only | ~15s |
| `cargo test --doc` | Doc tests only | ~15s |
| `cargo test -- --ignored` | Slow tests | ~15m |

---

**Need Help?** Check the [troubleshooting guide](testing-guide.md#troubleshooting) or review [examples](../crates/matric-db/tests/).

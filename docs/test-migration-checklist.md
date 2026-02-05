# Test Migration Checklist

Guide for migrating existing tests to use the new test infrastructure.

## Overview

This checklist helps you migrate existing tests to use:
- Test fixtures for database operations
- Mock inference backends
- Proper test categorization
- Best practices

## Migration Process

### Step 1: Identify Test Type

Determine what category your test falls into:

- [ ] **Fast Test**: No external dependencies (database, network)
  - Move to: `tests/` or keep in `src/` modules
  - Runs with: `cargo test --lib --doc`

- [ ] **Integration Test**: Requires database
  - Move to: `tests/` directory
  - Runs with: `cargo test --tests`

- [ ] **Slow Test**: Expensive operations (>5s runtime)
  - Move to: `tests/` directory
  - Add: `#[ignore = "slow test - description"]`
  - Runs with: `cargo test -- --ignored`

### Step 2: Update Database Tests

If your test uses database operations, migrate to test fixtures.

#### Before: Manual Database Setup

```rust
#[tokio::test]
async fn test_old() {
    // Manual connection
    let pool = PgPool::connect("postgres://...").await.unwrap();
    let db = Database::new(pool);

    // Manual data creation
    let note_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO note (content, format, source) VALUES ($1, $2, $3) RETURNING id"
    )
    .bind("content")
    .bind("markdown")
    .bind("test")
    .fetch_one(db.pool())
    .await
    .unwrap();

    // Test logic...

    // Manual cleanup
    sqlx::query("DELETE FROM note WHERE id = $1")
        .bind(note_id)
        .execute(db.pool())
        .await
        .unwrap();
}
```

#### After: Test Fixtures

```rust
use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};

#[tokio::test]
async fn test_new() {
    // Automatic setup
    let test_db = TestDatabase::new().await;

    // Fluent data creation
    let data = TestDataBuilder::new(&test_db.db)
        .with_note("content")
        .await
        .build()
        .await;

    // Test logic using data.notes[0]...

    // Automatic cleanup on drop
}
```

#### Checklist:

- [ ] Replace manual pool connection with `TestDatabase::new()`
- [ ] Replace raw SQL inserts with `TestDataBuilder`
- [ ] Use `test_db.db` instead of direct `Database` instance
- [ ] Remove manual cleanup code
- [ ] Add `.await` after `build()`

### Step 3: Update Inference Tests

If your test uses inference/embeddings, migrate to mock backend.

#### Before: Real Inference Backend

```rust
#[tokio::test]
async fn test_old() {
    // Real backend (network dependency, non-deterministic)
    let backend = OllamaBackend::from_env();

    let embedding = backend.embed("test").await.unwrap();

    // Test may be flaky due to network issues
    assert!(embedding.len() > 0);
}
```

#### After: Mock Backend

```rust
use matric_inference::mock::MockInferenceBackend;

#[tokio::test]
async fn test_new() {
    // Mock backend (deterministic, no network)
    let backend = MockInferenceBackend::new()
        .with_dimension(384);

    let embedding = backend.embed("test").await.unwrap();

    // Deterministic assertion
    assert_eq!(embedding.len(), 384);

    // Test determinism
    let embedding2 = backend.embed("test").await.unwrap();
    assert_eq!(embedding, embedding2);
}
```

#### Checklist:

- [ ] Replace `OllamaBackend` with `MockInferenceBackend`
- [ ] Configure dimension with `.with_dimension()`
- [ ] Remove `from_env()` and environment dependencies
- [ ] Update assertions for deterministic behavior
- [ ] Add determinism verification if applicable

### Step 4: Add Test Categorization

Mark tests appropriately for selective execution.

#### Fast Test (Default)

```rust
#[test]
fn test_fast() {
    // No special marker needed
}
```

#### Integration Test

```rust
// Place in tests/ directory
// No special marker needed
#[tokio::test]
async fn test_integration() {
    let test_db = TestDatabase::new().await;
    // ...
}
```

#### Slow Test

```rust
#[tokio::test]
#[ignore = "slow test - performance benchmark"]
async fn test_slow() {
    // Expensive operation
}
```

#### Checklist:

- [ ] Fast tests have no markers
- [ ] Integration tests are in `tests/` directory
- [ ] Slow tests have `#[ignore]` attribute
- [ ] Ignore message describes why test is slow

### Step 5: Update Test Structure

Follow consistent test structure.

#### Template

```rust
#[tokio::test]
async fn test_descriptive_name() {
    // 1. Setup
    let test_db = TestDatabase::new().await;
    let data = TestDataBuilder::new(&test_db.db)
        .with_note("content")
        .await
        .build()
        .await;

    // 2. Execute
    let result = test_db.db.notes
        .get(data.notes[0])
        .await
        .unwrap();

    // 3. Verify
    assert!(result.is_some());
    let note = result.unwrap();
    assert_eq!(note.content, "content");

    // 4. Cleanup (automatic)
}
```

#### Checklist:

- [ ] Test name is descriptive (what is being tested)
- [ ] Setup section creates fixtures
- [ ] Execute section performs operation
- [ ] Verify section contains assertions
- [ ] Cleanup is automatic (or explicit if needed)

### Step 6: Update Imports

Add necessary imports for test fixtures.

```rust
// At top of test file
use matric_db::test_fixtures::{
    TestDatabase,
    TestDataBuilder,
    seed_minimal_data,
    seed_search_corpus,
};

// For mock inference
use matric_inference::mock::{
    MockInferenceBackend,
    MockEmbeddingGenerator,
};
```

#### Checklist:

- [ ] Import `TestDatabase` for database tests
- [ ] Import `TestDataBuilder` for data creation
- [ ] Import seed functions if used
- [ ] Import mock types for inference tests
- [ ] Remove old manual setup imports

### Step 7: Verify Test Passes

Run the migrated test to ensure it works.

```bash
# Run specific test
cargo test test_name --package matric-db

# Run with output
cargo test test_name -- --nocapture

# Run all tests in file
cargo test --test filename
```

#### Checklist:

- [ ] Test passes locally
- [ ] Test passes in CI
- [ ] Test is deterministic (passes multiple times)
- [ ] Test cleanup works (no leaked resources)
- [ ] Test is appropriately categorized

## Common Migration Patterns

### Pattern 1: Simple Note Test

**Before:**
```rust
let note_id = create_note_manually(&pool, "content").await;
let note = get_note_manually(&pool, note_id).await;
assert_eq!(note.content, "content");
delete_note_manually(&pool, note_id).await;
```

**After:**
```rust
let test_db = TestDatabase::new().await;
let data = TestDataBuilder::new(&test_db.db)
    .with_note("content")
    .await
    .build()
    .await;

let note = test_db.db.notes.get(data.notes[0]).await.unwrap().unwrap();
assert_eq!(note.content, "content");
```

### Pattern 2: Search Test

**Before:**
```rust
let pool = setup_test_db().await;
seed_notes(&pool, 100).await;
let results = search(&pool, "query", 10).await;
assert!(results.len() > 0);
cleanup_test_db(pool).await;
```

**After:**
```rust
let test_db = TestDatabase::new().await;
let _data = seed_search_corpus(&test_db.db).await;
let results = test_db.db.search.search("query", 10).await.unwrap();
assert!(!results.is_empty());
```

### Pattern 3: Embedding Test

**Before:**
```rust
let backend = OllamaBackend::from_env();
let e1 = backend.embed("text1").await.unwrap();
let e2 = backend.embed("text2").await.unwrap();
// Test may be flaky
```

**After:**
```rust
let backend = MockInferenceBackend::new().with_dimension(384);
let e1 = backend.embed("text1").await.unwrap();
let e2 = backend.embed("text2").await.unwrap();
assert_eq!(e1.len(), 384);
assert_eq!(e2.len(), 384);
```

## Migration Example

### Complete Example: Before

```rust
// Old test without fixtures
#[tokio::test]
async fn test_note_tags() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric_test".to_string());

    let pool = PgPool::connect(&database_url).await.unwrap();
    let db = Database::new(pool.clone());

    // Create note
    let note_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO note (content, format, source) VALUES ($1, $2, $3) RETURNING id"
    )
    .bind("Test content")
    .bind("markdown")
    .bind("test")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create tag
    let tag_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tag (name) VALUES ($1) RETURNING id"
    )
    .bind("test-tag")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Associate tag with note
    sqlx::query(
        "INSERT INTO note_tag (note_id, tag_id) VALUES ($1, $2)"
    )
    .bind(note_id)
    .bind(tag_id)
    .execute(&pool)
    .await
    .unwrap();

    // Test: Get note with tags
    let note = db.notes.get_with_tags(note_id).await.unwrap().unwrap();
    assert_eq!(note.tags.len(), 1);
    assert_eq!(note.tags[0].name, "test-tag");

    // Cleanup
    sqlx::query("DELETE FROM note_tag WHERE note_id = $1")
        .bind(note_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM tag WHERE id = $1")
        .bind(tag_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM note WHERE id = $1")
        .bind(note_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

### Complete Example: After

```rust
// New test with fixtures
use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};

#[tokio::test]
async fn test_note_tags() {
    // Setup with automatic cleanup
    let test_db = TestDatabase::new().await;

    // Create test data with fluent API
    let data = TestDataBuilder::new(&test_db.db)
        .with_tagged_note("Test content", &["test-tag"])
        .await
        .build()
        .await;

    // Test: Get note with tags
    let note = test_db.db.notes
        .get_with_tags(data.notes[0])
        .await
        .unwrap()
        .unwrap();

    assert_eq!(note.tags.len(), 1);
    assert_eq!(note.tags[0].name, "test-tag");

    // Cleanup happens automatically on drop
}
```

## Benefits After Migration

✅ **Shorter code**: 45 lines → 20 lines in example above

✅ **Cleaner**: No manual SQL, no manual cleanup

✅ **Safer**: Automatic resource cleanup, no leaked state

✅ **Faster**: Reusable fixtures, parallel execution

✅ **Reliable**: Deterministic mocks, no network flakiness

✅ **Maintainable**: Standard patterns, less boilerplate

## Verification Checklist

After migration, verify:

- [ ] Test passes locally: `cargo test test_name`
- [ ] Test passes in CI
- [ ] Test is in correct category (fast/integration/slow)
- [ ] Test uses test fixtures for database
- [ ] Test uses mocks for inference
- [ ] Test has no manual cleanup
- [ ] Test has descriptive name
- [ ] Test follows template structure
- [ ] Test has proper imports
- [ ] Test is documented (comments if complex)

## Getting Help

If you encounter issues:

1. Check [testing guide](testing-guide.md) for examples
2. Review [existing migrated tests](../crates/matric-db/tests/)
3. Check [test fixtures source](../crates/matric-db/src/test_fixtures.rs)
4. Ask in team chat or open issue

## References

- [Testing Guide](testing-guide.md) - Full documentation
- [Test Infrastructure README](test-infrastructure-readme.md) - Quick reference
- [Quick Start](quick-start-testing.md) - Getting started
- [Test Fixtures Source](../crates/matric-db/src/test_fixtures.rs) - API reference
- [Mock Backend Source](../crates/matric-inference/src/mock.rs) - Mock API

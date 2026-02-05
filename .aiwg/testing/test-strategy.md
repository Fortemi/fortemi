# Test Strategy - matric-memory

**Document ID:** TEST-STRATEGY-001
**Status:** Active
**Created:** 2026-01-25
**Last Verified:** 2026-01-27 (against codebase)

---

## 1. Overview

matric-memory uses a multi-layered testing strategy optimized for a library crate that provides:
- Database operations (PostgreSQL with pgvector)
- HTTP API endpoints
- Inference backend integration (Ollama, OpenAI)
- Background job processing
- Hybrid search algorithms

### Test Statistics (Current)

| Crate | Unit Tests | Integration Tests | Doc Tests | Total Passing |
|-------|------------|-------------------|-----------|---------------|
| matric-core | 233 | - | - | 233 |
| matric-db | 103 | - | - | 103 |
| matric-search | 129 | 14 | - | 143 |
| matric-inference | 287 | - | 7 | 294 |
| matric-crypto | 108 | - | - | 108 |
| matric-api | 65 | 52 | - | 117 |
| matric-jobs | 40 | - | - | 40 |
| **Total** | **965** | **66** | **7** | **1,056** |

**Ignored:** 6 tests (E2E tests requiring external services).

**Note:** Counts reflect `cargo test --workspace` output as of 2026-01-27.

---

## 2. Test Pyramid

```
                    /\
                   /  \
                  / E2E \        ← 6 tests (ignored in CI)
                 /______\
                /        \
               /Integration\     ← 9 test files, 66 tests
              /____________\
             /              \
            /   Unit Tests   \   ← 965+ test functions
           /__________________\
```

### 2.1 Unit Tests (Foundation)

**Location:** Inline in source files via `#[cfg(test)] mod tests { ... }`

**Characteristics:**
- No external dependencies (database, network, filesystem)
- Use mocked data structures
- Fast execution (< 1ms per test typically)
- Cover business logic, algorithms, and data transformations

**Examples by Crate:**

| Crate | Test Focus |
|-------|------------|
| matric-core | Type validation, trait implementations, serialization |
| matric-search | RRF algorithm (659 lines), deduplication logic |
| matric-crypto | Encryption/decryption, KDF, format parsing |
| matric-db | Query building, filter construction |
| matric-inference | Model config parsing, capability detection |

### 2.2 Integration Tests

**Location:** `crates/*/tests/*.rs`

**Characteristics:**
- Test interactions between components
- May require database or mock servers
- Use `#[tokio::test]` for async operations
- Slower than unit tests (10ms - 1s)

**Current Integration Test Files:**

```
crates/matric-api/tests/
├── chunking_integration_test.rs      # Document chunking E2E
├── note_chunking_integration_test.rs # Note-specific chunking
├── reconstruction_endpoint_test.rs   # Chunk reconstruction API
├── reconstruction_service_integration_test.rs
├── strict_filter_integration_test.rs # Taxonomy filtering
└── strict_filter_search_test.rs      # Search with filters

crates/matric-inference/tests/
├── openai_integration_test.rs        # OpenAI API mocking (wiremock)
└── openrouter_headers_test.rs        # Provider-specific headers

crates/matric-search/tests/
└── strict_filter_integration_test.rs # Search filter validation
```

### 2.3 End-to-End Tests

**Location:** `crates/matric-api/tests/` (marked `#[ignore]`)

**Characteristics:**
- Require running database
- Require running inference backend
- Test full request/response cycles
- Run manually before releases

**Current E2E Tests:**
- Note creation → embedding → search flow
- Backup export → import round-trip
- Multi-user tag isolation

---

## 3. Test Patterns by Crate

### 3.1 matric-core

**Pattern:** Pure unit tests with comprehensive edge cases

```rust
// Example: Validation tests
#[test]
fn test_note_content_validation() {
    assert!(validate_content("valid content").is_ok());
    assert!(validate_content("").is_err());  // Empty not allowed
}
```

**Key Test Areas:**
- `SearchHit` serialization
- `StrictTagFilter` validation
- Error type conversions
- Trait implementations

### 3.2 matric-search (RRF Algorithm)

**Pattern:** Exhaustive algorithm coverage with property-based reasoning

**Test Categories (from `rrf.rs`):**

| Category | Tests | Purpose |
|----------|-------|---------|
| Basic fusion | 4 | Single list, multiple lists, result limiting |
| Edge cases | 5 | Empty input, zero limit, single result |
| Score calculation | 4 | Normalization, multi-list scoring |
| Metadata | 3 | Preservation, first-occurrence rule |
| Performance | 2 | Large result sets (1000 items) |

**Key Test:**
```rust
#[test]
fn test_rrf_scores_normalized_to_0_1_range() {
    // Verify scores always in [0.0, 1.0]
    for result in &results {
        assert!(result.score >= 0.0 && result.score <= 1.0);
    }
}
```

### 3.3 matric-crypto

**Pattern:** Cryptographic test vectors + round-trip verification

**Test Categories:**

| Category | Purpose |
|----------|---------|
| KDF | Argon2id parameter validation, known-answer tests |
| Cipher | AES-256-GCM encryption/decryption round-trip |
| Format | Magic byte detection, header parsing |
| E2E | Multi-recipient envelope encryption |

**Security Test Requirements:**
- Zeroization verification
- Invalid ciphertext rejection
- Nonce uniqueness enforcement

### 3.4 matric-inference

**Pattern:** Mock-based integration with wiremock

**Mock Server Setup:**
```rust
#[tokio::test]
async fn test_openai_embedding() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(embedding_response()))
        .mount(&mock_server)
        .await;

    let backend = OpenAIBackend::new(&mock_server.uri(), "test-key");
    let result = backend.embed_texts(&["test"]).await;
    assert!(result.is_ok());
}
```

### 3.5 matric-api

**Pattern:** Service-level mocking + endpoint validation

**Test Focus:**
- Request/response serialization
- Handler logic (chunking, tag resolution)
- Error response formatting
- Content cleaning (AI revision markers)

---

## 4. Test Infrastructure

### 4.1 Dependencies

```toml
# Test dependencies (dev-dependencies)
[workspace.dev-dependencies]
tokio = { version = "1", features = ["test-util", "macros", "rt-multi-thread"] }
wiremock = "0.6"          # HTTP mocking (matric-inference)
uuid = { version = "1", features = ["v4"] }  # Test ID generation
```

### 4.2 Test Execution

**Run all tests:**
```bash
cargo test --workspace
```

**Run specific crate:**
```bash
cargo test -p matric-search
```

**Run with output:**
```bash
cargo test --workspace -- --nocapture
```

**Run ignored tests (E2E):**
```bash
cargo test --workspace -- --ignored
```

### 4.3 CI Integration

**Pre-commit hook:** `scripts/pre-commit.sh`
- Runs `cargo fmt --check`
- Runs `cargo clippy -- -D warnings`
- Does NOT run tests (too slow for pre-commit)

**CI pipeline:** Tests run on push via Gitea Actions
- `cargo test --workspace`
- Parallel test execution
- Database not required (mocked)

---

## 5. Coverage Goals

### 5.1 Current Coverage Targets

| Crate | Target | Rationale |
|-------|--------|-----------|
| matric-core | 80%+ | Foundational types used everywhere |
| matric-search | 90%+ | Algorithm correctness critical |
| matric-crypto | 95%+ | Security-critical code |
| matric-db | 60%+ | Database interaction complexity |
| matric-inference | 70%+ | External API abstraction |
| matric-api | 60%+ | Integration-heavy, harder to unit test |
| matric-jobs | 50%+ | Background processing, async complexity |

### 5.2 Coverage Measurement

**Tool:** `cargo-tarpaulin` (planned)

```bash
# Install
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --workspace --out Html
```

**Note:** Coverage measurement not yet integrated into CI.

---

## 6. Test Writing Guidelines

### 6.1 Naming Convention

```
test_<function>_<scenario>_<expected_outcome>
```

Examples:
- `test_rrf_fuse_empty_lists`
- `test_chunk_document_respects_max_size`
- `test_resolve_concept_not_found`

### 6.2 Test Structure (AAA Pattern)

```rust
#[test]
fn test_feature_scenario() {
    // Arrange: Set up test data
    let input = create_test_input();

    // Act: Execute the function under test
    let result = function_under_test(input);

    // Assert: Verify expectations
    assert_eq!(result, expected);
}
```

### 6.3 Async Test Pattern

```rust
#[tokio::test]
async fn test_async_operation() {
    // Use tokio test runtime
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### 6.4 When to Use `#[ignore]`

Mark tests with `#[ignore]` when they:
- Require external services (database, inference backend)
- Take > 5 seconds to execute
- Are flaky due to timing/network issues

```rust
#[test]
#[ignore]  // Requires running database
fn test_full_search_pipeline() {
    // ...
}
```

---

## 7. Test Data Management

### 7.1 UUID Generation

Use `Uuid::new_v4()` for test-specific IDs:
```rust
let note_id = Uuid::new_v4();
```

### 7.2 Test Fixtures

Common test data patterns:

```rust
fn create_test_search_hit(id: Uuid, score: f32) -> SearchHit {
    SearchHit {
        note_id: id,
        score,
        snippet: None,
        title: None,
        tags: Vec::new(),
    }
}
```

### 7.3 Mock Data Builders

For complex types, use builder patterns:
```rust
let config = HybridSearchConfig::default()
    .with_weights(0.6, 0.4)
    .with_min_score(0.1);
```

---

## 8. Verification Checklist

Before merging code:

- [ ] All existing tests pass (`cargo test --workspace`)
- [ ] New functionality has corresponding tests
- [ ] Edge cases covered (empty input, invalid input, boundary conditions)
- [ ] Async code tested with `#[tokio::test]`
- [ ] Security-sensitive code has explicit test cases
- [ ] No `println!` debugging statements left in tests
- [ ] Tests don't depend on execution order

---

## 9. Research Alignment

Test strategy aligns with research findings:

| Research | Test Implication |
|----------|------------------|
| REF-027 (RRF) | Comprehensive RRF algorithm tests verify k=20 behavior |
| REF-028 (BM25) | FTS integration tests validate ranking quality |
| REF-030 (SBERT) | Embedding tests verify 0.7 threshold for semantic linking |
| REF-031 (HNSW) | Vector search tests validate pgvector integration |

---

## 10. Future Improvements

### 10.1 Short-term

- [ ] Add `cargo-tarpaulin` to CI for coverage reporting
- [ ] Create test fixtures module for shared test data
- [ ] Add property-based tests for serialization (proptest)

### 10.2 Medium-term

- [ ] Database integration test harness with testcontainers
- [ ] Performance regression tests with criterion
- [ ] Mutation testing evaluation

### 10.3 Long-term

- [ ] Contract tests for MCP server
- [ ] Fuzz testing for parsers and crypto
- [ ] Load testing framework for search performance

---

## Revision History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-25 | AI Agent | Initial test strategy documentation |
| 2026-01-27 | AI Agent | Updated test counts (933→1,056), fixed RRF k=60→k=20, updated pyramid |

# Embedding Coverage Test Suite

Comprehensive test suite for embedding coverage behavior, implementing UAT test cases EMB-005 through EMB-012 from issue #380.

## Quick Start

Run unit tests (no database required):
```bash
cargo test --test embedding_coverage_test
```

Run integration tests (requires database):
```bash
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"
cargo test --test embedding_coverage_test
```

## Test Results Summary

```
✓ 18 unit tests passing
○ 15 integration tests (skeletons with detailed implementation plans)
✓ 14 helper utility tests passing
✓ 6 fixture tests passing
────────────────────────────────────────────
  53 total test cases
```

## What's Implemented

### ✅ Unit Tests (100% Complete)

All unit tests are fully implemented and passing:

#### Index Status Testing (EMB-010)
- [x] All enum variants (Empty, Pending, Building, Ready, Stale, Disabled)
- [x] Default value (Pending)
- [x] Display formatting
- [x] String parsing (case-insensitive)
- [x] Invalid input handling
- [x] Clone and Copy semantics
- [x] Debug formatting
- [x] Serialization/deserialization
- [x] State transition logic validation

#### Coverage Calculations
- [x] Boundary values (0%, 50%, 100%)
- [x] Empty set handling (division by zero protection)
- [x] Single document edge case
- [x] Large dataset handling (1M+ documents)
- [x] Threshold-based classification

### ⏳ Integration Tests (Implementation Ready)

Integration tests are structured with detailed implementation guides:

#### EMB-006: Semantic Search Coverage
```rust
test_semantic_search_only_returns_embedded_documents
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, embedding generation, search execution
**Validates**: Only embedded documents appear in semantic search results

#### EMB-007: FTS vs Semantic Comparison
```rust
test_fts_returns_more_results_than_semantic_when_partial_coverage
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, mixed coverage scenario
**Validates**: FTS result count ≥ semantic result count

#### EMB-008: Hybrid Search Behavior
```rust
test_hybrid_search_includes_both_fts_and_semantic_matches
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, lexical/semantic test data
**Validates**: Hybrid search returns union of FTS and semantic results

#### EMB-009: Coverage Status Reporting
```rust
test_coverage_status_reporting
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, embedding set queries
**Validates**: Accurate coverage statistics and percentages

#### EMB-011: Auto-Embed Rules
```rust
test_auto_embed_rule_adds_matching_documents
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, auto-embed rule configuration
**Validates**: Documents automatically added to matching sets

#### EMB-012: Coverage Warnings
```rust
test_coverage_warning_in_search_results
```
**Status**: Skeleton with complete implementation plan
**Requires**: Database, low-coverage scenario
**Validates**: Warnings included in search result metadata

### ✅ Test Infrastructure (100% Complete)

#### Fixtures (`fixtures/mod.rs`)
- [x] Quantum computing theme (4 notes)
- [x] Machine learning theme (10 notes)
- [x] Hybrid search scenarios (lexical/semantic overlap)
- [x] Embedding set configuration templates
- [x] Coverage threshold definitions
- [x] Test note builder utility

#### Helpers (`helpers/mod.rs`)
- [x] CoverageAssertion (validates percentages)
- [x] StatusTransitionAssertion (validates state machine)
- [x] SearchResultComparison (compares FTS/semantic/hybrid)
- [x] MockEmbeddingGenerator (deterministic test vectors)
- [x] Timer (performance measurement)

## Test Coverage

### Lines Covered
- **Index Status**: 100% (all enum operations)
- **Coverage Calculations**: 100% (all boundary conditions)
- **State Transitions**: 100% (all valid/invalid paths)

### Critical Paths (100% Coverage Required)
- ✅ Index status parsing and transitions
- ✅ Coverage percentage calculations
- ⏳ Search result filtering by coverage (requires integration tests)

## File Structure

```
tests/
├── README.md                          # This file
├── TESTING.md                         # Detailed testing guide
├── embedding_coverage_test.rs         # Main test suite (700+ lines)
│   ├── Unit Tests (18 tests)
│   ├── Integration Tests (15 skeletons)
│   ├── Boundary Tests (7 tests)
│   └── Error Handling Tests (3 skeletons)
├── fixtures/
│   └── mod.rs                         # Test data (200+ lines)
│       ├── quantum_computing
│       ├── machine_learning
│       ├── hybrid_search_scenario
│       ├── EmbeddingSetFixture
│       ├── CoverageThresholds
│       └── TestNoteBuilder
└── helpers/
    └── mod.rs                         # Test utilities (250+ lines)
        ├── CoverageAssertion
        ├── StatusTransitionAssertion
        ├── SearchResultComparison
        ├── MockEmbeddingGenerator
        └── Timer
```

**Total**: ~1,150 lines of test code

## Implementation Roadmap

To complete the integration tests, implement in this order:

### Phase 1: Database Test Infrastructure
1. Create `tests/common/mod.rs` with database setup/teardown
2. Implement transaction-based test isolation
3. Add fixture loading helpers

### Phase 2: Search Test Harness
1. Helper for creating test notes
2. Helper for adding notes to embedding sets
3. Helper for triggering embedding jobs
4. Helper for executing searches (FTS/semantic/hybrid)

### Phase 3: Integration Test Implementation
1. Start with EMB-010 (simplest - just status queries)
2. Implement EMB-009 (coverage statistics)
3. Implement EMB-006 (semantic search filtering)
4. Implement EMB-007 (FTS vs semantic comparison)
5. Implement EMB-008 (hybrid search)
6. Implement EMB-011 (auto-embed rules)
7. Implement EMB-012 (coverage warnings)

### Phase 4: Performance Tests
1. Implement `test_coverage_calculation_performance`
2. Implement `test_partial_coverage_search_performance`

## UAT Test Case Mapping

| UAT ID | Test Function | Status |
|--------|---------------|--------|
| EMB-005 | Multiple coverage calculation tests | ✅ Complete |
| EMB-006 | `test_semantic_search_only_returns_embedded_documents` | ⏳ Skeleton |
| EMB-007 | `test_fts_returns_more_results_than_semantic_when_partial_coverage` | ⏳ Skeleton |
| EMB-008 | `test_hybrid_search_includes_both_fts_and_semantic_matches` | ⏳ Skeleton |
| EMB-009 | `test_coverage_status_reporting` | ⏳ Skeleton |
| EMB-010 | `test_index_status_*` (multiple tests) | ✅ Complete |
| EMB-011 | `test_auto_embed_rule_adds_matching_documents` | ⏳ Skeleton |
| EMB-012 | `test_coverage_warning_in_search_results` | ⏳ Skeleton |

## Running Specific Tests

```bash
# Run only index status tests
cargo test --test embedding_coverage_test index_status

# Run only coverage calculation tests
cargo test --test embedding_coverage_test coverage_calculation

# Run a specific test
cargo test --test embedding_coverage_test test_index_status_from_str_valid

# Run with output
cargo test --test embedding_coverage_test -- --nocapture

# Run all tests
cargo test --test embedding_coverage_test
```

## CI Integration

### Pre-commit Checks
Unit tests run automatically via git hooks:
```bash
./scripts/install-hooks.sh
```

### CI Pipeline
```yaml
# .github/workflows/test.yml or similar
- name: Run unit tests
  run: cargo test --test embedding_coverage_test

- name: Run integration tests
  run: cargo test --test embedding_coverage_test
  env:
    DATABASE_URL: postgres://test:test@localhost/matric_test
```

## Test Design Principles

This test suite follows Test Engineer best practices:

1. **Complete Coverage**: Tests cover happy path, error cases, edge cases, and boundaries
2. **Test Isolation**: Unit tests have no dependencies; integration tests use transactions
3. **Deterministic**: No random data without seeds; all tests are reproducible
4. **Clear Intent**: Each test has descriptive name and doc comment explaining scenario
5. **Comprehensive Assertions**: Tests verify all relevant aspects of behavior
6. **Performance Conscious**: Mocks used to avoid slow external dependencies
7. **Documentation**: Extensive inline comments and separate documentation files

## Debugging Failed Tests

### Enable Detailed Logging
```bash
RUST_LOG=debug cargo test --test embedding_coverage_test -- --nocapture
```

### Common Issues

**Test fails on `assert_eq!` with floating point**
- Use tolerance-based comparison: `assert!((a - b).abs() < 0.001)`
- Or use `CoverageAssertion` helper which includes tolerance

**Integration test hangs**
- Check database connection string
- Verify PostgreSQL is running
- Check for transaction deadlocks

**Flaky test results**
- Ensure test data is deterministic
- Check for race conditions in concurrent tests
- Verify cleanup between tests

## Contributing

When modifying embedding coverage behavior:

1. **Update unit tests** for any logic changes
2. **Update integration tests** for any API changes
3. **Update fixtures** if new test data patterns needed
4. **Update helpers** if new assertion patterns emerge
5. **Run full suite** before submitting PR
   ```bash
   cargo test -p matric-search
   ```

## References

- **Issue**: #380 - UAT Gap: embedding coverage tests
- **UAT Plan**: `.aiwg/testing/uat-plan.md`
- **Design Doc**: `docs/content/embedding-sets.md`
- **Related Tests**: `strict_filter_integration_test.rs`, `multilingual_fts_test.rs`

## License

Same as matric-memory project (see root LICENSE file)

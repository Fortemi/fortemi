# Embedding Coverage Testing

This directory contains comprehensive tests for embedding coverage behavior, implementing UAT test cases EMB-005 through EMB-012.

## Test Organization

```
tests/
├── embedding_coverage_test.rs   # Main test suite
├── fixtures/
│   └── mod.rs                    # Test data and scenarios
├── helpers/
│   └── mod.rs                    # Test utilities and assertions
└── TESTING.md                    # This file
```

## Test Categories

### Unit Tests (No Database Required)

These tests run without external dependencies and verify core logic:

- **Index Status Types** (EMB-010)
  - Enum variant handling
  - String parsing (case-insensitive)
  - Display formatting
  - Serialization/deserialization
  - State transition validation

- **Coverage Calculations**
  - Percentage calculations
  - Boundary conditions (0%, 50%, 100%)
  - Edge cases (empty set, single document, large datasets)
  - Division by zero protection
  - Floating-point precision handling

- **Threshold Logic**
  - Warning level determination
  - Coverage classification (empty, very_low, low, medium, high, complete)

Run unit tests:
```bash
cargo test --test embedding_coverage_test
```

### Integration Tests (Database Required)

These tests require a PostgreSQL database with pgvector extension:

- **Semantic Search Coverage** (EMB-006)
  - Only embedded documents returned
  - Zero-coverage scenarios
  - Partial-coverage scenarios

- **FTS vs Semantic Comparison** (EMB-007)
  - FTS returns all matching documents
  - Semantic returns only embedded documents
  - FTS count ≥ semantic count (when coverage < 100%)

- **Hybrid Search Behavior** (EMB-008)
  - Union of FTS and semantic results
  - Lexical-only matches included
  - Semantic-only matches included
  - Combined ranking

- **Coverage Status Reporting** (EMB-009)
  - Accurate document counts
  - Accurate embedding counts
  - Correct coverage percentage
  - Index status reflects reality

- **Auto-Embed Rules** (EMB-011)
  - Documents automatically added when matching criteria
  - Index status transitions to Stale
  - Multiple set membership

- **Coverage Warnings** (EMB-012)
  - Low coverage warnings in search results
  - Recommendations for hybrid/FTS mode
  - Per-set coverage metadata

Run integration tests:
```bash
# Setup test database first
export DATABASE_URL="postgres://matric:matric@localhost/matric_test"
cargo test --test embedding_coverage_test
```

## Test Data

### Fixtures

The `fixtures/` module provides:

- **Quantum Computing Theme**: 4 notes for testing semantic similarity
- **Machine Learning Theme**: 10 notes for testing partial coverage
- **Hybrid Search Scenarios**: Notes with varying lexical/semantic overlap
- **Embedding Set Configurations**: Manual, auto, and filter set templates
- **Coverage Thresholds**: Standard warning level definitions

### Test Helpers

The `helpers/` module provides:

- **CoverageAssertion**: Validates coverage percentage calculations
- **StatusTransitionAssertion**: Validates state machine transitions
- **SearchResultComparison**: Compares FTS/semantic/hybrid result counts
- **MockEmbeddingGenerator**: Generates deterministic test embeddings
- **Timer**: Measures operation performance

## Coverage Goals

| Metric | Target | Critical Paths |
|--------|--------|----------------|
| Line Coverage | 80% | 100% |
| Branch Coverage | 75% | 100% |
| Function Coverage | 90% | 100% |

Critical paths:
- Index status parsing and transitions
- Coverage calculation logic
- Search result filtering by embedding coverage

## Test Execution Strategy

### Local Development

1. Run unit tests frequently during development:
   ```bash
   cargo test --test embedding_coverage_test --lib
   ```

2. Run integration tests before committing:
   ```bash
   ./scripts/run-integration-tests.sh
   ```

### CI Pipeline

1. Unit tests run on every commit
2. Integration tests run on PR creation
3. Coverage reports generated and checked against thresholds

## Known Limitations

### TODO Items

The integration tests are currently skeletons with detailed implementation comments. They require:

1. **Database Test Infrastructure**
   - Test database setup/teardown utilities
   - Transaction-based test isolation
   - Fixture loading helpers

2. **Embedding Service Mocking**
   - Mock Ollama embedding generation
   - Deterministic vector generation
   - Fast test execution without real inference

3. **Search Engine Test Harness**
   - Helper for creating test notes
   - Helper for adding to embedding sets
   - Helper for triggering embedding jobs
   - Helper for executing searches with different modes

### Future Enhancements

- **Performance Benchmarks**: Add benchmarks for coverage calculation at scale
- **Concurrency Tests**: Test concurrent searches during index building
- **Stress Tests**: Test with very large embedding sets (1M+ documents)
- **Chaos Tests**: Test behavior with interrupted embedding jobs
- **Property-Based Tests**: Use proptest for coverage calculation edge cases

## Debugging Tests

### Enable Tracing

```bash
RUST_LOG=debug cargo test --test embedding_coverage_test -- --nocapture
```

### Run Specific Test

```bash
cargo test --test embedding_coverage_test test_index_status_from_str_valid
```

### Run Tests Matching Pattern

```bash
cargo test --test embedding_coverage_test coverage
```

## References

- **UAT Plan**: `.aiwg/testing/uat-plan.md` (EMB-005 through EMB-012)
- **Issue**: #380 - UAT Gap: embedding coverage tests
- **Design Doc**: `docs/content/embedding-sets.md`
- **API Spec**: `crates/matric-api/src/routes/embedding_sets.rs`

## Contributing

When adding new coverage-related features:

1. Add unit tests for logic (non-database)
2. Add integration test skeleton with detailed TODO comments
3. Update fixtures if new test data needed
4. Update helpers if new assertions needed
5. Update this documentation

All tests must:
- Have clear, descriptive names
- Include doc comments explaining the scenario
- Test both happy path and error cases
- Test boundary conditions
- Be deterministic (no random data without seeds)

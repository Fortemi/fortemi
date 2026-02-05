# Tests Directory

This directory contains test suites and test documentation for Matric Memory.

## Directory Structure

```
tests/
├── README.md              # This file
└── uat/                   # User Acceptance Testing
    ├── matric-memory-uat-executor.md  # Main UAT guide
    └── phases/                         # Phase-based test documents
        ├── README.md                   # Phase overview
        ├── phase-0-preflight.md        # System readiness checks
        ├── phase-1-seed-data.md        # Test data creation
        ├── phase-2-crud.md             # CRUD operations (critical)
        ├── phase-3-search.md           # Search capabilities (critical)
        ├── phase-4-tags.md             # Tag system
        ├── phase-5-collections.md      # Collections
        ├── phase-6-links.md            # Semantic links
        ├── phase-7-embeddings.md       # Embedding sets
        ├── phase-8-document-types.md   # Document type system
        ├── phase-9-edge-cases.md       # Edge cases & security
        ├── phase-10-backup.md          # Backup & export
        └── phase-11-cleanup.md         # Test data cleanup
```

## Unit Tests

Unit tests are located in each crate's source tree:
- `crates/matric-core/src/` - Core module tests
- `crates/matric-db/src/` - Database repository tests
- `crates/matric-api/src/` - API handler tests
- `crates/matric-search/src/` - Search algorithm tests

Run unit tests:
```bash
cargo test --workspace
```

## Integration Tests

Integration tests are in each crate's `tests/` directory:
- `crates/matric-core/tests/` - Core integration tests
- `crates/matric-db/tests/` - Database integration tests
- `crates/matric-api/tests/` - API integration tests
- `crates/matric-search/tests/` - Search integration tests

## User Acceptance Tests (UAT)

UAT tests verify end-to-end functionality via MCP tools.

### Running UAT

1. Ensure MCP server is running and connected
2. Execute phases in order (0 → 11)
3. Each phase document is self-contained with pass/fail criteria

### Phase Categories

**Critical (100% pass required)**:
- Phase 0: Pre-flight
- Phase 2: CRUD Operations
- Phase 3: Search Capabilities

**Standard (90% pass required)**:
- Phases 4-10

**Cleanup (required)**:
- Phase 11: Must always run to clean test data

### Success Criteria

- Critical phases: 100% pass required
- Standard phases: 90% pass acceptable
- Overall: 95% pass rate for release approval

## MCP Server Tests

MCP server connectivity tests:
```bash
cd mcp-server && npm test
```

## Test Coverage

Check coverage with:
```bash
cargo tarpaulin --workspace --out Html
```

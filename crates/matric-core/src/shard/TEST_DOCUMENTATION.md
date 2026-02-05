# Shard Migration System - Test Documentation

## Overview

Comprehensive test suite for the shard migration system, validating version compatibility, migration paths, and data integrity during knowledge shard imports/exports.

## Test Organization

### Unit Tests (Per Module)

- `version.rs` - Version parsing, comparison, compatibility (145 lines of tests)
- `compatibility.rs` - Compatibility checking logic (172 lines of tests)
- `migration.rs` - Migration registry and path-finding (280 lines of tests)
- `warning.rs` - Migration warning serialization (75 lines of tests)

### Integration Tests

`tests.rs` - Cross-module integration tests (~200 lines, 22+ tests):
- Manifest deserialization
- Version parsing edge cases
- Compatibility matrix validation
- Migration path finding
- Warning serialization round-trips

### Test Fixtures

Located in `/tests/fixtures/shards/`:

| Fixture | Purpose |
|---------|---------|
| `v1.0.0-minimal.json` | Minimal valid manifest |
| `v1.0.0-full.json` | Complete manifest with all components |
| `v1.0.0-with-embeddings.json` | Embedding-focused shard |
| `v1.1.0-with-mrl.json` | MRL migration testing |
| `v2.0.0-future.json` | Major version incompatibility |

## Running Tests

### All Shard Tests
```bash
cargo test --package matric-core shard
```

### Integration Tests Only
```bash
cargo test --package matric-core shard::tests
```

### Specific Test
```bash
cargo test --package matric-core test_version_parse_valid
```

### With Coverage
```bash
cargo tarpaulin --packages matric-core --lib -- shard
```

## Test Categories

### 1. Version Parsing (9 tests)
- Valid formats
- Invalid formats
- Edge cases (empty, whitespace, negatives)
- Boundary values

### 2. Compatibility Checking (6 tests)
- Same version
- Minor version differences
- Major version incompatibility
- Invalid version handling

### 3. Migration Registry (8 tests)
- Path finding algorithms
- Single/multi-hop migrations
- Error handling
- Registry operations

### 4. Serialization (4 tests)
- Warning types round-trip
- JSON format validation

## Coverage Targets

| Module | Target | Status |
|--------|--------|--------|
| version.rs | 100% | ✓ Complete |
| compatibility.rs | 100% | ✓ Complete |
| migration.rs | 100% | ✓ Complete |
| warning.rs | 90% | ✓ Complete |

## Test Patterns

### Arrange-Act-Assert
```rust
#[test]
fn test_version_parse() {
    // Arrange
    let version_str = "1.2.3";
    
    // Act
    let v = Version::parse(version_str).unwrap();
    
    // Assert
    assert_eq!(v.major, 1);
}
```

### Error Path Testing
```rust
#[test]
fn test_version_parse_invalid() {
    assert!(Version::parse("invalid").is_err());
}
```

## References

- Issue #419 - Add migration tests and documentation
- Issue #413 - Shard versioning and migration
- UC-009 - Generate Test Artifacts

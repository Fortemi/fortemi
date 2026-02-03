# Shard Migration Test Fixtures

This directory contains JSON fixtures for testing the shard migration system.

## Fixtures

### v1_0_0_minimal.json
Minimal valid v1.0.0 manifest with only required fields.

**Use cases:**
- Testing basic manifest deserialization
- Baseline compatibility checks
- Empty shard handling

### v1_0_0_full.json
Full v1.0.0 manifest with all components and optional metadata.

**Use cases:**
- Testing complete shard structure
- Checksum validation
- Metadata handling
- All component types

**Components included:**
- notes, collections, tags, templates
- links, embedding_sets, embeddings

### v1_1_0_forward_compat.json
Shard from v1.1.0 with new optional fields (forward-compatible).

**Use cases:**
- Testing forward compatibility
- Unknown field handling
- NewerMinor compatibility result
- Warning generation

**New fields:**
- `new_field_in_1_1_0` - Should be ignored by v1.0.0 readers
- `features` - New capability flags

### v2_0_0_incompatible.json
Shard from v2.0.0 with breaking changes (incompatible).

**Use cases:**
- Testing major version incompatibility
- Incompatible result generation
- Upgrade guidance messages
- Migration path requirements

**Breaking changes:**
- New required field: `schema_version`
- Changed checksum algorithm (SHA256 → BLAKE3)
- Renamed component type (`links` → `documents`)

### invalid_version.json
Manifest with malformed version string.

**Use cases:**
- Testing error handling
- Invalid version parsing
- Error message generation

## Usage in Tests

```rust
use std::fs;

#[test]
fn test_load_minimal_fixture() {
    let json = fs::read_to_string(
        "crates/matric-core/src/shard/fixtures/v1_0_0_minimal.json"
    ).unwrap();

    let manifest: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest["version"], "1.0.0");
}
```

## Fixture Design Principles

1. **Deterministic**: All fixtures use fixed timestamps and values
2. **Realistic**: Based on actual shard format specifications
3. **Edge cases**: Cover boundary conditions and error scenarios
4. **Self-documenting**: Clear naming and structure
5. **Minimal dependencies**: Pure JSON, no external files

## Maintenance

When adding new shard format versions:
1. Create new fixture file named `vX_Y_Z_description.json`
2. Update this README with use cases
3. Add tests in `tests.rs` using the new fixture
4. Document any breaking changes or new fields

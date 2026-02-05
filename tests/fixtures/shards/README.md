# Shard Test Fixtures

This directory contains test fixtures for the shard migration system.

## Fixtures

### v1.0.0-minimal.json

Minimal valid v1.0.0 shard manifest with no data.

**Use cases:**
- Testing basic deserialization
- Testing empty shard handling
- Baseline compatibility testing

**Key features:**
- All required fields present
- No optional fields
- Empty counts and components

### v1.0.0-full.json

Complete v1.0.0 shard manifest with all component types.

**Use cases:**
- Testing realistic shard import
- Testing checksum validation
- Testing component counts

**Key features:**
- All components included
- Realistic counts (50 notes, 5 collections, etc.)
- Checksums for data integrity
- matric_version field populated

### v1.0.0-with-embeddings.json

v1.0.0 shard focused on embeddings (100 notes with embeddings).

**Use cases:**
- Testing embedding import
- Testing large embedding sets
- Baseline for MRL comparison

**Key features:**
- 100 notes with embeddings
- Standard (non-MRL) embedding format
- Embedding configuration included

### v1.1.0-with-mrl.json

v1.1.0 shard with MRL (Matryoshka Representation Learning) embeddings.

**Use cases:**
- Testing downgrade impact analysis
- Testing MRL feature detection
- Testing migration history

**Key features:**
- Migration metadata populated
- min_reader_version specified
- migration_history with change log
- Embeddings use truncate_dim field (MRL)

### v2.0.0-future.json

Future major version (v2.0.0) for incompatibility testing.

**Use cases:**
- Testing major version incompatibility
- Testing upgrade guidance
- Testing version mismatch handling

**Key features:**
- Major version jump (1.x -> 2.x)
- min_reader_version requires future version
- Large dataset (200 notes)

## Usage in Tests

### Loading Fixtures

```rust
// In integration tests
let manifest_json = include_str!("../../../tests/fixtures/shards/v1.0.0-minimal.json");
let manifest: ShardManifest = serde_json::from_str(manifest_json).unwrap();
```

### Inline Fixtures

```rust
// In unit tests (preferred for small fixtures)
let json = r#"{
    "version": "1.0.0",
    "format": "matric-shard",
    ...
}"#;
let manifest: ShardManifest = serde_json::from_str(json).unwrap();
```

## Adding New Fixtures

When adding a new fixture:

1. **Create the JSON file** with a descriptive name
2. **Document it** in this README
3. **Specify use cases** - what tests will use it
4. **Note key features** - what makes it unique
5. **Update test suite** to use the new fixture

## Fixture Naming Convention

```
v{MAJOR}.{MINOR}.{PATCH}-{descriptor}.json
```

Examples:
- `v1.0.0-minimal.json` - Minimal v1.0.0 manifest
- `v1.1.0-with-mrl.json` - v1.1.0 with MRL features
- `v2.0.0-future.json` - Future v2.0.0 format

## Validation

All fixtures should be valid JSON and conform to the ShardManifest schema.

Validate with:

```bash
# Check JSON syntax
for f in tests/fixtures/shards/*.json; do
    echo "Validating $f"
    jq empty "$f" || echo "Invalid JSON: $f"
done
```

## Fixture Maintenance

When the shard format changes:

1. **Update existing fixtures** to match new schema
2. **Add new fixtures** for new features
3. **Keep old fixtures** for backward compatibility testing
4. **Document breaking changes** in fixture comments

## Migration Between Fixtures

Example migration scenarios:

| From | To | Migration Path | Test Purpose |
|------|-----|----------------|--------------|
| v1.0.0-minimal | v1.0.0-full | None (same version) | Data addition |
| v1.0.0-full | v1.1.0-with-mrl | 1.0→1.1 migration | MRL upgrade |
| v1.1.0-with-mrl | v1.0.0-full | Downgrade | Data loss analysis |
| v1.0.0-minimal | v2.0.0-future | Incompatible | Upgrade required |

## Reference

For the shard format specification, see:
- `/crates/matric-api/src/main.rs` - ShardManifest struct
- `/crates/matric-core/src/shard/` - Migration system
- Issue #413 - Shard versioning and migration
- Issue #419 - Migration tests and documentation

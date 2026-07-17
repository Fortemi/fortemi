# Shard Test Fixtures

This directory contains test fixtures for the shard migration system.

## Fixtures

### v1.0.0-minimal.json

Canonical minimal `core-v1` manifest for the current reader contract.

**Use cases:**
- Testing basic deserialization
- Testing empty shard handling
- Baseline compatibility testing

**Key features:**
- All required fields present
- Structured producer identity and schema minimum-reader version
- One empty, checksummed `notes.jsonl` component

### Legacy compatibility vectors

`v1.0.0-full.json`, `v1.0.0-with-embeddings.json`, and
`v1.1.0-with-mrl.json` preserve pre-profile manifests for legacy parsing and
migration tests. They are not current import-conformance fixtures and must not
be used to claim `core-v1` or `full-v1` support.

### v2.0.0-future.json

Canonical next-major `core-v1` manifest for incompatibility testing.

**Use cases:**
- Testing major version incompatibility
- Testing upgrade guidance
- Testing version mismatch handling

**Key features:**
- Major version jump (1.x -> 2.x)
- `min_reader_version` requires shard schema `2.0.0`
- Producer application identity remains separate from schema versions
- One empty, checksummed `notes.jsonl` component

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

All fixtures must be valid JSON. Files identified above as canonical must
conform to the current manifest contract; legacy vectors intentionally omit
new required conformance fields.

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

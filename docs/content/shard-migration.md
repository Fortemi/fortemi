# Shard Migration Guide

## Canonical Contract

Fortemi owns the normative Knowledge Shard schemas. The default export
contract is schema `1.2.0`, with registered `core-v1`, reduced `record-v1`, and
complete server `full-v1` profiles under
`contracts/knowledge-shard/1.2.0/`. Exact schema `2.0.0` tuples are available
only through explicit reader/export selection and add direct JSON-key presence
semantics. Immutable
`1.0.0` and `1.1.0` schemas, current and historical file digests, profile
corpora, and migration targets are recorded in
`contracts/knowledge-shard/contract.json`.

Consumers must pin an immutable Fortemi commit and verify the receipt before
using vendored copies. `core-v1` covers note, collection, tag, template, and
link records. `record-v1` covers notes, collections, tags,
note-to-note links, and attachment projections; its producer must return a
machine-readable report for every omitted or lossy source concept. Contract
revision 19 retains it after exact React producer archives passed Fortemi
dry-run, zero-mutation rejection, repeated replace import, server re-export,
and React return import. The `full-v1` server route requires the complete rich
component inventory and every declared attachment byte; its signed fixture
passes clean import, exact semantic re-export, and failure rollback. The
hardened nine-cell schema `1.2.0` matrix expresses only its individual
producer/consumer cells. External `2.0.0/full-v1` producer and consumer
receipts remain pending under Fortemi #1084 and fortemi-react #382, so the
server self-roundtrip must not be generalized into a suite-wide claim.

Schema validation is necessary but not a full recovery claim. The current
`core-v1` REST route is reference-only by default and can opt into verified
attachment sidecars with `include_blobs=true`; import restores present valid
sidecars and preserves missing entries as references. Schema `1.2.0` preserves
active and soft-deleted notes through explicit `deleted_at` values and carries
nullable embedding contract lineage. The default remains `core-v1`; use an
explicit `profile=full-v1` export for the complete server profile. A default
export is not a disaster-recovery-complete archive.

`min_reader_version` always names a Knowledge Shard schema reader floor. It is
never a Fortemi application version; application identity belongs only in
producer metadata.

This guide explains how Fortémi handles versioned knowledge shards, including compatibility checking, automatic migration, and troubleshooting.

> **Current implementation versus target contract:** This page documents the
> current default and opt-in implementation.
> [ADR-102](https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md)
> defines the cross-repository contract: named profiles, validation before
> writes, atomic import, and fail-closed integrity checks. The server now passes
> its `full-v1` self-route gates, but consumers may advertise only the exact
> profile and producer/consumer cells backed by immutable matrix evidence.

## Table of Contents

- [How Versioning Works](#how-versioning-works)
- [Compatibility Matrix](#compatibility-matrix)
- [Migration System](#migration-system)
- [Troubleshooting](#troubleshooting)
- [For Developers](#for-developers)

## How Versioning Works

Fortémi uses **semantic versioning** for knowledge shards to ensure safe import/export operations across different versions.

### Version Format: MAJOR.MINOR.PATCH

```
1.0.0
│ │ │
│ │ └─ PATCH: Bug fixes only, no schema changes
│ └─── MINOR: New features, backward compatible
└───── MAJOR: Breaking changes that require migration
```

### Version Examples

| Version | Change Type | Example |
|---------|-------------|---------|
| `1.0.0 → 1.0.1` | Patch | Fixed checksum calculation bug |
| `1.0.0 → 1.1.0` | Minor | Added the optional note tombstone field |
| `1.1.0 → 1.2.0` | Minor | Added nullable embedding contract lineage |
| `1.0.0 → 2.0.0` | Major | Changed embedding format (requires migration) |

### Supported versions

The default shard format version is defined in
`crates/matric-core/src/shard/version.rs`; the API's opt-in reader also accepts
exact schema `2.0.0` tuples:

```rust
pub const CURRENT_SHARD_VERSION: &str = "1.2.0";
// API reader ceiling: 2.0.0; the default remains 1.2.0.
```

## Compatibility Matrix

When you import a knowledge shard, Fortémi checks version compatibility and takes appropriate action.

| Scenario | Shard Version | Current Version | Behavior | Example |
|----------|---------------|-----------------|----------|---------|
| **Same version** | 1.2.0 | 1.2.0 | Import directly | No changes needed |
| **Registered older version** | 1.0.0 | 1.2.0 | Validate, migrate twice, revalidate, import | `deleted_at` absence defaults to `null` |
| **Previous registered version** | 1.1.0 | 1.2.0 | Validate, migrate, revalidate, import | Existing records are preserved |
| **Unregistered older version** | 1.0.1 | 1.2.0 | Reject before writes | No exact migration path |
| **Newer minor** | 1.3.0 | 1.2.0 | Reject before writes | Explicit reader support required |
| **Opt-in current major** | 2.0.0 | reader 2.0.0 | Import exact tuple | Preserve direct-key presence |
| **Newer major** | 3.0.0 | reader 2.0.0 | Reject before writes | Upgrade the schema reader |

### Compatibility Rules

#### 1. Same Version (Compatible)
```
Shard: 1.2.0, Current: 1.2.0
✓ Import directly
✓ All features available
✓ No warnings
```

#### 2. Registered Older Version
```
Shard: 1.0.0, Current: 1.2.0
✓ Validate original checksums and schema
✓ Migrate missing deleted_at to explicit null through 1.1.0
✓ Apply the registered 1.1.0 to 1.2.0 compatibility step
✓ Recompute the migrated component receipt
✓ Validate the 1.2.0 representation before writes
```

#### 3. Newer Minor Version
```
Shard: 1.3.0, Current: 1.2.0
✗ Import fails before writes
✗ Explicit schema and migration support is required
```

**Error message:**
```
Knowledge shard schema version is newer than this reader.
```

#### 4. Older Version With a Registered Path
```
Shard: 1.0.0, Current: 1.2.0
✓ Exact 1.0.0 schemas and checksums validated
✓ Registered 1.0.0 → 1.1.0 → 1.2.0 path applied
✓ Migrated representation revalidated before writes
```

#### 5. Unsupported Newer Major Version
```
Shard: 3.0.0, Maximum reader: 2.0.0
✗ Import fails
✗ A reader with explicit schema 3 support is required
```

**Error message:**
```
Shard major version 3 is incompatible with maximum reader major version 2
Minimum required schema reader: 3.0.0
```

## Migration System

When a shard requires migration, Fortémi uses a **migration registry** to automatically transform data.

### How Migrations Work

1. **Version Check**: System detects version mismatch
2. **Path Finding**: BFS algorithm finds shortest migration path
3. **Chain Execution**: Migrations applied in sequence
4. **Warning Collection**: Non-fatal issues logged
5. **Data Return**: Migrated data ready for import

### Migration Example

Importing a v1.0.0 shard into the current v1.2.0 system:

```
Shard version: 1.0.0
Current version: 1.2.0

Migration path found:
  1.0.0 → 1.1.0 (Add optional note deleted_at)
  1.1.0 → 1.2.0 (Register nullable embedding contract lineage)

Applying migrations...
  ✓ Original archive checksums and 1.0.0 schemas validated
  ✓ Missing deleted_at values defaulted to null
  ✓ Migrated checksums and 1.2.0 schemas validated

Import successful!
```

### Migration Warnings

Migrations may emit warnings for non-fatal issues:

| Warning Type | Description | Example |
|--------------|-------------|---------|
| `FieldRemoved` | Field no longer exists in new version | `old_checksum_algorithm` removed (5 occurrences) |
| `DefaultApplied` | Missing field filled with default value | `deleted_at` defaulted to `null` |
| `UnknownFieldIgnored` | Field from newer version ignored | `future_feature` not recognized |
| `DataTruncated` | Data shortened to fit new constraints | `description` truncated to 255 chars |

### Migration Path Finding

The migration registry uses **breadth-first search** to find the shortest path:

```
Available migrations:
  1.0.0 → 1.1.0
  1.1.0 → 1.2.0

Finding path from 1.0.0 to 1.2.0:
  BFS queue: [(1.0.0, [])]
  Visit 1.0.0 → neighbors: [1.1.0]
  Visit 1.1.0 → neighbors: [1.2.0]
  Found target! Path: [1.0.0→1.1.0, 1.1.0→1.2.0]
```

### Circular Migration Prevention

The BFS algorithm uses a visited set to prevent infinite loops:

```rust
let mut visited = HashSet::new();
visited.insert(from.to_string());

while let Some((current, path)) = queue.pop() {
    for migration in next_migrations {
        if !visited.contains(&next) {
            visited.insert(next.clone());
            queue.push((next, new_path));
        }
    }
}
```

## Troubleshooting

### "Shard requires newer version"

**Problem:** Your Fortémi is older than the shard format.

```
Error: Shard major version 2 is incompatible with current major version 1
Minimum required version: 2.0.0
```

**Solution:** Upgrade Fortémi to the version shown in the error message.

```bash
# Check current version
matric-api --version

# Upgrade to required version
git pull
git checkout v2.0.0
cargo build --release
```

### "Migration default reported during import"

**Meaning:** A schema `1.0.0` shard did not have a tombstone field.

```
Migrated Knowledge Shard schema 1.0.0 to 1.1.0;
defaulted deleted_at to null for N legacy note records.
```

**Impact:**
- Original bytes and checksums were validated before migration.
- Legacy records become active notes because schema 1.0.0 could not represent tombstones.
- The response manifest records `migrated_from` and `migration_history`.

**Solution:** No action is required. Retain the original archive as the
immutable source receipt.

### "No migration path found"

**Problem:** No registered migrations exist between versions.

```
Error: No migration path found from 1.5.0 to 2.0.0
```

**Causes:**
- Missing migration registration
- Version gap too large
- Development/testing version

**Solution:** Check available migrations:

```bash
# View registered migrations (developer tool)
cargo test test_registry -- --nocapture
```

### "Migration failed"

**Problem:** A migration encountered an error during transformation.

```
Error: Migration failed: Invalid embedding dimension (expected 768, got 384)
```

**Solution:** This indicates a data integrity issue. Contact support or:

1. Export shard manifest to inspect data
2. Manually fix data issues
3. Re-import with corrected shard

### Invalid version format

**Problem:** Shard manifest has malformed version string.

```
Error: Invalid shard version: not-a-version
```

**Common mistakes:**
- `v1.0.0` (prefix not allowed)
- `1.0` (must be three components)
- `1.0.0-beta` (suffixes not allowed)

**Solution:** Fix manifest version field:

```bash
# Extract and fix manifest
tar -xzf broken.shard manifest.json
# Edit manifest.json: "version": "1.0.0"
tar -czf fixed.shard manifest.json [other files...]
```

## For Developers

### Creating a Migration

When introducing breaking changes to the shard format, you must provide a migration.

#### 1. Implement `ShardMigration` Trait

```rust
// crates/matric-core/src/shard/migrations/v1_0_to_v1_1.rs
use crate::shard::{MigrationError, MigrationResult, ShardMigration, MigrationWarning};
use serde_json::Value;

pub struct MigrateV1_0ToV1_1;

impl ShardMigration for MigrateV1_0ToV1_1 {
    fn from_version(&self) -> &str {
        "1.0.0"
    }

    fn to_version(&self) -> &str {
        "1.1.0"
    }

    fn description(&self) -> &str {
        "Add the core-v1 deleted_at tombstone field"
    }

    fn migrate(&self, mut data: Value) -> Result<MigrationResult, MigrationError> {
        let mut warnings = Vec::new();

        // Record the active state explicitly for legacy notes.
        if data.get("deleted_at").is_none() {
            data["deleted_at"] = serde_json::Value::Null;
            warnings.push(MigrationWarning::DefaultApplied {
                field: "deleted_at".to_string(),
                default: "null".to_string(),
            });
        }

        Ok(MigrationResult { data, warnings })
    }
}
```

#### 2. Register in Migration Registry

```rust
// crates/matric-core/src/shard/migrations/mod.rs
pub mod v1_0_to_v1_1;

pub fn create_registry() -> MigrationRegistry {
    let mut registry = MigrationRegistry::new();

    // Register all migrations
    registry.register(Box::new(V1_0ToV1_1));

    registry
}
```

#### 3. Update Current Version

```rust
// crates/matric-core/src/shard/version.rs
pub const CURRENT_SHARD_VERSION: &str = "1.2.0";  // Updated
```

#### 4. Add Tests

```rust
#[test]
fn test_migrate_1_0_to_1_1() {
    let migration = V1_0ToV1_1;

    let v1_0_data = json!({
        "id": "018f2d2d-bc00-7cc8-8ad2-f147d6a2e77a"
    });

    let result = migration.migrate(v1_0_data).unwrap();

    assert!(result.data["deleted_at"].is_null());
    assert_eq!(result.warnings.len(), 1);
}
```

#### 5. Create Test Fixture

```json
// crates/matric-core/src/shard/fixtures/v1_2_0_minimal.json
{
  "version": "1.2.0",
  "format": "matric-shard",
  "created_at": "2026-02-01T00:00:00Z",
  "components": [],
  "counts": {},
  "checksums": {}
}
```

### Testing Checklist

Before releasing a new shard version:

- [ ] Migration implements `ShardMigration` trait
- [ ] Migration registered in `create_registry()`
- [ ] `CURRENT_SHARD_VERSION` updated
- [ ] Unit tests for migration logic
- [ ] Integration tests with real fixtures
- [ ] Test both upgrade and import scenarios
- [ ] Document breaking changes in CHANGELOG
- [ ] Test migration path finding (multi-hop)
- [ ] Verify warning messages are clear
- [ ] Run full test suite: `cargo test --workspace`

### Version Bump Decision Tree

```
Is this a breaking change?
├─ YES → Increment MAJOR version (1.x.x → 2.0.0)
│        └─ Create migration from 1.x.x to 2.0.0
└─ NO  → Is this a new feature?
         ├─ YES → Increment MINOR version (1.0.x → 1.1.0)
         │        └─ No migration needed (backward compatible)
         └─ NO  → Increment PATCH version (1.0.0 → 1.0.1)
                  └─ No schema changes allowed
```

### Breaking Changes Examples

| Change | Breaking? | Version Impact |
|--------|-----------|----------------|
| Add optional field | No | Minor (1.0.0 → 1.1.0) |
| Add required field | Yes | Major (1.0.0 → 2.0.0) |
| Remove field | Yes | Major (1.0.0 → 2.0.0) |
| Rename field | Yes | Major (1.0.0 → 2.0.0) |
| Change field type | Yes | Major (1.0.0 → 2.0.0) |
| Fix bug in serialization | No | Patch (1.0.0 → 1.0.1) |
| Change checksum algorithm | Yes | Major (1.0.0 → 2.0.0) |

### Architecture References

The shard migration system is based on industry best practices from:

- **Protocol Buffers**: Field number versioning, reserved fields
- **Alembic**: Migration chains, BFS path finding
- **SQLite**: Multiple version fields, forward/backward compatibility
- **Semantic Versioning**: MAJOR.MINOR.PATCH scheme

For detailed architecture decisions, see:
- Research: `docs/research/data-format-migration-strategies.md`
- Backup guide: `docs/content/backup.md`
- Shard exchange: `docs/content/shard-exchange.md`

## Related Documentation

- [Backup Guide](#/operations-backup) - Creating and restoring knowledge shards
- [Shard Exchange Primer](#/core-systems-shards) - Sharing encrypted shards
- [API Documentation](#/developers-api) - Shard import/export endpoints
- [MCP Tools](#/developers-mcp) - MCP shard operations

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-01-01 | Initial shard format release |
| 1.1.0 | 2026-07-18 | Optional note tombstones and registered 1.0.0 migration |
| 1.2.0 | 2026-07-20 | Nullable embedding contract lineage and registered 1.1.0 migration |

---

**Last updated:** 2026-07-21

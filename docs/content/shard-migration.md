# Shard Migration Guide

This guide explains how Fortémi handles versioned knowledge shards, including compatibility checking, automatic migration, and troubleshooting.

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
| `1.0.0 → 1.1.0` | Minor | Added optional metadata fields |
| `1.0.0 → 2.0.0` | Major | Changed embedding format (requires migration) |

### Current Version

The current shard format version is defined in `crates/matric-core/src/shard/version.rs`:

```rust
pub const CURRENT_SHARD_VERSION: &str = "1.0.0";
```

## Compatibility Matrix

When you import a knowledge shard, Fortémi checks version compatibility and takes appropriate action.

| Scenario | Shard Version | Current Version | Behavior | Example |
|----------|---------------|-----------------|----------|---------|
| **Same version** | 1.0.0 | 1.0.0 | Import directly | No changes needed |
| **Older minor** | 1.0.0 | 1.1.0 | Import directly | Ignore unknown fields from newer version |
| **Newer minor** | 1.1.0 | 1.0.0 | Import with warning | Some features may not be available |
| **Older major** | 1.x.x | 2.0.0 | Auto-migrate via registry | Apply migration 1.x.x → 2.0.0 |
| **Newer major** | 2.0.0 | 1.0.0 | Fail with guidance | Upgrade Fortémi to v2.0.0+ |

### Compatibility Rules

#### 1. Same Version (Compatible)
```
Shard: 1.0.0, Current: 1.0.0
✓ Import directly
✓ All features available
✓ No warnings
```

#### 2. Older Minor Version (Compatible)
```
Shard: 1.0.0, Current: 1.1.0
✓ Import directly
✓ New fields in current version use defaults
✓ No migration needed
```

#### 3. Newer Minor Version (Forward-Compatible)
```
Shard: 1.1.0, Current: 1.0.0
⚠ Import with warnings
⚠ Unknown fields are preserved but may be ignored
⚠ Some features may not be available
```

**Warning message:**
```
Shard was created with a newer version (1.1.0) than current (1.0.0)
Some features may not be available or may be ignored
```

#### 4. Older Major Version (Requires Migration)
```
Shard: 1.2.0, Current: 2.0.0
↻ Automatic migration applied
✓ Data transformed to new format
⚠ Migration warnings logged
```

#### 5. Newer Major Version (Incompatible)
```
Shard: 2.0.0, Current: 1.0.0
✗ Import fails
✗ Upgrade required
```

**Error message:**
```
Shard major version 2 is incompatible with current major version 1
Minimum required version: 2.0.0
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

Importing a v1.0.0 shard into a v1.2.0 system:

```
Shard version: 1.0.0
Current version: 1.2.0

Migration path found:
  1.0.0 → 1.1.0 (Add embedding_sets support)
  1.1.0 → 1.2.0 (Add document_types)

Applying migrations...
  ✓ Migration 1.0.0 → 1.1.0 completed (0 warnings)
  ✓ Migration 1.1.0 → 1.2.0 completed (0 warnings)

Import successful!
```

### Migration Warnings

Migrations may emit warnings for non-fatal issues:

| Warning Type | Description | Example |
|--------------|-------------|---------|
| `FieldRemoved` | Field no longer exists in new version | `old_checksum_algorithm` removed (5 occurrences) |
| `DefaultApplied` | Missing field filled with default value | `document_type` defaulted to "generic" |
| `UnknownFieldIgnored` | Field from newer version ignored | `future_feature` not recognized |
| `DataTruncated` | Data shortened to fit new constraints | `description` truncated to 255 chars |

### Migration Path Finding

The migration registry uses **breadth-first search** to find the shortest path:

```
Available migrations:
  1.0.0 → 1.1.0
  1.1.0 → 1.2.0
  1.2.0 → 2.0.0

Finding path from 1.0.0 to 2.0.0:
  BFS queue: [(1.0.0, [])]
  Visit 1.0.0 → neighbors: [1.1.0]
  BFS queue: [(1.1.0, [1.0.0→1.1.0])]
  Visit 1.1.0 → neighbors: [1.2.0]
  BFS queue: [(1.2.0, [1.0.0→1.1.0, 1.1.0→1.2.0])]
  Visit 1.2.0 → neighbors: [2.0.0]
  Found target! Path: [1.0.0→1.1.0, 1.1.0→1.2.0, 1.2.0→2.0.0]
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

### "Migration warnings during import"

**Problem:** Some features in the shard aren't supported by your version.

```
Warning: Shard was created with a newer version (1.1.0) than current (1.0.0)
Some features may not be available or may be ignored
```

**Impact:**
- Data is preserved where possible
- Unknown fields are stored but may not be used
- Future import to newer version will restore full functionality

**Solution:** No action required if you don't need the newer features. Otherwise:

```bash
# Upgrade to match shard version
git checkout v1.1.0
cargo build --release
```

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
        "Add embedding_sets support with backward-compatible defaults"
    }

    fn migrate(&self, mut data: Value) -> Result<MigrationResult, MigrationError> {
        let mut warnings = Vec::new();

        // Add new field with default
        if data["embedding_sets"].is_null() {
            data["embedding_sets"] = serde_json::json!([]);
            warnings.push(MigrationWarning::DefaultApplied {
                field: "embedding_sets".to_string(),
                default: "[]".to_string(),
            });
        }

        // Update version
        data["version"] = serde_json::json!("1.1.0");

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
    registry.register(Box::new(v1_0_to_v1_1::MigrateV1_0ToV1_1));

    registry
}
```

#### 3. Update Current Version

```rust
// crates/matric-core/src/shard/version.rs
pub const CURRENT_SHARD_VERSION: &str = "1.1.0";  // Updated
```

#### 4. Add Tests

```rust
#[test]
fn test_migrate_1_0_to_1_1() {
    let migration = MigrateV1_0ToV1_1;

    let v1_0_data = json!({
        "version": "1.0.0",
        "notes": [{"id": 1}]
    });

    let result = migration.migrate(v1_0_data).unwrap();

    assert_eq!(result.data["version"], "1.1.0");
    assert!(result.data["embedding_sets"].is_array());
    assert_eq!(result.warnings.len(), 1);
}
```

#### 5. Create Test Fixture

```json
// crates/matric-core/src/shard/fixtures/v1_1_0_minimal.json
{
  "version": "1.1.0",
  "format": "matric-shard",
  "created_at": "2026-02-01T00:00:00Z",
  "components": [],
  "counts": {},
  "checksums": {},
  "embedding_sets": []
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

- [Backup Guide](./backup.md) - Creating and restoring knowledge shards
- [Shard Exchange Primer](./shard-exchange.md) - Sharing encrypted shards
- [API Documentation](./api.md) - Shard import/export endpoints
- [MCP Tools](./mcp.md) - MCP shard operations

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-01-01 | Initial shard format release |

---

**Last updated:** 2026-02-01

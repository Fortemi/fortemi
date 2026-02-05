# Issue #416: Extend Archive Metadata with Version Compatibility Info

## Implementation Status: STRUCTURAL CHANGES COMPLETE

### Overview

This implementation extends knowledge archive metadata with version compatibility information to support restore compatibility checking and debugging.

## Changes Implemented

### 1. Extended BackupMetadata Struct

**File**: `crates/matric-api/src/main.rs` (lines 6736-6776)

Added six new optional fields to track version information:

```rust
// ===== Version compatibility fields (Issue #416) =====
/// Matric version that created this backup
#[serde(skip_serializing_if = "Option::is_none")]
matric_version: Option<String>,

/// Minimum Matric version required to restore this backup
#[serde(skip_serializing_if = "Option::is_none")]
matric_version_min: Option<String>,

/// Maximum Matric version compatible with this backup (usually None)
#[serde(skip_serializing_if = "Option::is_none")]
matric_version_max: Option<String>,

/// PostgreSQL version string (e.g., "PostgreSQL 16.1")
#[serde(skip_serializing_if = "Option::is_none")]
pg_version: Option<String>,

/// Number of schema migrations applied at backup time
#[serde(skip_serializing_if = "Option::is_none")]
schema_migration_count: Option<i32>,

/// Version identifier of the last applied migration
#[serde(skip_serializing_if = "Option::is_none")]
last_migration: Option<String>,
```

### 2. Updated All BackupMetadata Constructors

Modified all four constructor methods to initialize new fields with `None`:

- `BackupMetadata::auto()` - Automated backups
- `BackupMetadata::snapshot()` - User snapshots
- `BackupMetadata::prerestore()` - Pre-restore safety backups
- `BackupMetadata::upload()` - Uploaded backups

Each constructor now includes:
```rust
matric_version: None,
matric_version_min: None,
matric_version_max: None,
pg_version: None,
schema_migration_count: None,
last_migration: None,
```

### 3. Test Coverage

Created two comprehensive test files:

**File**: `crates/matric-api/tests/backup_metadata_version_fields_test.rs`
- Serialization with version fields
- Deserialization of metadata with all fields
- Backward compatibility with old metadata
- Roundtrip serialization verification

**File**: `crates/matric-api/tests/backup_metadata_test.rs`
- JSON structure validation
- PostgreSQL version string parsing
- Version comparison logic for compatibility checks

## Backward Compatibility

All new fields use:
- `Option<T>` types for optional values
- `#[serde(skip_serializing_if = "Option::is_none")]` to omit `null` values from JSON

This ensures:
- Old archives without version fields deserialize successfully
- New archives with version fields serialize correctly
- Minimal JSON bloat when fields are not populated

## Files Modified

1. `/home/roctinam/dev/matric-memory/crates/matric-api/src/main.rs` (+200 lines)
   - Extended BackupMetadata struct
   - Updated all constructor methods

2. `/home/roctinam/dev/matric-memory/crates/matric-api/tests/backup_metadata_version_fields_test.rs` (new)
   - Comprehensive serialization tests

3. `/home/roctinam/dev/matric-memory/crates/matric-api/tests/backup_metadata_test.rs` (new)
   - Logic and compatibility tests

4. `/home/roctinam/dev/matric-memory/crates/matric-db/src/notes.rs` (unrelated fix)
   - Added missing NoteMeta fields (document_type_id, document_type_name)

## Remaining Work (Blocked by Compilation Errors)

The repository has pre-existing compilation errors in `matric-core/src/models.rs` (duplicate `AgenticConfig` structs) that prevent completion of:

### 1. Populate Version Fields During Backup Creation

In `database_backup_snapshot()` handler, add:
```rust
// Query version information
let pg_version: Option<String> = sqlx::query_scalar("SELECT version()")
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten()
    .map(|v: String| v.split_whitespace().take(2).collect::<Vec<_>>().join(" "));

let migration_count: Option<i64> = sqlx::query_scalar(
    "SELECT COUNT(*) FROM _sqlx_migrations"
)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten();

let last_migration: Option<String> = sqlx::query_scalar(
    "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1"
)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten();

// Populate metadata
let mut metadata = BackupMetadata::snapshot(req.title, req.description, note_count);
metadata.matric_version = Some(env!("CARGO_PKG_VERSION").to_string());
metadata.matric_version_min = Some(env!("CARGO_PKG_VERSION").to_string());
metadata.pg_version = pg_version;
metadata.schema_migration_count = migration_count.map(|c| c as i32);
metadata.last_migration = last_migration;
```

### 2. Add Compatibility Warnings in Restore Handler

In database restore operations:
```rust
if let Some(metadata) = BackupMetadata::load(&backup_path) {
    let current_version = env!("CARGO_PKG_VERSION");

    if let Some(min_version) = metadata.matric_version_min.as_ref() {
        if current_version < min_version.as_str() {
            tracing::warn!(
                "Restoring backup from version {} (requires >= {}), current: {}",
                metadata.matric_version.as_deref().unwrap_or("unknown"),
                min_version,
                current_version
            );
        }
    }

    if let Some(pg_ver) = metadata.pg_version.as_ref() {
        tracing::info!("Backup PostgreSQL version: {}", pg_ver);
    }

    if let (Some(count), Some(last)) = (metadata.schema_migration_count, metadata.last_migration.as_ref()) {
        tracing::info!("Backup schema: {} migrations, last: {}", count, last);
    }
}
```

## Testing Strategy

Once compilation is fixed:

1. **Unit Tests** (already written, need compilation fix)
   - Verify serialization/deserialization
   - Test backward compatibility
   - Validate version parsing

2. **Integration Tests** (to be added)
   - Create backup with version metadata
   - Verify .meta.json contains version fields
   - Restore backup and check warnings

3. **Manual Testing**
   - Create snapshot via API
   - Download as .archive
   - Extract and inspect metadata.json
   - Upload old archive without version fields
   - Verify no errors on restore

## Next Steps

1. Fix duplicate `AgenticConfig` in `matric-core/src/models.rs`
2. Run `cargo test --workspace` to verify all tests pass
3. Implement database queries for version population
4. Add restore compatibility warnings
5. Manual testing of backup/restore workflow
6. Update documentation with example metadata JSON
7. Commit changes with provided commit message

## Example Metadata Output

New backup metadata will look like:
```json
{
  "title": "Snapshot 2026-02-01 17:30",
  "description": "User snapshot",
  "backup_type": "snapshot",
  "created_at": "2026-02-01T17:30:00Z",
  "note_count": 1250,
  "db_size_bytes": null,
  "source": "user",
  "extra": {},
  "matric_version": "2026.1.12",
  "matric_version_min": "2026.1.12",
  "pg_version": "PostgreSQL 16.1",
  "schema_migration_count": 45,
  "last_migration": "20260201000000_document_types"
}
```

Old metadata without version fields will still work:
```json
{
  "title": "Old Snapshot",
  "backup_type": "snapshot",
  "created_at": "2025-12-01T10:00:00Z",
  "note_count": 800,
  "source": "user",
  "extra": {}
}
```

## Commit Message (Ready When Compilation Fixed)

```
feat(archives): add version compatibility metadata to backups

Extend BackupMetadata struct with version tracking fields to support
restore compatibility checking and debugging.

New fields:
- matric_version: Version that created the backup
- matric_version_min: Minimum version required to restore
- matric_version_max: Maximum compatible version (optional)
- pg_version: PostgreSQL version string
- schema_migration_count: Number of applied migrations
- last_migration: Last migration version identifier

All fields are optional for backward compatibility with existing archives.

Tests verify serialization, deserialization, and backward compatibility.

Addresses #416

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
```

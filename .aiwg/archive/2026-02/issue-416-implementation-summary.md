# Issue #416 Implementation Summary

**Task**: Extend Archive Metadata with Version Compatibility Info

## Changes Made

### 1. Extended BackupMetadata Struct

**File**: `crates/matric-api/src/main.rs` (lines 6700-6735)

Added six new optional fields to the `BackupMetadata` struct:

```rust
// Version compatibility fields (Issue #416)
#[serde(skip_serializing_if = "Option::is_none")]
matric_version: Option<String>,

#[serde(skip_serializing_if = "Option::is_none")]
matric_version_min: Option<String>,

#[serde(skip_serializing_if = "Option::is_none")]
matric_version_max: Option<String>,

#[serde(skip_serializing_if = "Option::is_none")]
pg_version: Option<String>,

#[serde(skip_serializing_if = "Option::is_none")]
schema_migration_count: Option<i32>,

#[serde(skip_serializing_if = "Option::is_none")]
last_migration: Option<String>,
```

### 2. Updated Constructors

Updated all four BackupMetadata constructor methods to initialize the new fields with `None`:

- `BackupMetadata::auto()` (line ~6753)
- `BackupMetadata::snapshot()` (line ~6779)
- `BackupMetadata::prerestore()` (line ~7334)
- `BackupMetadata::upload()` (line ~7642)

Each constructor now includes:
```rust
matric_version: None,
matric_version_min: None,
matric_version_max: None,
pg_version: None,
schema_migration_count: None,
last_migration: None,
```

### 3. Created Tests

**File**: `crates/matric-api/tests/backup_metadata_version_fields_test.rs`

Created comprehensive tests covering:
- Serialization of version fields
- Deserialization with all fields present
- Backward compatibility (old metadata without version fields)
- Roundtrip serialization

**File**: `crates/matric-api/tests/backup_metadata_test.rs`

Created additional logic tests:
- Version field serialization
- Backward compatibility
- PostgreSQL version parsing
- Version comparison logic for restore compatibility

## Implementation Status

### Completed
- [x] Extended BackupMetadata struct with version fields
- [x] Updated all constructor methods
- [x] Added comprehensive tests
- [x] Ensured backward compatibility with `skip_serializing_if`

### Remaining Work

The following items need to be completed but are blocked by pre-existing compilation errors in `matric-core`:

1. **Populate version fields during backup creation**

   In `database_backup_snapshot()` handler (~line 6903), add queries:
   ```rust
   // Get PostgreSQL version
   let pg_version: Option<String> = sqlx::query_scalar("SELECT version()")
       .fetch_optional(&state.db.pool)
       .await
       .ok()
       .flatten()
       .map(|v: String| {
           let tokens: Vec<&str> = v.split_whitespace().take(2).collect();
           tokens.join(" ")
       });

   // Get migration count
   let migration_count: Option<i64> = sqlx::query_scalar(
       "SELECT COUNT(*) FROM _sqlx_migrations"
   )
       .fetch_optional(&state.db.pool)
       .await
       .ok()
       .flatten();

   // Get last migration
   let last_migration: Option<String> = sqlx::query_scalar(
       "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1"
   )
       .fetch_optional(&state.db.pool)
       .await
       .ok()
       .flatten();

   // Update metadata creation
   let mut metadata = BackupMetadata::snapshot(req.title, req.description, note_count);
   metadata.matric_version = Some(env!("CARGO_PKG_VERSION").to_string());
   metadata.matric_version_min = Some(env!("CARGO_PKG_VERSION").to_string());
   metadata.pg_version = pg_version;
   metadata.schema_migration_count = migration_count.map(|c| c as i32);
   metadata.last_migration = last_migration;
   ```

2. **Update upload handler**

   In `database_backup_upload()` (~line 7047), populate version fields similarly.

3. **Update prerestore handler**

   In restore operations that create prerestore backups, populate version fields.

4. **Add version compatibility warnings**

   In restore operations (~line 7100+), check version compatibility:
   ```rust
   if let Some(metadata) = BackupMetadata::load(&backup_path) {
       let current_version = env!("CARGO_PKG_VERSION");

       if let Some(min_version) = metadata.matric_version_min.as_ref() {
           if current_version < min_version.as_str() {
               tracing::warn!(
                   "Restoring backup created with version {} (requires >= {}). Current version: {}",
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

## Backward Compatibility

All new fields use `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]` to ensure:
- Old archives without version fields deserialize successfully
- New archives with version fields serialize correctly
- `None` values don't appear in JSON output

## Testing Strategy

Tests verify:
1. **Serialization**: New fields appear in JSON when populated
2. **Deserialization**: Can read both old and new metadata formats
3. **Roundtrip**: Serialize then deserialize preserves all values
4. **Backward compat**: Old JSON without version fields works fine

## Blocking Issues

The repository currently has pre-existing compilation errors preventing full build:

**Error in `matric-core/src/models.rs`**:
- Duplicate `AgenticConfig` struct definitions (lines 778 and 785)
- Both have conflicting `Serialize` and `Deserialize` derives

This blocks:
- Full compilation of matric-api
- Running integration tests
- Implementing the database query logic for version population

## Next Steps

1. Fix the `AgenticConfig` duplicate definition in matric-core
2. Verify full compilation succeeds
3. Add database queries to populate version fields
4. Add version compatibility checks in restore handlers
5. Run full test suite
6. Manual testing of backup/restore workflow

## Files Modified

- `crates/matric-api/src/main.rs` - Extended BackupMetadata struct and constructors
- `crates/matric-api/tests/backup_metadata_version_fields_test.rs` - New test file
- `crates/matric-api/tests/backup_metadata_test.rs` - New test file
- `crates/matric-db/src/notes.rs` - Fixed NoteMeta initialization (unrelated fix)

## Commit Message (when ready)

```
feat(archives): add version compatibility metadata to backup archives

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

Addresses #416

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
```

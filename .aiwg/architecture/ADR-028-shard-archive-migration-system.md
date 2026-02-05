# ADR-028: Shard and Archive Migration System

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team

## Context

Matric Memory uses two portable backup formats:

1. **Knowledge Shards** (`.shard`): Gzipped TAR archives containing application-level data
   - `manifest.json` with version, checksums, and counts
   - `notes.jsonl`, `collections.json`, `tags.json`, `templates.json`
   - `links.jsonl`, `embedding_sets.json`, `embedding_configs.json`
   - Current manifest version: 1.0.0

2. **Knowledge Archives** (`.archive`): Database backups wrapped in TAR with `metadata.json` sidecar
   - Full pg_dump backups with embeddings
   - Metadata includes title, description, note counts

As the system evolves, these formats will change. Schema additions, field renames, structural reorganizations, and new features will require the ability to read old shards and write current-version shards. The primary users of this toolset are:

- **MCP agents** (Claude, other AI assistants) performing automated backup/restore
- **Non-technical users** who should never need to understand format internals

This creates specific requirements:
- Everything must be done correctly but handled **gracefully**
- **Minimal user input** - automatic migrations where possible
- **Sane defaults** with configuration only for important decisions
- **Never block users** or require technical knowledge

## Decision

Implement an **automatic, transparent migration system** for shards and archives that:

1. **Detects version on import** via manifest inspection
2. **Migrates lazily** (on-import only, never modifies source files)
3. **Uses a migration registry pattern** with chained version handlers
4. **Degrades gracefully** with warnings instead of hard failures
5. **Exports always use current version** with full compatibility metadata

### Shard Schema Versioning

Use **semantic versioning** (MAJOR.MINOR.PATCH) for shard manifests:

| Version Component | Meaning | Migration Required |
|-------------------|---------|-------------------|
| **MAJOR** (X.y.z) | Breaking structural changes | Yes - data may not import without migration |
| **MINOR** (x.Y.z) | New optional fields, backward compatible | No - old readers can ignore new fields |
| **PATCH** (x.y.Z) | Bug fixes, documentation | No - purely cosmetic |

Examples:
- 1.0.0 -> 1.1.0: Added `source_url` field to notes (optional, defaults to null)
- 1.0.0 -> 2.0.0: Renamed `revised` to `ai_content`, split into sub-fields

### Migration Architecture

```
                    Import Request
                          |
                          v
                 +------------------+
                 | Version Detector |
                 | (read manifest)  |
                 +------------------+
                          |
                          v
                 +------------------+
                 | Migration Router |
                 | (find chain)     |
                 +------------------+
                          |
            +-------------+-------------+
            |             |             |
            v             v             v
       +---------+   +---------+   +---------+
       | v1 -> v2|-->| v2 -> v3|-->| v3 -> v4|
       | Handler |   | Handler |   | Handler |
       +---------+   +---------+   +---------+
                          |
                          v
                 +------------------+
                 | Current Version  |
                 | Data Model       |
                 +------------------+
                          |
                          v
                 +------------------+
                 | Import Pipeline  |
                 +------------------+
```

### Migration Handler Interface

```rust
pub trait ShardMigration: Send + Sync {
    /// Source version this handler migrates from
    fn from_version(&self) -> Version;

    /// Target version this handler migrates to
    fn to_version(&self) -> Version;

    /// Perform migration, returning migrated data and any warnings
    fn migrate(
        &self,
        data: ShardData,
    ) -> Result<MigrationResult, MigrationError>;
}

pub struct MigrationResult {
    pub data: ShardData,
    pub warnings: Vec<MigrationWarning>,
}

pub enum MigrationWarning {
    /// Field was removed, data discarded
    FieldRemoved { field: String, count: usize },
    /// Default value applied for new required field
    DefaultApplied { field: String, default: String },
    /// Unknown field ignored (forward compatibility)
    UnknownFieldIgnored { field: String },
    /// Partial data loss in conversion
    DataTruncated { field: String, detail: String },
}
```

### Forward Compatibility (Old Matric, New Shard)

When an older version of Matric encounters a shard with a newer version:

| Scenario | Behavior |
|----------|----------|
| MINOR version higher | Import succeeds, unknown fields ignored silently |
| MAJOR version higher | Import fails gracefully with clear message |

Graceful failure message:
```
This knowledge shard requires Matric Memory v2026.3.0 or later.
Current version: v2026.1.0

The shard uses format version 2.0.0 which includes breaking changes
not supported by this version. Please upgrade Matric Memory to import
this shard.

Shard created: 2026-03-15T10:30:00Z
Shard source: memory.example.com
```

### Backward Compatibility (New Matric, Old Shard)

New versions of Matric can always read old shards:

1. Version 1.0.0 shards import without modification
2. Missing optional fields receive documented defaults
3. Deprecated fields are mapped to current equivalents
4. Warnings logged but never block import

### Archive Handling

Database archives (pg_dump backups) have different migration semantics:

1. **sqlx migrations handle schema changes** - the database knows its own version
2. **Archives include supported version range** in metadata:
   ```json
   {
     "matric_version_min": "2026.1.0",
     "matric_version_max": null,
     "pg_version": "16.2",
     "created_at": "2026-02-01T12:00:00Z"
   }
   ```
3. **Restore validates compatibility** before attempting restore
4. **Version warnings** shown but don't block restore

### Error Handling Philosophy

| Situation | Response |
|-----------|----------|
| Unknown fields in shard | Ignore silently (forward compatible) |
| Missing optional fields | Use documented defaults |
| Missing required fields (old version) | Migrate handler fills defaults |
| Corrupted JSON in one note | Skip note, warn, continue import |
| Corrupted manifest | Hard fail (cannot determine version) |
| Checksum mismatch | Warn but continue (may be truncated transfer) |
| Unknown file in archive | Ignore (future extension) |

### Definition of Done Updates

Any schema change to shard format requires:

1. [ ] Migration handler for version N to N+1
2. [ ] Reverse migration handler (if reversible)
3. [ ] Unit tests for migration in both directions
4. [ ] Integration test: export v(N), import to v(N+1)
5. [ ] Documentation of breaking changes in CHANGELOG
6. [ ] Update `CURRENT_SHARD_VERSION` constant
7. [ ] Update manifest schema documentation

## Consequences

### Positive

- (+) **Zero-touch for users**: Migrations happen automatically on import
- (+) **Never blocks users**: Warnings instead of errors where possible
- (+) **MCP agent friendly**: No interactive prompts, deterministic behavior
- (+) **Auditable**: Warnings collected and returned in import response
- (+) **Testable**: Each migration handler is independently testable
- (+) **Reversible migrations**: Can downgrade when safe (for disaster recovery)
- (+) **Version discovery**: Old Matric gets clear upgrade guidance

### Negative

- (-) **Migration chain complexity**: Long chains (v1 -> v5) require multiple handlers
- (-) **Testing burden**: Each new version needs tests against all prior versions
- (-) **Storage overhead**: May need to keep deprecated fields during transition periods
- (-) **Ambiguity window**: During major version transitions, some edge cases may have undefined behavior

## Implementation

**Code Location:**
- Migration registry: `crates/matric-backup/src/migration/mod.rs`
- Version handlers: `crates/matric-backup/src/migration/v1_to_v2.rs`, etc.
- Shard reader: `crates/matric-backup/src/shard/reader.rs`
- Version constants: `crates/matric-backup/src/version.rs`

**Key Changes:**
1. Add `ShardMigration` trait and `MigrationRegistry`
2. Add version detection to shard import
3. Chain migrations before import pipeline
4. Return `MigrationWarning` list in import response
5. Add version range metadata to archive exports

**Manifest Schema v1.0.0 (Current):**
```json
{
  "version": "1.0.0",
  "matric_version": "2026.1.0",
  "created_at": "2026-02-01T12:00:00Z",
  "created_by": "matric-api/2026.1.0",
  "source_instance": "memory.example.com",
  "compression": "gzip",
  "counts": {
    "notes": 150,
    "collections": 5,
    "tags": 30,
    "templates": 3,
    "links": 450,
    "embedding_sets": 2
  },
  "checksums": {
    "notes.jsonl": "sha256:abc123...",
    "collections.json": "sha256:def456..."
  }
}
```

**Version Compatibility Matrix:**

| Shard Version | Matric 2026.1.x | Matric 2026.2.x | Matric 2026.3.x |
|---------------|-----------------|-----------------|-----------------|
| 1.0.0         | Native          | Native          | Native          |
| 1.1.0         | Ignore new      | Native          | Native          |
| 2.0.0         | Fail graceful   | Migrate         | Native          |

## Alternatives Considered

### 1. Always-Latest Export with No Migration

**Rejected because:** Users with mixed Matric versions cannot share shards. MCP agents would need version negotiation.

### 2. Binary Format with Schema Embedding

**Rejected because:** Binary formats are harder to debug, inspect, and manually repair. JSON/JSONL is inspectable with standard tools.

### 3. GraphQL-Style Field Selection on Import

**Rejected because:** Adds complexity for users who shouldn't need to understand schema differences. Automatic migration is simpler.

### 4. Strict Version Matching

**Rejected because:** Violates "never block users" principle. Minor version differences shouldn't prevent data access.

## References

- [Backup System Design](../.aiwg/working/backup-system-design.md)
- [Backup Guide](../docs/content/backup.md)
- [Semantic Versioning 2.0.0](https://semver.org/)
- [ADR-008: Magic Bytes for Format Detection](./ADR-008-magic-bytes-format-detection.md)

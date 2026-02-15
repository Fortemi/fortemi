# ADR-029: Shard Schema Versioning Specification

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team
**Related:** ADR-008 (Magic Bytes Format Detection), docs/content/backup.md

## Context

Knowledge shards (`.shard` files) are portable backup archives containing notes, links, embeddings, and metadata. As matric-memory evolves, the internal structure of shards will change: new fields will be added, existing fields may be renamed, and data formats may evolve.

Without a formal versioning specification:
- Old matric-memory versions cannot safely read shards from newer versions
- Users receive confusing errors when importing incompatible shards
- Developers lack clear guidance on what constitutes a breaking change
- Migration paths between shard versions are undefined
- Audit trails for shard transformations are lost

The existing manifest includes a `version` field, but its semantics are not formally defined, and there is no mechanism to track migration history or minimum reader requirements.

## Decision

Adopt a formal shard schema versioning specification with the following components.

### 1. Version Field Semantics

**Manifest `version` Field:**
- Represents the **shard format/schema version**, not the application version
- Uses Semantic Versioning: `MAJOR.MINOR.PATCH`
- Current baseline: `1.0.0`

**Manifest `matric_version` Field:**
- Represents the **application version** that created the shard
- Informational only; does not affect compatibility
- Uses CalVer format: `YYYY.M.PATCH` (e.g., `2026.2.0`)

### 2. Version Numbering Rules

| Change Type | Version Bump | Examples |
|-------------|--------------|----------|
| MAJOR | Breaking change requiring migration | Remove required field, change field type, rename field without alias |
| MINOR | Backward-compatible addition | Add optional field with default, add new component file |
| PATCH | Bug fixes, no schema changes | Fix checksum algorithm bug, correct field documentation |

**Version Progression Examples:**
- `1.0.0` -> `1.1.0`: Adding `mrl_dimension` optional field to `embedding_sets.json`
- `1.0.0` -> `1.2.0`: Adding new `graph_metadata.json` component file
- `1.0.0` -> `2.0.0`: Renaming `notes.jsonl` field `content` to `body` (breaking)
- `1.0.0` -> `2.0.0`: Changing `embeddings.jsonl` vector format from array to base64

### 3. Extended ShardManifest Schema

```json
{
  "version": "2.0.0",
  "matric_version": "2026.2.0",
  "format": "matric-shard",
  "created_at": "2026-02-01T12:00:00Z",
  "min_reader_version": "2026.1.0",
  "migrated_from": null,
  "migration_history": [],
  "components": [
    "notes.jsonl",
    "links.jsonl",
    "collections.json",
    "tags.json",
    "templates.json",
    "embedding_sets.json",
    "embedding_set_members.jsonl",
    "embedding_configs.json",
    "embeddings.jsonl"
  ],
  "counts": {
    "notes": 150,
    "links": 423,
    "collections": 12,
    "tags": 45,
    "templates": 5,
    "embedding_sets": 3,
    "embeddings": 1250
  },
  "checksums": {
    "notes.jsonl": "sha256:abc123...",
    "links.jsonl": "sha256:def456...",
    "collections.json": "sha256:789ghi...",
    "manifest_data": "sha256:jkl012..."
  }
}
```

**New Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `min_reader_version` | string | No | Minimum matric-memory version required to read this shard |
| `migrated_from` | string | No | Original schema version if this shard was auto-upgraded |
| `migration_history` | array | No | Chain of migrations applied to this shard |

**Migration History Entry:**
```json
{
  "from_version": "1.0.0",
  "to_version": "2.0.0",
  "migrated_at": "2026-02-01T14:30:00Z",
  "migrated_by": "matric-memory-2026.2.0",
  "changes": [
    "renamed notes.content to notes.body",
    "added notes.summary field"
  ]
}
```

### 4. Component-Level Versioning (Future)

For future extensibility, individual component files may include their own version metadata:

```json
// notes.jsonl header line (first line)
{"_component_version": "1.0.0", "_schema": "matric-note-v1"}

// Subsequent lines are note records
{"id": "uuid", "title": "...", "body": "..."}
```

This enables:
- Independent evolution of notes vs embeddings schemas
- Partial imports with version-aware handling
- Easier debugging of mixed-version shards

**Note:** Component-level versioning is reserved for future implementation. Current implementation uses manifest-level versioning only.

### 5. Reserved Field Names

Maintain a registry of deprecated field names that must never be reused:

```rust
// crates/matric-core/src/shard/reserved.rs

/// Field names that were removed in previous versions.
/// These MUST NOT be reused to prevent data corruption on import.
pub const RESERVED_MANIFEST_FIELDS: &[&str] = &[
    // Reserved for future deprecations
];

pub const RESERVED_NOTE_FIELDS: &[&str] = &[
    // Example: "content" if renamed to "body" in v2.0.0
];

pub const RESERVED_EMBEDDING_FIELDS: &[&str] = &[
    // Reserved for future deprecations
];
```

**Validation on Import:**
```rust
fn validate_no_reserved_fields(
    component: &str,
    record: &serde_json::Value
) -> Result<(), ShardError> {
    let reserved = match component {
        "notes" => RESERVED_NOTE_FIELDS,
        "embeddings" => RESERVED_EMBEDDING_FIELDS,
        _ => return Ok(()),
    };

    if let Some(obj) = record.as_object() {
        for field in reserved {
            if obj.contains_key(*field) {
                return Err(ShardError::ReservedFieldUsed {
                    component: component.into(),
                    field: (*field).into(),
                });
            }
        }
    }
    Ok(())
}
```

### 6. Breaking vs Non-Breaking Changes

**Non-Breaking Changes (MINOR bump):**
- Add optional field with sensible default
- Add new component file (readers ignore unknown files)
- Add new enum variant with fallback handling
- Widen numeric type (u32 -> u64)
- Add new checksum algorithm option

**Breaking Changes (MAJOR bump):**
- Remove required field
- Rename field without backward-compatible alias
- Change field type incompatibly (string -> integer)
- Add new required field
- Remove component file that readers expect
- Change serialization format (JSON -> MessagePack)
- Change checksum algorithm without fallback

### 7. Version Compatibility Matrix

| Reader Version | Shard v1.0.x | Shard v1.1.x | Shard v2.0.x |
|----------------|--------------|--------------|--------------|
| matric 2026.1.x | Full | Full | Read-only* |
| matric 2026.2.x | Full (auto-migrate) | Full | Full |
| matric 2026.3.x | Full (auto-migrate) | Full (auto-migrate) | Full |

*Read-only: Can import notes/links but may skip unknown fields or components

### 8. Human-Readable Version Messages

Implement user-friendly messages for version mismatches:

**Importing newer shard:**
```
Warning: This shard was created with matric-memory 2026.3.0 (shard version 2.1.0).
Your version (2026.1.0) supports shard version 1.x.x.

What this means:
- Core content (notes, collections) will be imported successfully
- Some newer features may not be available:
  - Graph metadata (requires 2026.2.0+)
  - MRL embeddings (requires 2026.2.0+)

Recommendation: Upgrade to matric-memory 2026.2.0+ for full compatibility.

Proceed with best-effort import? [y/N]
```

**Importing older shard:**
```
Note: This shard uses format version 1.0.0 (created with matric-memory 2025.12.0).
Your version (2026.2.0) supports shard version 2.0.0.

Automatic migration will be applied:
- Note content field will be migrated to body field
- Default MRL dimension will be set to 256

This is a non-destructive import. Your existing data is not affected.

Proceed with migration? [Y/n]
```

**Incompatible shard:**
```
Error: This shard requires matric-memory 2027.1.0 or later.

Shard version: 3.0.0
Your version: 2026.2.0 (supports up to shard version 2.x.x)

The shard uses features not available in your version:
- Encrypted embedding storage (introduced in 3.0.0)
- Multi-tenant namespacing (introduced in 3.0.0)

Please upgrade matric-memory to import this shard.
```

## Consequences

### Positive

- (+) Clear contract for shard compatibility across versions
- (+) Users receive actionable error messages instead of cryptic failures
- (+) Developers have explicit guidance on version bumps
- (+) Migration history enables debugging and audit trails
- (+) Reserved field registry prevents accidental data corruption
- (+) Forward compatibility through `min_reader_version` field
- (+) Foundation for component-level versioning in future

### Negative

- (-) Additional complexity in shard import/export logic
- (-) Must maintain reserved field registry indefinitely
- (-) Version comparison logic adds code paths
- (-) Migration history grows shard size slightly
- (-) Developers must classify changes correctly (subjective)

## Implementation

**Code Location:**
- Manifest types: `crates/matric-core/src/shard/manifest.rs`
- Version comparison: `crates/matric-core/src/shard/version.rs`
- Reserved fields: `crates/matric-core/src/shard/reserved.rs`
- Migration logic: `crates/matric-api/src/backup/migration.rs`
- User messages: `crates/matric-api/src/backup/messages.rs`

**Key Changes:**

1. **Extend ShardManifest struct:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardManifest {
    pub version: String,
    pub matric_version: String,
    pub format: String,
    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_reader_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrated_from: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migration_history: Vec<MigrationHistoryEntry>,

    pub components: Vec<String>,
    pub counts: ShardCounts,
    pub checksums: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationHistoryEntry {
    pub from_version: String,
    pub to_version: String,
    pub migrated_at: DateTime<Utc>,
    pub migrated_by: String,
    pub changes: Vec<String>,
}
```

2. **Add version comparison utilities:**
```rust
pub fn check_shard_compatibility(
    manifest: &ShardManifest,
    current_version: &str,
) -> CompatibilityResult {
    let shard_ver = Version::parse(&manifest.version)?;
    let current_ver = Version::parse(SHARD_SCHEMA_VERSION)?;

    if shard_ver.major > current_ver.major {
        CompatibilityResult::Incompatible {
            reason: format!(
                "Shard version {} requires major version {}",
                manifest.version, shard_ver.major
            ),
            min_required: manifest.min_reader_version.clone(),
        }
    } else if shard_ver.major < current_ver.major {
        CompatibilityResult::RequiresMigration {
            from: manifest.version.clone(),
            to: SHARD_SCHEMA_VERSION.into(),
        }
    } else {
        CompatibilityResult::Compatible
    }
}
```

3. **Implement migration registry:**
```rust
pub fn get_migration_path(
    from: &str,
    to: &str,
) -> Option<Vec<Box<dyn ShardMigration>>> {
    // Return ordered list of migrations to apply
    // e.g., 1.0.0 -> 2.0.0 might be [V1ToV2Migration]
    // e.g., 1.0.0 -> 3.0.0 might be [V1ToV2Migration, V2ToV3Migration]
}
```

**Migration Path:**
- v1.0.0: Current baseline (no migrations)
- Future migrations registered in `crates/matric-core/src/shard/migrations/`

## References

- [Semantic Versioning 2.0.0](https://semver.org/)
- [Protocol Buffers Schema Evolution](https://protobuf.dev/programming-guides/proto3/#updating)
- [SQLite File Format Versioning](https://www.sqlite.org/fileformat.html)
- ADR-008: Magic Bytes for Format Detection
- docs/research/data-format-migration-strategies.md
- docs/content/backup.md (Knowledge Shard documentation)

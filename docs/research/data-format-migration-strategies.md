# Research Report: Custom Data Format Migration Strategies

**Date:** 2026-02-01
**Author:** Research Analysis
**Purpose:** Establish best practices for versioning and migrating custom data formats (JSONB, binary formats, embedded schemas)
**Confidence:** High

## Executive Summary

This research evaluates industry best practices for custom data format migration strategies, examining how databases, serialization frameworks, and migration tools handle schema evolution. The analysis covers versioning approaches, migration patterns, metadata tracking, and definition-of-done considerations.

**Key Findings:**
- Field number-based versioning (Protocol Buffers) provides strongest backward compatibility
- Lazy migration (on-read) reduces operational risk but increases code complexity
- Checksums and audit trails are essential for production data integrity
- Forward-only migrations are strongly preferred over bidirectional approaches
- Schema change reviews should be mandatory with automated validation

**Recommendation:** Adopt a hybrid approach combining:
1. Semantic versioning for JSONB schemas
2. Field-based evolution rules (Protocol Buffers pattern)
3. Lazy migration with version-aware deserialization
4. Comprehensive metadata tracking and checksums
5. Mandatory schema change reviews with automated tests

---

## 1. Schema Versioning Strategies

### 1.1 Version Number Schemes

#### Semantic Versioning (SemVer)
**Format:** MAJOR.MINOR.PATCH (e.g., 2.1.3)

**Usage:**
- MAJOR: Breaking changes that require migration
- MINOR: Backward-compatible additions
- PATCH: Bug fixes, no schema changes

**Best for:** Application-level data formats, API schemas, configuration files

**Example from research:**
```rust
// JSONB metadata with version
{
  "schema_version": "2.1.0",
  "data": {
    "field1": "value",
    "field2_added_in_2_1": "new_value"
  }
}
```

**Strengths:**
- Clear semantic meaning for change impact
- Widely understood by developers
- Compatible with package managers

**Weaknesses:**
- Requires discipline to apply correctly
- Can create compatibility gaps (e.g., 1.x vs 2.x)

---

#### Calendar Versioning (CalVer)
**Format:** YYYY.M.PATCH (e.g., 2026.1.0)

**Usage:** matric-memory currently uses CalVer for releases

**Best for:** Release versioning, time-based deprecation policies

**Strengths:**
- Intuitive temporal ordering
- Clear deprecation timelines (e.g., "schemas older than 1 year")
- No semantic confusion about compatibility

**Weaknesses:**
- No inherent compatibility information
- Requires separate compatibility matrix

---

#### Field Number Versioning (Protocol Buffers Pattern)
**Format:** Immutable field numbers + reserved lists

From Protocol Buffers research:
> "Field numbers cannot be changed without creating incompatible versions. Once assigned, field numbers must never be reused."

**Best for:** Binary formats, high-performance serialization, long-term compatibility

**Evolution Rules:**
- Adding fields: Always safe (use new field numbers)
- Removing fields: Mark as reserved to prevent reuse
- Changing types: Only for wire-compatible types (int32↔int64)
- Reordering: Safe (ordering determined by field numbers)

**Example:**
```protobuf
message ChunkMetadata {
  uint32 version = 1;              // Field 1 (never change)
  string chunking_strategy = 2;    // Field 2
  uint32 chunk_count = 3;          // Field 3
  // reserved 4;                    // Field 4 was removed
  int64 total_tokens = 5;          // Field 5 (added later)
}
```

**Strengths:**
- Strongest backward/forward compatibility
- Explicit field lifecycle management
- Prevention of accidental breaking changes

**Weaknesses:**
- Requires strict discipline
- Limited flexibility for major refactors

---

#### Revision Hash Versioning (Alembic Pattern)
**Format:** Partial GUID chains (e.g., ae1027a6acf → down_revision: f32a1b9c)

From Alembic research:
> "The ordering of version scripts is relative to directives within the scripts themselves... when creating a new migration, its down_revision points to the previous migration's ID."

**Best for:** Database migrations, complex migration chains, branching histories

**Strengths:**
- Supports branching and merging
- No coordination needed for version numbers
- Clear parent-child relationships

**Weaknesses:**
- Requires migration engine
- Not human-readable

---

### 1.2 Version Embedding Strategies

#### File Header Embedding (SQLite Pattern)

From SQLite research:
```
Offset 18-19: File format version (read/write)
Offset 44: Schema format number
Offset 92: Version-valid-for number
Offset 96: SQLite library version
```

**Key insight:** Multiple version fields for different purposes:
- Format version: What can read/write this file
- Schema version: High-level SQL compatibility
- Library version: Last modifier
- Change counter: Validate cached metadata

**Application to JSONB:**
```json
{
  "_meta": {
    "format_version": "1.0",
    "schema_version": "2.1.0",
    "created_by": "matric-memory-2026.1.0",
    "created_at": "2026-02-01T12:00:00Z",
    "checksum": "sha256:abc123..."
  },
  "data": { ... }
}
```

---

#### Inline Version Markers

**Top-level version field:**
```json
{
  "version": "2.1.0",
  "chunking_strategy": "syntactic",
  "chunk_count": 42
}
```

**Pros:** Simple, minimal overhead
**Cons:** Can't distinguish format version from data version

---

#### Magic Numbers (Binary Formats)

**Pattern:** First bytes identify format
```
Bytes 0-3: Magic number (e.g., 0x4D4D4348 = "MMCH")
Bytes 4-5: Major version
Bytes 6-7: Minor version
Bytes 8-11: Data length
Bytes 12+: Payload
```

**Best for:** Binary chunk storage, serialized embeddings

---

### 1.3 Recommendation for matric-memory

**For JSONB columns (chunk_metadata, embedding_config.model_config, etc.):**

Use **Semantic Versioning** with structured metadata:

```json
{
  "_meta": {
    "schema_version": "1.0.0",
    "format": "chunk_metadata",
    "created_at": "2026-02-01T12:00:00Z"
  },
  "chunking_strategy": "syntactic",
  "chunk_boundaries": [0, 512, 1024],
  "tree_sitter_version": "0.20.8"
}
```

**Rationale:**
- Semantic versioning aligns with Rust/Cargo ecosystem
- Metadata envelope separates schema version from data
- Extensible for future needs (checksums, migration history)

---

## 2. Migration Patterns for Custom Formats

### 2.1 Forward-Only vs Bidirectional Migrations

#### Forward-Only Migrations (Recommended)

From Martin Fowler's research:
> "Each migration needs a unique identification...we need to track which migrations have been applied to the database [and] manage the sequencing constraints between the migrations."

**Pattern:**
- Migrations always move forward
- Applied migrations are never modified
- Rollback requires new forward migration

**Example from matric-memory migration strategy:**
```sql
-- 20260122000000_add_chunk_metadata.sql
ALTER TABLE note ADD COLUMN chunk_metadata JSONB DEFAULT NULL;

-- If we need to change this, create new migration:
-- 20260205000000_update_chunk_metadata_schema.sql
UPDATE note
SET chunk_metadata = jsonb_set(
  chunk_metadata,
  '{_meta,schema_version}',
  '"2.0.0"'
)
WHERE chunk_metadata IS NOT NULL;
```

**Strengths:**
- Clear audit trail
- No ambiguity about state
- Supports multiple concurrent versions

**Weaknesses:**
- Can't undo mistakes easily
- Migration count grows over time

---

#### Bidirectional Migrations (Not Recommended)

**Pattern:** Each migration has UP and DOWN operations

From golang-migrate research:
> "Each migration has an up and down migration."

**Example:**
```sql
-- 001_add_field.up.sql
ALTER TABLE note ADD COLUMN new_field TEXT;

-- 001_add_field.down.sql
ALTER TABLE note DROP COLUMN new_field;
```

**Strengths:**
- Can roll back quickly
- Useful for development

**Weaknesses:**
- Down migrations often untested
- Data loss risk
- False sense of safety
- Doesn't work for production data

**Industry consensus:** Down migrations are rarely safe in production. Use forward-only with explicit recovery migrations instead.

---

### 2.2 Lazy (On-Read) vs Eager (Batch) Migration

#### Lazy Migration (Recommended for JSONB)

**Pattern:** Migrate data when accessed, not all at once

**Implementation:**
```rust
#[derive(Deserialize)]
#[serde(tag = "_meta.schema_version")]
enum ChunkMetadata {
    #[serde(rename = "1.0.0")]
    V1(ChunkMetadataV1),
    #[serde(rename = "2.0.0")]
    V2(ChunkMetadataV2),
}

impl ChunkMetadata {
    fn migrate_to_latest(self) -> ChunkMetadataV2 {
        match self {
            ChunkMetadata::V1(v1) => v1.into(),
            ChunkMetadata::V2(v2) => v2,
        }
    }
}

impl From<ChunkMetadataV1> for ChunkMetadataV2 {
    fn from(v1: ChunkMetadataV1) -> Self {
        ChunkMetadataV2 {
            schema_version: "2.0.0".into(),
            chunking_strategy: v1.strategy,
            // ... map fields ...
            new_field: Default::default(), // Add new field
        }
    }
}
```

**On write back:**
```rust
async fn update_note_metadata(&self, id: Uuid, meta: ChunkMetadata) -> Result<()> {
    let latest = meta.migrate_to_latest(); // Always write latest version
    sqlx::query!(
        "UPDATE note SET chunk_metadata = $1 WHERE id = $2",
        serde_json::to_value(latest)?,
        id
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

**Strengths:**
- Zero downtime
- No large batch operations
- Only migrates active data
- Easy to test incrementally

**Weaknesses:**
- Multiple versions in DB simultaneously
- Read path complexity
- Must maintain old deserialization code
- Hard to deprecate old versions

**Best for:**
- JSONB columns with infrequent writes
- Large datasets where batch migration is risky
- Schemas that evolve frequently

---

#### Eager Migration (Batch)

**Pattern:** Migrate all data at once during deployment

From Martin Fowler's research on transition periods:
> "The developer would create a script that renames the table customer to client and also creates a view named customer that existing applications can use."

**Example:**
```sql
-- Migration: 20260205000000_migrate_chunk_metadata_v2.sql
UPDATE note
SET chunk_metadata = jsonb_build_object(
  '_meta', jsonb_build_object(
    'schema_version', '2.0.0',
    'migrated_at', NOW()
  ),
  'chunking_strategy', chunk_metadata->>'strategy',
  'chunk_boundaries', chunk_metadata->'boundaries',
  'new_field', 'default_value'
)
WHERE chunk_metadata IS NOT NULL
  AND chunk_metadata->>'_meta.schema_version' = '1.0.0';
```

**Strengths:**
- Clean cutover
- Single version in database
- Simpler read path

**Weaknesses:**
- Requires downtime or locking
- All-or-nothing risk
- Hard to rollback
- Can be slow for large tables

**Best for:**
- Small datasets
- Breaking changes that can't coexist
- When lazy migration complexity is too high

---

#### Hybrid Approach (Recommended)

**Pattern:** Lazy read, eager write, with background migration job

```rust
// 1. Read with version detection
async fn get_chunk_metadata(&self, note_id: Uuid) -> Result<ChunkMetadata> {
    let json: serde_json::Value = sqlx::query_scalar!(
        "SELECT chunk_metadata FROM note WHERE id = $1",
        note_id
    )
    .fetch_one(&self.pool)
    .await?;

    // Deserialize with version-aware enum
    let meta: ChunkMetadataVersions = serde_json::from_value(json)?;
    Ok(meta.migrate_to_latest())
}

// 2. Always write latest version
async fn update_chunk_metadata(
    &self,
    note_id: Uuid,
    meta: ChunkMetadata
) -> Result<()> {
    let latest = ChunkMetadataV2::from(meta); // Ensure latest
    sqlx::query!(
        "UPDATE note SET chunk_metadata = $1 WHERE id = $2",
        serde_json::to_value(latest)?,
        note_id
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}

// 3. Background migration job (low priority)
async fn migrate_old_chunk_metadata_batch(&self) -> Result<usize> {
    let batch_size = 100;
    let migrated = sqlx::query!(
        r#"
        UPDATE note
        SET chunk_metadata = (
            SELECT jsonb_build_object(
                '_meta', jsonb_build_object('schema_version', '2.0.0'),
                'chunking_strategy', chunk_metadata->>'strategy'
                -- ... field mapping ...
            )
        )
        WHERE id IN (
            SELECT id FROM note
            WHERE chunk_metadata->>'_meta.schema_version' = '1.0.0'
            LIMIT $1
        )
        "#,
        batch_size as i32
    )
    .execute(&self.pool)
    .await?
    .rows_affected();

    Ok(migrated as usize)
}
```

**Advantages:**
- No downtime
- Gradual migration reduces risk
- Can prioritize hot data
- Easy to monitor progress
- Simple rollback (stop background job)

---

### 2.3 Migration Chains and Composition

#### Sequential Chain Pattern (SQL Migrations)

**Pattern:** Each migration depends on previous state

```
20260102_initial.sql
  ↓
20260115_add_templates.sql
  ↓
20260122_add_chunk_metadata.sql
  ↓
20260205_update_chunk_metadata_v2.sql
```

**Tracked via:**
```sql
CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time INTEGER NOT NULL
);
```

**Strengths:**
- Clear dependency order
- Easy to reason about
- Atomic application

**Weaknesses:**
- Must apply all migrations in order
- Long chains can be slow

---

#### DAG Pattern (Alembic)

From Alembic research:
> "When creating a new migration, its down_revision points to the previous migration's ID."

**Pattern:** Migrations form directed acyclic graph

```
        ┌─ migration_c ─┐
migration_a              ├─ migration_e
        └─ migration_d ─┘
```

**Supports:**
- Branching (feature branches)
- Merging (combine migrations)
- Multiple developers

**Best for:** Complex projects with parallel development

---

#### Version Skipping

**Pattern:** Allow reading multiple old versions, migrate to latest

```rust
impl ChunkMetadata {
    fn migrate_to_latest(self) -> ChunkMetadataLatest {
        match self {
            ChunkMetadata::V1_0(v1) => {
                // V1 → V2 → V3
                let v2 = ChunkMetadataV2::from(v1);
                ChunkMetadataV3::from(v2)
            }
            ChunkMetadata::V2_0(v2) => {
                // V2 → V3
                ChunkMetadataV3::from(v2)
            }
            ChunkMetadata::V3_0(v3) => v3, // Already latest
        }
    }
}
```

**Advantages:**
- Don't require intermediate migrations
- Cleaner upgrade path

**Disadvantages:**
- Complex conversion logic
- Must maintain all version converters

---

### 2.4 Handling Breaking vs Non-Breaking Changes

#### Non-Breaking Changes (Additive)

From Protocol Buffers research:
> "Adding new fields - Old data parses correctly with defaults; new code handles old messages properly."

**Safe operations:**
- Add optional field with default
- Add new enum value
- Add validation that wasn't present
- Widen type constraints

**Example:**
```json
// V1
{
  "schema_version": "1.0.0",
  "chunking_strategy": "syntactic"
}

// V2 (non-breaking - adds optional field)
{
  "schema_version": "2.0.0",
  "chunking_strategy": "syntactic",
  "preserve_boundaries": true  // NEW, has default behavior
}
```

**Code:**
```rust
#[derive(Deserialize)]
struct ChunkMetadataV2 {
    schema_version: String,
    chunking_strategy: String,
    #[serde(default = "default_preserve_boundaries")]
    preserve_boundaries: bool, // Defaults to true if missing
}

fn default_preserve_boundaries() -> bool { true }
```

---

#### Breaking Changes (Requires Migration)

From Protocol Buffers research:
> "Encoding a field using one definition and then decoding that same field with a different definition can lead to...data corruption."

**Breaking operations:**
- Remove required field
- Rename field (unless aliased)
- Change field type incompatibly
- Change field semantics
- Tighten validation

**Example:**
```json
// V1
{
  "schema_version": "1.0.0",
  "strategy": "syntactic"  // Old field name
}

// V2 (breaking - renames field)
{
  "schema_version": "2.0.0",
  "chunking_strategy": "syntactic"  // Renamed
}
```

**Migration required:**
```rust
impl From<ChunkMetadataV1> for ChunkMetadataV2 {
    fn from(v1: ChunkMetadataV1) -> Self {
        ChunkMetadataV2 {
            schema_version: "2.0.0".into(),
            chunking_strategy: v1.strategy, // Map old → new
        }
    }
}
```

**Handling in SQL:**
```sql
UPDATE note
SET chunk_metadata = jsonb_set(
    jsonb_set(
        chunk_metadata,
        '{chunking_strategy}',
        chunk_metadata->'strategy' -- Copy old field
    ),
    '{schema_version}',
    '"2.0.0"'
) - 'strategy' -- Remove old field
WHERE chunk_metadata->>'schema_version' = '1.0.0';
```

---

#### Transition Period Pattern (Recommended for Breaking Changes)

From Martin Fowler's research:
> "The developer would create a script that renames the table customer to client and also creates a view named customer that existing applications can use."

**Pattern:** Support both old and new simultaneously

**Example for field rename:**
```rust
#[derive(Serialize, Deserialize)]
struct ChunkMetadataV2 {
    #[serde(alias = "strategy")] // Accept old name
    chunking_strategy: String,
}

impl Serialize for ChunkMetadataV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Write both old and new during transition
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("chunking_strategy", &self.chunking_strategy)?;
        map.serialize_entry("strategy", &self.chunking_strategy)?; // Deprecated
        map.end()
    }
}
```

**Deprecation timeline:**
1. Version 2.0: Add new field, keep old (both written)
2. Version 2.1: Document old field as deprecated
3. Version 3.0: Remove old field (breaking change)

**Duration:** 6-12 months for public APIs, 1-2 releases for internal formats

---

## 3. Industry Best Practices

### 3.1 Database Internal Format Versioning

#### SQLite Approach (Multi-Level Versioning)

From SQLite research:

**Multiple version fields:**
1. **File format version** (offset 18-19): Can this version read/write?
2. **Schema format version** (offset 44): SQL feature compatibility
3. **Library version** (offset 96): Last modifier
4. **Change counter** (offset 24): Detect stale metadata

**Compatibility rules:**
> "If a version of SQLite coded to the current file format specification encounters a database file where the read version is 1 or 2 but the write version is greater than 2, then the database file must be treated as read-only."

**Key insight:** Separate read and write versions enable:
- Forward compatibility (new code reads old files)
- Backward compatibility (old code reads new files, possibly read-only)
- Graceful degradation

**Application to matric-memory JSONB:**
```json
{
  "_meta": {
    "format_read_version": "1.0",   // Min version to read
    "format_write_version": "2.0",  // Min version to write
    "schema_version": "2.1.0",      // Current schema
    "change_counter": 42             // Increment on modify
  }
}
```

---

#### PostgreSQL pg_dump Approach (Feature Detection)

From pg_dump research:
> "We allow the server to be back to 9.2, and up to any minor release of our own major version."

**Pattern:** Version checks at runtime, not in dump format

```c
if (fout->remoteVersion >= 120000) {
    // Use PostgreSQL 12+ features
} else {
    // Fall back to older syntax
}
```

**Key insight:** Generate version-appropriate SQL rather than embed version in dump

**Application to matric-memory:**
- Detect schema version on read
- Generate appropriate queries based on version
- No explicit version in output format

```rust
match chunk_metadata.schema_version() {
    SchemaVersion::V1 => build_query_v1(),
    SchemaVersion::V2 => build_query_v2(),
}
```

---

### 3.2 Application Data Format Evolution

#### Protocol Buffers (Field Number Stability)

**Core principle:**
> "Field numbers cannot be changed without creating incompatible versions."

**Safe evolution rules:**
1. **Adding fields:** Always safe (assign new number)
2. **Removing fields:** Mark as reserved
3. **Changing types:** Only wire-compatible types
4. **Reusing numbers:** NEVER (causes data corruption)

**Reserved fields pattern:**
```protobuf
message ChunkMetadata {
  reserved 4, 8, 12; // Never reuse these
  reserved "old_field_name"; // Prevent name reuse

  uint32 version = 1;
  string chunking_strategy = 2;
  // Field 4 was removed - DO NOT REUSE
}
```

**Application to matric-memory JSONB:**

Use field name stability as equivalent to field numbers:

```rust
// NEVER rename these fields directly - use aliases instead
struct ChunkMetadataV2 {
    version: String,              // Field 1 (stable)
    chunking_strategy: String,    // Field 2 (stable)
    chunk_count: u32,             // Field 3 (stable)
    // old_field removed - DO NOT reuse name
    tree_sitter_version: String,  // Field 5 (new)
}

// Maintain reserved names list
const RESERVED_FIELD_NAMES: &[&str] = &[
    "old_field",     // Removed in v2.0.0
    "deprecated_id", // Removed in v2.1.0
];
```

**Validation:**
```rust
fn validate_no_reserved_fields(meta: &serde_json::Value) -> Result<()> {
    if let Some(obj) = meta.as_object() {
        for reserved in RESERVED_FIELD_NAMES {
            if obj.contains_key(*reserved) {
                return Err(Error::ReservedFieldUsed(*reserved));
            }
        }
    }
    Ok(())
}
```

---

#### Apache Avro (Schema Embedded with Data)

Research showed limited details, but key pattern:

**Pattern:** Embed schema with data, use schema registry

```json
{
  "schema": {
    "type": "record",
    "name": "ChunkMetadata",
    "fields": [
      {"name": "version", "type": "string"},
      {"name": "strategy", "type": "string"}
    ]
  },
  "data": {
    "version": "1.0.0",
    "strategy": "syntactic"
  }
}
```

**Advantages:**
- Self-describing format
- No out-of-band schema coordination

**Disadvantages:**
- Large payload overhead
- Redundant schema repetition

**Optimization:** Schema registry with schema ID
```json
{
  "schema_id": "abc123",  // Reference to registry
  "data": { ... }
}
```

**Application to matric-memory:**
- Consider schema registry for complex JSONB
- Useful for plugin architectures
- May be overkill for simple metadata

---

#### Cap'n Proto (Zero-Copy Evolution)

From Cap'n Proto research:
> "Later-numbered fields may be positioned into the padding left between earlier-numbered fields."

**Key insight:** Field layout designed for evolution

**Pattern:** Add fields to existing padding without breaking layout

**Application to matric-memory:**
- Not directly applicable to JSONB
- Relevant if implementing binary chunk storage
- Consider for high-performance embedding serialization

---

### 3.3 Migration Tool Patterns

#### Flyway (Checksum-Based Validation)

From Flyway research (documentation unavailable, but well-known pattern):

**Pattern:** Store checksum of each migration to detect tampering

```sql
CREATE TABLE flyway_schema_history (
    installed_rank INT NOT NULL,
    version VARCHAR(50),
    description VARCHAR(200) NOT NULL,
    type VARCHAR(20) NOT NULL,
    script VARCHAR(1000) NOT NULL,
    checksum INT,  -- Hash of migration file
    installed_by VARCHAR(100) NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    execution_time INT NOT NULL,
    success BOOLEAN NOT NULL
);
```

**Validation:**
1. Compute checksum of migration file
2. Compare with stored checksum
3. Error if mismatch (file was modified after application)

**Application to matric-memory (sqlx):**

SQLx already does this:
```sql
CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,  -- SHA-256 of migration content
    execution_time INTEGER NOT NULL
);
```

**Extend for JSONB migrations:**
```rust
async fn verify_jsonb_schema_integrity(&self) -> Result<Vec<IntegrityError>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            chunk_metadata,
            digest(chunk_metadata::text, 'sha256') as checksum
        FROM note
        WHERE chunk_metadata IS NOT NULL
        "#
    )
    .fetch_all(&self.pool)
    .await?;

    let mut errors = vec![];
    for row in rows {
        let meta: ChunkMetadata = serde_json::from_value(row.chunk_metadata)?;
        if let Some(stored_checksum) = meta.checksum() {
            let computed = compute_checksum(&meta);
            if stored_checksum != computed {
                errors.push(IntegrityError::ChecksumMismatch {
                    note_id: row.id,
                    stored: stored_checksum,
                    computed,
                });
            }
        }
    }
    Ok(errors)
}
```

---

#### Liquibase (Changeset Contexts and Labels)

From Liquibase research (documentation unavailable, but well-known pattern):

**Pattern:** Tag migrations with contexts and labels

```xml
<changeSet id="1" author="alice" context="dev,staging">
  <addColumn tableName="note">
    <column name="test_field" type="text"/>
  </addColumn>
</changeSet>

<changeSet id="2" author="bob" labels="feature-xyz">
  <!-- Only applied with label -->
</changeSet>
```

**Selective application:**
```bash
liquibase update --contexts=production
liquibase update --labels=feature-xyz
```

**Application to matric-memory:**

Add context to JSONB metadata:
```json
{
  "_meta": {
    "schema_version": "2.0.0",
    "context": ["production", "embedding-v2"],
    "features": ["tree-sitter", "mrl"]
  },
  "data": { ... }
}
```

Use for feature flags:
```rust
async fn load_chunk_metadata(
    &self,
    note_id: Uuid,
    context: &[&str]
) -> Result<ChunkMetadata> {
    let meta = self.get_raw_metadata(note_id).await?;

    // Filter by context
    if let Some(required_context) = meta.context() {
        if !context.iter().any(|c| required_context.contains(c)) {
            return Err(Error::ContextMismatch);
        }
    }

    Ok(meta)
}
```

---

#### Alembic (Revision Graph Management)

From Alembic research:
> "The ordering of version scripts is relative to directives within the scripts themselves."

**Pattern:** Each migration knows its parent via `down_revision`

```python
# migration_ae1027.py
revision = 'ae1027a6acf'
down_revision = 'f32a1b9c'  # Parent

def upgrade():
    # Migration logic
```

**Advantages:**
- Supports branching (multiple down_revisions)
- Supports merging (multiple revisions pointing to same down_revision)
- No need for centralized version numbering

**Application to matric-memory:**

Consider for complex JSONB schema evolution:
```json
{
  "_meta": {
    "schema_revision": "ae1027a6acf",
    "parent_revision": "f32a1b9c",
    "branches": ["main", "feature-xyz"]
  }
}
```

**When to use:**
- Multiple teams modifying schemas independently
- Feature branches with schema changes
- Complex merge scenarios

**When not to use:**
- Simple linear evolution (use SemVer)
- Single team with coordination

---

## 4. Migration Metadata and Tracing

### 4.1 Essential Metadata Fields

Based on industry research, minimum viable metadata:

```json
{
  "_meta": {
    // Version tracking
    "schema_version": "2.1.0",
    "format_version": "1.0",

    // Provenance
    "created_at": "2026-02-01T12:00:00Z",
    "created_by": "matric-memory-2026.1.0",
    "migrated_at": "2026-02-05T08:30:00Z",
    "migrated_from": "1.0.0",
    "migration_applied_by": "matric-memory-2026.1.1",

    // Integrity
    "checksum": "sha256:abc123...",
    "checksum_algorithm": "sha256",

    // Lifecycle
    "deprecated": false,
    "deprecation_date": null,
    "end_of_life_date": null
  },
  "data": { ... }
}
```

---

### 4.2 Audit Trail Pattern

**Pattern:** Track all schema changes with history

```json
{
  "_meta": {
    "schema_version": "3.0.0",
    "migration_history": [
      {
        "from_version": "1.0.0",
        "to_version": "2.0.0",
        "migrated_at": "2026-01-15T10:00:00Z",
        "migrated_by": "background_job",
        "migration_duration_ms": 42,
        "changes": ["added_preserve_boundaries_field"]
      },
      {
        "from_version": "2.0.0",
        "to_version": "3.0.0",
        "migrated_at": "2026-02-01T14:30:00Z",
        "migrated_by": "manual_update",
        "migration_duration_ms": 15,
        "changes": ["renamed_strategy_to_chunking_strategy"]
      }
    ]
  },
  "data": { ... }
}
```

**Implementation:**
```rust
struct MigrationHistoryEntry {
    from_version: String,
    to_version: String,
    migrated_at: DateTime<Utc>,
    migrated_by: String,
    migration_duration_ms: u64,
    changes: Vec<String>,
}

impl ChunkMetadata {
    fn record_migration(&mut self, from: &str, changes: Vec<String>) {
        let entry = MigrationHistoryEntry {
            from_version: from.into(),
            to_version: self.schema_version().into(),
            migrated_at: Utc::now(),
            migrated_by: "matric-memory".into(),
            migration_duration_ms: 0, // Set by caller
            changes,
        };
        self.migration_history.push(entry);
    }
}
```

**Query audit trail:**
```sql
-- Find all notes migrated from v1 to v2
SELECT
    id,
    chunk_metadata->'_meta'->'migration_history'
FROM note
WHERE chunk_metadata @> '{
  "_meta": {
    "migration_history": [
      {"from_version": "1.0.0", "to_version": "2.0.0"}
    ]
  }
}'::jsonb;
```

---

### 4.3 Checksum and Integrity Verification

#### SHA-256 Checksums

**Pattern:** Compute hash of data (excluding checksum field itself)

```rust
use sha2::{Sha256, Digest};

fn compute_checksum(meta: &ChunkMetadata) -> String {
    let mut hasher = Sha256::new();

    // Serialize data without _meta.checksum field
    let mut value = serde_json::to_value(meta).unwrap();
    if let Some(obj) = value.as_object_mut() {
        if let Some(meta_obj) = obj.get_mut("_meta") {
            if let Some(meta_map) = meta_obj.as_object_mut() {
                meta_map.remove("checksum");
            }
        }
    }

    // Hash canonical JSON (sorted keys)
    let canonical = serde_json::to_string(&value).unwrap();
    hasher.update(canonical.as_bytes());

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn verify_checksum(meta: &ChunkMetadata) -> Result<bool> {
    let stored = meta.checksum()
        .ok_or(Error::MissingChecksum)?;
    let computed = compute_checksum(meta);
    Ok(stored == computed)
}
```

**When to compute:**
- On write (always)
- On read (optional, for critical data)
- On migration (before and after)

---

#### Merkle Trees (Advanced)

For large collections of JSONB objects:

```
Root Hash
   ├─ Hash(chunk_1)
   ├─ Hash(chunk_2)
   ├─ Hash(chunk_3)
   └─ Hash(chunk_4)
```

**Application:**
```rust
struct ChunkCollection {
    chunks: Vec<ChunkMetadata>,
    merkle_root: String,
}

impl ChunkCollection {
    fn compute_merkle_root(&self) -> String {
        let leaf_hashes: Vec<_> = self.chunks
            .iter()
            .map(compute_checksum)
            .collect();

        merkle_tree::compute_root(&leaf_hashes)
    }

    fn verify_integrity(&self) -> bool {
        self.merkle_root == self.compute_merkle_root()
    }
}
```

**When to use:**
- Large collections (>1000 chunks)
- Need efficient partial verification
- Distributed systems with replication

---

### 4.4 Migration Metrics and Monitoring

**Track migration progress:**

```sql
CREATE TABLE jsonb_migration_metrics (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    table_name TEXT NOT NULL,
    column_name TEXT NOT NULL,
    from_version TEXT NOT NULL,
    to_version TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    rows_total INTEGER,
    rows_migrated INTEGER,
    rows_failed INTEGER,
    error_sample JSONB,
    status TEXT NOT NULL, -- 'running', 'completed', 'failed'

    CHECK (status IN ('running', 'completed', 'failed'))
);

CREATE INDEX idx_migration_metrics_status
    ON jsonb_migration_metrics(status, started_at)
    WHERE status = 'running';
```

**Background job:**
```rust
async fn run_background_migration(&self) -> Result<MigrationMetrics> {
    let metric_id = self.start_migration_tracking(
        "note",
        "chunk_metadata",
        "1.0.0",
        "2.0.0"
    ).await?;

    let mut migrated = 0;
    let mut failed = 0;
    let mut errors = vec![];

    loop {
        let batch = self.fetch_unmigrated_batch(100).await?;
        if batch.is_empty() {
            break;
        }

        for note_id in batch {
            match self.migrate_note_metadata(note_id).await {
                Ok(_) => migrated += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(json!({
                        "note_id": note_id,
                        "error": e.to_string()
                    }));
                }
            }
        }

        self.update_migration_progress(
            metric_id,
            migrated,
            failed,
            &errors
        ).await?;

        tokio::time::sleep(Duration::from_secs(1)).await; // Rate limit
    }

    self.complete_migration_tracking(metric_id).await?;

    Ok(MigrationMetrics { migrated, failed })
}
```

**Monitoring dashboard query:**
```sql
SELECT
    table_name,
    column_name,
    from_version,
    to_version,
    rows_migrated,
    rows_total,
    ROUND(100.0 * rows_migrated / NULLIF(rows_total, 0), 2) as progress_pct,
    started_at,
    NOW() - started_at as elapsed,
    status
FROM jsonb_migration_metrics
WHERE status = 'running'
ORDER BY started_at DESC;
```

---

## 5. Definition of Done Considerations

### 5.1 Schema Change Review Process

**Mandatory review checklist:**

```markdown
## Schema Change Review Checklist

### 1. Version Impact
- [ ] Is this a breaking change? (MAJOR version bump)
- [ ] Is this backward-compatible? (MINOR version bump)
- [ ] Is this a bug fix only? (PATCH version bump)
- [ ] Have you updated the schema version constant?

### 2. Migration Strategy
- [ ] Is lazy migration possible?
- [ ] Is eager migration required?
- [ ] Have you estimated migration time for production data?
- [ ] Is a transition period needed?
- [ ] Have you tested migration on production-sized dataset?

### 3. Backward Compatibility
- [ ] Can old code read new data?
- [ ] Can new code read old data?
- [ ] Are there any fields being removed?
- [ ] Are removed fields marked as reserved?
- [ ] Are field renames using serde aliases?

### 4. Testing
- [ ] Unit tests for version deserialization
- [ ] Integration tests for migration logic
- [ ] Property tests for migration idempotence
- [ ] Load tests for migration performance
- [ ] Rollback/recovery tests

### 5. Documentation
- [ ] Updated schema documentation
- [ ] Migration guide for users
- [ ] Deprecation warnings for old fields
- [ ] Example code for new fields
- [ ] CHANGELOG entry with breaking change warnings

### 6. Monitoring
- [ ] Metrics for migration progress
- [ ] Alerts for migration failures
- [ ] Dashboard for version distribution
- [ ] Error tracking for corrupted data

### 7. Rollback Plan
- [ ] Can migration be paused safely?
- [ ] Can migration be rolled back?
- [ ] Is there a recovery procedure for failures?
- [ ] Have you tested the rollback procedure?
```

---

### 5.2 Automated Validation

**Pre-commit hook:**
```bash
#!/bin/bash
# .git/hooks/pre-commit

# 1. Check for schema version updates
if git diff --cached --name-only | grep -q "chunk_metadata\|embedding_config"; then
    echo "Checking for schema version updates..."

    # Ensure SCHEMA_VERSION constant was updated
    if ! git diff --cached | grep -q "+.*SCHEMA_VERSION"; then
        echo "ERROR: Schema struct changed but SCHEMA_VERSION not updated"
        exit 1
    fi
fi

# 2. Run schema validation tests
cargo test --package matric-core --lib schemas::tests

# 3. Check for reserved field violations
./scripts/check-reserved-fields.sh
```

**Schema validation test:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_versions_deserialize() {
        // V1.0.0
        let v1_json = r#"{
            "version": "1.0.0",
            "strategy": "syntactic"
        }"#;
        let v1: ChunkMetadata = serde_json::from_str(v1_json).unwrap();
        assert_eq!(v1.schema_version(), "1.0.0");

        // V2.0.0
        let v2_json = r#"{
            "_meta": {"schema_version": "2.0.0"},
            "chunking_strategy": "syntactic"
        }"#;
        let v2: ChunkMetadata = serde_json::from_str(v2_json).unwrap();
        assert_eq!(v2.schema_version(), "2.0.0");
    }

    #[test]
    fn test_migration_idempotence() {
        let v1 = ChunkMetadataV1 { /* ... */ };
        let v2_first = migrate_v1_to_v2(v1.clone());
        let v2_second = migrate_v1_to_v2(v1.clone());
        assert_eq!(v2_first, v2_second);
    }

    #[test]
    fn test_migration_preserves_semantics() {
        let v1 = ChunkMetadataV1 {
            strategy: "syntactic".into(),
        };
        let v2 = migrate_v1_to_v2(v1);
        assert_eq!(v2.chunking_strategy, "syntactic");
    }

    #[test]
    fn test_no_reserved_fields() {
        let meta = ChunkMetadataV2 {
            // Should not compile if using reserved names
            // old_field: "value", // Compile error
        };
        // ...
    }

    #[test]
    fn test_checksum_integrity() {
        let meta = ChunkMetadataV2::new(/* ... */);
        let checksum = compute_checksum(&meta);

        // Modify and recompute
        let mut meta2 = meta.clone();
        meta2.chunking_strategy = "different".into();
        let checksum2 = compute_checksum(&meta2);

        assert_ne!(checksum, checksum2);
    }
}
```

---

### 5.3 Documentation Requirements

**Schema documentation template:**

```markdown
# ChunkMetadata Schema

## Current Version: 2.1.0

Last updated: 2026-02-01

### Schema Definition

```json
{
  "_meta": {
    "schema_version": "2.1.0",
    "created_at": "2026-02-01T12:00:00Z",
    "checksum": "sha256:..."
  },
  "chunking_strategy": "syntactic",
  "chunk_boundaries": [0, 512, 1024],
  "preserve_boundaries": true,
  "tree_sitter_version": "0.20.8"
}
```

### Fields

| Field | Type | Required | Default | Since | Deprecated |
|-------|------|----------|---------|-------|------------|
| `_meta.schema_version` | string | Yes | - | 2.0.0 | - |
| `chunking_strategy` | enum | Yes | - | 1.0.0 | - |
| `chunk_boundaries` | int[] | No | [] | 2.0.0 | - |
| `preserve_boundaries` | bool | No | true | 2.1.0 | - |
| `tree_sitter_version` | string | No | null | 2.1.0 | - |

### Version History

#### 2.1.0 (2026-02-01)
- Added `preserve_boundaries` field (non-breaking)
- Added `tree_sitter_version` field (non-breaking)

#### 2.0.0 (2026-01-15) - BREAKING
- Renamed `strategy` to `chunking_strategy`
- Added `_meta` envelope with versioning
- Added `chunk_boundaries` array
- Removed `chunk_size` field (use `chunk_boundaries.len()` instead)

#### 1.0.0 (2025-12-01)
- Initial version
- Fields: `strategy`, `chunk_size`

### Migration Guide

#### Migrating from 1.0.0 to 2.0.0

**Automatic migration:**
```rust
let v1 = ChunkMetadataV1::from_json(json)?;
let v2 = ChunkMetadataV2::from(v1);
```

**Manual migration (SQL):**
```sql
UPDATE note
SET chunk_metadata = jsonb_build_object(
    '_meta', jsonb_build_object('schema_version', '2.0.0'),
    'chunking_strategy', chunk_metadata->>'strategy',
    'chunk_boundaries', chunk_metadata->'boundaries'
)
WHERE chunk_metadata->>'version' = '1.0.0';
```

### Reserved Fields

The following field names are reserved and must not be reused:

- `strategy` (removed in 2.0.0, replaced by `chunking_strategy`)
- `chunk_size` (removed in 2.0.0, replaced by `chunk_boundaries`)

### Examples

See `tests/fixtures/chunk_metadata_examples.json` for complete examples of all versions.
```

---

### 5.4 Testing Strategies

#### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // 1. Deserialization tests for all versions
    #[test]
    fn test_deserialize_v1() { /* ... */ }

    #[test]
    fn test_deserialize_v2() { /* ... */ }

    // 2. Migration correctness
    #[test]
    fn test_v1_to_v2_migration() {
        let v1 = ChunkMetadataV1 {
            version: "1.0.0".into(),
            strategy: "syntactic".into(),
        };
        let v2 = ChunkMetadataV2::from(v1);
        assert_eq!(v2.schema_version, "2.0.0");
        assert_eq!(v2.chunking_strategy, "syntactic");
    }

    // 3. Idempotence (applying migration twice = applying once)
    proptest! {
        #[test]
        fn test_migration_idempotent(strategy in "[a-z]+") {
            let v1 = ChunkMetadataV1 { strategy: strategy.clone() };
            let v2_once = ChunkMetadataV2::from(v1.clone());
            let v2_twice = ChunkMetadataV2::from(v1);
            prop_assert_eq!(v2_once, v2_twice);
        }
    }

    // 4. Round-trip (serialize → deserialize = identity)
    proptest! {
        #[test]
        fn test_roundtrip(meta: ChunkMetadataV2) {
            let json = serde_json::to_string(&meta).unwrap();
            let decoded: ChunkMetadataV2 = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(meta, decoded);
        }
    }

    // 5. Checksum validation
    #[test]
    fn test_checksum_integrity() { /* ... */ }
}
```

---

#### Integration Tests

```rust
#[tokio::test]
async fn test_lazy_migration_on_read() {
    let db = test_db().await;

    // Insert V1 data
    sqlx::query!(
        "INSERT INTO note (id, chunk_metadata) VALUES ($1, $2)",
        Uuid::new_v4(),
        json!({"version": "1.0.0", "strategy": "syntactic"})
    )
    .execute(&db.pool)
    .await
    .unwrap();

    // Read should automatically migrate to V2
    let meta = db.get_chunk_metadata(note_id).await.unwrap();
    assert_eq!(meta.schema_version(), "2.0.0");
}

#[tokio::test]
async fn test_background_migration_job() {
    let db = test_db().await;

    // Seed 1000 V1 records
    for _ in 0..1000 {
        db.insert_v1_metadata(/* ... */).await.unwrap();
    }

    // Run background migration
    let metrics = db.run_background_migration().await.unwrap();
    assert_eq!(metrics.migrated, 1000);
    assert_eq!(metrics.failed, 0);

    // Verify all migrated
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM note
         WHERE chunk_metadata->>'_meta.schema_version' = '2.0.0'"
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(count, Some(1000));
}
```

---

#### Load Tests

```rust
#[tokio::test]
#[ignore] // Run with --ignored
async fn load_test_migration_performance() {
    let db = test_db().await;

    // Seed 100k V1 records
    for i in 0..100_000 {
        db.insert_v1_metadata(/* ... */).await.unwrap();
    }

    // Measure migration time
    let start = Instant::now();
    let metrics = db.run_background_migration().await.unwrap();
    let duration = start.elapsed();

    println!("Migrated {} records in {:?}", metrics.migrated, duration);
    println!("Rate: {} records/sec", metrics.migrated as f64 / duration.as_secs_f64());

    // Assert performance target (e.g., 1000 records/sec)
    assert!(duration.as_secs() < 100, "Migration too slow");
}
```

---

#### Property Tests (QuickCheck/Proptest)

```rust
use proptest::prelude::*;

proptest! {
    // 1. Any valid V1 can migrate to V2
    #[test]
    fn prop_v1_always_migrates(
        strategy in "[a-z]{3,20}",
        chunk_size in 1u32..10000
    ) {
        let v1 = ChunkMetadataV1 { strategy, chunk_size };
        let result = ChunkMetadataV2::try_from(v1);
        prop_assert!(result.is_ok());
    }

    // 2. Migration preserves semantics
    #[test]
    fn prop_migration_preserves_strategy(v1: ChunkMetadataV1) {
        let v2 = ChunkMetadataV2::from(v1.clone());
        prop_assert_eq!(v1.strategy, v2.chunking_strategy);
    }

    // 3. Checksum changes when data changes
    #[test]
    fn prop_checksum_detects_changes(
        mut meta: ChunkMetadataV2,
        new_strategy in "[a-z]+"
    ) {
        let original_checksum = compute_checksum(&meta);
        meta.chunking_strategy = new_strategy;
        let new_checksum = compute_checksum(&meta);
        prop_assert_ne!(original_checksum, new_checksum);
    }
}
```

---

## 6. Concrete Recommendations for matric-memory

### 6.1 JSONB Schema Versioning

**Adopt Semantic Versioning with metadata envelope:**

```rust
// crates/matric-core/src/schemas/chunk_metadata.rs

pub const CHUNK_METADATA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkMetadata {
    #[serde(flatten)]
    pub meta: SchemaMetadata,

    pub chunking_strategy: ChunkingStrategy,
    pub chunk_boundaries: Vec<usize>,
    pub preserve_boundaries: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sitter_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaMetadata {
    #[serde(rename = "_meta")]
    pub meta: MetaInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetaInfo {
    pub schema_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

impl Default for ChunkMetadata {
    fn default() -> Self {
        Self {
            meta: SchemaMetadata {
                meta: MetaInfo {
                    schema_version: CHUNK_METADATA_VERSION.into(),
                    created_at: Some(Utc::now()),
                    checksum: None,
                }
            },
            chunking_strategy: ChunkingStrategy::Semantic,
            chunk_boundaries: vec![],
            preserve_boundaries: true,
            tree_sitter_version: None,
        }
    }
}
```

---

### 6.2 Migration Strategy

**Use hybrid lazy-read + eager-write + background migration:**

```rust
// crates/matric-db/src/notes.rs

impl PgNoteRepository {
    /// Read chunk metadata with automatic version migration
    pub async fn get_chunk_metadata(
        &self,
        note_id: Uuid
    ) -> Result<ChunkMetadata> {
        let json: serde_json::Value = sqlx::query_scalar!(
            "SELECT chunk_metadata FROM note WHERE id = $1",
            note_id
        )
        .fetch_one(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;

        // Deserialize with version detection
        let versioned: ChunkMetadataVersions = serde_json::from_value(json)?;

        // Migrate to latest
        Ok(versioned.into_latest())
    }

    /// Write always uses latest version
    pub async fn update_chunk_metadata(
        &self,
        note_id: Uuid,
        meta: ChunkMetadata,
    ) -> Result<()> {
        // Ensure version is current
        let mut meta = meta;
        meta.meta.meta.schema_version = CHUNK_METADATA_VERSION.into();

        // Compute checksum
        let checksum = compute_checksum(&meta);
        meta.meta.meta.checksum = Some(checksum);

        sqlx::query!(
            "UPDATE note SET chunk_metadata = $1 WHERE id = $2",
            serde_json::to_value(&meta)?,
            note_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// Version-aware deserialization
#[derive(Deserialize)]
#[serde(untagged)]
enum ChunkMetadataVersions {
    V1(ChunkMetadataV1),
    V2(ChunkMetadata), // Current
}

impl ChunkMetadataVersions {
    fn into_latest(self) -> ChunkMetadata {
        match self {
            Self::V1(v1) => v1.into(),
            Self::V2(v2) => v2,
        }
    }
}

/// Legacy V1 structure (for migration only)
#[derive(Deserialize)]
struct ChunkMetadataV1 {
    version: String,
    strategy: String,
}

impl From<ChunkMetadataV1> for ChunkMetadata {
    fn from(v1: ChunkMetadataV1) -> Self {
        ChunkMetadata {
            meta: SchemaMetadata {
                meta: MetaInfo {
                    schema_version: CHUNK_METADATA_VERSION.into(),
                    created_at: Some(Utc::now()),
                    checksum: None,
                }
            },
            chunking_strategy: v1.strategy.parse().unwrap_or_default(),
            chunk_boundaries: vec![],
            preserve_boundaries: true,
            tree_sitter_version: None,
        }
    }
}
```

---

### 6.3 Background Migration Job

```rust
// crates/matric-jobs/src/migrations.rs

pub struct JsonbMigrationJob {
    db: Database,
    batch_size: usize,
}

impl JsonbMigrationJob {
    pub async fn run(&self) -> Result<MigrationMetrics> {
        let metric_id = self.start_tracking().await?;

        let mut total_migrated = 0;
        let mut total_failed = 0;

        loop {
            let batch = self.fetch_unmigrated_batch().await?;
            if batch.is_empty() {
                break;
            }

            for note_id in batch {
                match self.migrate_note(note_id).await {
                    Ok(_) => total_migrated += 1,
                    Err(e) => {
                        total_failed += 1;
                        tracing::warn!(
                            note_id = %note_id,
                            error = %e,
                            "Failed to migrate chunk metadata"
                        );
                    }
                }
            }

            self.update_progress(metric_id, total_migrated, total_failed).await?;

            // Rate limit to avoid overloading DB
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        self.complete_tracking(metric_id).await?;

        Ok(MigrationMetrics {
            migrated: total_migrated,
            failed: total_failed,
        })
    }

    async fn fetch_unmigrated_batch(&self) -> Result<Vec<Uuid>> {
        let ids = sqlx::query_scalar!(
            r#"
            SELECT id FROM note
            WHERE chunk_metadata IS NOT NULL
              AND (
                chunk_metadata->>'version' = '1.0.0'
                OR chunk_metadata->'_meta'->>'schema_version' != $1
              )
            LIMIT $2
            "#,
            CHUNK_METADATA_VERSION,
            self.batch_size as i32
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(ids)
    }

    async fn migrate_note(&self, note_id: Uuid) -> Result<()> {
        // Read (triggers lazy migration)
        let meta = self.db.notes.get_chunk_metadata(note_id).await?;

        // Write back (ensures latest version)
        self.db.notes.update_chunk_metadata(note_id, meta).await?;

        Ok(())
    }
}
```

---

### 6.4 Checksum Implementation

```rust
// crates/matric-core/src/schemas/checksums.rs

use sha2::{Sha256, Digest};
use serde::Serialize;

pub fn compute_checksum<T: Serialize>(value: &T) -> String {
    // Serialize to canonical JSON (sorted keys)
    let mut json = serde_json::to_value(value)
        .expect("Failed to serialize");

    // Remove checksum field to avoid circular dependency
    if let Some(obj) = json.as_object_mut() {
        if let Some(meta) = obj.get_mut("_meta") {
            if let Some(meta_obj) = meta.as_object_mut() {
                meta_obj.remove("checksum");
            }
        }
    }

    // Compute SHA-256
    let canonical = serde_json::to_string(&json)
        .expect("Failed to serialize JSON");

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn verify_checksum<T: Serialize>(value: &T, expected: &str) -> bool {
    let computed = compute_checksum(value);
    computed == expected
}
```

---

### 6.5 Validation and Testing

```rust
// crates/matric-core/src/schemas/tests.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_metadata_current_version() {
        let meta = ChunkMetadata::default();
        assert_eq!(
            meta.meta.meta.schema_version,
            CHUNK_METADATA_VERSION
        );
    }

    #[test]
    fn test_deserialize_v1() {
        let json = r#"{
            "version": "1.0.0",
            "strategy": "syntactic"
        }"#;

        let versioned: ChunkMetadataVersions =
            serde_json::from_str(json).unwrap();
        let latest = versioned.into_latest();

        assert_eq!(latest.chunking_strategy, ChunkingStrategy::Syntactic);
        assert_eq!(latest.meta.meta.schema_version, CHUNK_METADATA_VERSION);
    }

    #[test]
    fn test_migration_idempotence() {
        let v1 = ChunkMetadataV1 {
            version: "1.0.0".into(),
            strategy: "semantic".into(),
        };

        let v2_first = ChunkMetadata::from(v1.clone());
        let v2_second = ChunkMetadata::from(v1);

        assert_eq!(v2_first, v2_second);
    }

    #[test]
    fn test_checksum_computation() {
        let meta = ChunkMetadata::default();
        let checksum = compute_checksum(&meta);

        assert!(checksum.starts_with("sha256:"));
        assert_eq!(checksum.len(), 71); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_checksum_changes_on_modification() {
        let meta1 = ChunkMetadata::default();
        let checksum1 = compute_checksum(&meta1);

        let mut meta2 = meta1.clone();
        meta2.chunking_strategy = ChunkingStrategy::Syntactic;
        let checksum2 = compute_checksum(&meta2);

        assert_ne!(checksum1, checksum2);
    }
}
```

---

### 6.6 Schema Change Workflow

**1. Create new schema version:**

```rust
// crates/matric-core/src/schemas/chunk_metadata.rs

// Bump version
pub const CHUNK_METADATA_VERSION: &str = "2.0.0";

// Add new fields to current struct
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkMetadata {
    // ... existing fields ...

    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_field: Option<String>, // NEW in v2.0.0
}

// Keep old version for migration
#[derive(Deserialize)]
struct ChunkMetadataV1 {
    version: String,
    strategy: String,
}

// Update version enum
#[derive(Deserialize)]
#[serde(untagged)]
enum ChunkMetadataVersions {
    V1(ChunkMetadataV1),
    V2(ChunkMetadata),
}

// Add migration logic
impl From<ChunkMetadataV1> for ChunkMetadata {
    fn from(v1: ChunkMetadataV1) -> Self {
        ChunkMetadata {
            // ... field mapping ...
            new_field: None, // Default for new field
        }
    }
}
```

**2. Add tests:**

```rust
#[test]
fn test_v1_to_v2_migration() {
    let v1_json = r#"{"version": "1.0.0", "strategy": "semantic"}"#;
    let v2: ChunkMetadata = serde_json::from_str(v1_json).unwrap();
    assert_eq!(v2.meta.meta.schema_version, "2.0.0");
}
```

**3. Update documentation:**

```markdown
# docs/schemas/chunk_metadata.md

## Version 2.0.0 (2026-02-05)

### Changes
- Added `new_field` (optional, non-breaking)

### Migration
Automatic on read. No manual migration required.
```

**4. Deploy:**

- Background job will gradually migrate existing data
- New writes use v2.0.0
- Old data still readable via lazy migration

---

### 6.7 Monitoring Dashboard

```sql
-- Query: Schema version distribution
SELECT
    COALESCE(
        chunk_metadata->'_meta'->>'schema_version',
        chunk_metadata->>'version',
        'unknown'
    ) as schema_version,
    COUNT(*) as count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) as pct
FROM note
WHERE chunk_metadata IS NOT NULL
GROUP BY schema_version
ORDER BY count DESC;

-- Query: Migration progress
SELECT
    COUNT(*) FILTER (WHERE
        chunk_metadata->'_meta'->>'schema_version' = '2.0.0'
    ) as migrated,
    COUNT(*) FILTER (WHERE
        chunk_metadata->>'version' = '1.0.0'
    ) as pending,
    COUNT(*) as total
FROM note
WHERE chunk_metadata IS NOT NULL;

-- Query: Checksum integrity
SELECT
    id,
    chunk_metadata->'_meta'->>'checksum' as stored_checksum
FROM note
WHERE chunk_metadata IS NOT NULL
  AND chunk_metadata->'_meta'->>'checksum' IS NOT NULL
LIMIT 10;
```

---

## 7. Summary and Next Steps

### Key Takeaways

1. **Semantic versioning is best for JSONB schemas** in Rust/PostgreSQL ecosystems
2. **Lazy migration (on-read) + background job** provides safest production deployment
3. **Field stability** (Protocol Buffers pattern) prevents breaking changes
4. **Checksums are essential** for data integrity verification
5. **Forward-only migrations** are strongly preferred over rollback mechanisms
6. **Mandatory schema change reviews** prevent accidental breaking changes

---

### Recommended Implementation Priority

**Phase 1: Foundation (Week 1)**
1. Add `_meta` envelope to existing JSONB columns
2. Implement version-aware deserialization
3. Add checksum computation
4. Write unit tests for migrations

**Phase 2: Migration (Week 2)**
5. Implement lazy migration on read
6. Ensure writes use latest version
7. Add integration tests
8. Deploy to staging

**Phase 3: Background Migration (Week 3)**
9. Create background migration job
10. Add migration metrics tracking
11. Set up monitoring dashboard
12. Run migration on production data

**Phase 4: Governance (Week 4)**
13. Document schema change workflow
14. Add pre-commit hooks for schema validation
15. Create schema change review checklist
16. Train team on migration patterns

---

### References

- **Protocol Buffers Schema Evolution:** https://protobuf.dev/programming-guides/proto3/
- **SQLite File Format:** https://www.sqlite.org/fileformat.html
- **Martin Fowler on Evolutionary Database Design:** https://martinfowler.com/articles/evodb.html
- **Alembic Migrations:** https://alembic.sqlalchemy.org/en/latest/tutorial.html
- **golang-migrate:** https://github.com/golang-migrate/migrate
- **matric-memory migrations:** `/path/to/fortemi/migrations/`
- **matric-memory JSONB usage:** `chunk_metadata`, `embedding_config`, `skos_tags.changes`

---

**End of Report**

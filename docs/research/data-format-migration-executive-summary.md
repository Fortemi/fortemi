# Executive Summary: Data Format Migration Strategies

**Date:** 2026-02-01
**Research Report:** [data-format-migration-strategies.md](./data-format-migration-strategies.md)
**Confidence Level:** High

## Quick Reference

### Recommended Approach for matric-memory

1. **Versioning:** Semantic Versioning (MAJOR.MINOR.PATCH) with metadata envelope
2. **Migration Pattern:** Hybrid lazy-read + eager-write + background job
3. **Evolution Rules:** Protocol Buffers field stability pattern
4. **Integrity:** SHA-256 checksums on all writes
5. **Governance:** Mandatory schema change reviews with automated validation

---

## Critical Findings

### 1. Versioning Schemes Comparison

| Approach | Best For | Pros | Cons |
|----------|----------|------|------|
| **Semantic Versioning** | Application data, JSONB | Clear compatibility semantics | Requires discipline |
| **Calendar Versioning** | Releases, deprecation | Time-based reasoning | No compatibility info |
| **Field Numbers** (Protobuf) | Binary formats | Strongest compatibility | Rigid structure |
| **Revision Hashes** (Alembic) | Complex migration chains | Supports branching | Not human-readable |

**Recommendation:** Semantic versioning for matric-memory JSONB columns (chunk_metadata, embedding_config)

---

### 2. Migration Patterns Comparison

| Pattern | Downtime | Risk | Complexity | Best For |
|---------|----------|------|------------|----------|
| **Lazy (on-read)** | Zero | Low | Medium | Large datasets, frequent changes |
| **Eager (batch)** | Yes | High | Low | Small datasets, breaking changes |
| **Hybrid** | Zero | Low | Medium | Production systems (recommended) |
| **Bidirectional** | Zero | Very High | High | NOT RECOMMENDED |

**Recommendation:** Hybrid approach for matric-memory
- Lazy read with version detection
- Eager write (always latest version)
- Background job for gradual migration

---

### 3. Breaking vs Non-Breaking Changes

#### Safe Operations (Non-Breaking)
- Add optional field with default
- Add new enum value
- Widen type constraints
- Add validation

#### Unsafe Operations (Breaking - Requires Migration)
- Remove required field
- Rename field (without alias)
- Change field type incompatibly
- Tighten validation

**Key Insight:** Use transition periods (6-12 months) for breaking changes by supporting both old and new simultaneously

---

## Implementation Blueprint for matric-memory

### JSONB Schema Structure

```json
{
  "_meta": {
    "schema_version": "2.1.0",
    "created_at": "2026-02-01T12:00:00Z",
    "checksum": "sha256:abc123..."
  },
  "chunking_strategy": "syntactic",
  "chunk_boundaries": [0, 512, 1024],
  "preserve_boundaries": true
}
```

### Version-Aware Deserialization

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum ChunkMetadataVersions {
    V1(ChunkMetadataV1),  // Legacy
    V2(ChunkMetadata),    // Current
}

impl ChunkMetadataVersions {
    fn into_latest(self) -> ChunkMetadata {
        match self {
            Self::V1(v1) => v1.into(),
            Self::V2(v2) => v2,
        }
    }
}
```

### Read Path (Lazy Migration)

```rust
async fn get_chunk_metadata(&self, note_id: Uuid) -> Result<ChunkMetadata> {
    let json = sqlx::query_scalar!(
        "SELECT chunk_metadata FROM note WHERE id = $1",
        note_id
    ).fetch_one(&self.pool).await?;

    let versioned: ChunkMetadataVersions = serde_json::from_value(json)?;
    Ok(versioned.into_latest())  // Migrate on read
}
```

### Write Path (Always Latest)

```rust
async fn update_chunk_metadata(&self, note_id: Uuid, meta: ChunkMetadata) -> Result<()> {
    let mut meta = meta;
    meta.schema_version = CURRENT_VERSION;
    meta.checksum = Some(compute_checksum(&meta));

    sqlx::query!(
        "UPDATE note SET chunk_metadata = $1 WHERE id = $2",
        serde_json::to_value(&meta)?,
        note_id
    ).execute(&self.pool).await?;

    Ok(())
}
```

### Background Migration Job

```rust
async fn run_background_migration(&self) -> Result<MigrationMetrics> {
    loop {
        let batch = self.fetch_unmigrated_batch(100).await?;
        if batch.is_empty() { break; }

        for note_id in batch {
            let meta = self.get_chunk_metadata(note_id).await?;
            self.update_chunk_metadata(note_id, meta).await?;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(metrics)
}
```

---

## Essential Metadata Fields

### Minimum Viable Metadata

```json
{
  "_meta": {
    "schema_version": "2.1.0",      // REQUIRED
    "created_at": "2026-02-01T...", // RECOMMENDED
    "checksum": "sha256:..."         // REQUIRED for critical data
  }
}
```

### Full Metadata (Production)

```json
{
  "_meta": {
    "schema_version": "2.1.0",
    "format_version": "1.0",
    "created_at": "2026-02-01T12:00:00Z",
    "created_by": "matric-memory-2026.1.0",
    "migrated_at": "2026-02-05T08:30:00Z",
    "migrated_from": "1.0.0",
    "checksum": "sha256:abc123...",
    "migration_history": [
      {
        "from_version": "1.0.0",
        "to_version": "2.0.0",
        "migrated_at": "2026-01-15T10:00:00Z",
        "changes": ["renamed_strategy_field"]
      }
    ]
  }
}
```

---

## Checksum Implementation

### SHA-256 Checksums

```rust
use sha2::{Sha256, Digest};

fn compute_checksum<T: Serialize>(value: &T) -> String {
    let mut json = serde_json::to_value(value).unwrap();

    // Remove checksum field to avoid circular dependency
    if let Some(meta) = json.get_mut("_meta").and_then(|m| m.as_object_mut()) {
        meta.remove("checksum");
    }

    let canonical = serde_json::to_string(&json).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());

    format!("sha256:{}", hex::encode(hasher.finalize()))
}
```

### When to Compute Checksums
- Always on write
- Optionally on read (for critical data)
- Before and after migration
- During integrity audits

---

## Industry Patterns Reference

### SQLite: Multi-Level Versioning

- File format version (read/write compatibility)
- Schema format version (feature compatibility)
- Library version (last modifier)
- Change counter (detect stale metadata)

**Application:** Use separate read/write versions for graceful degradation

---

### Protocol Buffers: Field Number Stability

- Field numbers never change
- Removed fields marked as reserved
- Type changes only for wire-compatible types
- Never reuse field numbers

**Application:** Maintain reserved field name list for JSONB

```rust
const RESERVED_FIELD_NAMES: &[&str] = &[
    "old_field",     // Removed in v2.0.0
    "deprecated_id", // Removed in v2.1.0
];
```

---

### Martin Fowler: Transition Periods

- Support old and new simultaneously
- Create views/aliases for renamed fields
- Duration: 6-12 months for public APIs
- Gradual migration without coordination pressure

**Application:** Use serde aliases for renamed fields

```rust
#[derive(Deserialize)]
struct ChunkMetadata {
    #[serde(alias = "strategy")]  // Accept old name
    chunking_strategy: String,
}
```

---

## Definition of Done: Schema Changes

### Mandatory Checklist

- [ ] Version bump appropriate (MAJOR/MINOR/PATCH)
- [ ] Migration strategy documented
- [ ] Backward compatibility verified
- [ ] Unit tests for all versions
- [ ] Integration tests for migration
- [ ] Load tests for performance
- [ ] Documentation updated
- [ ] CHANGELOG entry added
- [ ] Reserved fields list updated
- [ ] Rollback plan documented

### Automated Validation

**Pre-commit hook:**
```bash
if git diff --cached | grep -q "ChunkMetadata"; then
    if ! git diff --cached | grep -q "CHUNK_METADATA_VERSION"; then
        echo "ERROR: Schema changed but version not updated"
        exit 1
    fi
fi
cargo test schemas::tests
```

**CI pipeline:**
```bash
cargo test --package matric-core --lib schemas
cargo test --package matric-db --test integration_tests
```

---

## Testing Strategy

### Unit Tests
- Deserialization of all versions
- Migration correctness
- Idempotence (apply twice = apply once)
- Round-trip (serialize → deserialize)
- Checksum integrity

### Integration Tests
- Lazy migration on read
- Background migration job
- Mixed version coexistence
- Error recovery

### Load Tests
- Migration performance on 100k+ records
- Throughput targets (1000 records/sec)
- Memory usage under load

### Property Tests
- Any valid V1 migrates to V2
- Migration preserves semantics
- Checksum detects all changes

---

## Migration Metrics and Monitoring

### Progress Tracking

```sql
-- Version distribution
SELECT
    chunk_metadata->'_meta'->>'schema_version' as version,
    COUNT(*) as count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) as pct
FROM note
WHERE chunk_metadata IS NOT NULL
GROUP BY version;

-- Migration progress
SELECT
    COUNT(*) FILTER (WHERE version = '2.0.0') as migrated,
    COUNT(*) FILTER (WHERE version = '1.0.0') as pending
FROM note;
```

### Alerts

- Migration job failure
- Checksum mismatch detected
- Old version percentage > 10% after 30 days
- Migration rate < 100 records/hour

---

## Implementation Timeline

### Phase 1: Foundation (Week 1)
- Add `_meta` envelope to JSONB columns
- Implement version-aware deserialization
- Add checksum computation
- Write unit tests

### Phase 2: Migration (Week 2)
- Implement lazy migration on read
- Ensure writes use latest version
- Add integration tests
- Deploy to staging

### Phase 3: Background Job (Week 3)
- Create migration job
- Add metrics tracking
- Set up monitoring dashboard
- Run on production

### Phase 4: Governance (Week 4)
- Document schema change workflow
- Add pre-commit hooks
- Create review checklist
- Team training

---

## Risk Mitigation

### High-Risk Scenarios

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Data corruption during migration | Low | Critical | Checksums, atomic updates, rollback plan |
| Performance degradation | Medium | High | Background job rate limiting, monitoring |
| Mixed version incompatibility | Medium | Medium | Lazy migration, version detection |
| Breaking change undetected | Low | High | Mandatory review, automated tests |

### Rollback Strategy

1. Stop background migration job
2. Revert application code
3. Old code can still read new data (via lazy migration)
4. No data loss (forward-only migrations)

---

## Key Resources

- **Full Report:** [data-format-migration-strategies.md](./data-format-migration-strategies.md)
- **Protocol Buffers Evolution:** https://protobuf.dev/programming-guides/proto3/
- **SQLite File Format:** https://www.sqlite.org/fileformat.html
- **Martin Fowler Database Evolution:** https://martinfowler.com/articles/evodb.html
- **matric-memory Migrations:** `/path/to/fortemi/migrations/`

---

## Next Steps

1. Review full research report
2. Prioritize implementation phases
3. Create GitHub issues for each phase
4. Schedule schema change training
5. Establish governance process

**Estimated Effort:** 4 weeks for complete implementation
**Priority:** High (foundational for chunking/embedding features)

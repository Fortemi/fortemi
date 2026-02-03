# ADR-033: UUIDv7-Based File Storage Architecture

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-031 (Intelligent Attachment Processing), Epic #430

## Context

The original file attachments design proposed storing files as PostgreSQL BYTEA blobs. After further analysis, this approach has limitations:

1. **Database bloat**: Large files inflate database size, slowing backups
2. **Memory pressure**: BYTEA requires loading full file into memory
3. **No streaming**: Can't serve large files efficiently
4. **Backup complexity**: pg_dump includes all blobs, increasing backup time

A filesystem-based approach with database metadata provides better:
- Streaming support for large files
- Independent file lifecycle management
- CDN/presigned URL compatibility
- Simpler backup strategies (files vs metadata)

## Decision

Implement **UUIDv7-named filesystem storage** with **2-level directory segmentation** and **BLAKE3 content-addressable deduplication**.

### 1. Storage Architecture

```
/var/lib/matric/storage/
├── blobs/                          # Primary file storage
│   ├── 01/                         # UUID prefix level 1 (2 hex chars)
│   │   ├── 94/                     # UUID prefix level 2 (2 hex chars)
│   │   │   └── 019477e8-94ab-7xxx.bin
│   │   └── 95/
│   │       └── 019477f1-9523-7xxx.bin
│   └── 02/
│       └── ...
├── previews/                       # Generated thumbnails
│   └── 01/94/019477e8-94ab-7xxx.thumb.jpg
└── temp/                           # Upload staging
    └── upload-{random}.tmp
```

### 2. Directory Segmentation Rationale

**Pattern:** `/aa/bb/{uuid}.bin` where `aa` and `bb` are first 4 hex chars of UUID

**Analysis:**

| Files | Directories | Files/Directory |
|-------|-------------|-----------------|
| 1,000 | ~16 used | ~63 |
| 100,000 | ~1,024 used | ~98 |
| 1,000,000 | ~4,096 used | ~244 |
| 10,000,000 | ~16,384 used | ~610 |
| 100,000,000 | 65,536 (max) | ~1,526 |

**Benefits:**
- 65,536 leaf directories (256 × 256)
- Uniform distribution via UUIDv7 random bits
- O(1) path calculation from UUID
- No external state required for routing

**Alternatives rejected:**
- `/YYYY/MM/DD/`: Creates hotspots, complicates retention
- `/type/aa/bb/`: Requires type at lookup time
- Single directory: Performance degrades at >10,000 files

### 3. Schema Design

```sql
-- File blob: one row per unique content (deduplication)
CREATE TABLE file_blob (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Content addressing
    content_hash TEXT NOT NULL UNIQUE,  -- 'blake3:{64 hex chars}'

    -- Metadata
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),

    -- Storage location
    storage_backend TEXT NOT NULL DEFAULT 'filesystem',  -- 'filesystem' or 'object'
    storage_path TEXT NOT NULL,  -- 'blobs/01/94/019477e8-94ab-7xxx.bin'

    -- S3 (optional)
    object_bucket TEXT,
    object_region TEXT,

    -- Reference counting
    reference_count INTEGER NOT NULL DEFAULT 0,

    -- Integrity
    verified_at TIMESTAMPTZ,
    verification_status TEXT,  -- 'valid', 'corrupted', 'pending'

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- File attachment: references from notes to blobs
CREATE TABLE file_attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- References
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES file_blob(id) ON DELETE RESTRICT,

    -- User metadata
    filename TEXT NOT NULL,
    display_order INTEGER DEFAULT 0,
    description TEXT,

    -- Processing
    processing_status TEXT NOT NULL DEFAULT 'pending',
    processing_error TEXT,
    extracted_text TEXT,
    extracted_metadata JSONB,

    -- Preview
    has_preview BOOLEAN DEFAULT FALSE,
    preview_path TEXT,

    -- Security
    virus_scan_status TEXT DEFAULT 'pending',
    quarantined BOOLEAN DEFAULT FALSE,
    quarantine_reason TEXT,

    -- Document type integration
    document_type_id UUID REFERENCES document_type(id),
    is_canonical_content BOOLEAN DEFAULT FALSE,

    -- Audit
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Reference counting triggers
CREATE OR REPLACE FUNCTION file_blob_ref_increment() RETURNS TRIGGER AS $$
BEGIN
    UPDATE file_blob SET reference_count = reference_count + 1
    WHERE id = NEW.blob_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION file_blob_ref_decrement() RETURNS TRIGGER AS $$
BEGIN
    UPDATE file_blob SET reference_count = reference_count - 1
    WHERE id = OLD.blob_id;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER file_attachment_insert_ref
AFTER INSERT ON file_attachment
FOR EACH ROW EXECUTE FUNCTION file_blob_ref_increment();

CREATE TRIGGER file_attachment_delete_ref
AFTER DELETE ON file_attachment
FOR EACH ROW EXECUTE FUNCTION file_blob_ref_decrement();
```

### 4. Content Addressing with BLAKE3

**Why BLAKE3:**
- 6x faster than SHA-256 on modern CPUs
- Cryptographically secure
- Streaming support for large files
- Prefix format allows algorithm migration

**Hash format:** `blake3:{64 hex characters}`

**Deduplication flow:**
1. Upload arrives, compute BLAKE3 hash
2. Check `file_blob.content_hash` for existing entry
3. If exists: create attachment referencing existing blob
4. If new: generate UUIDv7, store file, create blob record

### 5. Path Calculation

```rust
/// Calculate storage path from UUID.
/// UUID: 019477e8-94ab-7123-8456-789abcdef012
/// Path: blobs/01/94/019477e8-94ab-7123-8456-789abcdef012.bin
pub fn blob_storage_path(uuid: &Uuid) -> String {
    let hex = uuid.as_hyphenated().to_string();
    let prefix1 = &hex[0..2];
    let prefix2 = &hex[2..4];
    format!("blobs/{}/{}/{}.bin", prefix1, prefix2, hex)
}
```

**Key property:** Path is deterministic from UUID alone. No database lookup needed.

### 6. S3 Compatibility

Same path structure works for object storage:

```
s3://matric-storage/blobs/01/94/019477e8-94ab-7xxx.bin
```

**Backend abstraction:**

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn presigned_url(&self, path: &str, expires_in_secs: u64) -> Result<Option<String>>;
}
```

### 7. Shard/Backup Integration

**Shard structure with files:**

```
matric-shard-2026-02-03.tar.gz
├── manifest.json                 # Includes file_storage section
├── notes.jsonl
├── file_attachments.jsonl        # Attachment metadata
└── files/                        # Blob data
    └── 01/94/019477e8-94ab-7xxx.bin
```

**Manifest extension:**

```json
{
  "version": "2.0.0",
  "file_storage": {
    "blob_count": 150,
    "total_size_bytes": 524288000,
    "attachment_count": 200,
    "dedup_ratio": 1.33,
    "blobs": [
      {
        "id": "019477e8-94ab-7123-...",
        "content_hash": "blake3:abc123...",
        "content_type": "image/jpeg",
        "size_bytes": 1048576,
        "archive_path": "files/01/94/019477e8-94ab-7xxx.bin"
      }
    ]
  }
}
```

**Import with deduplication:**
- Check content_hash against existing blobs
- Skip file if hash exists (dedup on import)
- Generate new UUID for imported blob (preserves time-ordering)

### 8. Garbage Collection

**Orphan detection:**
```sql
SELECT id, storage_path
FROM file_blob
WHERE reference_count = 0
AND created_at < NOW() - INTERVAL '24 hours';
```

**Cleanup job:**
1. Query orphaned blobs (ref_count=0, age > 24h)
2. Delete file from storage
3. Delete database record

**Safety measures:**
- Minimum age prevents race conditions during upload
- Delete storage file BEFORE database row
- Reference counting via triggers ensures atomicity

### 9. Integrity Verification

```rust
pub async fn verify_blob_integrity(&self, blob_id: Uuid) -> Result<bool> {
    let blob = self.get_blob(blob_id).await?;
    let data = self.backend.read(&blob.storage_path).await?;
    let computed_hash = compute_content_hash(&data);

    let is_valid = computed_hash == blob.content_hash;

    sqlx::query!(
        "UPDATE file_blob SET verified_at = NOW(), verification_status = $2 WHERE id = $1",
        blob_id,
        if is_valid { "valid" } else { "corrupted" }
    ).execute(&self.pool).await?;

    Ok(is_valid)
}
```

## Consequences

### Positive

- (+) **Scalable**: Handles millions of files efficiently
- (+) **No database bloat**: Files separate from metadata
- (+) **Streaming**: Large files served without full memory load
- (+) **CDN-ready**: S3 presigned URLs for edge delivery
- (+) **Deduplication**: Same file = 1 copy across all notes
- (+) **Integrity**: BLAKE3 hash verifies file corruption
- (+) **Deterministic paths**: No DB lookup for file location

### Negative

- (-) **Two systems**: Database + filesystem must stay in sync
- (-) **Backup complexity**: Must backup both DB and files
- (-) **Atomic operations**: No ACID for file writes

### Mitigations

- Reference counting ensures sync between DB and files
- Shard export bundles both together
- Atomic write via temp file + rename

## Implementation

### Phase 1: Core Storage
- Schema migration
- `PgFileStorageRepository` implementation
- `FilesystemBackend` implementation
- API endpoints: upload, download, delete

### Phase 2: Shard Integration
- Manifest extension
- Export with files
- Import with deduplication
- Checksum verification

### Phase 3: S3 Backend
- `S3Backend` implementation
- Presigned URL generation
- MinIO local testing
- Configuration options

### Phase 4: Operations
- Garbage collection job
- Integrity verification job
- Storage metrics endpoint
- Quota enforcement

## References

- Git object storage: https://git-scm.com/book/en/v2/Git-Internals-Git-Objects
- Docker content-addressable storage: https://docs.docker.com/storage/storagedriver/
- BLAKE3 specification: https://github.com/BLAKE3-team/BLAKE3-specs

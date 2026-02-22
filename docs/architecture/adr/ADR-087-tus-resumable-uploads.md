# ADR-087: tus v1.0.0 Resumable Upload Protocol for Large Files

**Status:** Accepted
**Date:** 2026-02-22
**Deciders:** Architecture team
**Related:** ADR-033 (File Storage Architecture), ADR-031 (Intelligent Attachment Processing), ADR-036 (File Safety Validation), HotM#103 (Chrome HTTP/2 large upload bug)

## Context

Fortemi supports file attachments up to 1GB (`MATRIC_MAX_UPLOAD_SIZE_BYTES`), but the existing multipart upload mechanism becomes unreliable for files larger than approximately 200MB due to two compounding issues:

1. **Chrome HTTP/2 connection drops**: Chrome silently drops HTTP/2 connections mid-transfer for large uploads (tracked in HotM#103). The confirmed workaround is disabling HTTP/2 (`http2 off` in nginx), but this sacrifices HTTP/2 benefits for all traffic. curl and other clients are unaffected.

2. **No resume capability**: When any upload fails — whether from a network interruption, browser bug, or timeout — the entire upload must be restarted from byte zero. For a 1GB video file on a modest connection, this can mean losing 10+ minutes of transfer progress.

These limitations directly impact the attachment pipeline for large media files (video, audio, 3D models) that are central to Fortemi's multimodal knowledge capture. Users working with video recordings, audio transcriptions, and 3D model analysis need reliable uploads regardless of file size or client browser.

### Constraints

1. The existing attachment pipeline (`store_file_tx` in `PgFileStorageRepository`) handles BLAKE3 deduplication, blob storage, and extraction job queuing — any new upload mechanism must feed into this pipeline, not bypass it.
2. Nginx reverse proxy sits in front of the API; upload streaming requires `proxy_request_buffering off` to avoid buffering large request bodies to disk twice.
3. The current `TUS_CHUNK_MAX_SIZE` for nginx's `client_max_body_size` on the tus endpoint must accommodate individual PATCH chunks, not the full file.
4. Staging files consume disk space until finalized or expired — the system must bound this growth without requiring a dedicated background worker.

## Decision

Implement the [tus v1.0.0 protocol](https://tus.io/protocols/resumable-upload) with the `creation` and `creation-with-upload` extensions, integrated as a new endpoint group under the existing note attachment path.

### 1. Endpoint Design

Nest tus endpoints under the existing attachment resource to maintain the note-scoped authorization model:

```
OPTIONS /api/v1/notes/{note_id}/attachments/tus       — Protocol discovery
POST    /api/v1/notes/{note_id}/attachments/tus       — Create upload (+ optional initial chunk)
HEAD    /api/v1/notes/{note_id}/attachments/tus/{id}  — Query upload offset
PATCH   /api/v1/notes/{note_id}/attachments/tus/{id}  — Append chunk (+ finalize on completion)
DELETE  /api/v1/notes/{note_id}/attachments/tus/{id}  — Cancel upload
```

The `OPTIONS` response advertises protocol capabilities:

```
Tus-Resumable: 1.0.0
Tus-Version: 1.0.0
Tus-Extension: creation,creation-with-upload
Tus-Max-Size: 1073741824
```

### 2. Database Schema

New `tus_upload` table tracks in-progress uploads:

```sql
CREATE TABLE tus_upload (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,

    -- tus protocol state
    upload_length BIGINT NOT NULL CHECK (upload_length > 0),
    upload_offset BIGINT NOT NULL DEFAULT 0 CHECK (upload_offset >= 0),
    upload_complete BOOLEAN NOT NULL DEFAULT FALSE,

    -- File metadata (from Upload-Metadata header)
    filename TEXT NOT NULL,
    content_type TEXT,

    -- Staging
    staging_path TEXT NOT NULL,    -- relative to storage root: 'staging/tus/{uuid}.part'

    -- Lifecycle
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tus_upload_note_id ON tus_upload(note_id);
CREATE INDEX idx_tus_upload_expires_at ON tus_upload(expires_at) WHERE NOT upload_complete;
```

A `PgTusRepository` in `matric-db` provides CRUD operations: `create`, `get`, `update_offset`, `mark_complete`, `delete`, and `delete_expired`.

### 3. Upload Lifecycle

```
Client                          API                         Disk
  |                              |                           |
  |-- POST (Upload-Length, meta) |                           |
  |                              |-- INSERT tus_upload        |
  |                              |-- create staging file ---->|
  |<-- 201 Location: /tus/{id}   |                           |
  |                              |                           |
  |-- PATCH (offset=0, chunk 1)  |                           |
  |                              |-- verify offset matches    |
  |                              |-- append to staging ------>|
  |                              |-- UPDATE offset            |
  |<-- 204 Upload-Offset: N      |                           |
  |                              |                           |
  |  (network interruption)      |                           |
  |                              |                           |
  |-- HEAD /tus/{id}             |                           |
  |<-- 200 Upload-Offset: N      |                           |
  |                              |                           |
  |-- PATCH (offset=N, chunk 2)  |                           |
  |                              |-- append to staging ------>|
  |                              |-- offset == length?        |
  |                              |-- YES: finalize            |
  |                              |   call store_file_tx() --->|
  |                              |   (BLAKE3, blob, attach)   |
  |                              |-- DELETE tus_upload        |
  |                              |-- remove staging file ---->|
  |<-- 204 Upload-Offset: total  |                           |
```

**Finalization** (when `upload_offset == upload_length`):

1. Run file safety validation (magic bytes check, extension blocklist) per ADR-036
2. Open a read stream from the staging file
3. Call `store_file_tx` — the existing attachment pipeline handles BLAKE3 hashing, `file_blob` deduplication, `file_attachment` creation, and extraction job queuing
4. Delete the `tus_upload` row and staging file
5. Return the created `file_attachment` ID in the `X-Attachment-Id` response header

### 4. Staging File Layout

```
/var/lib/matric/storage/
├── blobs/          # Existing blob storage (ADR-033)
├── previews/       # Existing preview storage
├── temp/           # Existing upload temp files
└── staging/
    └── tus/        # tus in-progress uploads
        ├── 019500a1-xxxx.part
        └── 019500b2-yyyy.part
```

Staging files use the `tus_upload` UUID as the filename. The path is deterministic: `staging/tus/{upload_id}.part`.

### 5. Chunk Size and Limits

| Parameter | Default | Env Var | Description |
|-----------|---------|---------|-------------|
| Max file size | 1GB | `MATRIC_MAX_UPLOAD_SIZE_BYTES` | Shared with multipart uploads |
| Max chunk size | 50MB | `TUS_CHUNK_MAX_SIZE` | Per-PATCH request body limit |
| Upload expiry | 24 hours | `TUS_UPLOAD_EXPIRY_HOURS` | Time before incomplete uploads are cleaned |

The nginx location block for the tus endpoint sets `client_max_body_size` to match `TUS_CHUNK_MAX_SIZE` (50MB default), not the full file size. This prevents nginx from rejecting legitimate chunks while still bounding individual request sizes.

### 6. Expiry and Cleanup

Incomplete uploads expire after 24 hours (configurable via `TUS_UPLOAD_EXPIRY_HOURS`). Cleanup uses a **lazy strategy**: each incoming `POST` (new upload creation) triggers a sweep of expired uploads before processing the new request:

```rust
async fn cleanup_expired(repo: &PgTusRepository, storage: &StorageBackend) {
    let expired = repo.delete_expired().await?;
    for upload in expired {
        let _ = storage.delete(&upload.staging_path).await;
    }
}
```

This approach avoids a dedicated background worker while ensuring staging disk usage is bounded. The worst case is 24 hours of orphaned staging files, which is acceptable given the 1GB per-file limit and typical upload patterns.

A dedicated cleanup job can be added later if lazy cleanup proves insufficient for high-volume deployments.

### 7. Nginx Configuration

```nginx
# tus resumable upload endpoint
location ~ ^/api/v1/notes/[^/]+/attachments/tus {
    proxy_pass http://127.0.0.1:3000;
    proxy_request_buffering off;
    proxy_http_version 1.1;
    client_max_body_size 50m;

    # Forward tus headers
    proxy_set_header Upload-Offset $http_upload_offset;
    proxy_set_header Upload-Length $http_upload_length;
    proxy_set_header Tus-Resumable $http_tus_resumable;
    proxy_set_header Content-Type $http_content_type;
}
```

Key points:
- `proxy_request_buffering off` streams the request body directly to the API without buffering to a temp file on disk (prevents double disk write)
- `client_max_body_size 50m` matches the chunk size limit, not the total file size
- Standard `proxy_set_header` for tus protocol headers

### 8. Concurrency and Safety

- **Sequential PATCH**: The tus protocol requires chunks to be appended sequentially (each PATCH must specify the current offset). Concurrent PATCHes to the same upload are rejected with `409 Conflict` if the offset does not match.
- **File locking**: The PATCH handler holds the `tus_upload` row lock (via `SELECT ... FOR UPDATE`) for the duration of the append to prevent race conditions between concurrent PATCH requests for the same upload.
- **Atomicity**: Each PATCH appends to the staging file and updates the database offset in a single transaction. If either fails, the offset is not advanced and the client can retry the same chunk.

## Consequences

### Positive

- (+) **Resumable uploads**: Network interruptions no longer require restarting from scratch; clients resume from the last successful byte offset
- (+) **Standard protocol**: tus v1.0.0 has broad client support (tus-js-client, Uppy, TusDart, tuskit) reducing frontend implementation effort
- (+) **Chrome HTTP/2 workaround**: Chunked uploads of 50MB each avoid the Chrome connection drop threshold (~200MB) without disabling HTTP/2 globally
- (+) **Pipeline compatibility**: Finalization feeds into the existing `store_file_tx` path, so BLAKE3 deduplication, blob storage, extraction jobs, and downstream processing (embedding, concept tagging, linking) all work unchanged
- (+) **Bounded resource usage**: Chunk size limits, upload expiry, and lazy cleanup prevent unbounded staging disk consumption
- (+) **Note-scoped authorization**: Endpoints nested under `/notes/{id}/attachments/` inherit the existing note access control model

### Negative

- (-) **Two upload paths**: Small files continue using multipart upload; large files use tus. Frontend must choose between the two (or always use tus)
- (-) **Staging disk space**: In-progress uploads consume disk space until finalized or expired; worst case is `MAX_MEMORIES * concurrent_uploads * 1GB` of staging data
- (-) **Protocol complexity**: Five new handlers, a new database table, and a new nginx location block increase surface area
- (-) **No parallel chunks**: The `creation` extension requires sequential appends; parallel chunk upload (the `concatenation` extension) is not implemented, limiting upload speed on high-bandwidth connections

### Mitigations

- **Two upload paths**: Document both paths clearly in the API docs. Frontend can default to tus for all uploads since tus handles small files efficiently too (single POST with `creation-with-upload` sends the entire file in one request). The multipart path can be deprecated in a future release.
- **Staging disk space**: The 24-hour expiry and lazy cleanup bound worst-case growth. Monitoring via the existing storage metrics endpoint can alert on excessive staging usage.
- **Protocol complexity**: The tus protocol is well-specified and the implementation is self-contained in 5 handlers + 1 repository. No changes to the existing attachment pipeline are required.
- **No parallel chunks**: Sequential upload throughput is sufficient for the target use case (1GB files). The `concatenation` extension can be added later if users report bandwidth bottlenecks.

## Implementation

**Migration:** `migrations/{timestamp}_create_tus_upload_table.sql`

**Code Changes:**

| File | Change |
|------|--------|
| `crates/matric-db/src/tus.rs` | New `PgTusRepository` with CRUD operations |
| `crates/matric-db/src/lib.rs` | Register `tus` module, add to `Database` |
| `crates/matric-api/src/handlers/tus.rs` | New handler module: `options`, `create`, `head`, `append`, `cancel` |
| `crates/matric-api/src/main.rs` | Mount tus routes under `/api/v1/notes/:note_id/attachments/tus` |
| `deploy/nginx/memory.integrolabs.net.conf` | Add tus location block with streaming config |

**Testing:**

- Unit test: `PgTusRepository` CRUD operations (create, update offset, mark complete, delete expired)
- Unit test: Offset mismatch returns 409 Conflict
- Unit test: Finalization calls `store_file_tx` and cleans up staging
- Unit test: Expired upload cleanup removes both database rows and staging files
- Integration test: Full upload lifecycle (POST + multiple PATCHes + HEAD resume + finalization)
- Integration test: `creation-with-upload` single-request upload for small files
- Integration test: File safety validation rejects blocked file types at finalization

## References

- [tus v1.0.0 Protocol Specification](https://tus.io/protocols/resumable-upload)
- [tus-js-client](https://github.com/tus/tus-js-client)
- [Uppy file uploader (tus support)](https://uppy.io/docs/tus/)
- ADR-033: UUIDv7-Based File Storage Architecture
- ADR-031: Intelligent Attachment Processing
- ADR-036: File Safety Validation
- HotM#103: Chrome HTTP/2 large upload bug
- `crates/matric-db/src/file_storage.rs` — `store_file_tx` pipeline
- `deploy/nginx/memory.integrolabs.net.conf` — nginx proxy configuration

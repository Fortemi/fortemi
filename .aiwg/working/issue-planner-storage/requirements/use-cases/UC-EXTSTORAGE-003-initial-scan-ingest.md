# UC-EXTSTORAGE-003: Initial Scan-and-Ingest on Referenced Archive Creation

**Workstream**: WS-4 (Scan-and-Ingest Job Pipeline)
**Source**: synthesis §4 WS-4, §3 Decision 3
**Status**: Draft
**Priority**: HIGH (this is the value-delivery moment for #736)

## Actor

**Primary**: `DirectoryScanHandler` (background job worker)
**Secondary**: ScanWalker (UC-EXTSTORAGE-002), BLAKE3 hasher, extraction pipeline, pgvector embedder, ArchiveRepository

## Goal

For a newly-created Referenced archive, walk its source directory, dedup files by content hash, register them in the per-archive blob/attachment/note tables, and queue per-file Extraction jobs that produce chunks and embeddings — all without copying file bytes into managed storage.

## Preconditions

- Referenced archive exists (UC-EXTSTORAGE-001 completed)
- `DirectoryScan` job is queued and a worker has picked it up
- `archive_registry.scan_status` transitioned from `idle` → `scanning`
- WS-1 (`ReferencedBackend`) and WS-3 (`ScanWalker`) are functional

## Main Success Scenario

1. DirectoryScanHandler reads `archive_registry` row, retrieves `source_path`, `scan_config`
2. Handler invokes ScanWalker (UC-EXTSTORAGE-002) to enumerate `FileCandidate` records
3. For each FileCandidate:
   - 3a. Compute streaming BLAKE3 hash: `compute_content_hash_stream(path)` (WS-1)
   - 3b. Query `archive_<id>.blobs` for existing row with `content_hash = $hash`
   - 3c. If exists: increment reference count, link to new attachment record (dedup hit)
   - 3d. If not exists: INSERT new `blobs` row with `storage_backend='referenced'`, `source_path=<absolute path>`, `content_hash`, `size_bytes`
   - 3e. INSERT `attachments` row referencing the blob
   - 3f. INSERT `notes` row with `attachment_id` populated (one note per source file)
   - 3g. Enqueue `Extraction` job for the note with `extraction_strategy` selected by file type (code, text, image, etc.)
4. After all candidates processed, handler updates `archive_registry.scan_status = 'idle'`, `last_scan_at = NOW()`, `last_scan_summary = {files_ingested, files_deduped, files_quarantined, duration_ms}`
5. Extraction workers consume the queued jobs (existing extraction pipeline, extended at line 146 of `extraction_handler.rs` to recognize `storage_backend='referenced'` for path-access)
6. Each Extraction job:
   - 6a. Reads source file via `ReferencedBackend::read(path)` (no copy; direct mmap or buffered read)
   - 6b. Runs `CodeAstAdapter` or other adapter to produce chunks
   - 6c. Calls embedder (Ollama or configured backend) to produce vectors
   - 6d. INSERTs chunks and embeddings into per-archive pgvector tables
   - 6e. Routes derived artifacts (thumbnails, transcripts) to `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/` (Decision 3) — never to source_path
7. Search API returns results for ingested chunks via existing `/api/v1/search` endpoint

## Alternative Flows

### AF-1: Idempotent re-scan (no changes)

- Re-running the scan on an unchanged source directory
- At step 3b: every file's hash matches an existing blob → all dedup hits, zero new INSERTs
- At step 4: summary `{files_ingested: 0, files_deduped: N, files_quarantined: 0}`
- No Extraction jobs queued (the existing chunks are still valid)

### AF-2: File modified since last scan

- File content changed; hash differs from prior `blobs.content_hash`
- At step 3b: no match found → treated as new blob (3d)
- At step 3f: new note inserted, old note for previous version is NOT deleted (orphan cleanup is out of scope for v1)
- At step 3g: new Extraction job queued; old chunks remain in pgvector (acceptable v1 behavior; documented limitation)
- Future v2 work: implement delete-on-mtime-change or content-hash-correlation rename detection (synthesis §5 R-9)

### AF-3: New files added since last scan

- Source directory gained 50 new files since previous scan
- At step 3: walker yields 50 new candidates; existing files dedup-hit
- At step 4: summary reflects 50 new ingests

### AF-4: File quarantined by secret scan

- File matches secret denylist in UC-EXTSTORAGE-002
- ScanWalker emits `QuarantineEvent`, does NOT emit `FileCandidate`
- At step 3: file is absent from the candidate list → never enters blob/attachment/note tables → never embedded
- Quarantine record is queryable via UC-EXTSTORAGE-009

## Exception Flows

### EF-1: BLAKE3 hash failure (read error mid-stream)

- At step 3a: file becomes unreadable mid-hash (mount disconnect, permission revoked)
- Handler logs `{event: "hash_failed", path, error}`, skips file, continues with remaining candidates
- Summary includes `partial: true, hash_failures: [...]`

### EF-2: Extraction job fails

- At step 6: extraction adapter raises (e.g., malformed PDF, OOM on huge file)
- Existing extraction-pipeline error handling applies (retry, dead-letter, log)
- DirectoryScan job is NOT affected; it completes after queueing all extraction jobs
- Failed extractions are visible in existing job-status API

### EF-3: Source directory disappears mid-scan

- At step 3 or 6: mount unmounts, NFS times out
- Handler logs `{event: "source_unavailable", path, error}`
- Handler updates `archive_registry.scan_status = 'error'`, `scan_error = "source_path unreachable"`
- Subsequent reads fall back to Decision 8 behavior (fail-open on reads, 503 on writes)

### EF-4: pgvector INSERT fails (DB full, quota exceeded)

- At step 6d: DB write fails
- Extraction job is retried per existing extraction-handler retry policy
- If retries exhausted: extraction status `error`, visible in job-status

## Postconditions

- `archive_<id>.blobs`, `.attachments`, `.notes` populated with one row set per source file (minus quarantined)
- `archive_<id>.chunks` and pgvector embeddings populated after Extraction jobs complete
- `archive_registry.scan_status = 'idle'`, `last_scan_at = NOW()`
- Source directory unchanged (verifiable by mtime/inode/checksum comparison before vs after)
- Derived artifacts (if any) exist under `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/`

## Acceptance Criteria

- [ ] AC-1: Initial scan-and-ingest of a 1k-file fixture repo completes with `scan_status='idle'` and zero errors
- [ ] AC-2: After scan, `SELECT COUNT(*) FROM archive_<id>.notes` equals (walker candidates count) minus (quarantine count)
- [ ] AC-3: After Extraction jobs complete, `SELECT COUNT(*) FROM archive_<id>.chunks WHERE embedding IS NOT NULL` is > 0
- [ ] AC-4: `GET /api/v1/search?query=<known-term>&archive=<name>` returns at least one hit from the ingested chunks
- [ ] AC-5: Re-running scan on unchanged source is a no-op (zero new INSERTs; AC verified via `pg_stat_user_tables` row-insert delta = 0 between two scans)
- [ ] AC-6: Source directory checksums (computed via `find $source -type f -exec sha256sum {} \;`) are byte-identical before and after scan
- [ ] AC-7: Derived artifacts for a test image file land in `{derived_root}/{archive_id}/` not in `$source_path`
- [ ] AC-8: Per-archive PostgreSQL schema isolation preserved: `SELECT * FROM archive_other.notes` from within archive context returns access denied (per ADR-090-style middleware)

## Non-Functional Requirements

Applies (see `nfr-external-storage.md`):

- NFR-EXTSTORAGE-002 (no source writes)
- NFR-EXTSTORAGE-005 (scan throughput)
- NFR-EXTSTORAGE-006 (embedding throughput dependency on backend)
- NFR-EXTSTORAGE-007 (initial scan duration target)
- NFR-EXTSTORAGE-010 (per-archive scan metrics)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 3, §3 Decision 7, §4 WS-4, §6 Q-7
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-093 (planned): Derived artifact placement

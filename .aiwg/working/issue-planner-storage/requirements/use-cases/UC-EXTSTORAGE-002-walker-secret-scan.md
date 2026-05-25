# UC-EXTSTORAGE-002: Background Worker Walks Directory and Reports Secret-Scan Findings

**Workstream**: WS-3 (Walker + Ignore + Secret-Scan)
**Source**: synthesis §4 WS-3, §3 Decision 5, §3 Decision 7
**Status**: Draft
**Priority**: HIGH (security-critical — protects against R-1 secret leakage)

## Actor

**Primary**: `ScanWalker` (background module within `DirectoryScan` job)
**Secondary**: Admin (consumes quarantine report via API), `ignore::WalkBuilder`, BLAKE3 hasher

## Goal

Walk a Referenced archive's source directory, applying the default ignore list and `.gitignore` discovery, while pre-scanning each candidate file for secret content. Skip and log secrets-bearing files BEFORE they enter the extraction/embedding pipeline so they never appear in pgvector chunks.

## Preconditions

- A Referenced archive exists with `storage_mode='referenced'` and `source_path` populated
- API process has read permission on the directory tree
- `scan_config` is loaded (uses defaults from Decision 7 unless overridden)
- A `DirectoryScan` job is currently executing (called from UC-EXTSTORAGE-004)

## Main Success Scenario

1. ScanWalker initializes `ignore::WalkBuilder` with: `add_custom_ignore_filename(".fortemiignore")`, standard `.gitignore` discovery, `git_global=false`, `git_exclude=false`
2. ScanWalker overlays the default ignore patterns from Decision 7 (`.git/`, `node_modules/`, `dist/`, `*.log`, files >10MB, etc.)
3. ScanWalker calls `builder.build_parallel()` with `threads(min(4, num_cpus))`
4. For each visited file entry, walker invokes per-file inspector:
   - 4a. Check path-based denylist (Decision 7): `*.pem`, `*.key`, `.env*`, `id_rsa*`, `.ssh/*`, etc.
   - 4b. If path matches denylist: emit `QuarantineEvent{path, reason: "path_denylist", pattern}`, skip file
   - 4c. Otherwise, open file and read first 64KB (or whole file if smaller)
   - 4d. Run content-based secret regex set against the buffer: PEM PRIVATE KEY header, AWS access key, GitHub PAT, JWT prefix
   - 4e. If any regex matches: emit `QuarantineEvent{path, reason: "content_denylist", pattern, line}`, skip file
   - 4f. Otherwise, file passes the gate; emit `FileCandidate{path, size, mtime}` to the ingest pipeline
5. ScanWalker collects all `QuarantineEvent` records and persists them to per-archive table `archive_<id>.quarantined_files` (path, reason, pattern, scanned_at)
6. ScanWalker returns summary `{files_scanned, files_passed, files_quarantined, walk_duration_ms}` to the DirectoryScan job

## Alternative Flows

### AF-1: User-supplied `.fortemiignore` overlay

- Source directory contains `.fortemiignore` with custom patterns
- At step 1: `WalkBuilder` honors both `.gitignore` AND `.fortemiignore`
- At step 4: `node_modules/foo/bar` matches custom rule → skipped; emits `IgnoreEvent` (informational, not quarantine)

### AF-2: Empty directory

- Source directory exists but contains no files
- At step 4: walker yields zero entries
- At step 6: returns `{files_scanned: 0, files_passed: 0, files_quarantined: 0}` cleanly

### AF-3: Symlink encountered

- Walker encounters a symlink during traversal
- `ignore` crate default: do NOT follow symlinks
- Walker emits `SymlinkSkipped{path, target}` log entry, does not follow

## Exception Flows

### EF-1: Permission denied on subdirectory

- At step 4: `read_dir()` returns `EACCES` for a subdir mid-walk
- Walker logs warning `{event: "permission_denied", path: "/srv/data/private"}`, skips subdir, continues walk (does NOT abort entire scan)
- Summary includes `partial: true, skipped_subdirs: [...]`

### EF-2: Symlink loop detected

- At step 4: `ignore` crate's built-in loop detection trips
- Walker logs `{event: "symlink_loop", path}`, skips, continues

### EF-3: File too large to read for content scan

- At step 4c: file is >10MB (already ignored by default size cap), or 4MB-10MB range
- If file is in the file-size cap range: walker skips it entirely at step 2 (never reaches 4c)
- If file is below cap but content scan reads only first 64KB: scan proceeds against the prefix; secrets in tail are NOT detected (documented limitation; acceptable trade-off per Decision 5)

### EF-4: File modified during scan

- At step 4c: file's content changes between path enumeration and read (race)
- Walker hashes whatever it reads; subsequent re-ingest on next scan picks up the new content (idempotent on content_hash)
- No special handling; the next scan will reconcile

## Postconditions

- `archive_<id>.quarantined_files` populated with all skipped files and reasons
- `FileCandidate` records emitted for all passing files (consumed by WS-4)
- No source files modified, no source files deleted
- Walker thread pool released (`min(4, num_cpus)` workers)
- Walk duration logged in JSON for observability

## Acceptance Criteria

- [ ] AC-1: Walker correctly respects `.gitignore` for 5 representative test cases: `node_modules/`, `.env`, `*.log`, `dist/`, custom user patterns
- [ ] AC-2: Walker catches PEM-formatted RSA private key in a fixture file named `not-obviously-a-key.txt` (content-based, not path-based)
- [ ] AC-3: Walker catches AWS access key pattern `AKIA[0-9A-Z]{16}` in a fixture file
- [ ] AC-4: Walker skips files matching path denylist (`secrets.json`, `.aws/credentials`) without reading content
- [ ] AC-5: Walker yields zero quarantine events for a clean fixture repo (no secrets) — false-positive rate is 0
- [ ] AC-6: Permission-denied subdir produces warning log but does not abort scan; remaining files are still walked
- [ ] AC-7: Symlink to outside source root is skipped and logged; symlink loop is detected and broken
- [ ] AC-8: Walk throughput meets NFR-EXTSTORAGE-005 target (≥500 files/sec on warmed cache for files <100KB each)
- [ ] AC-9: Quarantined files are accessible via `GET /api/v1/archives/{name}/quarantined-files` (UC-EXTSTORAGE-009)

## Non-Functional Requirements

Applies (see `nfr-external-storage.md`):

- NFR-EXTSTORAGE-002 (no source-directory writes)
- NFR-EXTSTORAGE-005 (scan throughput target)
- NFR-EXTSTORAGE-008 (per-file quarantine logging in JSON)
- NFR-EXTSTORAGE-010 (observability: scan-completion metrics per archive)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 5, §3 Decision 7, §4 WS-3, §5 R-1
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-095 (planned): Secret detection at ingest

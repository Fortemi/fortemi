# UC-EXTSTORAGE-006: Generate Derived Artifacts in Companion Managed Location

**Workstream**: WS-6 (Derived Artifact Companion Location)
**Source**: synthesis §4 WS-6, §3 Decision 3
**Status**: Draft
**Priority**: HIGH (architectural invariant — preserves the "never write to source" guarantee)

## Actor

**Primary**: Extraction worker (background job, runs `extract_handler` adapters)
**Secondary**: ImageThumbnailAdapter, VideoKeyframeAdapter, AudioTranscriptAdapter

## Goal

When the Extraction pipeline produces derived artifacts (thumbnails, transcripts, keyframes, optimized media variants) for files in a Referenced archive, write them to a Fortemi-managed companion directory — never to the user-owned source directory. This preserves the absolute invariant that Fortemi does not modify source paths.

## Preconditions

- Source file in a Referenced archive has been ingested (UC-EXTSTORAGE-003)
- Extraction job for that file is dispatched
- Adapter determines the file needs a derived artifact (image → thumbnail, video → keyframes + transcript, etc.)
- `FORTEMI_DERIVED_STORAGE_PATH` is set (default `{FILE_STORAGE_PATH}/derived/`)

## Main Success Scenario

1. Extraction worker invokes adapter (e.g., `ImageThumbnailAdapter` for a `.jpg` in source path)
2. Adapter reads source bytes via `ReferencedBackend::read($source_path)` (no copy; mmap or buffered)
3. Adapter generates derived artifact in memory (e.g., 256×256 thumbnail)
4. Adapter resolves derived storage path: `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}/thumb-256.jpg`
5. Adapter ensures parent directory exists: `mkdir -p {derived_root}/{archive_id}/`
6. Adapter writes derived bytes via the Filesystem (Managed) backend (not Referenced backend)
7. Adapter calls existing `store_derived_attachment_tx()` with `storage_backend='filesystem'` and path under derived root
8. Database records: per-archive `attachments` row for the derived artifact with parent-child link to the source attachment
9. Extraction continues to chunking/embedding using the derived artifact (e.g., transcribed text from a video)

## Alternative Flows

### AF-1: Source archive is Managed (existing behavior — no change)

- At step 4: archive's `storage_mode='managed'`
- Adapter uses existing logic: derived artifact lands at `{FILE_STORAGE_PATH}/managed/{archive_id}/{blob_id}/`
- No behavior change for Managed archives

### AF-2: Custom derived path per archive

- Operator sets `archive_registry.scan_config.derived_storage_override` to a custom path
- At step 4: adapter uses override path instead of `FORTEMI_DERIVED_STORAGE_PATH`
- Use case: spill derived artifacts of a large media archive to a separate disk

### AF-3: Derived artifact already exists (re-extraction)

- At step 6: target file already present (previous extraction)
- Adapter overwrites atomically (write-to-temp + rename); existing extraction pipeline behavior preserved

## Exception Flows

### EF-1: Insufficient disk space on derived root

- At step 6: write fails with `ENOSPC`
- Adapter logs error, marks extraction job failed, surfaces via job-status API
- Source file is NOT modified; archive entry remains with extraction `error` state

### EF-2: Derived root does not exist or unwritable

- At step 5: `mkdir` fails with `EACCES` or `EROFS`
- Adapter logs `{event: "derived_root_unwritable", path}`, fails extraction
- Operator must fix `FORTEMI_DERIVED_STORAGE_PATH` permissions

### EF-3: Adapter attempts to write to source_path (BUG GUARD)

- BUG: an adapter accidentally constructs a path like `{source_path}/.fortemi-thumb.jpg`
- ReferencedBackend's `write()` MUST return `Err(NotSupported)` per WS-1
- Adapter must handle this error; this exception flow is a regression test guard, not a normal flow

## Postconditions

- Derived artifact file exists at `{derived_root}/{archive_id}/{blob_id}/<artifact-name>`
- Database `attachments` row links derived to source
- `source_path` directory contents: byte-identical to pre-extraction state (verifiable by checksum diff)
- Dropping the archive cleans up `{derived_root}/{archive_id}/` (UC implied by WS-6 drop semantics)

## Acceptance Criteria

- [ ] AC-1: After extracting a 1MB image file in a Referenced archive, thumbnail exists at `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}/thumb-256.jpg`
- [ ] AC-2: Source directory checksum (`sha256sum $source_path/image.jpg`) is unchanged after extraction
- [ ] AC-3: No file is created in `$source_path` after extraction (verifiable via `find $source_path -newer /tmp/before_extraction`)
- [ ] AC-4: For a video file: keyframes land in `{derived_root}/{archive_id}/{blob_id}/keyframes/`, transcript in `{derived_root}/{archive_id}/{blob_id}/transcript.json`
- [ ] AC-5: Dropping the Referenced archive (`DELETE /api/v1/archives/{name}`) removes `{derived_root}/{archive_id}/` entirely but leaves `source_path` untouched
- [ ] AC-6: Managed archives continue to write derived artifacts to existing path (`{FILE_STORAGE_PATH}/managed/{archive_id}/`) — no regression
- [ ] AC-7: ReferencedBackend `write()` method returns `Err(NotSupported)` when invoked (defense-in-depth invariant)
- [ ] AC-8: If `FORTEMI_DERIVED_STORAGE_PATH` is unset, default resolves to `{FILE_STORAGE_PATH}/derived/`

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-002 (no source writes invariant)
- NFR-EXTSTORAGE-011 (managed-mode archives unchanged)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 3, §4 WS-6, §5 R-8
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-093 (planned): Derived artifact placement

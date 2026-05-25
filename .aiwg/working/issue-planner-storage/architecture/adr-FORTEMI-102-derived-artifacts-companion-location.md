# ADR-FORTEMI-102: Derived Artifacts for Referenced Archives Go to a Managed Companion Location

**Status**: Proposed
**Date**: 2026-05-21
**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (commissioned by Phase 3 SDLC corpus generation)
**Source**: Synthesis §3 Decision 3

## Context

Fortemi's extraction pipeline generates **derived artifacts** for ingested files: thumbnails for images, transcripts and keyframes for video/audio, embedding vectors for chunked text, sprite sheets and 720p preview variants for media. The existing pipeline writes these via `extraction_handler::store_derived_attachment_tx` to the Managed blob store rooted at `FILE_STORAGE_PATH`.

For Referenced archives, the source files live in user-owned directories that Fortemi must never write to (the read-only invariant — see ADR-FORTEMI-101 and the broader synthesis §1.2 consensus that "source files must never be written to or deleted by the indexer"). But derived artifacts must still land *somewhere* — the existing extraction pipeline expects to write them, and skipping derivation would break video/audio search, thumbnail rendering, and the embedding-cache layer that hybrid search depends on.

Three placement options were considered:

- **A: Companion managed directory** at `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}.bin`, with `FORTEMI_DERIVED_STORAGE_PATH` defaulting to `{FILE_STORAGE_PATH}/derived/`.
- **B: Inline in PostgreSQL** — store derived artifact bytes in a per-archive BLOB column, avoiding any filesystem write outside the existing Managed root.
- **C: Sidecar in source directory** at `{source_path}/.fortemi-derived/`, alongside the user's files.

Stream A (§8.1) explicitly names Option C as a **critical anti-pattern**. Real users mount read-only volumes (compliance-driven, immutable infrastructure, content-distribution snapshots); writing a sidecar would either silently fail or, worse, fail loudly at the worst moment (during ingest after the user's just configured the archive). Other users have CI that fails on dirty working trees — a hidden `.fortemi-derived/` directory in a git-tracked repo would surface as a confusing untracked-files report on every CI run. Option C is non-negotiable: ruled out.

Option B (inline DB) is technically possible — PostgreSQL handles large objects via `pg_largeobject` or `bytea` — but the synthesis (§3 Decision 3) flags the disk-usage profile as the disqualifier. Video and audio archives generate transcripts and keyframe sets that can be tens of MB per source file. A Referenced archive over a 10k-file media collection could push tens of GB into the database, where it cannibalizes shared_buffers, complicates backup/restore, and bloats WAL replication.

Option A is the standard pattern Stream C found Fortemi already using for Managed-archive derived artifacts (the existing `store_derived_attachment_tx` writes to `FILE_STORAGE_PATH`). For Referenced archives, the change is parameterizing the root path — derived artifacts go to a separate volume per `FORTEMI_DERIVED_STORAGE_PATH` so the operator can size and back up derived storage independently from the Managed source store.

## Decision

**Derived artifacts for Referenced archives are written to a managed companion location at `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}.bin`** using the existing `FilesystemBackend`. The companion root is operator-configurable via env var `FORTEMI_DERIVED_STORAGE_PATH` (default `{FILE_STORAGE_PATH}/derived/`).

Concretely:

1. The blob row for a derived artifact in a Referenced archive has `storage_backend='filesystem'` (not `'referenced'`) and `storage_path` pointing under the companion root. This is the "mixed-mode at the blob layer, single-mode at the archive layer" pattern (ADR-FORTEMI-100).
2. `extraction_handler::store_derived_attachment_tx` is extended to consult the archive's `storage_mode`. For Managed archives, it writes to `FILE_STORAGE_PATH` (unchanged behavior). For Referenced archives, it writes to `FORTEMI_DERIVED_STORAGE_PATH/{archive_id}/`.
3. `drop_archive_schema()` is extended with a best-effort `rm -rf` of `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/` after the schema drop. Source path under user control is **never** touched (enforced by `ReferencedBackend::delete` returning `Err` — ADR-FORTEMI-101).
4. The companion root is a managed directory; Fortemi has full read/write/delete authority within it.
5. Per-archive subdirectories are created lazily on first derived-artifact write.

## Consequences

### Positive

- **Preserves the read-only invariant on user source directories.** Fortemi never writes anywhere under `source_path`. The trait-level enforcement (`ReferencedBackend::delete = Err`) is reinforced by handler-level routing.
- **Existing extraction pipeline works unchanged.** `store_derived_attachment_tx` already expects a filesystem-backed write target; the only change is parameterizing the root. No new write code path.
- **Operator-tunable storage layout.** `FORTEMI_DERIVED_STORAGE_PATH` lets the operator put derived artifacts on a separate volume (e.g., faster SSD for thumbnail latency, or cheaper bulk storage when derived data is small per file). Default keeps it co-located with Managed for simplicity in single-volume deployments.
- **Clean drop semantics.** Dropping a Referenced archive removes its derived artifacts (the operator's reasonable expectation) but leaves the source untouched. The per-archive subdirectory layout makes this an `rm -rf` against one directory rather than per-blob deletion.
- **Database stays focused on metadata.** PG holds blob rows, chunks, embeddings — small-record OLTP work. Bulk binary content stays on filesystem volumes, which is what PG and the underlying disk are optimized for.

### Negative

- **Two filesystem roots for the operator to reason about.** Managed deployments have one storage volume; Referenced deployments effectively have two (Managed blob store + derived companion store), plus the user-mounted source directory. Operator documentation (WS-10) must clearly enumerate all three and their backup/sizing implications. Synthesis §5 R-8 flags derived disk usage as a moderate risk to track.
- **Per-archive subdirectory cleanup is best-effort.** If the `rm -rf` after schema drop fails (permissions, filesystem error), an orphan derived-storage directory may remain. The integrity sweep job should detect and report these. Worst case is wasted disk; correctness is unaffected.
- **Inconsistency in `storage_backend` column within a single archive.** A Referenced archive will have some blob rows with `storage_backend='referenced'` (source) and others with `storage_backend='filesystem'` (derived). This requires clear documentation for operators reasoning about the schema. The ADR-FORTEMI-100 model partly addresses this by establishing archive-level mode as the user-facing concept; per-blob mode is implementation detail.

### Neutral

- A deployment migration to a separate derived volume is straightforward: stop Fortemi, `mv {FILE_STORAGE_PATH}/derived /new/volume/derived`, set `FORTEMI_DERIVED_STORAGE_PATH=/new/volume/derived`, restart. No data migration code required.
- The default of `{FILE_STORAGE_PATH}/derived/` means deployments that don't care about the distinction need no new env-var configuration; behavior is transparent.

## Alternatives Considered

### Alternative B: Inline derived artifacts in PostgreSQL

Store derived artifact bytes in a per-archive BLOB column (`bytea` or `pg_largeobject`). **Rejected** because:
- Video/audio archives push tens of GB into PG, cannibalizing `shared_buffers` and bloating WAL/replication.
- Complicates backup/restore — `pg_dump` of a 50GB-of-derived-artifacts database is operationally awkward.
- The existing extraction pipeline expects filesystem-backed derived artifacts; switching to inline storage requires new code paths for read/write/stream.
- Defensible only if "simplify deployment by eliminating a second filesystem root" is the operator's overriding constraint. Stated in synthesis §3 Decision 3 operator alternative.

### Alternative C: Sidecar in source directory

Write derived artifacts to `{source_path}/.fortemi-derived/`. **Rejected** because:
- Stream A §8.1 names this as a critical anti-pattern.
- Breaks the read-only invariant on the user's source directory.
- Fails immediately on read-only mounts (some compliance and immutable-infra deployments).
- Surfaces as untracked-files noise in git-tracked source directories.
- Violates ADR-FORTEMI-101's trait-level read-only guarantee (would require `ReferencedBackend` to allow writes, undermining defense-in-depth).

## References

- Synthesis §3 Decision 3 — derived artifact placement; option-tree and rationale
- Synthesis §1.2 — consensus that source files must never be written to
- Synthesis §5 R-8 — derived disk usage risk + mitigation
- Synthesis §2.3 constraint 6 — `drop_archive_schema` is already safe for Referenced
- ADR-FORTEMI-100 — archive-level storage mode (with this exception for derived artifacts)
- ADR-FORTEMI-101 — `ReferencedBackend` trait impl (the read-only invariant this ADR upholds)
- Stream A §8.1 — sidecar-in-source as critical anti-pattern
- `.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` §4.5 — WS-6 component design

# ADR-FORTEMI-101: Extend `StorageBackend` Trait with `ReferencedBackend` Variant (No Breaking Change)

**Status**: Proposed
**Date**: 2026-05-21
**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (commissioned by Phase 3 SDLC corpus generation)
**Source**: Synthesis §3 Decision 2, §2.2 Disagreement 4, §2.3 Constraint 1

## Context

Fortemi's existing `StorageBackend` trait (`crates/matric-db/src/file_storage.rs` lines 60-80) abstracts over different storage implementations. The trait is consumed by `PgFileStorageRepository`, every adapter in `crates/matric-jobs/src/adapters/`, and every API streaming download handler in `main.rs`. Any breaking change to the trait surface touches every consumer site — a high blast-radius refactor.

The trait already includes an **optional** `resolve_path(&self, path: &str) -> Option<PathBuf>` method that backends override when they can resolve a logical storage path to an absolute on-disk path. `FilesystemBackend` implements it; non-filesystem backends (a hypothetical S3 impl) return `None`. The synthesis Stream C survey (§2.3 constraint 1) confirms this was a deliberate design choice — the trait was originally shaped to accommodate "backends that have an on-disk path." That is precisely the Referenced primitive: source files have an absolute on-disk path; that path is the storage identity.

Three implementation options were considered:

- **A: Additive variant + new impl.** Add `FileSource::Referenced(PathBuf)` enum variant. Implement a new `ReferencedBackend: StorageBackend` struct whose `write`/`delete` are no-ops (or `Err(NotSupported)`) and whose `read`/`exists`/`resolve_path` work against the absolute user path stored in the blob row. Existing trait surface unchanged.
- **B: Sibling trait + split.** Introduce a `ReadableStorage` trait and split `StorageBackend` into `ReadableStorage + WritableStorage`. Compile-time guarantees that read-only paths cannot accidentally write. Every consumer site needs re-typing.
- **C: Mode-wrapping enum.** Wrap existing backends in a `StorageMode` enum that gates write methods. Adds indirection without compile-time guarantees beyond what option A provides.

## Decision

**Adopt Option A: extend the existing `StorageBackend` trait with a `ReferencedBackend` implementation.** No breaking change to the trait. The `FileSource` enum gains a `Referenced(PathBuf)` variant. `ReferencedBackend` is a new struct that implements `StorageBackend` with:

- `read(&self, path: &str)` — opens the file at the absolute path stored in `path`, reads bytes, returns them. May `Err` on `NotFound` (file deleted since indexing) or `PermissionDenied`.
- `exists(&self, path: &str)` — returns `Ok(true)` if the file is reachable, `Ok(false)` if not. Never errors on unreachability — used for liveness checks.
- `resolve_path(&self, path: &str)` — returns `Some(PathBuf::from(path))`. This is the load-bearing override that enables the existing extraction-handler path-access code path to work for Referenced sources without modification.
- `write(&self, _path: &str, _data: &[u8])` — returns `Err(Error::ReadOnlyBackend)`. No-op writes are forbidden; this is defense-in-depth complementing the API-level write-gate.
- `delete(&self, _path: &str)` — returns `Err(Error::ReadOnlyBackend)` for the same reason.

A new streaming-hash helper `compute_content_hash_stream(path: &Path) -> Result<String>` is added alongside the existing `compute_content_hash` (line 317 area). The streaming variant is required because the existing function reads the whole file into memory — fine for Managed blobs already streamed via the upload path, prohibitive for Referenced source files that may be hundreds of MB.

`PgFileStorageRepository` extends its backend-dispatch logic to recognize `storage_backend='referenced'` and route to `ReferencedBackend`. Existing `'filesystem'` dispatch is unchanged.

## Consequences

### Positive

- **Zero breaking changes to the trait surface.** Every existing consumer site (`PgFileStorageRepository`, all adapters in `crates/matric-jobs/src/adapters/`, streaming download handlers) continues to work without modification. The new variant is purely additive.
- **Minimum-viable code change.** Implementing the trait for a read-only backend is the smallest possible code change that exposes the new capability. The trait was already shaped for this case (synthesis §2.3 constraint 1) — the trait designer's foresight is paying off.
- **Defense-in-depth.** The `write`/`delete` returning `Err(ReadOnlyBackend)` is a second layer beneath the API-level write-gate. If a future code path neglects to check archive `storage_mode` before attempting a write, the trait still refuses. (This complements but does not replace the API-level enforcement.)
- **Symmetric with existing video/audio path-access mechanism.** The extraction handler's existing `_source_path` code path (used today for video/audio adapters) works identically for Referenced source files because `resolve_path()` returns the literal absolute path. No new code path needed in extraction (synthesis §2.3 constraint 2 — extending the gate at line 146 to include `storage_backend='referenced'` is a one-line change).

### Negative

- **Runtime mode dispatch, not compile-time.** A future code path could in principle call `referenced_backend.write(...)` and get a runtime `Err` rather than a compile-time refusal. Stream A noted (§1.3) that compile-time guarantees via trait split (Option B) catch a real class of bug. Option A accepts that this class of bug must be caught by code review and tests rather than the type system.
- **`FileSource` enum is now public-API-shaped.** Adding the `Referenced(PathBuf)` variant changes pattern-match exhaustiveness for any caller that destructures `FileSource`. Existing exhaustive matches in the codebase will need a new arm.
- **Streaming hash is a second implementation of BLAKE3.** Until the existing `compute_content_hash` is refactored to also stream (a sensible follow-on, but out of scope for this epic), there are two functions doing similar work. Documentation must note that the streaming variant is preferred for any file >10MB.

### Neutral

- The `Err(NotSupported)` variant for `ReferencedBackend::delete` needs to be handled gracefully in `drop_archive_schema` (synthesis §2.3 constraint 6 already confirms `drop_archive_schema` is safe for Referenced — it never calls `backend.delete()` for blob files, only the orphan-deletion path which is gated on `storage_backend='filesystem'`). This invariant must be preserved in any future refactoring of the drop path.

## Alternatives Considered

### Alternative B: Sibling-trait split (`ReadableStorage` + `WritableStorage`)

Refactor `StorageBackend` into `ReadableStorage + WritableStorage`. Backends declare which they implement; the compiler refuses any attempt to call write methods on a read-only backend. **Rejected for v1** because:
- High refactoring blast radius: every consumer site needs re-typing (handler signatures, repository methods, etc.).
- Forces a design choice between using `dyn` (preserving runtime polymorphism but losing some compile-time benefit) or generics (requiring monomorphization throughout the call graph).
- The bug class it catches (accidental writes to read-only) is also catchable by runtime `Err` + test coverage, which is sufficient given the API-level write-gate as the primary enforcement.
- Defensible as a future refactor. State this as the v2 path if the runtime-Err approach proves error-prone in practice.

### Alternative C: `StorageMode` enum wrapper

Wrap each backend in a `StorageMode::ReadOnly(Box<dyn StorageBackend>)` / `StorageMode::ReadWrite(Box<dyn StorageBackend>)` enum that gates write methods. **Rejected** because:
- Adds an extra layer of indirection without buying type-system enforcement (the gate is still a runtime check).
- Forces every consumer site to unwrap the mode before calling — verbose with no benefit over option A.

## References

- Synthesis §3 Decision 2 — `ReferencedBackend` trait impl; option-tree and rationale
- Synthesis §2.2 Disagreement 4 — Stream A's two options vs Stream C's source-survey constraint
- Synthesis §2.3 constraint 1 — `resolve_path()` is the existing optional method that maps onto Referenced
- Synthesis §2.3 constraint 2 — extraction_handler line 146 path-access gate extension
- Synthesis §2.3 constraint 6 — drop_archive_schema already safe for Referenced
- ADR-FORTEMI-100 — archive-level storage mode (drives the dispatch decision)
- ADR-FORTEMI-102 — derived artifacts use `FilesystemBackend` even for Referenced archives (the mixed-mode case)
- Source file: `crates/matric-db/src/file_storage.rs` lines 40-220 — trait definition and `FilesystemBackend` impl
- `.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` §4.1 — WS-1 component design

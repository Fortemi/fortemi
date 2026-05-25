# ADR-FORTEMI-100: Storage Mode Is an Archive-Level Property, Not Per-Blob

**Status**: Proposed
**Date**: 2026-05-21
**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (commissioned by Phase 3 SDLC corpus generation)
**Source**: Synthesis §3 Decision 1, §2.2 Disagreement 1

## Context

The #736 epic introduces a second storage mode — Referenced — where Fortemi indexes user-owned source files in place rather than copying them into the Managed BLAKE3 blob store. A core design question is the **granularity** of that mode declaration:

- **Per-blob**: Each blob row carries `storage_backend ∈ {filesystem, referenced, ...}`. An archive may freely mix Managed and Referenced blobs.
- **Per-archive**: The archive itself declares its mode; all source blobs in an archive share that mode (with a narrow internal exception for derived artifacts — see ADR-FORTEMI-102).

Per-blob granularity is technically possible — the existing blob schema already carries `storage_backend` per row, so the database could accommodate it without change. The question is whether to *expose* per-blob mode to users, and whether to treat archives as homogeneous in mode for the purposes of routing, UX, and tooling.

Stream C's source survey (synthesis §1.2, §1.4) found that every existing Fortemi UX touchpoint — archive list, archive create, `archive_routing_middleware`, MCP `manage_archives` — already thinks in terms of archives, not blobs. The middleware loads archive metadata once per request and propagates a single `ArchiveContext`; there is no machinery for per-blob mode decisions in the request path. Conversely, the one place where per-blob mode is genuinely needed — derived artifacts within a Referenced archive must be Managed (a thumbnail of a video is owned by Fortemi, not the user) — is a system implementation detail, not a user-facing concept.

## Decision

**Storage mode is an archive-level property in Fortemi's user model and API surface.** The `archive_registry` table gains a `storage_mode ∈ {'managed', 'referenced'}` column. All source blobs within a Referenced archive use `storage_backend='referenced'`. The single internal exception is derived artifacts (thumbnails, transcripts, embeddings) for Referenced archives: those blobs use `storage_backend='filesystem'` rooted at the new companion location (ADR-FORTEMI-102).

Concretely:

1. The user-facing concept exposed via API and MCP is "Referenced archive" or "Managed archive" — never "mixed-mode archive."
2. `archive_registry.storage_mode` is the source of truth for routing decisions in `ArchiveContext` middleware and the write-gate.
3. The per-blob `storage_backend` column is retained for implementation flexibility (lets derived artifacts use a different backend than source) but is not surfaced to users.
4. Users wanting "some Referenced source code plus some inline notes" must create two archives.
5. `archive_registry.storage_mode` is immutable after archive creation. To change a Managed archive to Referenced (or vice versa), the operator drops and recreates the archive.

## Consequences

### Positive

- **UX consistency.** Every existing user touchpoint already thinks in archives; archive-level mode aligns with the mental model the codebase already enforces.
- **No combinatorial UX explosion.** Avoids "what does it mean to convert a Managed archive to Referenced halfway through?" — the answer is "you can't; create a new one."
- **Single middleware read.** `ArchiveContext` reads `storage_mode` once per request and propagates; no per-blob dispatch decisions in the hot path.
- **Cleaner write-gate.** Mutating routes are forbidden for the whole archive when `storage_mode='referenced'`, not per-blob — a simpler invariant to enforce and verify.
- **Migration is uncontroversial.** Default `storage_mode='managed'` makes the schema change a no-op for all existing archives.
- **Implementation flexibility preserved.** The per-blob `storage_backend` column remains, accommodating the one legitimate mixed case (derived artifacts) without leaking it to users.

### Negative

- **Users wanting hybrid archives must create two.** A user with a referenced git repo and a few inline notes will have two archives instead of one. The synthesis notes (§3 Decision 1 operator alternative) this is a deliberate trade-off — solving it would introduce per-blob mode as a first-class user concept.
- **No incremental "convert this Managed archive to Referenced" workflow.** The drop-and-recreate path is the only migration. For a 10k-file Managed archive whose owner now wants to switch to indexing the source from a directory, this is more friction than an in-place conversion.
- **Source-path mutability is also forbidden** by extension. To "move" a Referenced archive to a new source path, drop and recreate.

### Neutral

- Per-blob storage flexibility for derived artifacts means the database column `storage_backend` is **not** redundant with `archive_registry.storage_mode`. Both exist; both serve distinct purposes. Documentation must be clear that operators reasoning about an archive's character should look at `archive_registry.storage_mode`, while implementation code routing a specific blob looks at `blob.storage_backend`.

## Alternatives Considered

### Alternative A: Per-blob mode as user-facing concept

Each blob carries its own mode, mixed-mode archives are first-class, and `manage_archives` exposes per-blob mode declarations. **Rejected** because:
- Doubles UX surface area (users must reason about archive-level intent *and* per-blob mode).
- No mechanism in `ArchiveContext` middleware for per-blob routing — every handler would need to re-load blob metadata to make routing decisions.
- The legitimate motivating case (Referenced code + inline notes) is solvable with two archives at near-zero additional cost.

### Alternative B: Per-blob mode internally only (current proposal)

Per-blob mode exists in the schema but archive-level mode is what users see. **Selected.**

### Alternative C: Single column on archive, no per-blob distinction at all

Drop the `blob.storage_backend` column; derive backend choice from archive mode alone. **Rejected** because:
- Forces derived artifacts for Referenced archives to use a contorted mechanism (e.g., a separate "managed sidecar archive" per Referenced archive) — adds a lot of indirection.
- Loses future flexibility (e.g., if v3 adds inline-DB storage for very-small blobs alongside filesystem for large blobs, per-blob discrimination is the natural shape).

## References

- Synthesis §3 Decision 1 — archive-level storage mode declaration; option-tree and rationale
- Synthesis §2.2 Disagreement 1 — reconciliation between Stream A's initial per-blob framing and Stream C's archive-level constraint
- Synthesis §1.2 — Stream C source survey: Fortemi UX touchpoints already think in archives
- ADR-FORTEMI-101 — `ReferencedBackend` storage trait implementation (which the per-blob `storage_backend` column dispatches to)
- ADR-FORTEMI-102 — derived artifact placement (the legitimate per-blob exception)
- ADR-090 (existing) — per-archive PostgreSQL schemas; this ADR extends the archive-level isolation model to storage mode
- `.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` §4.2, §6 — schema changes and data model

# Business Case Sketch: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Inception
**Date**: 2026-05-21
**Source**: @.aiwg/working/issue-planner-storage/synthesis.md

## Value Proposition

- **Zero-duplication code search for users with local source trees.** Operators who already maintain source code on local disks, NAS, or external drives can give a Fortemi-backed AI agent semantic access without doubling their storage footprint or maintaining a copy that drifts from the editor's source of truth.
- **Architectural reuse with minimal surface expansion.** The existing `StorageBackend` trait's `resolve_path()` method, the existing `extraction_handler.rs` path-access code path used by video/audio, and the existing per-archive PG schema isolation all extend cleanly to support the new mode (synthesis §2.3). The work is largely additive: one new `ReferencedBackend` impl, one schema migration adding columns to `archive_registry`, one new `DirectoryScanHandler`, two new API endpoints, one new MCP tool. No breaking changes to managed mode.
- **Unblocks AI-agent workflows over real codebases.** A Fortemi-backed MCP client can now perform semantic search across a developer's actual working tree — the code they edit every day — without an indexing pipeline they have to operate themselves.

## Cost Categories

Not numbers — categories of cost only (per `no-time-estimates` rule):

- **Development effort** across 8 workstreams (synthesis §4): WS-1 trait extension, WS-2 schema migration, WS-3 walker/ignore/secret-scan, WS-4 scan-and-ingest job pipeline, WS-6 derived artifact companion location, WS-7 API surface, WS-8 MCP tool surface, WS-9 multi-tenant security tests, WS-10 documentation. Most workstreams are independent and can be parallelized once WS-1 and WS-2 land.
- **Ongoing maintenance**: a new code path through the trait dispatch (runtime mode discriminant), a new background job type (`DirectoryScan`), a new env var (`FORTEMI_REFERENCED_STORAGE_ROOTS`, `FORTEMI_DERIVED_STORAGE_PATH`), and a new operator-facing concept (Referenced vs Managed mode) that documentation must keep current.
- **Support burden**: operators will ask about (a) why their initial scan is slower than expected on monorepos, (b) what the secret-scan quarantined and why, (c) what happens when their NFS mount drops mid-query (Decision 8 lenient-read behavior is documented but counterintuitive), (d) why their changes don't appear in search until they call `POST /rescan` (Decision 4 eventual-consistency model). All four are addressed in WS-10 operator docs.
- **Opportunity cost**: development effort committed to this feature is not available for the parallel improvements that would benefit managed mode (tree-sitter activation, embedding-set optimization, etc.). The synthesis explicitly defers tree-sitter and live watching to keep #736 scope tight.

## Alternatives Considered

- **Do nothing.** Users who want code search continue copying their code into managed archives, doubling storage and maintaining a stale copy. Operator dissatisfaction with this status quo is the originating motivation for #736.
- **Use an external solution.** Sourcegraph, Cody, and similar tools provide code-indexing as a primary feature but require operating a second system alongside Fortemi, splitting the agent's tool surface across two APIs, and operating two indexes. The integration cost typically exceeds the cost of building this in.
- **Build differently — sibling trait split.** Synthesis Decision 2 considered splitting `StorageBackend` into `ReadableStorage + WritableStorage` to get compile-time guarantees that Referenced backends cannot write. Rejected for v1 because of invasive blast radius (every consumer site needs re-typing). The trait-extension path is defensible to refactor later if compile-time guarantees become a priority.
- **Build differently — per-blob storage mode as user concept.** Synthesis Decision 1 considered exposing mixed-mode archives (some blobs Referenced, some Managed) to users. Rejected for v1 because it doubles UX surface area without solving a real user problem; users wanting both should create two archives.

## Decision Criteria — What Makes This Worth Doing

- The architectural pattern is established (synthesis §2.1, cross-stream consensus). This is not a speculative bet.
- The Fortemi-specific constraints have been surveyed (synthesis §2.3) and the chosen approach respects all of them. The work is additive; no breaking change to managed mode; no regression to multi-tenant isolation; no regression to ADR-094 fail-closed auth.
- The risks are known and have explicit mitigations (synthesis §5, risk register v1). The two highest-severity items (R-1 secret leakage, R-2 multi-tenant breach) have established mitigations that can be tested in WS-9.
- The deferred items (live watching WS-5, tree-sitter, remote storage) are scoped out cleanly. v1 ships with a coherent, defensible operator model; the deferred work has clean follow-on entry points.
- The operator's stated need (synthesis §1) maps cleanly to deliverable success criteria (vision.md success criteria 1-6). Each criterion is measurable.

This is worth doing if the operator confirms (a) the deferred-live-watching trade-off at Phase 5 Q-1, and (b) the secret-scan mandatory-on default at Phase 5 Q-3. Both are documented as operator approval gates in synthesis §6.

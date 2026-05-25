# Problem Statement: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Inception
**Date**: 2026-05-21
**Source**: @.aiwg/working/issue-planner-storage/synthesis.md

## Problem

Users who keep code on local disks, NAS mounts, or external drives currently have no way to give a Fortemi-backed AI agent semantic search over that code without first copying every byte into Fortemi's managed blob store. Copying duplicates storage, breaks the "single source of truth" invariant operators expect (the code stays where they put it), and decouples the index from filesystem reality — when the user edits a file in their editor, the agent is still searching a stale copy. The operator's stated need is: "point Fortemi at a directory, have it indexed in place, search it via the existing API and MCP surface, and never have Fortemi own the bytes."

## Why Now

Three things have changed in 2025-2026 that make this timely rather than premature (per synthesis §1, §2.1):

- The two-mode architectural pattern (Managed vs Referenced storage) has reached cross-industry consensus — Sourcegraph, Cody, Plex, Lightroom, and every comparable production tool draws the same line. This is no longer a novel architectural bet; it is the established design.
- Fortemi's existing `StorageBackend` trait already exposes a `resolve_path()` escape hatch that maps cleanly onto Referenced semantics (Stream C source survey, synthesis §2.3 constraint 1). The trait was designed in anticipation of this case; the work is finishing what was started, not reinventing.
- Code-indexing tools that omitted secret-scanning at ingest have produced documented incidents (synthesis §2.2 disagreement 3, Stream B §5.1). The required mitigations are well-understood; defaults can be set conservatively without operator overload.

## Stakeholders

| Role | What they get | What they care about |
|------|---------------|----------------------|
| Single-user / desktop operator | Semantic search over a local code directory without copying it into Fortemi's blob store | Zero data duplication; fast initial ingest; correct semantic search results |
| Multi-tenant deployment operator | Per-tenant Referenced archives with enforced source-path allowlist; fail-closed auth preserved | Tenant isolation cannot regress; no cross-tenant boundary breach via crafted source_path |
| AI agent (Claude Code, MCP client) | Same `search`, `manage_archives`, attachment APIs; one new `rescan_archive` tool | Backward compatibility — existing tool invocations keep working unchanged |
| Fortemi maintainer | Additive trait extension, no breaking changes to managed mode, schema migration that defaults all existing archives to `storage_mode='managed'` | Architectural invariants preserved (per-archive PG schema isolation, ADR-094 fail-closed auth); test suite still passes |
| Security reviewer | Pre-ingest secret-scan with quarantine logging; path canonicalization and allowlist enforcement | Secrets never enter pgvector embeddings; multi-tenant boundary tests in WS-9 cover all attack vectors |

## Constraints

These are non-negotiable for v1 (synthesis §2.3):

- **No breaking changes to managed-mode behavior.** Existing archives migrate cleanly with `storage_mode='managed'` default; all current handlers, middleware, and MCP tools keep working.
- **Multi-tenant isolation cannot regress.** ADR-090-style per-archive PG schema isolation with `SET LOCAL search_path` is preserved. Referenced archives inherit the same isolation model.
- **Fail-closed authentication (ADR-094) preserved.** Referenced archive creation still requires Bearer token on `/api/v1/*`; multi-tenant deployments still refuse to start with `REQUIRE_AUTH=false`.
- **Fortemi never writes to user-owned directories.** The trait's `write`/`delete` methods become explicit no-ops in the `ReferencedBackend` impl. Derived artifacts (thumbnails, transcripts, embeddings) land in a managed companion location per archive (synthesis Decision 3).
- **Source files never deleted by Fortemi.** `drop_archive_schema()` already gates orphan-deletion on `storage_backend='filesystem'`; Referenced archives drop cleanly without touching source.

## Out of Scope (Non-Goals)

Pulled verbatim from synthesis §7. v1 explicitly does NOT include:

1. Remote storage backends (S3, GCS, Azure Blob, MinIO, HTTP) — separate epic, different consistency model
2. Tree-sitter activation for code parsing — `CodeAstAdapter` continues using regex; tree-sitter is a parallel issue
3. Live filesystem watching (`notify-rs` + polling) — deferred to WS-5 backlog; v1 is on-demand `POST /rescan` only
4. Cross-archive overlap detection — overlap allowed with warning (synthesis Q-6)
5. Rename detection via content-hash correlation — deferred to v2 (couples to live watching)
6. GUI for archive management beyond API/MCP — CLI and API are the v1 surface
7. AI-assisted ignore-list generation — defaults are static (synthesis Decision 7)
8. Cross-tenant federated search over Referenced archives — out of scope
9. Source-path migration after archive creation — drop and recreate is the supported path
10. Source-file write-back from Fortemi — agents edit code via their own filesystem tools, bypassing Fortemi entirely

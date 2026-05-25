# Synthesis: Referenced Storage Mode + Scan-and-Ingest for Code Archives

**Issue**: fortemi/fortemi#736 — Allow users to point at a local directory or mount as the storage backend for an archive, with on-add scan-and-ingest for code indexing.

**Phase**: Phase 2 — Research Synthesis (issue-planner workflow)
**Date**: 2026-05-21
**Inputs**: Streams A (best practices), B (current state 2024-2026), C (Fortemi source + vendor docs)

This synthesis is the sole input to Phase 3 (SDLC corpus generation). It picks a side on every architectural fork, names the load-bearing decisions, decomposes the epic into workstreams, and surfaces the open questions that require operator approval at Phase 5.

---

## 1. Executive Summary

Users with code on local disks, NAS mounts, or external drives currently have no good way to give a Fortemi-backed AI agent semantic access to that code without copying every byte into Fortemi's managed blob store. The proposed feature adds a second storage mode — **Referenced** — alongside the existing Managed mode, in which Fortemi indexes the user-owned source files in place: BLAKE3-hashes them, chunks them via the existing tree-sitter/regex extraction pipeline, embeds the chunks via the existing pgvector pipeline, and exposes the resulting archive over the existing search API and MCP tool surface. The source files are never copied, never written to, and never deleted — Fortemi owns the index, not the data (C§1.1, A§1.1).

The architecture is **additive, not a redesign**. The existing `StorageBackend` trait already exposes a `resolve_path()` escape hatch that maps cleanly onto Referenced semantics; storage mode becomes an archive-level property recorded in a new `archive_registry.storage_mode` column; the existing extraction pipeline already has a "path-access" code path used by video/audio that Referenced storage extends to all file types; derived artifacts (thumbnails, transcripts, embeddings) land in a companion managed location per archive so Fortemi never writes to user-owned directories (C§1.1, C§1.4, A§1.2).

The risk hot-spots are three: **secret leakage** from auto-indexing `.env`/`.pem`/`.ssh/` files (mitigated by default denylist + pre-ingest content-based secret scanning); **multi-tenant boundary violations** if path validation is sloppy (mitigated by canonicalization + allowlist enforcement at archive-create time); and **filesystem-event reliability** on Docker bind mounts and NFS (mitigated by shipping v1 with on-demand rescan only and deferring `notify-rs` live watching to a follow-up workstream) (B§5.1, B§5.3, C§2.2).

Scope decomposes into 8 workstreams ranging from schema/trait extension (WS-1, WS-2) through scan-and-ingest pipeline (WS-3, WS-4), API and MCP exposure (WS-7, WS-8), security validation (WS-9), and documentation (WS-10). Live update detection is a separate, deferred workstream (WS-5) and explicit non-goal for v1.

---

## 2. Reconciled Findings

This section names the places the three streams converged, the places they disagreed, and the places the Fortemi source survey (Stream C) constrains design choices that the abstract industry patterns (Stream A) treat as open.

### 2.1 Consensus across all three streams

| Finding | Streams | Notes |
|---|---|---|
| Two-mode pattern (Managed vs Referenced) is the right primary architecture | A§1.1, B§1.7, C§1.1 | Universal: every production tool (Sourcegraph, Cody, Plex, Lightroom, Fortemi-the-codebase) draws this line |
| Source files must never be written to or deleted by the indexer | A§6.3, A§8.1, B§5.3, C§1.2 | Defense-in-depth via trait design (Referenced backend's `delete`/`write` are no-ops or errors) |
| Derived artifacts must live in indexer-managed location, not alongside source | A§1.2, B§5.1, C§1.4 | Anti-pattern explicitly named in Stream A; Stream C identifies the existing `store_derived_attachment_tx` as the integration point |
| Content addressing (BLAKE3) + path identity (absolute path) is the right hybrid | A§3.3, C§1.1, C§2.6 | BLAKE3 already in Fortemi (`compute_content_hash` line 317); streaming variant needed for large files |
| Tree-sitter chunking is the right primitive for code search | A§5.1, B§3.2, C§1.4 | Existing `CodeAstAdapter` uses regex; tree-sitter feature flag wired but unused; v1 ships with regex, tree-sitter is follow-on |
| Ignore-respecting walker via Rust `ignore` crate (BurntSushi) | A§2.1, B§5.2, C§2.1 | All three streams independently land on the same library; needs to be added as workspace dep |
| Default denylist for secrets is mandatory, not optional | A§4.1, B§5.1, C-implicit | Stream B documents real incidents; Stream A documents the patterns; Stream C confirms no existing Fortemi gate for this |
| Per-archive PostgreSQL schema isolation must be preserved | A§4.2, B§5.3, C§1.2 | Fortemi's ADR-090-style model (per-archive schemas + `SET LOCAL search_path`) is stronger than any community MCP server; the directory-archive feature must not weaken this |

### 2.2 Disagreements and how they're resolved

**Disagreement 1: Per-archive vs per-blob storage mode.**
Stream A initially framed it as a per-blob decision via a `StorageMode` enum that could vary across blobs. Stream C's source survey makes clear that **storage mode is naturally an archive-level property** in Fortemi's domain — every existing handler, middleware, and UX touchpoint thinks in terms of "this archive's storage" not "this blob's storage." Stream C also identifies the one place where per-blob storage IS needed: **derived artifacts within a Referenced archive** must use Managed storage (a thumbnail of a video isn't owned by the user).

**Resolution: archive-level mode declaration, per-blob backend dispatch.** The user-facing concept is "Referenced archive" or "Managed archive." Inside a Referenced archive, ~98% of blobs are `storage_backend='referenced'` and the remaining derived artifacts are `storage_backend='filesystem'` under a companion managed path (`{FILE_STORAGE_PATH}/derived/{archive_id}/`). This is "mixed-mode at the blob layer, single-mode at the archive layer." (Reconciles A§1.1 + A§1.2 with C§1.4.)

**Disagreement 2: Live update mechanism.**
Stream A leans toward a notify-rs + polling hybrid. Stream B observes that several community tools defer live watching entirely. Stream C documents Fortemi-specific blockers: Docker bind mounts on `overlay2` silently drop host-side inotify events, Linux's default `max_user_watches=8192` is laughably small for code repos, and the entire feature would require a new long-running watcher process that doesn't exist today.

**Resolution: v1 ships with explicit reindex API only.** Defer notify-rs hybrid to a follow-up workstream (WS-5, marked deferred). The operator's documented expectation should be eventual-consistency-on-demand, not real-time. This conflicts with the user-facing description in #736 ("automatically scan-and-ingest"); we interpret "automatic" in v1 as "automatically triggered by the archive-create action and re-triggerable by an API call," not "automatically triggered by filesystem events." If the operator wants real-time at v1, that's the most important Phase 5 question. (Reconciles A§2.2 with B§5.4 and C§2.2.)

**Disagreement 3: Secret detection — pre-ingest only vs continuous.**
Stream A treats pre-ingest secret detection as one defense layer among several. Stream B observes real incidents where embeddings persisted secrets even after source rotation. Stream C identifies that Fortemi has no existing secret-scanning hook.

**Resolution: pre-ingest content-based detection + re-ingest on hash change.** The scan-and-ingest pipeline runs a secret-pattern check (PEM private-key headers, gitleaks-style regexes for AWS/GitHub/JWT tokens) on each file BEFORE chunking and embedding. Files matching are skipped and logged with a quarantine record visible via API. There is no "redact embeddings" attempt; if a source file changes, the content_hash changes, and the new chunks fully replace the old (re-ingest, not patch). A `POST /rescan?full=true` provides operator-triggered full re-validation. (Reconciles A§4.1 with B§5.1 and the lack of any existing gate.)

**Disagreement 4: Storage abstraction — extend trait vs sibling trait.**
Stream A presents both patterns (single-trait with mode flag, two-trait split with compile-time guarantees) as valid. Stream C surveys the existing `StorageBackend` trait and finds `resolve_path()` already optional — the trait was designed to accommodate backends that "have an on-disk path." This is exactly the Referenced primitive.

**Resolution: extend the existing `StorageBackend` trait with a `ReferencedBackend` implementation.** No breaking change. The trait's `write`/`delete` methods become explicit no-ops (or `Err(NotSupported)`) in the Referenced impl; `read`/`exists`/`resolve_path` work unchanged. This is the lowest-risk path — every existing call site continues to work, and the type system is allowed to surface "wrong mode" failures at runtime rather than at compile time (consistent with Quickwit's approach noted in A§1.3). The two-trait split is a defensible future refactor but is not blocking. (Reconciles A§1.3 with C§1.1.)

### 2.3 Constraints surfaced by Stream C that are now non-negotiable

These are facts about the Fortemi source that bound the design space:

1. **The trait surface is fixed.** `StorageBackend` cannot get a breaking change without touching every consumer (`PgFileStorageRepository`, every adapter in `crates/matric-jobs/src/adapters/`, every API streaming download handler in `main.rs`). Stream C confirms `resolve_path()` is the existing optional method that maps onto Referenced semantics.
2. **`extraction_handler.rs` line 146 already has "path access" mode.** Video and audio adapters bypass in-memory download via `config._source_path`. The Referenced scan-and-ingest pipeline must ride this same code path — extending the gate from `strategy_supports_path_access(strategy)` to `strategy_supports_path_access(strategy) || storage_backend == "referenced"`.
3. **The MCP server is Node.js, not Rust.** The MCP tool additions go in `mcp-server/`, not the Rust workspace. New tools become entries in the core tool set (43 today).
4. **`CodeAstAdapter` is regex-based today.** Tree-sitter is a workspace dep behind a feature flag but the existing adapter doesn't use it. v1 of #736 must work with regex extraction. Tree-sitter is a separate parallel improvement, not a #736 blocker.
5. **The `archive_registry` table is in `public` schema** (it's in the `SHARED_TABLES` deny list per `archives.rs:101`). Adding columns there is uncontroversial and not subject to per-archive migration.
6. **`drop_archive_schema()` is already safe for Referenced.** It drops the PG schema CASCADE but never calls `backend.delete()` for blob files. Referenced archives drop cleanly because the orphan-deletion code path is gated on `storage_backend='filesystem'`.

---

## 3. Recommended Architecture — Load-Bearing Decisions

These are the 8 architecturally significant decisions this epic locks in. Each is presented as: decision, options considered, recommended choice, rationale, what an ADR would later codify, and the alternative the operator could choose at Phase 5.

### Decision 1: Archive-level storage mode (not per-blob)

**Options considered**:
- A: Mode is a per-blob property; archives can mix freely
- B: Mode is an archive-level property; all source blobs in an archive share mode

**Recommended**: B (archive-level).

**Rationale**: Every existing Fortemi UX touchpoint (archive list, archive create, archive context middleware, MCP `manage_archives`) thinks in terms of archives, not blobs. Per-blob mode adds combinatorial complexity (mixed archives, partial-mode archives, mode-migration mid-archive) without solving any real user problem. The one legitimate per-blob need — derived artifacts in a Referenced archive must be Managed — is handled by a single internal exception, not by exposing per-blob mode to users (C§1.2, C§1.4).

**ADR scope**: ADR-091 — Archive-level storage mode declaration. Notes that the schema records `storage_backend` per blob for implementation flexibility, but the user model is per-archive.

**Operator alternative**: If the operator wants per-blob mode as a user-facing concept (e.g., "I want to reference my git repo but also paste in some inline notes"), v1 should explicitly defer this and the user must create a second Managed archive for the inline content. State this in Phase 5.

### Decision 2: Extend existing `StorageBackend` trait with `ReferencedBackend` impl

**Options considered**:
- A: Add a new `Referenced(PathBuf)` variant to `FileSource` + new `ReferencedBackend: StorageBackend` impl
- B: Introduce a sibling `ReadableStorage` trait and split `StorageBackend` into `ReadableStorage + WritableStorage`
- C: Add a `StorageMode` enum that wraps backends

**Recommended**: A (additive variant + new impl).

**Rationale**: Stream C confirms `StorageBackend::resolve_path()` is already optional and was designed for backends that can resolve to an on-disk path. A `ReferencedBackend` implementation that no-ops `write`/`delete` and returns the literal absolute path from `resolve_path()` is the smallest possible change. Option B is a defensible future refactor but invasive (every consumer site needs re-typing). Option C adds an extra layer of indirection without buying type safety. The `FileSource::Referenced(PathBuf)` variant makes the discriminant explicit in the API surface (C§1.1).

**ADR scope**: ADR-092 — `ReferencedBackend` storage trait implementation. Records the decision to use runtime mode dispatch via enum discriminant rather than compile-time type splits.

**Operator alternative**: If the operator wants compile-time guarantees that read-only paths can never write (which catches a real class of bug), state this as Phase 5 input and v1 changes to Option B. The cost is higher refactoring blast radius.

### Decision 3: Derived artifacts go to managed companion location per archive

**Options considered**:
- A: Companion managed directory: `{FILE_STORAGE_PATH}/derived/{archive_id}/`
- B: Inline-in-database for Referenced archives (avoids any filesystem write outside the original Managed root)
- C: Sidecar in source directory (e.g., `{source_path}/.fortemi-derived/`)

**Recommended**: A (managed companion location).

**Rationale**: Stream A explicitly names option C as a critical anti-pattern (§8.1) — some users will mount read-only volumes, some have CI that fails on dirty working trees. Option B (inline DB) would explode database size for video/audio archives where transcripts and keyframes can be tens of MB per file. Option A preserves the invariant that Fortemi never writes to user-owned paths while keeping derived artifacts file-system-backed (which the existing extraction pipeline expects) (A§1.2, C§1.4).

**ADR scope**: ADR-093 — Derived artifact placement for Referenced archives. Records the directory layout and the per-blob `storage_backend` mix (`referenced` for source, `filesystem` for derived).

**Operator alternative**: If the operator wants all derived artifacts in the DB to simplify deployment (no second filesystem root to manage), B is viable but flag the DB-size trade-off in Phase 5.

### Decision 4: Live update detection — defer to v2; v1 is on-demand rescan only

**Options considered**:
- A: notify-rs + polling hybrid with explicit reindex API (Stream A primary recommendation)
- B: Explicit reindex API only (`POST /rescan`), no filesystem watching
- C: notify-rs only, no polling fallback

**Recommended**: B (explicit reindex only) for v1.

**Rationale**: Stream C documents that Docker bind mounts on `overlay2` (the default storage driver) silently drop host-side inotify events. Linux's default `max_user_watches=8192` is exhausted by any non-trivial code repo. macOS FSEvents can coalesce events under load. Windows ReadDirectoryChangesW has a 64KB event buffer that overflows on `npm install`. notify-rs solves none of these — it surfaces the platform's underlying behavior. The hybrid pattern from Stream A is correct in principle but adds substantial implementation cost (new long-running watcher process, watcher lifecycle management, fallback-polling scheduler) and the failure mode is silent (events dropped, index drifts, user is confused). v1 ships with `POST /rescan` only; users get predictable behavior. Live watching becomes WS-5 in the backlog with its own design RFC (C§2.2, A§2.2, B§5.4).

**ADR scope**: ADR-094 — Update detection model. Records the rationale for deferring live watching and the user-facing semantics ("Referenced archives are eventually consistent on operator action, not on filesystem events").

**Operator alternative**: If the operator wants live watching at v1, this becomes the highest-cost Phase 5 question. The pragmatic compromise is "periodic polling at 60s interval, no inotify" — simpler to reason about, works on all platforms equally, no Docker pitfalls. State this as the v1.5 path.

### Decision 5: Pre-ingest secret detection with quarantine logging

**Options considered**:
- A: Path-based denylist only (cheap, fast, misses content-level secrets)
- B: Pre-ingest content-based scanning (gitleaks-style regex + PEM headers)
- C: A + B + post-ingest re-scan on rotation

**Recommended**: B + path-denylist (combined path + content check at ingest time; no continuous re-scan since file changes already trigger re-ingest via hash mismatch).

**Rationale**: Stream B documents real incidents from code-indexing tools that omitted this. Stream A names it as defense in depth. Path denylist (default: `.env*`, `*.pem`, `*.key`, `id_rsa*`, `.ssh/`, `.gnupg/`, `.aws/credentials`, `.config/`, `.kube/config`) catches the obvious cases at near-zero cost. Content-based regex (PEM `-----BEGIN .* PRIVATE KEY-----`, AWS access key patterns, GitHub personal access tokens, JWT prefix patterns) catches secrets in unexpected files. Continuous re-scan (option C) is unnecessary because file modification changes the content_hash, which triggers re-ingest, which re-runs the secret check. The quarantine log is exposed via `GET /archives/{name}/quarantined-files` so users can audit what was skipped (A§4.1, B§5.1).

**ADR scope**: ADR-095 — Secret detection at ingest. Records the denylist patterns, regex sources, and quarantine semantics.

**Operator alternative**: If the operator wants opt-out (skip-the-check-at-user-risk) for performance on very large repos, expose `scan_config.skip_secret_scan: bool`. State this as Phase 5 question — the v1 default should be enforce-on regardless.

### Decision 6: MCP surface extends existing tools, no new family

**Options considered**:
- A: New `directory_archive_*` family of MCP tools (clean naming, no overload)
- B: Extend existing `manage_archives` tool with `storage_mode` and `source_path` params; add one new `rescan_archive` tool
- C: Mixed — extend `manage_archives` for create/read/delete, new tool only for rescan

**Recommended**: B (extend existing).

**Rationale**: `manage_archives` is one of the 43 core MCP tools (per CLAUDE.md tool list). It already handles archive CRUD. Adding `storage_mode: "managed" | "referenced"` and `source_path: string?` params to its create operation preserves the agent's mental model. The one genuinely new operation — `rescan_archive` — gets its own tool because it's async-job-returning and has different semantics (per C§2.7's MCP tool signature). Option A (new family) doubles the tool surface and forces agents to learn two parallel APIs (C§2.7, B§2.1).

**ADR scope**: ADR-096 — MCP surface for Referenced archives. Records the tool signature changes and the async-job pattern for rescan.

**Operator alternative**: If the operator wants strict separation (some users won't be allowed to create Referenced archives), having a separate `create_referenced_archive` tool makes permission gating easier. State this as Phase 5 question.

### Decision 7: Default ignore list and secret denylist (explicit choices for first ship)

**Decision recorded inline** (not really an option-tree; these are concrete choices):

**Default `.gitignore`-supplementary ignore list** (in addition to honoring `.gitignore` if present):
- `.DS_Store`, `Thumbs.db`, `desktop.ini`
- `node_modules/`, `__pycache__/`, `*.pyc`, `.pytest_cache/`
- `.git/`, `.svn/`, `.hg/`, `.bzr/`
- `dist/`, `build/`, `target/`, `out/`, `.next/`, `.nuxt/`
- `.venv/`, `venv/`, `env/`
- `*.log`, `*.tmp`, `*.cache`
- Files >10MB by default (configurable; rationale: skips minified bundles, generated SQL dumps, image binaries that shouldn't be embedded as code)

**Secret denylist (path-based, hard-stop)**:
- `.env*`, `.envrc`
- `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.jks`
- `id_rsa*`, `id_ed25519*`, `id_ecdsa*`, `*_rsa`, `*_dsa`, `*_ecdsa`
- `.ssh/`, `.gnupg/`, `.aws/credentials`, `.aws/config`
- `.kube/config`, `.docker/config.json`
- `secrets.*`, `*.secret`, `credentials.json`

**Secret denylist (content-based, hard-stop)**:
- `-----BEGIN .* PRIVATE KEY-----` regex (catches PEM-formatted RSA, EC, DSA, OpenSSH, PGP)
- AWS access key pattern: `(AKIA|ASIA)[0-9A-Z]{16}`
- GitHub PAT pattern: `ghp_[a-zA-Z0-9]{36}`, `github_pat_[a-zA-Z0-9_]{82}`
- JWT pattern: `eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}` (with confidence threshold)

These are deliberate, conservative defaults that catch the 95% case without false-positive overload. The user can override via `scan_config.additional_ignores` (extends) or `scan_config.disable_default_ignores: true` (replaces). Secret denylist is not user-overridable for v1 (forcing opt-in to indexing secrets would be an additional Phase 5 question).

### Decision 8: Failure mode for unreachable source paths

**Options considered**:
- A: Mark archive `offline`, fail all reads with 503, retry on next request
- B: Fail open — serve stale results from the index, surface a warning
- C: Block all reads with 503 until manual operator action

**Recommended**: B (fail open with warning) for reads; A (mark offline) for writes/rescans.

**Rationale**: A user mounts an NFS share. The mount briefly disconnects. The agent is mid-query. Option C would cause every read to hard-fail and the agent abandons the conversation. Option B serves the cached index — the chunks and embeddings are still in PostgreSQL, only the original file content for streaming downloads is unreachable. The semantic-search returns results with a warning flag in the response indicating staleness; the user gets degraded but useful behavior. Writes (rescans, new ingests) refuse cleanly with 503 because they cannot complete safely. The integrity sweep job (Stream C §1.1 recommendation) detects the offline state and emits a metrics/log event for operator awareness (A§4.3 implicit, C§1.1 — integrity sweep section).

**ADR scope**: ADR-097 — Failure modes for Referenced archive source-path unavailability. Records the read-vs-write asymmetry and the integrity-sweep detection model.

**Operator alternative**: If the operator wants strict consistency (refuse all reads if source unreachable), expose `scan_config.strict_consistency: bool`. v1 default should be the lenient mode; state this as Phase 5 input.

---

## 4. Construction Workstreams

Decomposition into 8 workstreams that the next phase turns into issue trees. No time estimates per `.claude/rules/no-time-estimates.md`. Each workstream lists: scope, dependencies, gates.

### WS-1: Storage Backend Abstraction Extension

**Scope**: Add `FileSource::Referenced(PathBuf)` variant. Implement `ReferencedBackend: StorageBackend` (no-op `write`/`delete`, absolute-path `read`/`exists`/`resolve_path`). Add streaming `compute_content_hash_stream(path: &Path)` for large files. Extend `PgFileStorageRepository` dispatch to handle the third storage backend.

**Depends on**: nothing (pure trait extension, no schema or API surface impact)

**Gated by**: existing test suite still passes; new unit tests cover ReferencedBackend behavior (writes refused, reads work, resolve_path returns literal path).

**File touchpoints**: `crates/matric-db/src/file_storage.rs`

### WS-2: Archive Schema and Registry

**Scope**: Migration adding `storage_mode`, `source_path`, `scan_config`, `last_scan_at`, `scan_status` columns to `archive_registry`. Extend `ArchiveInfo` struct and `ArchiveRepository` trait. Add `create_referenced_archive()` convenience method. Update `drop_archive_schema()` to confirm source-path-safe semantics. Update `DefaultArchiveCache` and `ArchiveContext` to carry storage_mode.

**Depends on**: WS-1 (needs the `StorageMode` enum which lives in `matric-core` or alongside the backend)

**Gated by**: migration applies cleanly to existing deployments (default `storage_mode='managed'` preserves all current archives); schema-clone (`clone_archive_schema`) refuses to clone a Referenced archive into another Referenced archive without explicit source_path; cache TTL expiry continues working with the new fields.

**File touchpoints**: `migrations/<new>_referenced_storage.sql`, `crates/matric-db/src/archives.rs`, `crates/matric-api/src/middleware/archive_routing.rs`

### WS-3: Walker + Ignore + Secret-Scan

**Scope**: Add `ignore = "0.4"` workspace dep. Build `ScanWalker` module that wraps `ignore::WalkBuilder` with Fortemi's default ignore list (Decision 7) and secret-detection layer. Provide both path-denylist and content-denylist secret checking. Emit `QuarantineEvent` records for skipped files. Threaded walking via `WalkBuilder::build_parallel()` with `min(4, num_cpus)`.

**Depends on**: nothing (standalone library)

**Gated by**: walker correctly respects `.gitignore` for 5 representative test cases; secret detection catches PEM private keys and AWS access keys in fixture files; symlink-loop protection works (default in `ignore` crate); permission-denied subdirs are skipped with logged warnings, not fatal errors.

**File touchpoints**: new `crates/matric-jobs/src/scan_walker.rs`, `Cargo.toml`

### WS-4: Scan-and-Ingest Job Pipeline

**Scope**: New `JobType::DirectoryScan` variant. New `DirectoryScanHandler` that orchestrates: walk (WS-3), per-file streaming BLAKE3 hash (WS-1), blob dedup query, blob/attachment/note INSERTs, queue Extraction jobs for each file. Idempotent on content_hash. Updates `archive_registry.scan_status` lifecycle: `idle → scanning → idle | error`. Mixed-mode at blob layer per Decision 3 (source blobs `storage_backend='referenced'`, derived artifacts route to companion managed location). Extend `extraction_handler.rs` line 146 path-access gate to include `storage_backend='referenced'`.

**Depends on**: WS-1, WS-2, WS-3

**Gated by**: scan-and-ingest of a 1k-file fixture repo completes; chunks and embeddings appear in the archive's pgvector store; existing search API returns results for ingested chunks; re-running the scan is a no-op (idempotent); derived artifacts (test by ingesting a fixture image) land in `{FILE_STORAGE_PATH}/derived/{archive_id}/` not in the source directory.

**File touchpoints**: new `crates/matric-jobs/src/directory_scan_handler.rs`, `crates/matric-jobs/src/extraction_handler.rs` (extend path-access gate), `crates/matric-core/src/lib.rs` (JobType enum variant)

### WS-5: Live Update Detection (DEFERRED — backlog)

**Scope** (for future, not v1): `notify-rs` + `notify-debouncer-full` integration with platform-specific behavior matrix. Polling-fallback scheduler for Docker/NFS environments. Watcher process lifecycle management (one watcher per archive, started on archive-create, stopped on archive-drop).

**Depends on**: WS-1 through WS-4

**Gated by**: not in v1 scope per Decision 4. Backlog issue with separate design RFC required before construction begins.

### WS-6: Derived Artifact Companion Location

**Scope**: Configurable companion derived-storage root (env var `FORTEMI_DERIVED_STORAGE_PATH`, default `{FILE_STORAGE_PATH}/derived/`). Per-archive subdirectory creation (`{derived_root}/{archive_id}/`). Cleanup on archive drop (removes the companion subdirectory only — never touches `source_path`). Update extraction handler's `store_derived_attachment_tx` to route Referenced-archive derived artifacts to the companion location.

**Depends on**: WS-2 (needs `storage_mode` to dispatch)

**Gated by**: dropping a Referenced archive deletes derived artifacts but leaves source untouched; managed-archive derived artifacts continue going to existing path (no behavior change for existing archives).

**File touchpoints**: `crates/matric-jobs/src/extraction_handler.rs`, possibly new helper in `matric-core`

### WS-7: API Surface

**Scope**: New routes — `POST /api/v1/archives/referenced` (create with source_path validation), `POST /api/v1/archives/{name}/rescan` (queue scan job, return job_id), `GET /api/v1/archives/{name}/scan-status` (poll), `GET /api/v1/archives/{name}/quarantined-files` (audit secret-scan skips). Extend write-gate in `archive_routing_middleware`: Referenced archives reject mutating routes (POST/PUT/DELETE on notes/attachments) with 403 — except for the rescan endpoint which is allow-listed. Path canonicalization + allowlist enforcement at create time (env `FORTEMI_REFERENCED_STORAGE_ROOTS`).

**Depends on**: WS-2 (schema), WS-4 (scan jobs)

**Gated by**: cannot create Referenced archive with path outside allowlist (returns 400); cannot create Referenced archive with non-existent source_path (returns 400); read operations work on Referenced archives via existing handlers; mutating operations return 403 with clear error message; rescan endpoint returns 202 with job_id and the job runs to completion.

**File touchpoints**: `crates/matric-api/src/main.rs`, `crates/matric-api/src/middleware/archive_routing.rs`

### WS-8: MCP Tool Surface

**Scope**: Extend `manage_archives` Node MCP tool to accept `storage_mode` and `source_path` params on create. Add new `rescan_archive` tool with async-job-return semantics (per C§2.7). Update tool descriptions and JSON schemas. Update `get_documentation` tool output to surface the new capabilities.

**Depends on**: WS-7 (API endpoints to wrap)

**Gated by**: agents can create a Referenced archive via MCP; agents can trigger rescan via MCP and poll job status; tool schemas validate correctly in MCP Inspector; backward compat: existing `manage_archives` invocations without the new params continue working (default to Managed mode).

**File touchpoints**: `mcp-server/` (Node.js)

### WS-9: Multi-Tenant Security Tests (TI-style suite)

**Scope**: Cross-tenant boundary test suite. Path traversal tests (request `../../../etc/passwd` via download endpoint, expect 404 not 200). Symlink-out-of-root tests. Multi-tenant isolation: Tenant A's Referenced archive at `/srv/fortemi/A/code` cannot be accessed by Tenant B even if B knows the archive name. Secret-scan red-team test: drop a file with PEM private-key header into a scanned directory, verify it's quarantined and embeddings absent. Drop-archive safety test: confirm `drop_archive_schema` never touches `source_path`. Mount-disappearance test: unmount source path mid-query, verify Decision 8 failure semantics (warn-on-read, 503-on-write).

**Depends on**: WS-1 through WS-7

**Gated by**: all tests pass on CI; security findings logged with appropriate severity; any test that documents an accepted-risk produces a tracked exception in `.aiwg/security/`.

**File touchpoints**: `crates/matric-api/tests/`, new `crates/matric-db/tests/referenced_security.rs`

### WS-10: Documentation and Deployment Plan Updates

**Scope**: Update `CLAUDE.md` with Referenced storage mode section. Update `docs/deployment-bundle.md` (or equivalent) with Docker bind-mount requirements (uid/gid mapping, `:ro` flag for read-only enforcement, FS-event propagation caveats per platform). Update `docs/multi-memory-agent-guide.md` to cover Referenced archives. Add `docs/referenced-storage.md` operator guide covering: when to use Referenced vs Managed, secret-scan behavior, performance expectations (initial ingest time, disk usage estimates from B§6.2), failure modes (Decision 8). Update API reference for new endpoints.

**Depends on**: nothing structurally; should track implementation progress so docs ship with code

**Gated by**: doc-sync skill passes (no broken @-mentions); operator can follow the deployment doc to mount a directory and create a Referenced archive end-to-end.

**File touchpoints**: `CLAUDE.md`, `docs/` directory

---

## 5. Risk Register (Top 10)

Severity × Likelihood, both 1-5 scale. Workstream column identifies primary affected work area.

| # | Risk | Sev | Lik | WS | Mitigation |
|---|---|---|---|---|---|
| R-1 | Secret leakage from auto-indexing user-owned config files | 5 | 4 | WS-3, WS-4 | Combined path-denylist + content-regex secret detection at ingest, hard-stop with quarantine logging (B§5.1, A§4.1). Defaults explicit in Decision 7. |
| R-2 | Multi-tenant boundary breach via crafted source_path | 5 | 3 | WS-7, WS-9 | Path canonicalization + allowlist enforcement at archive-create (env var `FORTEMI_REFERENCED_STORAGE_ROOTS`). Tenant-scoped root for `FORTEMI_MULTI_TENANT=true` deployments. Path-traversal test suite in WS-9 (A§4.2, C§1.2). |
| R-3 | Performance death on monorepo ingest | 4 | 4 | WS-3, WS-4 | Default file-size cap (10MB), default ignore list excludes `node_modules/`/`vendor/`/`target/`/`dist/`, threaded walker capped at `min(4, num_cpus)`, content-hash dedup prevents re-embedding identical files (B§5.2, A§2.1). |
| R-4 | Docker bind-mount FS events drop silently → index drifts | 3 | 5 | WS-5 (deferred) | Decision 4: defer live watching to v2. v1 uses explicit reindex API, eliminating this risk entirely. Documented as known limitation in WS-10 docs (B§5.4, C§2.2). |
| R-5 | Source directory disappears mid-query → all reads fail | 3 | 3 | WS-7 | Decision 8: fail-open for reads with warning, fail-closed for writes. Integrity sweep job detects offline state and emits metrics (A§4.3, C§1.1). |
| R-6 | `CodeAstAdapter` regex extraction produces low-quality chunks for code search | 3 | 4 | (out of scope for #736) | Tree-sitter upgrade is a parallel issue, not blocking #736. v1 ships with regex extraction; quality is "acceptable" not "great." Operators get a working feature; tree-sitter improves it later (C§1.4, C§2.3). |
| R-7 | Symlink loops or out-of-root symlinks cause runaway scan | 3 | 2 | WS-3 | `ignore` crate has symlink-loop protection by default. Decision: never follow symlinks pointing outside root (configurable but default-off). Skip and log on out-of-root targets (A§2.4). |
| R-8 | Derived artifact disk usage explodes for large media-heavy referenced archives | 3 | 3 | WS-6, WS-10 | Document the disk-usage model in operator docs. Operator-tunable `FORTEMI_DERIVED_STORAGE_PATH` allows pointing derived to a different volume. Per-archive disk-usage stats exposed via existing `manage_archives` (A§1.2). |
| R-9 | Rename detection failure surfaces as "file deleted + new file created" pairs | 2 | 4 | WS-4 (basic), WS-5 (improved) | v1: no special rename detection (delete-event removes index entries, create-event adds new ones). v2 (with WS-5): content-hash correlation with 30s TTL pending-delete buffer per A§6.2. User-visible impact in v1 is search results may briefly show paths that have changed; tolerable for on-demand-rescan model (A§6.2). |
| R-10 | Tree-sitter parser instances cause memory pressure on large repos | 2 | 2 | (deferred) | Tree-sitter parsing is not in #736 scope (regex extraction continues). When tree-sitter is activated, per-thread parsers (not global) per C§2.3. Documented but not addressed in v1. |

---

## 6. Open Questions for Operator (Phase 5 Approval Gate)

These are the questions the operator MUST answer before Construction begins. Framed as ADR-style decisions with recommendations and alternatives.

### Q-1: Live update detection in v1?

**Decision**: Live filesystem watching for Referenced archives.

**Options**:
- (A, recommended) Defer entirely. v1 is explicit-reindex-only via `POST /rescan`. Live watching becomes WS-5 in the backlog with its own design RFC.
- (B) Add polling-fallback only at 60s interval — no inotify, works on all platforms. Lower risk than full hybrid, but new long-running scheduler component to introduce.
- (C) Full notify-rs + polling hybrid per Stream A's primary recommendation. Highest user value, highest implementation cost, foot-gun on Docker bind mounts.

**Recommendation**: A. The Docker bind-mount inotify behavior is a real silent-failure risk and the on-demand reindex API satisfies the core user need ("I added files, please reindex").

### Q-2: Per-blob storage mode as user-facing concept?

**Decision**: Whether to expose mixed-mode archives (some blobs Referenced, some Managed) to users.

**Options**:
- (A, recommended) No. Storage mode is archive-level in the user model. Mixed-mode at the blob layer exists only for derived artifacts (system implementation detail, not user-facing).
- (B) Yes. Users can have a single archive that references some external code AND contains uploaded notes.

**Recommendation**: A. Per-blob mode doubles UX surface area without solving a real user problem. Users wanting both should create two archives.

### Q-3: Secret-scan opt-out for performance?

**Decision**: Whether to expose `scan_config.skip_secret_scan: bool` for users who accept the risk.

**Options**:
- (A, recommended) No opt-out in v1. Secret detection is mandatory.
- (B) Allow opt-out with documented warning.

**Recommendation**: A. The performance cost of regex-based secret detection is minimal (sub-millisecond per file for the patterns in Decision 7). The risk of opt-out becoming the default for users who don't read warnings outweighs the marginal speedup.

### Q-4: MCP tool surface — extend existing or new family?

**Decision**: How to expose Referenced-archive operations via MCP.

**Options**:
- (A, recommended) Extend `manage_archives` with new params; one new `rescan_archive` tool.
- (B) Create `directory_archive_*` family separate from `manage_archives`.

**Recommendation**: A. Backward compat preserved, agent mental model unchanged.

### Q-5: Path allowlist for Referenced source paths?

**Decision**: Whether to require operator pre-configuration of allowed source path roots.

**Options**:
- (A, recommended) Yes — env var `FORTEMI_REFERENCED_STORAGE_ROOTS` (colon-separated list); archives can only reference paths under one of those roots. Empty/unset allows any path (single-user deployments).
- (B) No restriction; trust the user.
- (C) Restriction always-on in multi-tenant deployments (`FORTEMI_MULTI_TENANT=true`), optional otherwise.

**Recommendation**: C. Multi-tenant deployments need the restriction; single-user deployments shouldn't be forced to configure it.

### Q-6: Multi-archive directory overlap?

**Decision**: What happens if two archives reference overlapping directories.

**Options**:
- (A, recommended) Allowed with warning. Each archive gets its own pgvector chunks; embedding compute is duplicated but the user explicitly opted in.
- (B) Forbidden. `create_referenced_archive` fails if source_path overlaps any existing archive's source_path.
- (C) Allowed silently.

**Recommendation**: A. Forbidding overlap creates a confusing constraint (subdirectories overlap with parent directories — is `/srv/foo` an overlap with `/srv/foo/bar`?). Warning surfaces the duplication cost without blocking legitimate use cases.

### Q-7: Initial scan performance target?

**Decision**: Operator expectation for "initial scan of 10k-file referenced directory."

**Options**:
- (A) <2 minutes (requires GPU embedding always-on, may starve other GPU workloads)
- (B, recommended) <10 minutes (default CPU embedding via Ollama nomic-embed-text, no GPU contention)
- (C) "Best effort, runs in background, search results stream in as chunks complete" (no explicit SLA)

**Recommendation**: B. Per B§6.2, nomic-embed-text on a 3060 12GB runs ~500 chunks/sec; 10k files × ~10 chunks/file = 100k chunks ≈ 3-4 min. Setting expectation at 10 minutes gives headroom for monorepos and conservative hardware.

### Q-8: Failure mode for source-path unavailability — strict or lenient?

**Decision**: Behavior when source directory becomes unreachable (mount disappears, NFS timeout).

**Options**:
- (A, recommended) Lenient: serve cached search results with warning flag; reject writes/rescans with 503.
- (B) Strict: refuse all reads with 503 until source path is reachable.
- (C) Configurable per-archive via `scan_config.strict_consistency`.

**Recommendation**: A as v1 default; C as Phase 5 follow-up if operator requests.

---

## 7. Non-Goals (What This Epic Does NOT Do)

Explicit out-of-scope. Surfacing these prevents scope creep at Phase 5.

1. **Remote storage backends.** No S3, GCS, Azure Blob, MinIO, or HTTP-based remote references in this epic. Storage is local filesystem only (`PathBuf`-addressed). Remote storage is a separate, larger epic (would touch the same `StorageBackend` trait but with very different failure modes, auth, and consistency semantics).

2. **Tree-sitter activation for code parsing.** The existing `CodeAstAdapter` uses regex. v1 of #736 produces regex-quality chunks. Tree-sitter upgrade is a parallel issue (existing feature flag, separate deferred work).

3. **Live filesystem watching.** Deferred to WS-5 backlog. v1 is explicit-reindex-only.

4. **Cross-archive overlap detection.** Q-6 resolves to allow-with-warning; the system does not enforce uniqueness of source paths across archives.

5. **Rename detection via content-hash correlation.** Stream A §6.2's pending-delete TTL pattern is deferred to v2 (couples to live watching).

6. **GUI for archive management beyond API/MCP.** No web UI changes for creating or managing Referenced archives. CLI and API are the v1 surface.

7. **AI-assisted ignore-list generation.** Defaults are static (Decision 7). No "learn what to ignore from your project" feature.

8. **Cross-tenant federated search over Referenced archives.** Existing `federated_search` API may incidentally work, but no specific cross-archive ranking or relevance work in this epic.

9. **Source-path migration.** Once an archive's `source_path` is set, it cannot be changed. To move source, the operator drops the archive and creates a new one. (Defensible to relax in v2; explicit non-goal here.)

10. **Source-file write-back from Fortemi.** Even if a future agent feature wanted to "edit code via Fortemi," v1 explicitly forbids writes to source paths. Edits flow through the agent's normal filesystem tools (Edit, Write), bypassing Fortemi entirely.

---

## 8. Citations

Every load-bearing claim above traces back to one or more source streams (A, B, C) by section. The mapping:

| Synthesis section | Primary sources |
|---|---|
| §1 Exec summary | A§1.1, A§1.2, B§1.7, C§1.1, C§1.4, B§5.1, B§5.3, C§2.2 |
| §2.1 Consensus (storage mode) | A§1.1, A§1.2, A§6.3, A§8.1, B§5.3, B§5.1, C§1.1, C§1.2, C§1.4 |
| §2.2 Disagreement 1 (per-archive vs per-blob) | A§1.1, A§1.2, C§1.2, C§1.4 |
| §2.2 Disagreement 2 (live update) | A§2.2, B§5.4, C§2.2 |
| §2.2 Disagreement 3 (secret detection) | A§4.1, B§5.1 |
| §2.2 Disagreement 4 (trait extension) | A§1.3, C§1.1 |
| §2.3 Stream C constraints | C§1.1, C§1.2, C§1.3, C§1.4, C§2.7 |
| §3 Decision 1 (archive-level mode) | C§1.2, C§1.4 |
| §3 Decision 2 (extend trait) | A§1.3, C§1.1 |
| §3 Decision 3 (derived artifacts) | A§1.2, A§8.1, C§1.4 |
| §3 Decision 4 (defer live updates) | A§2.2, B§5.4, C§2.2 |
| §3 Decision 5 (pre-ingest secret scan) | A§4.1, B§5.1 |
| §3 Decision 6 (MCP surface) | C§2.7, B§2.1 |
| §3 Decision 7 (defaults) | A§2.1, A§4.1, B§5.1, B§5.2 |
| §3 Decision 8 (failure modes) | A§4.3, C§1.1 |
| §4 Workstreams | Cross-cutting; primarily Stream C source survey for file touchpoints |
| §5 Risk register | A§2.1, A§2.4, A§4.1, A§4.2, A§4.3, A§6.2, B§5.1, B§5.2, B§5.4, C§1.1, C§1.2, C§1.4, C§2.2, C§2.3 |
| §6 Open questions | Drawn from B§7 (Stream B open questions) and resolved against Stream A/C constraints |
| §7 Non-goals | Inferred from scope boundaries surfaced across all three streams |

The three research streams are also wholly @-referenceable from the next phase:

- `@.aiwg/working/issue-planner-storage/research-best-practices.md` — Stream A
- `@.aiwg/working/issue-planner-storage/research-current-state.md` — Stream B
- `@.aiwg/working/issue-planner-storage/research-vendor-docs.md` — Stream C

---

**End of Synthesis**.

Phase 3 (SDLC corpus generation) takes this document as its sole input. The synthesis decisions and open questions in §3 and §6 are designed to feed directly into the Inception/Elaboration artifact set: §3 decisions become ADRs (ADR-091 through ADR-097), §6 questions become the operator approval gate at Phase 5, §4 workstreams become epic-to-issue decomposition, §5 risks populate the risk register, §7 non-goals scope the construction plan.

# ADR-FORTEMI-103: Defer Live Filesystem Watching to v2; v1 Ships with Explicit Reindex API Only

**Status**: Proposed
**Date**: 2026-05-21
**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (commissioned by Phase 3 SDLC corpus generation)
**Source**: Synthesis §3 Decision 4, §2.2 Disagreement 2

## Context

#736 user-facing description says "automatically scan-and-ingest" when a Referenced archive is created. A natural reading is that Fortemi should also detect post-creation changes to the source directory automatically — files added, modified, or deleted — and keep the index in sync without operator intervention.

The standard library choice for cross-platform filesystem watching is Rust's `notify-rs` (and the related `notify-debouncer-full`). Stream A's primary recommendation (synthesis §2.2 Disagreement 2) was a `notify-rs` + polling hybrid: use inotify/FSEvents/ReadDirectoryChangesW when available, fall back to polling when not. Stream B observed that several community tools (Sourcegraph, Cody, code-indexing MCP servers) defer live watching entirely. Stream C's source survey found Fortemi has no existing long-running watcher process and no fallback-polling scheduler — both would be net-new infrastructure.

The Fortemi-specific blockers documented in synthesis §2.2 and §5 R-4 are not abstract:

- **Docker bind mounts on `overlay2` (the default storage driver) silently drop host-side inotify events.** The kernel inotify subsystem and `overlay2` do not propagate events across the layer boundary in either direction. A user mounts `/home/user/projects` into the Fortemi container with `-v /home/user/projects:/srv/source`, edits a file on the host, and the container sees the file change via subsequent reads — but inotify reports nothing. The index drifts silently; the user trusts the index because the system advertised "automatic" sync.
- **Linux's default `max_user_watches=8192` is exhausted by any non-trivial code repo.** A 5k-file repo with `node_modules/` triples that. `inotify_add_watch` starts returning `ENOSPC`; some events are dropped silently. Raising the limit is a sysctl operator action that adds operational friction and doesn't compose well across multi-tenant deployments where each tenant might need different limits.
- **macOS FSEvents coalesces events under load.** Bursts of changes (a `git checkout` of a branch with 200 file changes) may come back as a single coalesced event, requiring rescan of the entire directory.
- **Windows ReadDirectoryChangesW has a 64KB event buffer that overflows on `npm install`.** Buffer overflow returns a single "overflow" event with no detail.

`notify-rs` does not solve any of these — it surfaces the platform's underlying behavior. The hybrid pattern from Stream A is correct in principle but adds substantial implementation cost (new long-running watcher process, watcher lifecycle management tied to archive create/drop events, fallback-polling scheduler) and the failure modes are silent — the user gets the "automatic" UX promise but the index quietly drifts.

The alternative — explicit `POST /rescan` — is straightforward to implement (already in WS-7 scope), produces predictable behavior on every platform, and has no silent-failure mode (the operator triggers rescan, the operator sees the result).

## Decision

**v1 of #736 ships with explicit-reindex-only behavior.** Live filesystem watching is deferred to a follow-up workstream (WS-5 in synthesis §4, marked deferred). The user-facing semantic guarantee for v1 is **eventually consistent on operator action, not on filesystem events**.

Concretely:

1. The `POST /api/v1/archives/{name}/rescan` endpoint is the supported mechanism to refresh a Referenced archive's index after source changes. It is documented as part of the v1 surface (WS-7, WS-10).
2. `MCP::rescan_archive` is the supported mechanism for agent-driven rescan (WS-8).
3. The `archive_registry.last_scan_at` column is exposed via `GET /api/v1/archives/{name}/scan-status` so operators can see how stale the index is.
4. No `notify-rs` dependency is added to the workspace in v1. No watcher process is started.
5. The operator documentation (WS-10) explicitly states the eventually-consistent-on-demand semantics and the v2 roadmap for live watching.
6. WS-5 (live watching) is filed as a backlog issue with its own design RFC; it must pass its own architecture gate before construction begins.

The v1 scan-and-ingest pipeline is idempotent (synthesis §4 WS-4 gate). Re-running a scan over an unchanged source directory is a no-op (content_hash dedup + `archive_file_cache` mtime check). Re-running over a changed source directory adds new files, updates changed files, and tombstones removed files. This makes the explicit-rescan path safe to invoke as often as the operator wants — including cron-style polling from the operator side if they need approximate auto-detection without Fortemi taking on the responsibility.

## Consequences

### Positive

- **No silent-failure mode.** The operator triggers rescan, sees the result. There is no scenario where the index drifts without the operator's knowledge.
- **No Docker bind-mount inotify pitfall.** v1 is correct on every supported deployment topology (Linux Docker, Docker Desktop on macOS/Windows, bare-metal Linux, NFS mounts, SMB mounts). The platform's underlying FS-event behavior is irrelevant to v1's correctness.
- **No new long-running process to manage.** No watcher lifecycle (start on archive-create, stop on archive-drop, restart on crash, deal with watcher-process memory pressure under thousands of watches). Construction scope is bounded to the existing job-worker model.
- **Smaller scope means faster construction.** WS-5 was estimated as the largest workstream by far (cross-platform behavior matrix + watcher process + polling scheduler + lifecycle management). Deferring it cuts the v1 scope substantially.
- **Operators can polyfill with cron.** `crontab -e: 0 * * * * curl -X POST .../rescan` gives hourly approximate freshness on any platform with zero Fortemi changes. Operators who want lower-latency freshness can poll more frequently; the rescan is idempotent.

### Negative

- **The "automatically scan-and-ingest" phrasing in #736 is half-met.** v1 automatically scans on archive-create and on explicit-rescan trigger — but not on filesystem events. This is the most direct trade-off and must be communicated clearly in user-facing docs.
- **No real-time UX for users who edit files frequently and immediately query.** The workflow "edit file → immediately ask the agent about it" requires a manual `rescan` call between the two. Documentation must guide users on this.
- **WS-5 becomes a separate epic.** The work to add live watching is preserved (scope, design considerations, platform-behavior matrix) but is deferred. There is a risk that v2 never ships and v1's "eventually consistent on operator action" becomes the permanent semantic — which is fine as a steady state, but the operator should make that choice deliberately, not by default.

### Neutral

- The integrity sweep job (synthesis §1.1 Stream C recommendation) can serve as a poor-man's auto-rescan if scheduled — it detects offline state and emits metrics. Operators can extend it to trigger rescan on detected drift, which is an opt-in middle ground between fully manual and fully automatic.

## Alternatives Considered

### Alternative A (Stream A primary recommendation): notify-rs + polling hybrid

Use inotify/FSEvents/ReadDirectoryChangesW where available; fall back to polling at e.g. 60s intervals where not. Run a long-running watcher per Referenced archive. **Rejected for v1** because:
- Docker bind-mount inotify silently fails on the default `overlay2` driver; the failure is invisible to users.
- Adds substantial v1 scope (new process, lifecycle, fallback scheduler).
- Failure modes are silent — index drift the user doesn't know about.
- Defensible as v2 with explicit per-platform behavior documentation, but high-cost-low-confidence for v1.

### Alternative B: Polling-only at 60s interval (no inotify)

Drop `notify-rs` entirely; just poll every 60s. Works the same on every platform. **Rejected for v1** because:
- Still introduces a new long-running scheduler that doesn't exist today.
- Still has silent drift between polls (60s of staleness windows).
- For operators who do want low-latency freshness, 60s is too slow; for operators who don't care, on-demand rescan is sufficient. Polling-only satisfies neither audience.
- Defensible as the v1.5 path (synthesis §3 Decision 4 operator alternative) — lower-risk than full hybrid, simpler than notify-rs, and works on Docker bind mounts.

### Alternative C: notify-rs only, no polling fallback

Use `notify-rs` where it works, do nothing where it doesn't. **Rejected** because:
- Inconsistent UX: works on bare-metal Linux, silently breaks on Docker. Users will report bugs that are actually documentation gaps.
- Forces all multi-tenant deployments (most of which use Docker) into a broken state.

## References

- Synthesis §3 Decision 4 — defer live watching; option-tree and rationale
- Synthesis §2.2 Disagreement 2 — Stream A's hybrid vs Stream C's deployment-blocker analysis
- Synthesis §5 R-4 — Docker bind-mount FS-event drop risk + mitigation (= this ADR)
- Synthesis §6 Q-1 — operator approval gate question (recommendation: A = defer)
- Synthesis §7 non-goal 3 — live filesystem watching explicitly out of scope for v1
- ADR-FORTEMI-100 — archive-level storage mode (context: `last_scan_at` lives on `archive_registry`)
- WS-5 (synthesis §4) — deferred workstream description for the v2 design
- `.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` §4.6 — WS-7 API surface including `POST /rescan`

# UC-EXTSTORAGE-010: Live Filesystem Update Detection (DEFERRED — v2)

**Workstream**: WS-5 (Live Update Detection — DEFERRED)
**Source**: synthesis §4 WS-5, §3 Decision 4, §6 Q-1
**Status**: DEFERRED — Stub only. NOT in v1 scope.
**Priority**: N/A (out of scope for v1)

## Deferral Rationale

Per synthesis §3 Decision 4 and §6 Q-1: live filesystem watching for Referenced archives is DEFERRED to a follow-up workstream with its own design RFC. The v1 substitute is explicit-rescan-only via `POST /api/v1/archives/{name}/rescan` (UC-EXTSTORAGE-004).

### Why deferred

1. **Docker bind-mount inotify drops**: `overlay2` storage driver silently drops host-side inotify events. Failure is silent — index drifts, user is confused.
2. **Linux watcher limit**: default `max_user_watches=8192` exhausted by any non-trivial code repo.
3. **Cross-platform variance**: macOS FSEvents coalesces under load; Windows ReadDirectoryChangesW has 64KB buffer that overflows on `npm install`.
4. **New long-running component**: live watching requires a watcher process that does not exist today. Lifecycle management (start on archive-create, stop on archive-drop, restart on watcher crash) is a non-trivial new system.

### Operator path until v2

- Use UC-EXTSTORAGE-004 (`POST /rescan`) whenever you add, modify, or remove files in a Referenced archive's source directory
- For batch workflows: invoke rescan from a script or cron after your build/sync completes
- For agent workflows: have the agent call `rescan_archive` MCP tool (UC-EXTSTORAGE-007) after modifying files

## Sketch of Future Goal (v2)

When a file is added, modified, or removed in a Referenced archive's source directory, Fortemi automatically detects the change within ~60 seconds (polling fallback) or ~1 second (inotify on supported platforms) and re-indexes the affected files without operator action.

## v2 Design Considerations (recorded for future design RFC)

- **Hybrid notify-rs + polling**: synthesis §3 Decision 4 alternative C — full notify-rs + polling per Stream A
- **Polling-only at 60s**: synthesis §6 Q-1 alternative B — simpler, works everywhere, no Docker pitfalls
- **Watcher process lifecycle**: one watcher per archive, OR shared multiplexed watcher
- **Event-driven re-ingest**: same scan-and-ingest pipeline as UC-EXTSTORAGE-003 but triggered per event
- **Rename detection**: synthesis §5 R-9 — content-hash correlation with 30s TTL pending-delete buffer

## v2 Open Questions for Future Design

1. Is the polling-fallback at 60s acceptable to operators, or do they expect <5s latency?
2. Is per-archive watcher OK, or do we need a multiplexed watcher to scale beyond ~100 archives?
3. How do we handle events during a manual rescan? (Drain, queue, ignore?)

## v1 Acceptance Criteria

- [ ] AC-1: Documentation clearly states "live updates not supported in v1; use explicit rescan API"
- [ ] AC-2: WS-5 work is tracked as a separate backlog issue with a placeholder design RFC reference
- [ ] AC-3: No code in v1 attempts to set up filesystem watchers (failure to honor this deferral causes scope creep)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 4, §4 WS-5, §6 Q-1, §5 R-4, §7 non-goal #3
- ADR-094-storage (planned, v1): Update detection model — defer live watching
- Future ADR (v2): Live filesystem watching design (TBD)

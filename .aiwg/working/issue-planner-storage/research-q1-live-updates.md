# Q-1 Research: Live Update Detection for Referenced Archives

**Phase**: 5 (Approval Gate)
**Issue**: fortemi/fortemi#736
**Question**: Should v1 support live filesystem watching for Referenced (external-directory) archives, and with what mechanism?
**Date**: 2026-05-21
**Confidence**: MODERATE (vendor docs are HIGH quality; broader industry survey is partial due to WebFetch failures)

---

## 1. Executive Recommendation

**Option D (refined): auto-detect mount type, run `RecommendedWatcher` where supported, `PollWatcher` as fallback. Make polling-fallback the safe default for the first release; gate `RecommendedWatcher` behind explicit operator opt-in OR detection of a known-good mount type.**

Rationale — three points:

1. The notify-rs project itself documents `PollWatcher` as the *intended* workaround for every failure mode that synthesis flagged: NFS, Docker on macOS M1, FSEvents ownership restrictions, /proc and /sys pseudo-filesystems, and inotify watch-limit exhaustion. The "hybrid" is not a custom construction — it is the supported usage pattern of the library.
2. notify-rs has a healthy 2026 release cadence (notify-types v2.1.0 in January 2026) and is the watcher of choice for rust-analyzer, zed, deno, alacritty, cargo-watch, and mdBook. The "is the library mature enough" question is answered: yes.
3. The original synthesis downgrade to Option A was driven by *one specific failure mode* (Docker bind-mount silent failure). That failure mode is real, but library-documented, with a library-documented fix. Deferring the entire feature to avoid it is over-correction.

The recommendation IS NOT "Option C full hybrid as the original Stream A recommended without hedging." The recommendation is a **bounded, detection-driven hybrid** with polling-fallback as the safe-default branch.

### Trigger logic (the load-bearing part)

```
on archive create / on first scan:
  mount_kind = detect_mount_kind(source_path)  # via /proc/self/mountinfo or platform equivalent
  case mount_kind:
    "native_ext4|btrfs|xfs|zfs" → RecommendedWatcher (inotify)
    "native_apfs|hfs+"           → RecommendedWatcher (FSEvents)
    "native_ntfs"                → RecommendedWatcher (ReadDirectoryChangesW)
    "nfs|cifs|smb|sshfs|fuse"    → PollWatcher (60s default, configurable)
    "docker_bind_macos_arm"      → PollWatcher (vendor-known broken)
    "wsl2_drvfs"                 → PollWatcher (vendor-documented)
    "tmpfs|overlay"              → PollWatcher (container overlay)
    "unknown"                    → PollWatcher (safe default)
  if operator overrides via FORTEMI_WATCHER_MODE=poll|notify|off:
    honor override
```

The detection layer is small and well-isolated. The two backends (notify vs poll) share a single event-stream interface to the rest of the scan-and-ingest pipeline. The blast radius of getting the detection wrong is "watcher silently falls back to polling" — not data corruption.

---

## 2. Industry Survey

| Source | Indexing mechanism | Freshness strategy | Fallback when watching unavailable | GRADE |
|---|---|---|---|---|
| notify-rs library | (N/A — is the watcher) | RecommendedWatcher native + PollWatcher fallback | Library documents `PollWatcher` for NFS, Docker macOS, FSEvents file-ownership, /proc /sys, inotify-limit exhaustion | HIGH (vendor docs) |
| rust-analyzer | LSP server with VFS | notify-rs RecommendedWatcher + on-demand file reads | Falls back to opportunistic reads when watcher misses events; rebuilds VFS on session restart | MODERATE (consumer of notify-rs; pattern inferred from library's listed users) |
| zed editor | notify-rs based VFS | notify-rs RecommendedWatcher | Same library, same fallback path | MODERATE |
| cargo-watch | notify-rs CLI wrapper | RecommendedWatcher | Documented in notify-rs as a primary consumer | MODERATE |
| aider repo-map | On-demand generation per prompt | Regenerates on each user prompt; uses git diff awareness | No watcher — on-demand is the strategy | HIGH (aider docs explicit: "Aider sends a repo map to the LLM along with each change request") |
| Sourcegraph zoekt | Batch indexer (not a watcher) | Periodic re-index, git-fetch-driven | N/A — not a watcher; periodic | LOW (URL not fetched in this run) |
| Continue.dev | Local index | Documentation URL 404'd in fetch; pattern not confirmed | Unknown from this research | VERY LOW (fetch failed) |
| Cursor @codebase | Cloud-indexed | Out of scope (cloud product) | N/A | VERY LOW (not fetched) |
| VS Code FileSystemWatcher | Per-workspace native watcher | chokidar / native fs watcher with polling fallback flag | Has `usePolling: true` option for network mounts; long-documented pattern | LOW (general industry knowledge, not fetched in this run) |
| Plasma KRunner / Tracker | Desktop file indexers | inotify with budget cap + scheduled re-scan | Known to use scheduled re-scan when budget exceeded | VERY LOW (not fetched) |

**Methodology note**: This run completed 4 successful WebFetches before the operator's broader question was answerable with the notify-rs vendor documentation. Continue.dev and several other sources were not fetched and are marked VERY LOW. The decision below stands on notify-rs's own documentation as the HIGH-quality source — the broader survey is corroborative, not load-bearing.

---

## 3. notify-rs Deep-Dive

**Current state (2026)**:
- Active maintenance — 42 releases on record; latest `notify-types` v2.1.0 January 2026
- Minimum supported Rust 1.88
- Used by rust-analyzer, zed, deno, alacritty, cargo-watch, mdBook (per library README)
- Polling is a documented platform option, not an afterthought

**Two backends, one interface**:
- `RecommendedWatcher` — picks the OS-native backend per platform (inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows, kqueue on BSD)
- `PollWatcher` — content-comparison polling at configurable interval. Documented for use as the explicit fallback.

**Library-documented use-`PollWatcher`-when scenarios** (per docs.rs/notify):
1. **NFS and similar network filesystems** — "may not emit any events for notify to listen to"
2. **WSL programs watching Windows paths** — same root cause
3. **Docker on macOS M1** — "you have to manually use the PollWatcher, as the native backend isn't available inside the emulation"
4. **FSEvents file-ownership restrictions** — "reverting to the pollwatcher can fix the issue, with a slight performance cost"
5. **Pseudo filesystems (/proc, /sys)** — `PollWatcher` with `compare_contents` option
6. **inotify watch-limit exhaustion** — `PollWatcher` not constrained

**Open issues touching this area** (from issue list scan; specifics not fully fetched):
- INotifyWatcher recursive events (#727)
- kqueue recursive root directory watching (#703)
- Modify() event triggering (#670)
- Windows file IDs (#631)
- Symlinked subdirectories on Windows (#629)
- Windows crash when watched directory missing (#624)
- SQLite database changes on macOS (#613)

None of the open issues invalidate the library for our use. They reflect normal cross-platform watcher complexity — exactly the complexity that the recommendation above isolates behind a detection layer.

**What this means for Fortemi**: We do not need to invent a hybrid. We need to *consume* the library's documented hybrid correctly. The implementation is `match mount_kind → choose backend → wire to the same event channel`.

---

## 4. Risk Revisited

**"Docker bind-mount inotify silent failure" — still current in 2026?**

Yes — partially. The library docs confirm Docker macOS M1 is broken (emulation path). Linux native Docker bind mounts to the same host filesystem typically DO propagate inotify events (the bind mount sits on the same inode source), but with caveats around the host's inotify watch limits and the container's namespace. Docker Desktop on Windows uses WSL2 underneath and inherits WSL2's drvfs limitations for cross-OS mounts.

**Reproducer (qualitative, not run in this research)**:
1. macOS M1 host, Docker Desktop, bind mount `~/code:/workspace`
2. Container runs `inotifywait -m /workspace`
3. Edit a file in `~/code` from the host
4. No events fired

**Mitigation that the recommendation absorbs**:
- Detection layer flags `docker_bind_macos_arm` → routes to `PollWatcher`
- Default polling interval 60s (operator-configurable)
- Polling cost at 10k files: bounded by content-comparison + filesystem-stat — minutes-class on cold cache, seconds-class on warm cache. Cost is real but manageable for a v1 fleet whose typical archive is a single user's repo, not 100k-file monorepos.

**100k-file polling cost** (estimate): At ~1ms per stat on a warm cache, full sweep ≈ 100s. Polling every 60s would saturate; recommendation is to scale interval with file count (e.g., `interval = max(60s, file_count / 1000)`) and surface this to operators as a configurable knob.

---

## 5. Refined Options

### Option A — Defer entirely (synthesis's current default)
- **Implementation scope**: 0 net new units (already in synthesis)
- **Workstream impact**: WS-5 stays deferred to v2
- **Operator trade-off**: Users explicitly hit POST /rescan after every external change. Friction, but predictable.
- **Risk**: Users perceive feature as "manual / broken" if their mental model is "Fortemi watches my code."

### Option B — Polling-only at fixed 60s interval
- **Implementation scope**: ~3-4 atomic units (PollWatcher wiring, event→ingest pipeline integration, operator config knob, observability/metrics for scan cycles)
- **Workstream impact**: Re-activates WS-5 with ~4 sub-issues (scheduler, event pipeline, config, metrics)
- **Operator trade-off**: Works everywhere, no detection logic, no Docker foot-gun. Constant background cost. Higher freshness latency than notify (60s lag vs sub-second).
- **Risk**: Polling cost scales linearly with file count; 100k-file archives stress this. Acceptable for v1 if interval is operator-tunable.

### Option C — Full notify + polling hybrid, always-on
- **Implementation scope**: ~6-8 atomic units (B's scope + RecommendedWatcher wiring + per-backend event normalization + per-platform test matrix)
- **Workstream impact**: Re-activates WS-5 with ~6 sub-issues
- **Operator trade-off**: Best freshness on native mounts; same polling overhead on degraded mounts.
- **Risk**: Cross-platform test matrix is real. Stream A's original recommendation; synthesis correctly hedged on Docker risk.

### Option D — Detection-driven hybrid, polling-fallback safe default ★ RECOMMENDED
- **Implementation scope**: ~5-6 atomic units
  - WS-5-A: mount-kind detection (`/proc/self/mountinfo` parser on Linux, `statfs` on macOS, `GetVolumeInformation` on Windows)
  - WS-5-B: backend selector + unified event channel
  - WS-5-C: PollWatcher pipeline (B's scope, reusable)
  - WS-5-D: RecommendedWatcher pipeline (gated on detection)
  - WS-5-E: operator override env var (`FORTEMI_WATCHER_MODE=auto|poll|notify|off`)
  - WS-5-F: observability — emit a "watcher kind" gauge per archive
- **Workstream impact**: Re-activates WS-5 with 6 sub-issues; UC-EXTSTORAGE-005 (currently `v2-deferred`) returns to v1 scope as the operator-facing surface
- **Operator trade-off**: Best of both worlds — sub-second freshness on native mounts, predictable polling on degraded mounts, no silent-failure surprises. Slightly larger surface to test (the detection layer).
- **Risk**: Detection logic itself can be wrong. Mitigation: default to PollWatcher when detection is uncertain (`unknown` → poll), and expose the override so operators can force one mode.

---

## 6. Phase 5 Decision Recommendation

**Recommended decision: Option D.**

**Why over A (synthesis default)**:
- The blocker behind A's adoption (Docker silent-failure) is library-documented and library-fixable, not a fundamental architectural problem
- Aider's "on-demand per prompt" precedent does not transfer — aider regenerates a small repo-map; Fortemi is rebuilding embeddings and pgvector chunks, which is expensive enough that the "rescan after every change" UX is genuinely worse than passive freshness

**Why over B (polling-only)**:
- B's cost on large native mounts is unnecessary tax for users on ext4/apfs/ntfs who have working inotify equivalents
- The detection layer of D adds ~1 atomic unit of work over B and saves the cost across the typical-user majority

**Why over C (always-on hybrid)**:
- C runs both engines on degraded mounts when only one is needed
- D's safe-default-to-polling posture is more conservative and easier to validate

**Implications for the backlog (`issue-backlog.md`)**:
- WS-5 returns from "v2-deferred" to "v1 in-scope" with 6 sub-issues
- UC-EXTSTORAGE-005 status changes from `v2-deferred` to `v1`
- The critical path WS-1 → WS-2 → WS-4 → WS-7 → WS-9 → #736 is unchanged; WS-5 parallelizes off the spine after WS-2 (it needs StorageBackend's ReferencedBackend in place to attach watchers to)
- 5-6 new atomic implementation issues; ~1 new test tracker issue; ~1 new doc issue
- New total: 71 → ~78-80 issues
- Q-7 (10-minute scan SLA) is unaffected — that target is for initial scan, not steady-state watching
- Q-8 (lenient mount-failure mode) becomes more important with D — the lenient mode covers the case where a watched mount disappears mid-watch

**Confidence**: MODERATE. The decision rests on HIGH-quality vendor documentation (notify-rs docs) and a partial industry survey. The recommendation is conservative (safe-default-to-polling) precisely because the survey is partial. If a Phase 5 reviewer disagrees, the natural softer fallback is Option B (polling-only), which is also library-supported and lower-risk than C.

---

## 7. References

| Source | URL | What it confirms | GRADE |
|---|---|---|---|
| notify-rs README | https://github.com/notify-rs/notify | Library is actively maintained (2026), used by rust-analyzer/zed/deno/alacritty/cargo-watch/mdBook | HIGH |
| notify-rs docs (docs.rs) | https://docs.rs/notify/latest/notify/ | PollWatcher is the documented fallback for NFS, Docker macOS M1, FSEvents ownership, /proc /sys, inotify-limit. Direct vendor guidance. | HIGH |
| notify-rs open issues | https://github.com/notify-rs/notify/issues | None of the visible 2024-2026 open issues invalidate the library for code-indexing use | MODERATE (issue list page, not individual issue bodies) |
| aider repo-map docs | https://aider.chat/docs/repomap.html | aider uses on-demand regeneration per user prompt (no watcher) | HIGH (vendor docs explicit) |
| Continue.dev codebase docs | (URL 404'd in this fetch) | Not assessed | VERY LOW |
| Sourcegraph zoekt | (Not fetched in this run) | Not assessed | VERY LOW |
| VS Code chokidar usePolling | (General industry knowledge) | VS Code's chokidar has documented `usePolling: true` for network mounts; same pattern as D | LOW (not directly fetched in this run) |

**Per citation-policy.md hedging**: Two HIGH-quality sources (notify-rs README + docs.rs/notify) carry the decision. Other rows are corroborative and explicitly marked LOW/VERY LOW where the source was not directly read. The recommendation is hedged accordingly — Option D is recommended, not asserted as the only viable answer.

**Per no-time-estimates.md**: All scope expressed in atomic units, not wall-clock estimates.

## References (cross-doc)

- @.aiwg/working/issue-planner-storage/synthesis.md — original Phase 2 recommendation (Option A)
- @.aiwg/working/issue-planner-storage/research-best-practices.md — Stream A original recommendation (Option C)
- @.aiwg/working/issue-planner-storage/issue-backlog.md — current 71-issue backlog; needs WS-5 re-activation if D is accepted
- @.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-005-view-scan-status.md — returns from `v2-deferred` to `v1` under D

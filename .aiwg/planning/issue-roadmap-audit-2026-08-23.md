# Fortemi Open Issue and Roadmap Audit - 2026-08-23

Scope: Gitea `Fortemi/fortemi`, with AIWG and `Fortemi/fortemi-react` consumer checks

Guidance: "audit open issues and lets take a look at our planned roadmap vs the outstanding worek, we likley neeed to make some updates to our wider plan to help priooritze our work"; file issues in the AIWG and `fortemi-react` repositories as needed to correct problems and fill gaps.

Mode: Existing-issue cleanup is read-only. Authorized new issue creation and reciprocal cross-repository links were applied.

## Counts

- Open Fortemi issues audited: **238**
- Priority labels: **54 P0**, **100 P1**, **61 P2**, **18 P3**, **5 without priority**
- Issues labeled `blocked`: **124**
- Not updated since 2026-07-01: **173**, including **114 P0/P1**
- Open issues without a milestone: **45**
- Taxonomy diagnostic: `.aiwg/aiwg.config` does not define semantic `issues.labels`; this audit used the repository's literal label names and did not create or rename labels.

| Tracker lane | Open | P0 | P1 | Blocked | Not updated since July |
|---|---:|---:|---:|---:|---:|
| Unmilestoned | 45 | 1 | 10 | 20 | 24 |
| Hosted auth and multi-tenancy | 69 | 11 | 47 | 12 | 41 |
| Referenced storage follow-up | 70 | 42 | 28 | 59 | 65 |
| Bridge foundation | 17 | 0 | 4 | 13 | 16 |
| Bridge provider expansion | 3 | 0 | 0 | 3 | 3 |
| Streaming realtime | 12 | 0 | 9 | 6 | 9 |
| Native distribution | 21 | 0 | 2 | 10 | 14 |
| Referenced storage v2 deferred | 1 | 0 | 0 | 1 | 1 |

## Findings

### High Priority

- **#1098 - stranded embedding jobs:** the fix is implemented and released in `v2026.7.22`; the issue remains open for one previously failing customer note to complete or terminate within the new bound. Recommended action: keep this as the first operational gate, then close on reporter confirmation or a documented bounded hypercare decision.
- **#1081 - full-fidelity suite portability:** the declared Linux x86_64, Linux arm64, and macOS arm64 authority-to-React/core-to-HotM matrix is green, but the parent remains `NO-GO` for unqualified portability/parity/complete-backup claims pending an independent final audit. Recommended action: schedule the audit and publish an explicit scoped verdict before adding feature scope.
- **#710 - authorization foundation:** most route/action and object-normalization work has landed, but per-tool MCP authorization remains dependent on the #718/mcp-gate contract. Recommended action: keep it as the active Phase 0 authorization gate; do not restart completed route inventory work.
- **#1090 -> #1091 - source identity then typed retrieval:** #1091 depends on #1090. `fortemi-react v2026.8.0` now ships the #404 source-upsert and #405 typed-retrieval consumers, with delivered-main CI green. Recommended action: pin the Fortemi authority contracts and complete shared server/PGlite/RecordStore/provider fixtures, hashes, and receipts; do not treat the React-local evidence as cross-backend parity.
- **#1092 - graph purge/deletion receipts:** `fortemi-react v2026.8.0` now ships the #406 PGlite/RecordStore consumer implementation. The remaining gate is authority-owned shared semantics and evidence, including crash/resume and restore-time re-erasure receipts that preserve ADR-102 validate-before-write and exact profile claims.
- **#733 / #734 / #943 - hosted construction:** these are the current actionable hosted P0 umbrellas. Closed #897 and #926 must no longer appear as active blockers.
- **#1072 - runner capacity recurrence:** the repository preflight exists, but CI still exhausted storage after a passing check. Recommended action: assign host-side automated cleanup, monitoring, and concurrency reservation to an operational owner; repository code alone cannot close the remaining criterion.

### Roadmap Drift

- The roadmap's former immediate list contained closed #967.
- The former critical P0 list contained closed #897 and #926.
- #1081 and #1090-#1092 were filed after the June roadmap baseline and had no phase/lane placement.
- #1098 and #1041 are release/field gates but were absent from the open-build lane.
- Referenced storage contributes 42 of 54 P0 labels. Most are blocked implementation/test descendants, so selecting work by flat P0 label order would starve actionable production, contract, and open-build work.
- The roadmap needs a separate parallel lane for gated integrations (#1053 CustodyCore, #1007 ROKO) and discovery spikes (#1015) so they do not silently compete with GA gates.

### Cleanup Candidates

- **#1078** appears already fixed by commit `79e60214` (`fix(ci): consolidate release finalization`), which removed the two redundant release-creation jobs, added a single tag-gated finalizer, and added workflow guard tests. Recommended action: attach exact main/tag CI evidence and close as fixed.
- **Five issues lack priority:** #1096, #1078, #1015, #1008, and #1007. Four are completely unlabeled. Recommended action: classify them explicitly; #1096 should remain deferred and outside the supported-platform gate until a Windows runner exists.
- **Blocked/stale hygiene:** 124 blocked issues and 173 pre-July updates need parent-gate ownership, a concrete unblock trigger, and a check date. Do not individually promote blocked descendants while their parent contract/design gate is unresolved.
- **Milestone hygiene:** the 70-issue Referenced storage milestone and 69-issue hosted milestone should expose a maintained parent/gate view. Child labels are useful for inventory but should not substitute for the active sequence in the roadmap.
- **#1098:** avoid closing solely because the patch shipped; its issue body explicitly retains customer confirmation as the final acceptance gate.

### Relationship Gaps Resolved

- Filed `roctinam/aiwg#2155`: existing project-local skills disappear from default `discover/show` when the Fortemi project cache is absent. It references the narrower closed scaffold-path fix #1758.
- Filed `Fortemi/fortemi-react#404` for #1090 PGlite/RecordStore source-addressed upsert conformance.
- Filed `Fortemi/fortemi-react#405` for #1091 typed metadata predicates and evidence locators.
- Filed `Fortemi/fortemi-react#406` for #1092 previewable graph purge and content-free deletion receipts.
- Added reciprocal consumer links to Fortemi #1090, #1091, and #1092. Each consumer issue keeps Fortemi as the canonical authority and forbids independent schema/profile semantics.
- Reconciled `fortemi-react v2026.8.0` at commit `4c335a86804dcf5f306218459ccccd8137ceafda`: #404-#406 are implemented and released with green delivered-main CI, but remain open for authority-pinned shared fixtures and receipts. Removed their inaccurate `status: backlog` labels and recorded the remaining gates on both trackers.

## Recommended Next Moves

1. Complete #1098 field confirmation and close or record the bounded residual trigger.
2. Run the independent final audit for #1081 and publish the exact claim boundary.
3. Finish the remaining #710 per-tool MCP authorization dependency path without reopening completed inventory work.
4. Pin #1090/#1092 authority revisions against the released `fortemi-react v2026.8.0` consumers, then generate shared server/PGlite/RecordStore fixtures and receipts; follow with the #1091 cross-backend retrieval corpus after #1090 stabilizes.
5. Keep Phase 1 open-build hardening ahead of licensed-server feature depth; advance #733/#734/#943 as the separate hosted lane.
6. Move #1072's remaining host automation to an operational owner and close already-fixed #1078 after evidence is attached.
7. Triage the five no-priority issues and add explicit unblock triggers/check dates to stale blocked parents.

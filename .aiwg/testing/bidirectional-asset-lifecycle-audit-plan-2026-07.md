---
title: Bidirectional Asset Lifecycle Audit Plan
status: required-current-audit
date: 2026-07-28
decision: scoped-contract-cells-green-live-lifecycle-incomplete
derived_from:
  - "@.aiwg/requirements/bidirectional-asset-lifecycle-requirements-2026-07.md"
  - "@docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md"
  - "@docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md"
---

# Bidirectional Asset Lifecycle Audit Plan

## Decision

The exact `2.0.0/full-v1` contract and Fortemi persistence/archive path have strong executable
evidence. The suite is not feature complete for unqualified bidirectional asset portability
because no single launched-GUI automated test currently drives either complete user lifecycle:

- HotM desktop/browser local file -> real TUS/network -> live Fortemi -> signed `full-v1` ->
  clean Fortemi -> real local download; or
- existing server asset -> real local download -> clean server re-upload/recovery -> exact
  byte/metadata comparison.

Fortemi's strongest route cells use PostgreSQL plus temporary filesystem storage and prove signed
export, required-signature validation, clean repeated import, semantic re-export, tamper rejection,
rollback, exact required sidecar bytes, route-level TUS resume, post-commit restart survival, and
same-byte upload/delete/import refcount concurrency. HotM now has a separate clean-worktree
headless receipt through its production Tauri local-file upload/download command cores, live
Fortemi filesystem TUS, signed `2.0.0/full-v1`, trust-required clean-memory import, and exact
reopened recovery bytes. That receipt does not launch the Tauri GUI or operate interactive native
dialogs in the same run.

## Focused Current-Head Evidence

The 2026-07-27 baseline focused run passed **135/135** selected tests:

| Cell | Result | Scope |
|---|---:|---|
| Fortemi full-v1 route | 1/1 | PostgreSQL, temporary filesystem, signed export, clean import, re-export, exact sidecars |
| Fortemi sidecar rollback | 1/1 | No partial storage on invalid or failed recovery |
| Fortemi file storage/refcounts | 5/5 | Dedup, shared deletion, orphan cleanup, scan gate |
| React/PGlite | 27/27 | Blob/full-v1 recovery, AIWG conversion, consumer cell, signature and rollback |
| AIWG bridge | 8/8 | Released converter, deterministic fixture, clean PGlite, loss/rejection matrix |
| HotM clients | 93/93 | Shard, backup, attachment, upload-store, and TUS component behavior |

Run from a sibling suite checkout:

```bash
scripts/ci/verify-bidirectional-asset-lifecycle.sh
```

Use `--install` to restore lockfile-declared dependencies first. The runner explicitly does not
claim to execute the open enabled-live Fortemi browser receipt, native Tauri GUI, approved budget,
or immutable CI publication scenarios.
It now also invokes the deterministic continuation guards for storage promotion crash-window
recovery, the AL-PERF01 bundle verifier, HotM live metrics verifier fixtures, the Tauri
command-core receipt verifier fixtures, the Tauri local-file/native-dialog command tests, and the
default skipped `live-assets` schema guard. Those
add reproducibility coverage to the focused runner without claiming enabled live Fortemi or native
Tauri GUI execution.

The expanded 2026-07-27 focused runner passed locally with `status=0` after exercising the Fortemi
full-v1 route, sidecar rollback, filesystem refcount, storage staging/promotion/journal crash-window, AL-SYS
restart/crash/concurrency, AL-PERF01 receipt, AL-PERF01 bundle verifier, React/PGlite recovery
cells, AIWG released bridge, HotM asset client tests, HotM receipt verifier tests, and the default
HotM `live-assets` schema guard (`3 passed`, `1 skipped` enabled-live test).

The 2026-07-27 Fortemi AL-SYS focused continuation added route-level coverage for the Fortemi-owned
restart and concurrency rows:

| Cell | Result | Scope |
|---|---:|---|
| Fortemi AL-SYS04 live restart slice | 1/1 | Real HTTP/TUS upload with offset mismatch/resume, PostgreSQL archive, filesystem storage, post-commit AppState/server restart, signed `2.0.0/full-v1` export, exact sidecar bytes, signed clean-destination import, digest/refcount/staging cleanup oracles |
| Fortemi AL-SYS04 TUS finalization crash-window slice | 1/1 | Simulated process death after durable total offset and before attachment finalization; empty retry over real HTTP finalizes from retained staging bytes, removes staging only after durable attachment/finalized metadata commit, and leaves one attachment, one blob, refcount 1 |
| Fortemi AL-SYS04 import sidecar during-staging crash slice | 1/1 | Test-only one-shot fault inside verified sidecar filesystem staging immediately after a staged sidecar is written. The route-level sidecar import test now carries two distinct filesystem-backed sidecars, proves no notes/attachments/blobs are inserted, proves the just-staged sidecar is discarded before the staging helper returns, keeps staging empty, returns storage file count to the pre-abort baseline, and retries into the same clean destination with two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar partial-copy process-abort slice | 1/1 | Parent/child subprocess oracle for true process termination after exactly a 7-byte sidecar prefix is written. The child dies before integrity verification, verified staging, journal ownership, or component mutation. The parent proves zero notes/attachments/blobs, exactly one 7-byte `.blob.stage.tmp`, no journal/canonical stage/final blob, exact restoration of the pre-abort filesystem baseline after an explicit startup-equivalent sweep, and clean retry convergence to two notes, three attachment references, two blobs, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar post-staging crash slice | 1/1 | Test-only one-shot fault after verified sidecar filesystem staging and before component apply starts. The route-level sidecar import test now carries two distinct filesystem-backed sidecars, proves no notes/attachments/blobs are inserted, staged sidecars are discarded, staging is empty, storage file count returns to the pre-abort baseline, and retry into the same clean destination succeeds with two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar post-staging process-abort slice | 1/1 | Parent/child subprocess oracle for true process termination after verified sidecar staging and before component apply. The child test imports the two-sidecar archive with a `cfg(test)` abort hook and terminates the process before any component rows commit; the parent proves the destination still has zero notes/attachments/blobs, exactly two verified staged sidecars were abandoned, a normal retry imports cleanly with two blobs, three attachment references, and refcount sum 3, and an explicit staged-sidecar sweep removes the process-abort leftovers while preserving the retried final blobs. |
| Fortemi AL-SYS04 import sidecar promotion/commit process-abort slices | 4/4 | Parent/child subprocess oracles for true process termination at four two-sidecar boundaries: after the first final hard link but before its receipt (`promoting` + `pending`), after the first persisted promotion receipt (`promoted` + `pending`), after both persisted promotion receipts but before database commit (`promoted` + `promoted`, zero committed rows), and immediately after successful database commit (`promoted` + `promoted`, two notes/three attachments/two blobs committed). The importer first persists a mode-0600 schema-scoped journal with `pending`, `promoting`, `promoted`, and `already_promoted` ownership states and holds a process-scoped file lock so another live importer or startup cannot reclaim active work. Before commit, restart compensates only owned final orphans and returns storage to the exact pre-abort baseline. After commit, restart verifies and preserves both final blobs and removes only journal/lock state. Every boundary then converges under clean retry to two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar mid-promotion crash slice | 1/1 | Test-only one-shot fault after the first verified sidecar is promoted and before the second sidecar is promoted. The route-level two-sidecar import test proves the partial promotion rolls back to zero notes/attachments/blobs, compensates the already-promoted final sidecar, leaves staging empty, returns storage file count to the pre-abort baseline, and retries into the same clean destination with two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar post-promotion crash slice | 1/1 | Test-only one-shot fault after verified sidecar filesystem promotion and before import transaction commit. The route-level sidecar import test now carries two distinct filesystem-backed sidecars, proves the transaction rolls back to zero notes/attachments/blobs, compensates promoted final sidecars, discards staging, keeps preexisting imported bytes intact, and retries into the same clean destination with two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 import sidecar post-commit/pre-discard slice | 1/1 | Test-only one-shot fault after component apply returns from a committed import and before the outer journal/discard pass. The route-level two-sidecar import test proves the interruption reports an operation failure while the committed notes/attachments/blobs remain durable, both final sidecar byte streams are readable, restart recovery verifies the committed rows and exact final bytes, preserves both final blobs, removes only journal/lock state, and retry is idempotent with two blobs, three attachment references, and refcount sum 3. |
| Fortemi AL-SYS04 storage staging/promotion link-unlink crash windows | 2/2 | `stage_shard_blob_recovers_link_before_temp_unlink_crash_window` simulates an interrupted filesystem staging operation after the verified staged hard link exists but before the temp sibling is unlinked or the staging helper returns. Retried staging re-verifies the staged bytes, returns the same staged blob handle, removes the leftover temp file, does not publish final bytes, and can then promote exact bytes. `promote_staged_shard_blob_recovers_link_before_stage_unlink_crash_window` simulates an interrupted filesystem promotion after the final hard link exists but before the staged file is unlinked. Retried promotion re-verifies the final bytes, returns `AlreadyPromoted`, removes the leftover staging file, preserves the final blob bytes, and receipt-bound compensation leaves the durable final file intact. |
| Fortemi AL-SYS04 sidecar recovery-journal ownership slice | 1/1 | `shard_import_journal_persists_promotion_receipts_for_process_recovery` proves atomic journal persistence, private mode, stable blob/hash/size/path identity, pre-promotion rollback ownership, final promotion receipt, committed-byte verification, compensation/discard, repeat-safe removal, and a nonblocking lock oracle that prevents a second process from claiming a live import journal. |
| Fortemi AL-SYS04 sidecar journal persistence checkpoints | 4/4 deterministic checkpoints + 3 salvage/fail-closed oracles | `shard_import_journal_atomic_rewrite_exposes_only_complete_authority` interrupts persistence immediately after temporary-record write, temporary-file sync, atomic rename, and parent-directory sync. Before rename, the loader returns only the complete prior journal while the complete candidate remains non-authoritative; after rename, it returns only the complete candidate and no temporary sibling remains. Journal parent-directory sync failures now propagate on Unix. `shard_import_journal_salvages_complete_inactive_initial_candidate` proves a complete all-`pending` candidate is untouched while its operation lock is held and is durably promoted after lease release. `shard_import_journal_loader_fails_closed_on_orphan_temporary_rewrite` and `shard_import_journal_salvage_rejects_ambiguous_promotion_state` prove malformed or post-promotion orphan state remains on disk without authority; API startup suppresses the stale sidecar-staging sweep after any recovery failure. These are in-process deterministic checkpoint oracles on the current Unix filesystem, not true process-kill or kernel fsync-failure receipts and not non-Unix durability evidence. |
| Fortemi AL-SYS04 sidecar journal persistence process-abort slices | 4/4 | The route-level two-sidecar harness enables a `matric-db` abort hook only in `matric-api` dev/test builds and terminates a real child immediately after initial journal temporary write, temporary-file sync, atomic rename, and parent-directory sync. Every child leaves zero notes/attachments/blobs, exactly two verified staged sidecars, one complete `pending` journal candidate, and one dead-process lock. Post-rename cases recover directly. Pre-rename restart recovery acquires the dead operation's lock, validates the complete all-`pending` candidate and canonical paths, syncs it, atomically promotes it to authority, syncs the directory, and invokes normal reconciliation without test-side or operator file mutation. Every case returns to the exact pre-abort filesystem baseline and retries to two notes, three attachment references, two blobs, and refcount sum 3. This proves automatic recovery after process death immediately following completed persistence operations on the current Unix filesystem; it does not prove mid-write/mid-syscall death, kernel fsync failure, power-loss durability, or a non-Unix matrix. |
| Fortemi AL-SYS05 concurrency slice | 1/1 | Concurrent identical-byte TUS uploads, signed `2.0.0/full-v1` export, concurrent source delete plus clean-destination import, digest byte downloads, source/destination refcount oracles |
| Fortemi AL-PERF01 1 MiB receipt scaffold | 1/1 | Configurable corpus defaults to 1 MiB, records upload/download/export/import timings, throughput, RSS high-water, disk usage, archive size, digest, zero-byte-loss signed full-v1 RPO oracle, import+verify-download RTO timing, focused reproducibility metadata, package/platform/filesystem context, optional threshold-backed budget gates, and limit-plus-one TUS rejection-before-mutation; CI wrapper writes `target/al-perf01-receipt.json` |
| Fortemi AL-PERF01 bounded server asset I/O slice | 3/3 focused oracles + 1 MiB/100 MiB/max-count routes | TUS PATCH now streams request frames directly to staging and truncates to the committed offset after an overrun/body/write failure; finalization retains only an 8 KiB safety/content-detection prefix; filesystem persistence hashes and atomically copies with 64 KiB buffers while re-verifying byte count and BLAKE3 identity; unsupported backends reject instead of whole-buffer fallback. `tus_finalization_inspection_enforces_caps_and_bounds_prefix_memory`, `tus_request_body_streams_frames_and_rolls_back_overrun_residue`, and `filesystem_write_file_copies_with_bounded_identity_verification` pass. The receipt records the exact scoped contract and keeps `wholeTestProcessBoundedMemoryPassed=false` because the measurement harness itself retains corpus/archive vectors. Full-v1 filesystem sidecar export/import remains disk-spooled with 64 KiB stream buffers. This is server filesystem TUS/full-v1 sidecar evidence, not an approved whole-process RSS budget or non-filesystem/scanner claim. |
| Fortemi AL-PERF01 process-isolated TUS memory guard | 2/2 Linux child-process corpora | `al_perf01_process_isolated_streamed_tus_memory_guard` runs separate 1 MiB and 100 MiB child processes, generates 64 KiB request frames on demand, and measures `/proc/self/status` immediately around filesystem TUS PATCH/finalization. The verified wrapper run recorded 2,523,136 and 3,293,184 byte RSS high-water deltas respectively, only 770,048 bytes of growth for the 99 MiB corpus increase. Both children proved exact database hash/size/refcount, bounded-file hash/size, and zero staging residue. Non-policy guards cap the 100 MiB delta at 64 MiB and growth over the 1 MiB control at 32 MiB. `tus-bounded-memory.json` explicitly keeps approved peak-RSS, whole asset-lifecycle process, scanner, non-filesystem, and suite portability claims false. |
| Fortemi AL-PERF01 clean-checkout statistical receipt | configured five-sample CI gate + 20/20 verifier tests | CI now runs five 1 MiB lifecycle samples with `FORTEMI_AL_PERF_EXPECT_CLEAN_CHECKOUT=1`. `observed-percentiles.json` uses nearest-rank p50/p95/p99 for upload, download, signed export, clean import, recovery RTO, four throughput fields, RSS high-water delta, and storage-plus-TUS disk bytes. It binds all five source receipt names, byte counts, and SHA-256 values plus the exact commit, package, target OS/architecture, filesystem identities, and a deterministic trend key. Bundle generation rejects missing samples, dirty source receipts, invalid metrics, or an invalid exact commit; normal verification recomputes the complete artifact and rejects any drift. Unit tests cover the percentile method, missing fifth sample, dirty source, and tampered statistics. The artifact explicitly keeps approved percentile budgets, historical trend comparison, whole-lifecycle bounded memory, and suite portability false. |
| Fortemi AL-PERF01 immutable CI bundle | run 6183 / job 104296 passed | Exact commit `0ef4eccc21affe0ed4efe7b3da41ccb8b5e67f86` completed Build & Unit Test and uploaded the finalized 13-file `al-perf01-asset-lifecycle-receipts` artifact after bundle verification. This closes immutable clean-checkout reproduction publication, not approved budgets, historical trend comparison, whole-lifecycle bounded memory, or suite portability. |
| HotM live Tauri signed recovery | 1/1 clean headless receipt | HotM commit `6950658a2de62c5084c74f98454d9bcfcf80fae7` against Fortemi `0ef4eccc21affe0ed4efe7b3da41ccb8b5e67f86` uploaded and reopened 262,144 production Tauri command-core bytes, exported signed `2.0.0/full-v1`, imported into a distinct clean memory with `verify_signature=require`, and matched source/recovery byte count, SHA-256, and BLAKE3. The versioned receipt is `.aiwg/evidence/hotm-live-tauri-full-v1-recovery-receipt-2026-07-28.json` in HotM commit `1ca1c755680c9c7de1642e714556d50779a4278b`; launched GUI, interactive dialogs in the same run, immutable CI publication, non-Linux platforms, and suite portability remain false. |
| Fortemi AL-PERF01 100 MiB corpus receipt | 1/1 | `FORTEMI_AL_PERF_CORPUS_BYTES=104857600` passed by splitting the deterministic corpus into two 50 MiB full-v1 sidecar entries inside the existing shard entry limit; recorded 331 ms upload, 121 ms source download, 2452 ms export, 481 ms import, 140 ms recovery download, 622 ms import+verify RTO, 513,847,296 byte RSS high-water delta, 209,715,270 storage+TUS disk bytes, and `hundredMiBCorpusPassed=true`. The follow-up `FORTEMI_AL_PERF_CORPUS_BYTES=104857600 FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES=104857600 scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-max-corpus-100mib-receipt.json` also passed and recorded `limits.expectedMaxCorpusBytes=104857600` plus `claims.maxCorpusPassed=true`; without that explicit target the default receipt keeps `maxCorpusPassed=false`. |
| Fortemi AL-PERF01 bounded 100 MiB follow-up | 1/1 | After the TUS finalization refactor, the guarded 100 MiB run passed with two 50 MiB sidecars, exact source/recovery bytes, `boundedServerTusAndFullV1SidecarIoPassed=true`, upload/download/export/import of 355/183/2668/643 ms, 840 ms import+verify RTO, 461,062,144 byte whole-test RSS delta, and 209,715,270 storage+TUS disk bytes. The receipt explicitly keeps whole-test-process bounded memory false; server TUS and full-v1 filesystem sidecar buffers are the bounded scope. |
| Fortemi AL-PERF01 maximum-count corpus receipt | 1/1 | `FORTEMI_AL_PERF_CORPUS_BYTES=29360128 FORTEMI_AL_PERF_SEGMENT_BYTES=1048576 FORTEMI_AL_PERF_EXPECT_MAX_SIDECAR_COUNT=28` passed with 28 distinct one-MiB sidecars. Full-v1 reserves 34 component entries plus manifest/signature under the 64-entry consumer limit, leaving exactly 28 sidecar slots. The receipt records `archiveEntryCount=64`, `expectedMaxSidecarCount=28`, `maxCountCorpusPassed=true`, exact clean recovery, 509/40/844/352 ms upload/download/export/import, 417 ms RTO, 115,421,184 byte whole-test RSS delta, and 58,720,326 storage+TUS disk bytes. |
| Fortemi AL-PERF01 approved-budget receipt | conditional CI wiring | `FORTEMI_AL_PERF_EXPECT_APPROVED_BUDGETS=1` plus seven explicit `FORTEMI_AL_PERF_MAX_*` thresholds passed locally with deliberately generous thresholds, recording `approvedBudgetsPassed=true`, `rpoRtoPassed=true`, and per-budget actual/max/pass fields. `.gitea/workflows/ci-builder.yaml` now emits `approved-budget.json` only when all seven repository variables are configured; otherwise it emits `approved-budget.not-configured.json` so CI cannot claim approved budget/RPO-RTO pass from partial configuration. The per-receipt wrapper and CI bundle verifier now require an approved-budget receipt to have enabled/complete/passed gate flags, positive `max`, non-negative `actual`, and `passed=true` for all seven budget fields, plus `recovery.approvedRpoRtoBudgetPassed=true`. This proves the guard mechanics, not the final approved policy. |
| Fortemi AL-PERF01 CI artifact wiring | immutable execution passed | `.gitea/workflows/ci-builder.yaml` captures `default.json`, five clean-checkout `repeatability.json[.repeat-N.json]` samples, derived `observed-percentiles.json`, `clean-checkout.json`, `max-corpus-100mib.json`, `max-count-28-sidecars.json`, `tus-bounded-memory.json`, and exactly one approved-budget branch. It runs `scripts/ci/verify-al-perf01-receipt-bundle.py --write-manifest target/al-perf01-receipts`, which derives and exact-recomputes the statistical artifact before writing the checksum manifest, then uploads the bundle as `al-perf01-asset-lifecycle-receipts`. The verifier requires exact default/100 MiB/28-sidecar corpus roles, the 64-entry max-count boundary, scoped bounded-I/O and process-isolated TUS memory evidence, five-sample deterministic-field agreement, clean-checkout `worktreeDirty=false`, exact statistical source digests and percentile recomputation, exactly one approved-budget branch, conservative cross-client claims, and non-empty platform/filesystem identity. `manifest.json` records sorted filenames, byte counts, and SHA-256 digests and rejects missing, stale, extra, or manipulated files. Workflow YAML, shell syntax, and `python3 -m unittest tests/test_verify_al_perf01_receipt_bundle.py` (20 tests) pass locally; run 6183 job 104296 finalized the verified 13-file artifact. Final closure still requires approved percentile/RSS/disk/RPO/RTO policy and historical trend comparison. |
| HotM live browser asset receipt | 3/3 enabled, skipped by default | The no-auth receipt `HOTM_LIVE_ASSET_E2E=1 HOTM_LIVE_MEMORY=hotm_live_<ts> HOTM_API_URL=http://localhost:3000/api/v1 npm run test:e2e:live-assets` passed against a local Fortemi harness with CORS/TUS headers, rate limiting disabled, valid full-v1 signing key, and isolated archive routing. The authenticated receipt `HOTM_LIVE_ASSET_E2E=1 HOTM_LIVE_REQUIRE_AUTH=1 HOTM_LIVE_MEMORY=hotm_live_auth_<ts> HOTM_API_URL=http://127.0.0.1:3017/api/v1 VITE_API_BEARER_TOKEN=<oauth-token> npm run test:e2e:live-assets` passed against `REQUIRE_AUTH=true`, local OAuth client-credentials bearer auth, unauthenticated `/notes` 401 oracle, strict browser CORS, writable file/TUS storage, valid signing key, and isolated archive routing. The 2026-07-27 saved-file receipt `HOTM_LIVE_ASSET_E2E=1 HOTM_LIVE_MEMORY=hotm_live_filesave_<ts> HOTM_API_URL=http://127.0.0.1:3018/api/v1 npm run test:e2e:live-assets` passed against the same local no-auth harness after clicking the real attachment action-menu `Download`, capturing Playwright's browser download, saving it to the test output filesystem, and hashing the saved bytes. The enabled receipts drove HotM UI `setInputFiles` multipart upload with server byte verification, browser-origin TUS create/PATCH mismatch/HEAD resume/final PATCH/finalized GET, browser download with bearer headers, browser re-upload, server download verification, signed `2.0.0/full-v1` export inspection, clean recovery-archive import, and recovered attachment byte verification. |
| HotM live browser metrics artifact | completed enabled local artifact plus fail-closed CI guard | `ui/e2e/live/asset-lifecycle-live.spec.ts` now emits `hotm-live-asset-browser-metrics.json` during enabled live runs. The artifact records schema/profile, isolated source/recovery memory names, auth-required/token-supplied flags, UI upload and browser TUS byte counts/SHA256 values, TUS mismatch/resume/final offsets, simulated browser disconnect/resume interrupted/resume/final offsets, browser-boundary timings for UI upload, saved-file download, browser TUS upload/reupload, simulated disconnect/resume upload and download, browser/server downloads, signed full-v1 export/import, recovery polling/download, and guarded claims that keep desktop GUI and suite-wide portability false. The enabled live test now abandons a browser-origin TUS upload after a partial PATCH, resumes from a fresh HEAD/final PATCH sequence, downloads the completed attachment, and hashes the bytes. The live writer validates completed enabled-run evidence before attaching the metrics JSON: positive archive size, all required timing fields, 409 TUS mismatch, positive resume offset, final offset equal to browser TUS bytes, positive disconnect interrupted/resume offsets, disconnect final offset equal to browser TUS bytes, and true browser-boundary/TUS-resume/browser-disconnect-resume/full-v1-recovery claims. `ui/scripts/verify-live-asset-metrics.cjs` is the shared CI verifier for the manual live receipt job; it fails when a successful Playwright run does not produce exactly one completed metrics artifact, writes `metrics-validation.json`, and has Vitest coverage for pass, missing artifact, duplicate artifact, and under-asserted artifact cases. `ui/e2e/live/asset-lifecycle-receipt.spec.ts` runs without a live Fortemi API and verifies the required receipt fields, completed-run guard, and rejection of broad desktop/suite-wide claims. Local verification covered `npm run typecheck`, `npm exec vitest run -- scripts/verify-live-asset-metrics.test.js`, `npx playwright test --project=live-assets --list`, default `npm run test:e2e:live-assets` with 3 passed schema tests and 1 skipped enabled-live test, and workflow YAML parsing. The 2026-07-27 enabled local no-auth run `HOTM_LIVE_ASSET_E2E=1 HOTM_LIVE_MEMORY=hotm_live_codex_1785189409 HOTM_API_URL=http://127.0.0.1:3018/api/v1 npm run test:e2e:live-assets` passed 4/4 against a live Fortemi API with CORS/TUS headers, disabled rate limiting/scanning, `MAX_MEMORIES=1000`, isolated filesystem/TUS storage, and local full-v1 signing/trust files. `npm run verify:live-asset-metrics -- test-results test-results/live-asset-receipt` passed on the emitted artifact, which recorded 196,608 browser TUS bytes, 131,072 UI upload bytes, 12,645 archive bytes, 409 mismatch resume, 98,304-byte resume offset, 65,536-byte simulated disconnect resume offset, true browser-boundary/TUS-resume/disconnect-resume/signed-full-v1-recovery claims, and guarded false desktop/suite-wide claims. Immutable CI publication of this receipt remains pending. |
| HotM Tauri local upload command-core receipt | 2/2 | `TAURI_CONFIG='{"bundle":{"externalBin":[]}}' cargo test local_file_ -- --nocapture` passed. The Rust tests read real temporary local files, send the desktop upload core through local TUS-compatible HTTP servers, verify exact uploaded bytes, `Authorization` and `X-Fortemi-Memory` headers on create/PATCH, upload metadata, final attachment JSON, progress events, and a desktop TUS offset-conflict path where the first PATCH receives 409, the client performs authenticated HEAD, seeks to the server offset, and resumes with only the remaining bytes. This is command-core evidence only; it does not launch the Tauri GUI, use real Fortemi, or prove publisher trusted-key allowlist behavior. |
| HotM Tauri local download/save and native-dialog command receipt | 3/3 local-file + 2/2 native-dialog | `TAURI_CONFIG='{"bundle":{"externalBin":[]}}' cargo test local_file_ -- --nocapture` and `TAURI_CONFIG='{"bundle":{"externalBin":[]}}' cargo test native_ -- --nocapture` passed. The download core drives a local HTTP server, verifies the attachment download route plus `Authorization` and `X-Fortemi-Memory` headers, writes response bytes through the native filesystem path, reopens the saved file, and asserts byte count plus SHA-256 digest. HotM uses the official `tauri-plugin-dialog` API with dialogs parented to the main webview; focused native tests prove selected paths become bounded local-file metadata and downloaded bytes are saved, reopened, and hashed. Separate launched Linux evidence operated the real plugin picker/save path. This does not prove a launched GUI plus signed recovery in the same run, non-Linux platforms, or publisher trusted-key allowlist behavior. |
| HotM receipt publication wiring | configured; live publication pending | `.gitea/workflows/ui-ci.yml` has a manual `live-asset-receipt` job that runs the enabled browser lifecycle, validates exactly one completed metrics artifact, writes a digest manifest, and uploads `hotm-live-asset-browser-receipt`. `.gitea/workflows/tauri-build.yml` runs the local-file/native command-core receipt by default and now has an opt-in exact ignored Rust test against an operator-supplied signed Fortemi API. The live Tauri job requires isolated source/recovery memories, optionally proves bearer enforcement with an unauthenticated 401 oracle, validates exact clean-worktree commits and source/recovery SHA-256+BLAKE3 equality, writes and verifies an artifact manifest, and uploads `hotm-live-tauri-full-v1-recovery-receipt`. Verifier and manifest adversarial tests pass locally. Immutable enabled-live browser and live signed-Tauri artifact runs remain pending. |
| HotM direct preview/download auth guard | targeted component tests passed | Header-only bearer auth is now kept off raw media/preview URLs: `useBlobUrl` supports forced header-aware blob fetching, attachment grid thumbnails force blob fetches with `Authorization`/memory headers when bearer or memory routing is active, streaming video/audio players start in blob mode under bearer or memory routing instead of rendering headerless direct URLs, media poster fetches use the same header-aware blob path, mini-player pop-outs start in blob mode under bearer or memory routing, and subtitle/thumbnail VTT sidecar fetches include bearer and memory headers. `npm exec vitest run -- src/components/attachments/__tests__/StreamingMedia.test.tsx src/api/__tests__/attachments.test.ts src/api/__tests__/client.test.ts` passed, including new bearer tests for blob playback and sidecar fetch headers. |
| HotM trusted publisher pre-upload guard | targeted API tests passed | `uploadKnowledgeShard` now supports explicit `trustedPublisherKeyIds` and the `hotm_trusted_shard_publisher_key_ids` local setting for signed `2.0.0/full-v1` recovery. When configured, HotM streams/inspects `signature.json`, rejects an unlisted `signer.key_id` before any dry-run or mutating upload request, and still requires Fortemi `verify_signature=require` dry-run before mutation for trusted signers. `npm exec vitest run -- src/api/__tests__/backup.test.ts src/api/__tests__/knowledgeShard.test.ts` passed. This is a HotM pre-upload guard; server trust-store enforcement and native desktop GUI journeys remain separate evidence. |
| Fortemi full-v1 route regression | 1/1 | Existing signed `2.0.0/full-v1` route roundtrip still passes after filtering members whose parent embedding set is not exported and including configs referenced by exported embedding sets |
| Fortemi blob storage/refcount regression | expanded focused shard-blob/refcount filters | Existing blob, staged sidecar, staging link/temp-unlink crash-window recovery, promotion, promotion link/unlink crash-window recovery, staging sweep, and refcount tests still pass after same-byte concurrent insert handling |

Focused commands:

```bash
cargo fmt -p matric-db -p matric-api
cargo test -p matric-api al_sys
cargo test -p matric-api shard_sidecar -- --nocapture
cargo test -p matric-db promote_staged_shard_blob -- --nocapture
cargo test -p matric-db file_storage -- --nocapture
bash -n scripts/ci/verify-bidirectional-asset-lifecycle.sh
FORTEMI_AL_PERF_REPETITIONS=2 \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-platform-repeatability-receipt.json
scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-platform-receipt.json
scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-wrapper-receipt.json
FORTEMI_AL_PERF_CORPUS_BYTES=104857600 \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-100mib-receipt.json
FORTEMI_AL_PERF_CORPUS_BYTES=104857600 \
  FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES=104857600 \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-max-corpus-100mib-receipt.json
FORTEMI_AL_PERF_EXPECT_APPROVED_BUDGETS=1 \
  FORTEMI_AL_PERF_MAX_UPLOAD_MILLIS=60000 \
  FORTEMI_AL_PERF_MAX_DOWNLOAD_MILLIS=60000 \
  FORTEMI_AL_PERF_MAX_EXPORT_MILLIS=60000 \
  FORTEMI_AL_PERF_MAX_IMPORT_MILLIS=60000 \
  FORTEMI_AL_PERF_MAX_RECOVERY_RTO_MILLIS=60000 \
  FORTEMI_AL_PERF_MAX_RSS_DELTA_BYTES=1073741824 \
  FORTEMI_AL_PERF_MAX_STORAGE_AND_TUS_DISK_BYTES=1073741824 \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-budget-gate-receipt.json
cargo test -p matric-api shard_full_v1_route_round_trip_preserves_every_component_and_required_blob
cargo test -p matric-db blob
python3 - <<'PY'
from pathlib import Path
import yaml
yaml.safe_load(Path(".gitea/workflows/ci-builder.yaml").read_text())
PY
bash -n scripts/ci/verify-asset-lifecycle-perf-receipt.sh
python3 -m py_compile \
  scripts/ci/verify-al-perf01-receipt-bundle.py \
  tests/test_verify_al_perf01_receipt_bundle.py
python3 -m unittest tests/test_verify_al_perf01_receipt_bundle.py
scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-clean-gate-default.json
# Dirty local worktree fail-closed check:
FORTEMI_AL_PERF_EXPECT_CLEAN_CHECKOUT=1 \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh /tmp/fortemi-al-perf01-clean-gate-dirty-expected-fail.json
(cd ../HotM/ui && npm run test:e2e:live-assets)
(cd ../HotM/ui && npx playwright test --project=live-assets --list)
(cd ../HotM/ui && npm run typecheck)
(cd ../HotM/ui && npm exec vitest run -- scripts/verify-live-asset-metrics.test.js)
(cd ../HotM/ui && npm exec vitest run -- scripts/verify-tauri-command-core-receipt.test.js)
(
  cd ../HotM/ui && \
  HOTM_LIVE_ASSET_E2E=1 \
  HOTM_LIVE_MEMORY=hotm_live_<timestamp> \
  HOTM_API_URL=http://localhost:3000/api/v1 \
  npm run test:e2e:live-assets
)
(
  cd ../HotM/ui && \
  HOTM_LIVE_ASSET_E2E=1 \
  HOTM_LIVE_REQUIRE_AUTH=1 \
  HOTM_LIVE_MEMORY=hotm_live_auth_<timestamp> \
  HOTM_API_URL=http://127.0.0.1:3017/api/v1 \
  VITE_API_BEARER_TOKEN=<oauth-token> \
  npm run test:e2e:live-assets
)
(
  cd ../HotM/ui && \
  HOTM_LIVE_ASSET_E2E=1 \
  HOTM_LIVE_MEMORY=hotm_live_filesave_<timestamp> \
  HOTM_API_URL=http://127.0.0.1:3018/api/v1 \
  npm run test:e2e:live-assets
)
(cd ../HotM/ui && npm exec vitest run -- src/services/__tests__/tusUploader.test.ts src/services/__tests__/uploadStore.test.ts src/api/__tests__/auth.test.ts src/api/__tests__/attachments.test.ts)
(cd ../HotM/ui && npm exec vitest run -- src/components/attachments/__tests__/StreamingMedia.test.tsx src/api/__tests__/attachments.test.ts src/api/__tests__/client.test.ts)
(cd ../HotM/ui && npm exec vitest run -- src/api/__tests__/backup.test.ts src/api/__tests__/knowledgeShard.test.ts)
(cd ../HotM/ui && node -e "import fs from 'node:fs'; import YAML from 'yaml'; for (const f of ['../.gitea/workflows/ui-ci.yml','../.gitea/workflows/tauri-build.yml']) { YAML.parse(fs.readFileSync(f,'utf8')); console.log(f + ' ok'); }")
(cd ../HotM/ui/src-tauri && cargo fmt && TAURI_CONFIG='{"bundle":{"externalBin":[]}}' cargo test local_file_ -- --nocapture)
(cd ../HotM/ui/src-tauri && TAURI_CONFIG='{"bundle":{"externalBin":[]}}' cargo test native_ -- --nocapture)
(
  cd ../HotM/ui/src-tauri && \
  TAURI_CONFIG='{"bundle":{"externalBin":[]}}' \
  HOTM_LIVE_TAURI_API_URL=http://127.0.0.1:<port>/api/v1 \
  HOTM_LIVE_TAURI_SOURCE_MEMORY=hotm_live_<source> \
  HOTM_LIVE_TAURI_RECOVERY_MEMORY=hotm_live_<recovery> \
  HOTM_LIVE_TAURI_RECEIPT_PATH=<receipt.json> \
  cargo test tests::live_fortemi_tauri_local_file_full_v1_recovery_receipt \
    -- --ignored --exact --nocapture
)
(cd ../HotM/ui && npm run verify:live-tauri-full-v1-receipt -- <receipt.json>)
```

This continuation does not yet prove process termination in the middle of sidecar read/write
syscalls, journal writes, or other kernel syscalls, kernel write/fsync failure or power loss,
in-flight commit acknowledgement ambiguity, full Tauri desktop GUI local-file journeys, the
approved performance corpus, approved peak RSS budgets, approved RPO/RTO budgets, immutable CI
publication of the enabled HotM live browser or live signed-Tauri receipts.
Fortemi clean-checkout reproduction is immutable in run 6183's finalized AL-PERF01 artifact.
HotM live receipt publication wiring is present, but completed enabled-live artifacts are pending.

## Current Requirement Posture

### Proven

- Content-addressed filesystem persistence, deduplication, refcount behavior, orphan cleanup, and
  scan-gated download.
- Exact `2.0.0/full-v1` component and sidecar inventory, signatures, clean atomic recovery,
  semantic/byte convergence, repeat import, fail-before-mutation, rollback, version/profile
  rejection, receipt-backed advertisements, and AIWG bridge boundaries.
- Parser/request resource limits, integrity checks, tested redaction boundaries, and profile-scoped
  claim guards.
- Fortemi route-level TUS resume after an offset conflict, post-commit restart survival,
  offset-committed TUS finalization retry after a crash-window state, during-staging import sidecar
  discard/retry, post-staging import sidecar discard/retry, mid-promotion partial compensation/retry,
  post-promotion import sidecar transaction abort compensation/retry, post-commit/pre-discard
  committed import journal recovery plus idempotent retry, a true subprocess abort after verified
  sidecar staging and before component apply with retry plus staged-leftover sweep oracles, a true
  subprocess abort after exactly a 7-byte sidecar copy prefix with a single unverified temporary
  file, zero component mutation, startup-equivalent sweep, and clean retry, a true subprocess abort
  after the first final sidecar link and before its promotion receipt, after the first persisted
  receipt, after both persisted receipts before transaction commit, and immediately after successful
  transaction commit, each with schema-scoped restart reconciliation plus clean retry, helper-level recovery from
  interrupted staging where both verified stage and temp filesystem links exist, helper-level
  recovery from an interrupted promotion where both final and staged filesystem links exist, clean destination
  recovery with two distinct sidecars and three attachment references, same-byte concurrent upload
  deduplication, delete/import concurrency, and exact digest/refcount oracles for the focused
  AL-SYS04/05 slices.

### Partial

- HotM browser `setInputFiles`, TUS client state, browser download, browser re-upload, and signed
  full-v1 export/import recovery now cross a live Fortemi network boundary in the opt-in
  `live-assets` run, including a local OAuth bearer-authenticated browser receipt and clean
  recovery-archive byte verification. A no-auth browser receipt now also clicks the real attachment
  action-menu `Download`, saves the Playwright browser download to the test output filesystem, and
  verifies the saved-file SHA256. Enabled live browser runs are now instrumented to emit a
  per-test metrics artifact with browser-boundary timings, TUS resume offsets, and a simulated
  browser disconnect/resume upload that resumes from a fresh HEAD/final PATCH sequence before
  download+hash verification. Both the Playwright writer and manual CI receipt job fail closed when
  completed metrics evidence is missing or under-asserted; immutable enabled metrics publication is
  still pending. The Tauri
  desktop local-file upload command core now has Rust receipts for real local bytes, auth/memory
  headers, TUS create/PATCH, progress, and
  offset-conflict HEAD resume with seek-to-server-offset behavior against local mock servers. The
  Tauri desktop local-download command core now has a Rust receipt
  for authenticated server-origin bytes saved to a native filesystem path, reopened from disk, and
  SHA-256 verified, and the existing UI download handlers route through that Tauri save path before
  falling back to browser blob download. The Tauri native picker/save commands use the official
  parented `tauri-plugin-dialog` API and have focused selected-path/save/reopen/hash tests plus a
  separate launched Linux receipt. A clean-worktree headless receipt now drives those production
  upload/download cores against exact live Fortemi through filesystem TUS, signed `2.0.0/full-v1`,
  trust-required clean-memory import, and exact reopened source/recovery bytes. A launched GUI plus
  interactive native dialogs and signed recovery in the same run, bearer-authenticated publisher
  mode, non-Linux platforms, and immutable enabled-live publication remain open.
- Authorized download is covered through the enabled live browser scaffold with bearer headers and
  the Tauri local-download command-core mock-server receipt.
  Direct preview/media URLs now avoid headerless bearer leakage/failure by using header-aware blob
  fetches for thumbnails, posters, media playback, mini-player pop-outs, subtitle VTT, and
  thumbnail VTT when bearer auth or memory routing is active. The headless production Tauri
  command-core receipt proves the real Fortemi-to-local-file boundary; launched-GUI and
  bearer-authenticated desktop cells remain open.
- Streaming implementations exist without an approved peak-RSS receipt.
- Security still lacks the desktop/local-file authenticated lifecycle and trusted publisher-key
  allowlist proof across a real native desktop/Fortemi journey. Browser bearer lifecycle coverage
  is limited to the local OAuth client-credentials harness plus targeted direct-preview/media header
  tests. HotM now has a targeted pre-upload allowlist guard for signed `full-v1` recovery that
  rejects untrusted `signature.json` signer key IDs before any upload request when configured.
- Scalability, observability, degraded-mode availability, and reproducibility have component or
  current-workspace evidence but not complete acceptance receipts.
- AL-SYS04 is only partially satisfied: post-commit restart, the TUS offset-committed finalization
  crash window, the import sidecar during-staging abort window, true process death after an exact
  7-byte sidecar copy prefix, the import sidecar post-staging/pre-apply abort window, real process aborts at first-promotion
  final-link/pre-receipt, first-promotion post-receipt, and all-promotions
  post-receipt/pre-commit plus post-commit boundaries with locked-journal restart recovery, the import sidecar
  mid-promotion partial-compensation window, the import sidecar post-promotion/pre-commit abort
  window, the import sidecar post-commit/pre-discard boundary, the storage staging
  link-before-temp-unlink crash window, and the storage promotion link-before-stage-unlink crash
  window are covered with route/helper oracles. Deterministic journal rewrite interruption
  checkpoints after temporary write, file sync, rename, and Unix parent-directory sync prove that
  only a complete prior or complete new journal becomes authoritative; orphan temporary state
  fails recovery without deletion and suppresses startup's stale staging sweep. Real child-process
  aborts immediately after all four persistence operations prove direct post-rename recovery and
  lock-guarded automatic complete-candidate salvage before pre-rename recovery. Process termination in the
  middle of sidecar read/write syscalls or journal writes/syscalls, kernel-level write/fsync
  failure or power loss, the in-flight commit acknowledgement
  window, non-Unix directory durability, and all supported platform/filesystem combinations remain
  unproven.
- AL-PERF01 has a finalized immutable CI artifact for the default 1 MiB corpus, throughput fields,
  limit-plus-one rejection, a zero-byte-loss signed full-v1 RPO oracle, import+verify-download RTO
  timing, focused reproducibility metadata, package/platform/filesystem context, five-sample
  clean-checkout p50/p95/p99 observations with exact source digests and a stable trend key, and optional environment-driven budget
  gates. Server-side filesystem TUS request/finalization and full-v1 sidecar I/O now have explicit
  8 KiB prefix and 64 KiB stream/copy bounds with fail-closed receipt validation. A separate Linux
  child-process receipt shows only 770,048 bytes of RSS high-water growth from the 1 MiB control to
  the 100 MiB TUS corpus, while whole asset-lifecycle process bounded memory remains false and still
  needs an approved peak-RSS budget. The 100 MiB deterministic
  corpus passes as two 50 MiB sidecar entries. The exact maximum-count corpus passes with 28 distinct
  one-MiB sidecars and all 64 allowed full-v1 archive entries. The max-corpus claim is threshold-backed by
  `FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES`: the claim remains false by default and passed locally
  for a 100 MiB target. The clean-checkout claim is now gated by
  `FORTEMI_AL_PERF_EXPECT_CLEAN_CHECKOUT=1`. Run 6183 job 104296 emitted five exact-commit samples,
  observed percentiles, scale/memory receipts, and a verified manifest in the finalized 13-file
  `al-perf01-asset-lifecycle-receipts` artifact. The approved-budget gate
  now has conditional CI wiring: all seven `FORTEMI_AL_PERF_MAX_*` variables must be present before
  CI emits `approved-budget.json`, otherwise a not-configured receipt is uploaded. The CI artifact
  directory now has a separate bundle verifier with unit coverage for the configured and
  not-configured budget branches, missing clean-checkout receipt, dirty clean-checkout receipt,
  double budget-branch ambiguity, approved-budget missing-max/failed-RTO receipts, missing
  platform/filesystem context, false max-corpus claim, missing/understated TUS memory receipt,
  missing/dirty statistical samples, tampered p50/p95/p99 output, missing manifest, stale manifest,
  and write-manifest mode. The generic budget-gate path has been
  exercised with deliberately generous local thresholds, but the approved threshold values,
  historical comparison, whole-lifecycle bounded-memory policy, and approved RPO/RTO budgets
  remain open.
  HotM client-boundary metrics are proven for a local enabled no-auth browser run, with fail-closed
  schema and CI validation for the enabled live browser metrics file. Immutable CI publication of
  that receipt remains pending. A separate clean-worktree headless HotM receipt now proves the
  production Tauri local-file command cores against live Fortemi signed `2.0.0/full-v1` recovery
  with exact reopened bytes; it explicitly does not claim launched GUI or immutable CI execution.

### Open

- A launched Tauri GUI plus interactive native file selection/save and signed clean-memory recovery
  in the same run.
- Successful immutable CI artifact runs for the enabled HotM live browser receipt and the new live
  Tauri signed-recovery receipt.
- Process-kill crash recovery in the middle of sidecar read/write syscalls, journal writes, or
  other kernel syscalls, kernel write/fsync failure or power loss, and in-flight commit
  acknowledgement ambiguity, beyond the verified automatic pre-rename complete-candidate salvage,
  exact-prefix sidecar-copy abort, post-staging process abort, four journal-operation process
  aborts, three promotion-boundary process aborts, immediate post-commit process abort,
  deterministic journal persistence checkpoints, and route/helper crash-window slices.
- Approved maximum-size policy beyond the local 100 MiB target, if required.
- Approved threshold set and approved RPO/RTO receipt publication.
- Immutable publication of the enabled live browser disconnect/resume artifact against Fortemi.
- Declared platform/filesystem matrix and approved RPO/RTO objectives.

## Required System Tests

| ID | Owner | Required flow | Green oracle |
|---|---|---|---|
| AL-SYS01 | HotM + Fortemi | Launch isolated Fortemi and HotM desktop uploader; upload deterministic file over real TUS; export signed `2.0.0/full-v1`; stop source; import clean; download locally. | Partial: HotM's clean-worktree ignored Rust receipt drives the production Tauri local-file upload/download cores against exact live Fortemi, filesystem TUS, signed `2.0.0/full-v1`, `verify_signature=require` import into a distinct clean memory, exact reopened source/recovery byte count, SHA-256 and BLAKE3, and mandatory isolated-memory cleanup. Separate launched Linux evidence proves the supported Tauri dialog plugin with a real native picker and save. A launched GUI plus interactive picker/save and signed recovery in one run, bearer-authenticated publisher mode, non-Linux platforms, and immutable CI publication of this live receipt remain required. |
| AL-SYS02 | HotM + Fortemi | Playwright `setInputFiles` against HotM connected to live Fortemi; export/import clean; save download to a file. | Partial: `ui/e2e/live/asset-lifecycle-live.spec.ts` passed enabled UI `setInputFiles` upload plus browser-origin TUS/download/reupload/signed full-v1 export, clean recovery-archive import, recovered-byte verification, and browser saved-file hash verification against live local Fortemi archives; both no-auth and local OAuth bearer-authenticated modes pass; unauthenticated `/notes` returns 401 in the authenticated harness; the enabled test now records and verifies a browser-origin simulated disconnect/resume path in a completed local metrics artifact; Tauri command-boundary native save receipt passes, but launched Tauri desktop/native file-save receipt remains required. |
| AL-SYS03 | HotM + Fortemi | Seed server attachment; download to clean filesystem; upload to clean server and independently recover source via signed `full-v1`. | Partial: the enabled live browser scaffold covers authenticated server-origin browser download, browser re-upload, browser saved-file hash verification, clean recovery-archive import, and recovered-byte verification; targeted component tests cover bearer-auth direct-preview/media fallback to header-aware blob paths; the Tauri download command core covers authenticated server-origin bytes saved to and reopened from a native filesystem path with SHA-256 verification; HotM's pre-upload full-v1 allowlist guard rejects untrusted `signature.json` signer key IDs before any upload request when configured; native desktop GUI save/open against real Fortemi and end-to-end authenticated publisher trust remain required. |
| AL-SYS04 | Fortemi | Repeat lifecycle with restart after upload/import commit and termination during staging/promotion. | Partial: post-commit restart is covered by `al_sys04_live_tus_restart_and_clean_full_v1_recovery_preserves_asset_bytes`; offset-committed TUS finalization retry is covered by `al_sys04_tus_crash_after_offset_commit_retries_finalization_without_partial_state`; import sidecar during-staging abort discard/retry, true subprocess abort after an exact 7-byte sidecar copy prefix with one unverified temp, zero component mutation, startup-equivalent sweep, and clean retry, post-staging/pre-apply abort discard/retry, true subprocess abort after post-staging/pre-apply with retry plus staged-leftover sweep, true subprocess aborts immediately after initial journal temporary write, temporary-file sync, atomic rename, and parent-directory sync with direct or lock-guarded automatic candidate recovery, true subprocess abort after the first final promotion link before its receipt, after the first persisted receipt, after both persisted receipts before commit, and immediately after successful commit with locked-journal restart reconciliation/retry, mid-promotion partial compensation/retry, post-promotion/pre-commit abort compensation/retry, and post-commit/pre-discard committed-journal recovery/idempotent retry are covered by `shard_optional_sidecars_round_trip_and_fail_without_partial_storage`; `shard_import_journal_persists_promotion_receipts_for_process_recovery` proves durable receipt ownership and active-import exclusion; `shard_import_journal_atomic_rewrite_exposes_only_complete_authority` covers four deterministic journal rewrite checkpoints and strict Unix directory sync; `shard_import_journal_salvages_complete_inactive_initial_candidate`, `shard_import_journal_loader_fails_closed_on_orphan_temporary_rewrite`, and `shard_import_journal_salvage_rejects_ambiguous_promotion_state` cover inactive initial candidate salvage and unknown or ambiguous temporary state retention; `stage_shard_blob_recovers_link_before_temp_unlink_crash_window` covers the helper-level staging state where verified staged bytes are linked but temp cleanup/helper return was interrupted; `promote_staged_shard_blob_recovers_link_before_stage_unlink_crash_window` covers the helper-level promotion state where final bytes are linked but staging cleanup was interrupted. Process termination in the middle of sidecar read/write syscalls, journal writes, or other kernel syscalls, kernel write/fsync failure or power loss, in-flight commit acknowledgement ambiguity, non-Unix directory durability, and the supported platform/filesystem matrix remain required. |
| AL-SYS05 | Fortemi | Concurrent identical-byte upload, archive import, and selected-reference deletion. | Covered locally by `al_sys05_same_byte_upload_import_and_reference_delete_preserve_refcounts`; final closure still requires immutable CI receipt publication. |
| AL-PERF01 | Fortemi + HotM | Execute approved size/count corpus with RSS, disk, latency, and throughput capture. | Partial: `al_perf01_configurable_corpus_records_receipt_and_limit_plus_one_gate` plus `scripts/ci/verify-asset-lifecycle-perf-receipt.sh` records the default 1 MiB, bounded 100 MiB, and exact 28-sidecar/64-entry maximum-count Fortemi corpora, throughput, RSS/disk, limit-plus-one TUS mutation gate, scoped server filesystem TUS/full-v1 sidecar buffer bounds, package/platform/filesystem context, five clean-checkout samples, nearest-rank p50/p95/p99 observations, exact statistical source digests, a commit/environment trend key, zero-byte-loss signed full-v1 RPO, recovery RTO, optional threshold-backed budget gates, and explicit max-size/max-count gates. `al_perf01_process_isolated_streamed_tus_memory_guard` plus `scripts/ci/verify-tus-bounded-memory-receipt.sh` separately proves low-growth Linux child-process RSS for 1 MiB versus 100 MiB filesystem TUS PATCH/finalization without broadening the whole-lifecycle claim. Run 6183 job 104296 on exact commit `0ef4eccc21affe0ed4efe7b3da41ccb8b5e67f86` passed and finalized the verified 13-file `al-perf01-asset-lifecycle-receipts` artifact. HotM live browser client-boundary metrics and the clean headless Tauri signed-recovery receipt passed locally. Approved percentile latency/throughput/peak-RSS/disk/RPO/RTO policy values, whole asset-lifecycle process bounded-memory proof under those budgets, historical trend comparison, any approved maximum beyond 100 MiB, non-filesystem/scanner memory evidence, and immutable HotM live receipt publication remain required. |

## Required Issue Graph

1. [Fortemi #1093](https://git.integrolabs.net/Fortemi/fortemi/issues/1093) owns the live test
   environment, restart/crash recovery, and refcount concurrency gates.
2. [Fortemi #1094](https://git.integrolabs.net/Fortemi/fortemi/issues/1094) owns lifecycle memory,
   throughput, scale, reproducibility, and timed RPO/RTO receipts after #1093.
3. [HotM #283](https://git.integrolabs.net/Fortemi/HotM/issues/283) owns real desktop/browser
   upload/download journeys, reverse processing, and TUS disconnect/resume behavior after #1093.
4. All three issues link each other, Fortemi #1081, this plan, the authority commit, and required
   final immutable CI receipts.
5. #1081 remains open until independent audit acceptance; issue closure alone does not authorize
   broad portability language.

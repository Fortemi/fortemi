---
title: Bidirectional Asset Lifecycle Audit Plan
status: required-current-audit
date: 2026-07-27
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
because no single automated test currently drives either complete user lifecycle:

- HotM desktop/browser local file -> real TUS/network -> live Fortemi -> signed `full-v1` ->
  clean Fortemi -> real local download; or
- existing server asset -> real local download -> clean server re-upload/recovery -> exact
  byte/metadata comparison.

Fortemi's strongest route cell uses PostgreSQL plus temporary filesystem storage and proves signed
export, required-signature validation, clean repeated import, semantic re-export, tamper rejection,
rollback, and exact required sidecar bytes. HotM's upload, attachment, backup, and shard tests are
currently mock/component tests around that live boundary.

## Focused Current-Head Evidence

The 2026-07-27 focused run passed **135/135** selected tests:

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
claim to execute the open live-client, restart, concurrency, or performance scenarios.

## Current Requirement Posture

### Proven

- Content-addressed filesystem persistence, deduplication, refcount behavior, orphan cleanup, and
  scan-gated download.
- Exact `2.0.0/full-v1` component and sidecar inventory, signatures, clean atomic recovery,
  semantic/byte convergence, repeat import, fail-before-mutation, rollback, version/profile
  rejection, receipt-backed advertisements, and AIWG bridge boundaries.
- Parser/request resource limits, integrity checks, tested redaction boundaries, and profile-scoped
  claim guards.

### Partial

- HotM local-file metadata, TUS client state, and invocation wiring do not cross a live Fortemi
  network boundary.
- Authorized download is tested below the real client-to-local-file boundary.
- Streaming implementations exist without an approved peak-RSS receipt.
- Security lacks one live authenticated lifecycle.
- Scalability, observability, degraded-mode availability, and reproducibility have component or
  current-workspace evidence but not complete acceptance receipts.

### Open

- Browser/desktop convergence and server-origin local return.
- Process-restart durability and process-kill crash recovery.
- Concurrent upload/import/delete refcount races.
- Quantitative performance and maximum-corpus completion.
- Live disconnect/resume against Fortemi.
- Declared platform/filesystem matrix and timed RPO/RTO recovery.

## Required System Tests

| ID | Owner | Required flow | Green oracle |
|---|---|---|---|
| AL-SYS01 | HotM + Fortemi | Launch isolated Fortemi and HotM desktop uploader; upload deterministic file over real TUS; export signed `2.0.0/full-v1`; stop source; import clean; download locally. | Source, sidecar, destination storage, and downloaded file have identical bytes, BLAKE3, and length; metadata/relationships match. |
| AL-SYS02 | HotM + Fortemi | Playwright `setInputFiles` against HotM connected to live Fortemi; export/import clean; save download to a file. | Same byte and semantic oracle as AL-SYS01 across browser and network boundaries. |
| AL-SYS03 | HotM + Fortemi | Seed server attachment; download to clean filesystem; upload to clean server and independently recover source via signed `full-v1`. | Re-upload and shard destinations agree on bytes, digest, length, safe filename/media metadata, and ownership. |
| AL-SYS04 | Fortemi | Repeat lifecycle with restart after upload/import commit and termination during staging/promotion. | Committed state survives; uncommitted staging is cleaned; no partial state or refcount drift. |
| AL-SYS05 | Fortemi | Concurrent identical-byte upload, archive import, and selected-reference deletion. | One content identity, correct final refcount, exact surviving downloads, no premature/orphan deletion. |
| AL-PERF01 | Fortemi + HotM | Execute approved size/count corpus with RSS, disk, latency, and throughput capture. | Budgets pass and limit-plus-one inputs fail before mutation. |

## Required Issue Graph

1. Fortemi owns the live test environment, restart/crash, refcount concurrency, and performance
   instrumentation.
2. HotM owns real desktop/browser upload/download journeys and TUS disconnect/resume behavior.
3. Both issues must link each other, Fortemi #1081, this plan, the exact implementation commits,
   and final immutable CI receipts.
4. #1081 remains open until independent audit acceptance; issue closure alone does not authorize
   broad portability language.

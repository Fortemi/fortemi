# Code-to-Docs Sync Report

**Date:** 2026-07-29  
**Direction:** code-to-docs  
**Workflow:** `release-doc-sync`  
**Repository:** `Fortemi/fortemi`  
**Baseline:** `v2026.7.12` (`a036f723a3308c377a636532053505d5fcd`)
to `5bfecfe8d55caced3652a225a60f5217b4c192e8`

## Summary

The bounded dry-run started from a clean worktree and compared delivered code,
contracts, CI, and documentation since `v2026.7.12`. It found eight
high-confidence documentation drifts. The applied changes distinguish the
default `1.2.0/core-v1` tuple from the only advertised schema-2 opt-in,
`2.0.0/full-v1`; record Gitea run 6393 as the passing three-platform
authority-to-consumer aggregate; and link Windows-only follow-up issue #1096.

The synced docs retain Fortemi as schema/API/runtime authority,
`@fortemi/core` as a reusable conformance consumer, and HotM as an application
consumer. They preserve the exact claim boundary: Linux x86_64, Linux arm64,
and macOS arm64 at receipt-bound revisions. Windows, launched GUI/native
dialogs, architectures outside the matrix, universal portability, complete
backup, and one shared suite schema remain unproven. Parent issue #1081 remains
`NO-GO` pending independent audit.

No runtime code, machine contract manifest, release version field, tag, commit,
or remote branch was changed.

## Scope

- Read repository policies, release configuration, ADR-102, ADR-104, and the
  suite authority configuration, ADR, and 2026-07-17 audit.
- Compared `v2026.7.12..HEAD`, current schema registries, route selectors,
  conformance matrix, platform workflow, verifier tests, and run 6393.
- Limited edits to documentation and `.aiwg/reports/` state.

## Findings

| ID | Finding | Resolution |
|---|---|---|
| DS-01 | Knowledge Shard README still called schema 2 implementation-pending. | Record exact `2.0.0/full-v1` as receipt-bound opt-in. |
| DS-02 | README said cross-repository receipts remained pending without separating 1.2 self-route and schema-2 evidence. | Separate server self-route evidence from the immutable schema-2 cells. |
| DS-03 | Backup and migration docs described schema-2 tuples too broadly. | Name only `2.0.0/full-v1` as advertised; keep core/record unadvertised. |
| DS-04 | Release-facing changelog omitted completed platform and recovery work. | Add bounded platform, sidecar, and crash-recovery entries. |
| DS-05 | ADR-102 retained stale 1.x gap language around later schema-2 evidence. | Qualify 1.x/default gaps and add the bounded run 6393 result. |
| DS-06 | ADR-103 still framed delivered receipt work as downstream and implied matrix completion could unlock broad portability. | Record delivered exact-cell evidence and preserve the #1081 block. |
| DS-07 | ADR-104 and suite README lacked final aggregate evidence and the Windows story. | Add run 6393 and link open issue #1096. |
| DS-08 | The ADR index omitted ADR-104 and showed ADR-102/103 as target-only. | Add ADR-104 and current accepted statuses. |

## Changed Files

- `CHANGELOG.md`
- `contracts/knowledge-shard/README.md`
- `contracts/suite-conformance/README.md`
- `docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md`
- `docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md`
- `docs/architecture/adr/ADR-104-supported-platform-suite-conformance.md`
- `docs/architecture/adr/README.md`
- `docs/content/backup.md`
- `docs/content/shard-migration.md`
- `.aiwg/reports/doc-sync-20260729-code-to-docs.md`
- `.aiwg/reports/doc-sync-last-run.json`

## Validation

- `python3 scripts/ci/verify-knowledge-shard-presence.py`: passed; 220 fields
  and 22 canonical cases verified.
- `python3 scripts/ci/verify-suite-platform-matrix.py manifest`: passed.
- `python3 scripts/ci/verify-adr-rebaseline.py`: passed.
- `git diff --check`: passed.
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract --
  --profile=hosted_strict`: passed after release preparation replaced the
  credential-shaped executable DSN literal at
  `scripts/ci/run-suite-platform-contract.sh:170` with structured runtime
  formatting; the blocking rerun reported zero findings.

## Human Review

- Independent audit and any change to Fortemi #1081 remain human-owned.
- Fortemi #1096 requires a maintained native Windows x86_64 execution
  authority before Windows can enter the supported matrix.
- Release version selection, announcement creation, tagging, and publication
  are intentionally outside this documentation-only run.

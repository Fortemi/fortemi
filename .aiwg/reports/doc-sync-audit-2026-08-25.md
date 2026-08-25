# Documentation sync audit — 2026-08-25

- Direction: `code-to-docs`
- Baseline: `v2026.7.22..9a43c755`
- Scoped lanes: product/API/operator documentation and v2026.7.23 release coverage
- Dry-run evidence:
  - `.aiwg/working/doc-sync/product-docs-audit-v2026.7.22-to-9a43c755-2026-08-25.md`
  - `.aiwg/working/doc-sync/release-docs-v2026.7.23-dry-run-20260825.md`

## Findings by severity

### Blockers

- The release version authorities remain at 2026.7.22, `Unreleased` is empty,
  and the v2026.7.23 announcement does not yet exist. These are intentionally
  deferred to the version/changelog release gates after branch CI is green.

### High — resolved in documentation sync

- Replaced MCP troubleshooting commands and transcripts that could reveal a
  client secret; documented presence-only checks and masked registration logs.
- Distinguished Community Edition OAuth/API keys from the internal external-
  OIDC hosted profile and documented its fail-closed configuration bounds.
- Added a bounded hosted startup checklist covering distinct database roles,
  durable audit, AWS KMS, Redis admission, scanning, and destination policy.
- Corrected Community Edition memory-boundary claims versus hosted forced-RLS
  tenant boundaries and added explicit hosted-readiness claim limits.
- Corrected hosted quota documentation from future-only language to the live
  feature-gated Redis behavior, while retaining plan/billing limitations.
- Defined semantic chunk sizes, overlaps, and source offsets as UTF-8 bytes and
  documented exact newline/source-boundary preservation.

### Medium — resolved in documentation sync

- Documented Streamable HTTP stale-session 404/reinitialization behavior.
- Added hosted stored-credential/inference route summaries and runbook links.
- Documented the non-secret 16 MiB rustc stack guard and Rust 1.92 builder pin.
- Added central configuration entries for hosted circuit-breaker and rewrap
  controls and linked the internal hosted runbooks from README.

## Files changed

- `.env.example`
- `README.md`
- `docs/content/api.md`
- `docs/content/authentication.md`
- `docs/content/configuration.md`
- `docs/content/container-release-evidence.md`
- `docs/content/embedding-pipeline.md`
- `docs/content/mcp-deployment.md`
- `docs/content/troubleshooting.md`

## Human/release-owner items

- Prepare the v2026.7.23 version bump, changelog, announcement, AsyncAPI
  snapshot, and documentation shard only after the branch CI gate passes.
- The announcement must name all six forward migrations and the
  restore-to-separate-destination rollback boundary. It must not claim hosted
  readiness, complete backup, full portability, or unqualified suite parity.
- If Knowledge Shard behavior is mentioned, name the exact supported profile;
  this release does not change the Knowledge Shard contract.

## Validation

- `git diff --check`
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
- scoped Markdown link/path checks through the project documentation build


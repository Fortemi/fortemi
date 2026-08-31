# Documentation sync audit — v2026.8.3 — 2026-08-31

- Direction: `code-to-docs`
- Baseline: `v2026.8.2..7cd7feb4426a61b63cbb4c1e43eee252018623cc` plus the release working tree
- Scoped lanes: release, semantic chunking, customer retest, redaction, and versioned contract documentation
- Dry-run evidence:
  - `.aiwg/working/doc-sync/release-v2026.8.3-dry-run-20260831.md`

## Findings by severity

### Blockers

- None.

### High

- None.

### Advisory

- The documentation build retains one pre-existing `risky-raw-html` warning in
  `docs/content/job-monitoring.md:65`. It is unrelated to this release delta
  and did not fail the build.

## Files changed

- No additional product-documentation drift fixes were required.
- Release authorities intentionally prepared by the surrounding workflow:
  `CHANGELOG.md`, `docs/releases/v2026.8.3-announcement.md`, the canonical
  AsyncAPI version/checksum, and the exact-version documentation shard receipt.

## Human/release-owner items

- Ask the GitHub #56 submitter to retest `v2026.8.3` and keep diagnostic content
  within the documented redaction boundary.
- Keep Gitea #1100 open until reporter confirmation or a separately documented
  issue disposition.

## Validation

- `git diff --check` — passed
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
  — passed with 0 findings
- `npm run docs:check-assets` — passed
- `npm run docs:build` — passed with the pre-existing advisory above
- `scripts/ci/asyncapi-contract.sh check` — passed

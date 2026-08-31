# Documentation sync audit — 2026-08-31

- Direction: `code-to-docs`
- Baseline: `v2026.8.0..d7666e57f5d088c3c365cdbf1d1effe6e1c204ac`
- Scoped lanes: release/API, operator/migration, and CI/publication documentation
- Dry-run evidence:
  - `.aiwg/working/doc-sync/release-v2026.8.1-dry-run-20260831.md`

## Findings by severity

### Blockers

- None.

### High

- None.

### Advisory

- The documentation build retains one pre-existing `risky-raw-html` warning in
  `docs/content/job-monitoring.md:65`. It is unrelated to this release delta and
  did not fail the build.

## Files changed

- No product-documentation drift fixes were required.
- Release authorities intentionally prepared by the surrounding workflow:
  `CHANGELOG.md`, `docs/releases/v2026.8.1-announcement.md`, and the canonical
  AsyncAPI version/checksum.

## Human/release-owner items

- None before tagging. Continue to qualify all suite compatibility statements
  by exact artifact, profile, and producer/consumer receipt.

## Validation

- `git diff --check`
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
- `npm run docs:check-assets`
- `npm run docs:build`

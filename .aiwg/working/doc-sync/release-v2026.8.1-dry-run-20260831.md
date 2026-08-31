# Fortemi v2026.8.1 code-to-docs dry-run

- Direction: `code-to-docs`
- Baseline: `v2026.8.0..d7666e57f5d088c3c365cdbf1d1effe6e1c204ac`
- Candidate release: `v2026.8.1`
- Scope: changed product/API, operator, CI/publication, migration, and release documentation
- Result: no release-blocking documentation drift found

## Bounded evidence

- The typed SNN retention result and aggressive-pruning override are present in
  the canonical OpenAPI artifact, ADR-078, configuration reference, knowledge
  graph guide, operator guide, changelog, and release announcement.
- The extraction-strategy migration is represented by the forward migration,
  deployment/migration guidance, changelog, and release verification steps.
- Bundle snapshot/PKE hardening preserves the existing secret-redaction rules;
  the PKE documentation already requires 12+ character passphrases, while the
  administrative dump implementation detail remains internal to the bundle.
- Release-only container publication is documented in `docs/content/ci-cd.md`,
  `docs/content/deployment-and-migrations.md`, the changelog, and the release
  announcement. Ordinary branch/main validation is distinguished from signed
  CalVer publication.
- The archive DDL advisory lock changes concurrency behavior without changing
  the public API contract; its regression and redaction boundary are covered by
  issue #1122 evidence, the changelog, and the release announcement.
- The release announcement avoids unqualified full parity, complete-backup, and
  broad portability claims. Its compatibility statement is exact-artifact and
  producer/consumer receipt scoped.

## Validation

- `git diff --check v2026.8.0..d7666e57`
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
  — 0 findings
- `npm run docs:check-assets` — passed
- `npm run docs:build` — passed; one pre-existing advisory warning in
  `docs/content/job-monitoring.md:65`

## Proposed changes

No product documentation changes are required by this dry run. The pending
version, changelog, announcement, contract snapshot, and release evidence are
the intentional release-gate changes.

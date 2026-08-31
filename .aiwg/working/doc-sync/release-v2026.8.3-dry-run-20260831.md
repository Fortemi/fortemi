# Fortemi v2026.8.3 code-to-docs dry run

- Direction: `code-to-docs`
- Baseline: `v2026.8.2..7cd7feb4426a61b63cbb4c1e43eee252018623cc` plus the release working tree
- Candidate release: `v2026.8.3`
- Scope: semantic-chunking panic, customer retest guidance, diagnostic redaction, versioned contracts, and release documentation
- Result: no release-blocking documentation drift found

## Bounded evidence

- Commit `b6f528cdaf26b6f6dc1a99ab52ecb8736aa4cd9e` is an ancestor of
  the release baseline and contains the exact-source-span and checked-slicing
  implementation for CRLF, LF, lone-CR, mixed endings, U+2028/U+2029, and
  non-ASCII input.
- The changelog and v2026.8.3 announcement distinguish the v2026.7.22 queue
  recovery change from the later root-cause panic correction.
- The announcement gives a bounded customer retest procedure and preserves the
  existing rule against publishing note bodies, extracted document content,
  raw database errors, credentials, or tokens.
- No REST, AsyncAPI message shape, database migration, or persistence-format
  behavior changed. The AsyncAPI release version and checksum advance in
  lockstep with the workspace version.
- Knowledge Shard language remains limited to exact `core-v1` cells and
  immutable producer/consumer receipts. The release does not claim full parity,
  complete backup, or portability while the suite audit is `NO-GO`.

## Proposed changes

No additional product, architecture, operator, or API documentation changes
are required. The version files, changelog, announcement, AsyncAPI version and
checksum, release evidence, and documentation-shard producer receipt are the
intentional release artifacts.

## Validation

- `git diff --check` — passed
- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
  — passed with 0 findings
- `npm run docs:check-assets` — passed
- `npm run docs:build` — passed with one pre-existing advisory warning in
  `docs/content/job-monitoring.md:65`
- `scripts/ci/asyncapi-contract.sh check` — passed

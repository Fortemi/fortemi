# Documentation sync audit: release v2026.9.5

Direction: scoped incremental code-to-docs; dry-run first; no commit.
Base: `40a0c928282ea4d7f83bb0a851cf62419e8835d9` plus the reviewed corrective release diff for #1142.
Artifact root: `/home/roctinam/dev/fortemi-suite/fortemi/.aiwg`.

## Scope and findings

Inspected `.gitea/workflows/test.yml`,
`crates/matric-api/src/bulk_reprocess_tests.rs`, `docs/content/releasing.md`,
relevant `CLAUDE.md`/`AIWG.md` testing/releasing statements, the 2026.9.5 changelog
section and `docs/releases/v2026.9.5-announcement.md`.

| Severity | Finding | Resolution |
|---|---|---|
| Medium | Provider-facing context did not distinguish ordinary unit-test commands from shared-database release validation. | Added matching narrow notes to CLAUDE.md and AIWG.md: serial release cases, preserved within-test concurrency, disposable database and immutable failed tags. Generic unit-test commands remain. |
| Medium | Release instructions needed the distinction between public containers/release entries and later comprehensive/native publication completion. | Release conductor's existing releasing.md changes correctly document that ordering and all-gates completion. Reviewed without further edits. |

Dry-run findings were sent to the release conductor before the context edits.
No additional findings in the scoped changelog/announcement: both accurately
limit the change to CI/fixture isolation, preserve v2026.9.4, and describe v2026.9.5
as a recovery candidate pending its configured checks. Neither claims a proved
SQL deadlock, production behavior change or completed qualification.

## Code-to-docs agreement

Integration and other coverage commands now use `--test-threads=1`, matching the
existing build lane. This serializes test cases while keeping concurrency inside
a test. Bulk fixture cleanup delegates to the archive repository transaction and
advisory-lock protocol. No source API, MCP or authority/receipt contract changes
are introduced. Dataset alpha scope and suite qualification limits remain explicit.

## Validation and ownership

- Blocking `docs:contract` with `hosted_strict`: PASS, zero findings.
- `git diff --check`: PASS.
- Added local release-guide target and `pre-release` anchor: PASS.
- No `lint:claude-context` script exists in root package.json.
- Release conductor owns repro, runtime tests, versioning, changelog/announcement,
  seed-shard regeneration and publication gates. Their outcomes are not asserted
  by this documentation audit.

Auditor changed only CLAUDE.md, AIWG.md, this report, working evidence and sync stamp.
Remaining documentation review items: none. No source changes, commits or pushes
performed by this auditor.

Recorded: 2026-09-06T13:34:24.630948+00:00

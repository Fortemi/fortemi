# Documentation sync audit: release v2026.9.4

Direction: code-to-docs, incremental, dry-run first, no commit.
Scope: `v2026.9.3..6f9f0078c0fada94fab688f569f70d21ea3a6bff`, affected `docs/content/`, `CLAUDE.md`,
`AIWG.md`, `docs/architecture/`, and `scripts/qualification/README.md`.
Artifact root: `/home/roctinam/dev/fortemi-suite/fortemi/.aiwg`.
Release policy and tag-wrapper drift was added by the release conductor before fixes.

## Findings and resolution

| Severity | Finding | Resolution |
|---|---|---|
| High | Release instructions prescribed unsigned annotated tags and publishing every local tag, contrary to the mandatory OpenBao-backed release-key wrapper. MCP lockstep was optional and lockfiles omitted. | Fixed release sections in CLAUDE.md, AIWG.md and docs/content/releasing.md: distinct commit/tag authorities, mandatory wrapper, exact tag push, lockfiles, clean isolated checkout, and publication gates. |
| Medium | MCP user guide omitted connection/inconsistent-response ambiguity, conditional committed checkpoint and unresolved archive rejection added by the dataset controller. | Added exact-retry recovery semantics, checkpoint restriction, RUN_OUTCOME_UNRESOLVED and current validation authority path. Kept lifecycle diagnostics separate from HTTP RFC 9457. |
| Medium | Load-tool evidence guide did not distinguish public receipt redaction from fixture/raw observation artifacts. | Documented possible content/identity exposure, synthetic fixtures, operator-controlled storage and adapter responsibility; writer hashes/bounds bytes and does not redact. |
| Low | Provider-facing context lacked current dataset validation and configurable qualification-tool navigation. | Added matching focused links in CLAUDE.md and AIWG.md, with explicit local-evidence and ADR-100/Enterprise limits. |

Dry-run findings were sent to the release conductor before edits. All fixes are
high-confidence documentation changes supported by the current code/policy.
No source, contract schema, release configuration, changelog or announcement changed.

## Reviewed without edits

Bulk reprocess documentation matches tenant-scoped filtering, bounded candidates,
zero-work behavior and response fields. Operator SQL documentation matches the
forced-RLS recipe. ADR-107 already covers strict validation and storage ambiguity.
Configurable-load ADR and guide preserve suite NO-GO and candidate 2.0.0 boundaries.
No changed inference provider or CE/EE implementation justified expanding the audit.
Existing HTTP RFC 9457 documentation remains valid; the MCP Enterprise gate remains
proposed, not supplied by local qualification tooling.

## Validation

- `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`: PASS, zero findings.
- `git diff --check`: PASS.
- Targeted local-link and MCP-anchor checks: PASS (six targets and anchor).
- `lint:claude-context` is not defined in root package.json; unavailable, not skipped as a passing check.
- No source tests required for these documentation-only edits; release conductor owns source/CI gates.

Remaining human-review items: none in the audited documentation scope. Runtime
qualification prerequisites and release publication gates remain separate.
Handoff: release conductor stages these reviewed files and continues flow-release.

Recorded: 2026-09-06T03:37:11.129461+00:00

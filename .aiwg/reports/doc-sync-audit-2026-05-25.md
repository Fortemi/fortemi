# Doc Sync Audit — 2026-05-25

Scope: release-prep code-to-docs sync for Fortemi v2026.5.12.

## Direction

`code-to-docs`: code, migrations, issue closures, and current CI are the source of truth. Documentation and release metadata were reconciled to match the current `main` head.

## Findings And Actions

| ID | Severity | Finding | Action |
|---|---|---|---|
| DOC-DRIFT-001 | High | Workspace and MCP package versions still pointed at `2026.5.11`; `mcp-server/package-lock.json` still pointed at `2026.5.5`. | Bumped `Cargo.toml`, `Cargo.lock`, `mcp-server/package.json`, and `mcp-server/package-lock.json` to `2026.5.12`. |
| DOC-DRIFT-002 | High | Release notes did not describe the realtime provider milestone implemented after `v2026.5.11`. | Added `CHANGELOG.md` section for `2026.5.12` and `docs/releases/v2026.5.12-announcement.md`. |
| DOC-DRIFT-003 | High | Repository documentation contained BT6-specific deployment references. | Removed the dedicated deployment document, removed references from `SETUP.md`, and generalized the historical changelog entry. Follow-up search found zero `BT6`/`BT-6`/`bt6`/`bt-6` references. |
| DOC-DRIFT-004 | Medium | Changelog comparison links stopped at older release entries. | Added comparison links for `2026.5.0` through `2026.5.12` and updated `[Unreleased]`. |
| DOC-DRIFT-005 | Medium | Release validation surfaced a new clippy warning in a test under current toolchain. | Applied the mechanical collapsible-match fix in `crates/matric-jobs/tests/worker_integration_test.rs`; reran clippy clean. |

## Validation

- `rg -n "BT6|BT-6|bt6|bt-6|bt6-bollard" -S` — no matches.
- `cargo fmt --check` — passed.
- `cargo check --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed after the mechanical test fix.
- `bash scripts/ci/forbid-provider-imports.sh .` — passed.
- `git diff --check` — passed.
- `npm ls` in `mcp-server/` — passed and reports `@fortemi/mcp@2026.5.12`.
- `npm run validate:schemas` in `mcp-server/` — passed for 205 schemas.
- `npm run test:annotations` in `mcp-server/` — passed.

## Non-Gating Note

`npm test` in `mcp-server/` was attempted and failed because the API-backed MCP suites require a running Fortemi API. The failure mode was HTML returned where the tests expected API JSON. Static MCP validation and annotation tests passed separately.

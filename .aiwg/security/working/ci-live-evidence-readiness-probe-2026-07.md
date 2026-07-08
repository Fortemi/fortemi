# CI Live Evidence Readiness Probe - 2026-07

## Purpose

Record the current evidence state for the Fortemi CI/release hardening live-evidence gate tracked by `Fortemi/fortemi#1021`. This probe complements `ci-live-evidence-runbook-2026-07.md`, which defines the required live pull-request and release/tag receipts.

## Current Probe Result

| Check | Result | Evidence |
|---|---|---|
| Executable suite preflight | Blocked | Latest `.aiwg/scripts/check-gitea-actions-live-evidence-preflight.sh` receipt on 2026-07-08 returned blocked with 7 readiness blockers: no Gitea token variable, no configured `tea` login, unauthenticated `gh`, unauthenticated Actions API access for `Fortemi/fortemi` and `Fortemi/HotM`, and local Fortemi/HotM live-evidence file changes not proven by a remote Actions run. |
| Gitea Actions API access | Blocked | `GET https://git.integrolabs.net/api/v1/repos/Fortemi/fortemi/actions/runs?limit=10` requires authentication without a token in the environment. |
| Gitea token in environment | Not available | `GITEA_TOKEN` and `GITEA_ACCESS_TOKEN` were unset. |
| GitHub token in environment | Not available | `GITHUB_TOKEN` and `GH_TOKEN` were unset. |
| Gitea CLI login | Not configured | `tea logins list -o csv` returned only the header row, so no configured login is available for CLI-based fallback checks. |
| GitHub CLI login | Not configured | `gh auth status` reported no login. |
| Workflow changes pushed | Not proven | The 2026-07-08 preflight confirms both Fortemi and HotM `HEAD` match `origin/main`, but relevant live-evidence files still have local changes not proven by a remote Actions run. |
| Static release-job guard lint | Passed | `scripts/ci/verify-release-job-guards.py` returned `release job guard check passed`. |
| Workflow YAML parse | Passed | Python/YAML parse succeeded for `.gitea/workflows/ci-builder.yaml` and `.gitea/workflows/publish-sidecar.yml`. |
| HotM workflow YAML parse | Passed | Python/YAML parse succeeded for HotM `.gitea/workflows/tauri-build.yml`, `.gitea/workflows/desktop-build-matrix.yml`, and `.gitea/workflows/desktop-release.yml`. |

## Gate Status

The Fortemi CI hardening slice remains partially covered:

- Static workflow guard lint, workflow parsing, pinned artifact actions, Redis digest pinning, runner/token documentation, and Docker digest documentation are proven locally.
- Live pull-request and release/tag receipts remain open because authenticated Gitea Actions API access is not available through the documented token path and the workflow changes have not been pushed into a CI run target.

## Connector-Visible Supplement

`.aiwg/reports/gitea-actions-connector-evidence-supplement-2026-07-07.md` records Actions metadata visible through the Gitea connector after the token/CLI preflight remained blocked. The connector saw successful generic Fortemi push runs `4076` at SHA `999c16570a3873c97812eb8cd20936f87c827274` and `4080` at SHA `cd509cd33a9a6b0275a80847e3ac62d338fdfb9d`, a successful HotM docs-only push run `867` at SHA `7aa7a7d7ab7e9b54bc296f641098debe2add56a3`, and a successful `Fortemi/fortemi-react` publish run `256` for the `2026.7.3` package drop.

This connector metadata does not close `Fortemi/fortemi#1021`: it does not prove the Fortemi guarded pull-request workflow, Fortemi release/tag workflow, skipped secret-bearing publish jobs, approved release-context publish behavior, or immutable API/bundle image digests.

## Required Live Evidence

Capture the following after the workflow changes are committed/pushed or a PR is opened:

| Receipt | Required proof |
|---|---|
| Pull-request run | Run URL, run ID, head SHA, guard-lint job pass, build/test job pass, and proof that secret-bearing publish jobs are skipped for `pull_request`. |
| Release/tag run | Run URL, run ID, tag/SHA, guarded publish jobs running only in approved release context, registry login without token exposure, and immutable API/bundle image digests. |
| Log hygiene | Sanitized review confirming no token, registry password, or secret-bearing environment value appears in logs. |
| Runner labels | Job summary or log evidence showing runner labels match the intended builder/release jobs. |

## Authenticated API Commands

Use a token with read access to `Fortemi/fortemi` Actions:

```bash
curl -fsSL \
  -H "Authorization: token ${GITEA_TOKEN}" \
  "https://git.integrolabs.net/api/v1/repos/Fortemi/fortemi/actions/runs?limit=20"
```

After selecting the run ID:

```bash
curl -fsSL \
  -H "Authorization: token ${GITEA_TOKEN}" \
  "https://git.integrolabs.net/api/v1/repos/Fortemi/fortemi/actions/runs/<run-id>/jobs"
```

Record the run URL, run ID, head SHA, workflow name, job status, skipped guarded jobs, and sanitized log excerpts in the receipt template from `ci-live-evidence-runbook-2026-07.md`.

## Do Not Claim

Do not close `Fortemi/fortemi#1021` or claim live CI/release guard verification until the required live receipts are attached.

## Latest Preflight Result - 2026-07-08

Command:

```bash
.aiwg/scripts/check-gitea-actions-live-evidence-preflight.sh
```

Result: blocked with 7 external/readiness check(s), matching the July 7 blocker shape.

Sanitized findings:

- `GITEA_TOKEN` and `GITEA_ACCESS_TOKEN` are unset.
- `tea` is installed but has no configured login.
- `gh` is installed but is not authenticated.
- `Fortemi/fortemi` Actions API requires authentication without a token.
- `Fortemi/HotM` Actions API requires authentication without a token.
- Fortemi `HEAD` matches `origin/main`, but Fortemi live-evidence files have local changes not proven by a remote Actions run.
- HotM `HEAD` matches `origin/main`, but HotM live-evidence files have local changes not proven by a remote Actions run.

Passing local checks from the same run:

- Fortemi release-job guard lint passes.
- Fortemi CI and sidecar workflows parse as YAML.
- HotM sidecar provenance manifest exists.
- HotM pinned sidecar downloader is executable and wired into desktop/Tauri workflows.
- HotM desktop/Tauri workflows parse as YAML.
- HotM HUX traceability anchors pass.

Suite-root receipt: `.aiwg/reports/gitea-actions-live-evidence-preflight-rerun-2026-07-08.md`.

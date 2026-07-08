# CI Live Evidence Runbook - 2026-07

## Purpose

Collect live Gitea Actions evidence for the July 2026 CI/release hardening slice. Static checks now prove workflow shape, but `Fortemi/fortemi#1021` should not close until live pull-request and release/tag behavior confirms Gitea evaluates the guards as intended.

Current readiness probe: `ci-live-evidence-readiness-probe-2026-07.md` records that authenticated Gitea Actions access is not available through an environment token and the workflow changes are still local workspace changes, so live PR/release receipts remain open. From the suite root, `.aiwg/scripts/check-gitea-actions-live-evidence-preflight.sh` reproduces the same gate and also checks the HotM sidecar live-CI prerequisite.

## Scope

This runbook covers:

- Pull-request workflow execution where secret-bearing publish jobs must not run.
- Version tag or manual release workflow execution where guarded publish jobs may run only in the intended context.
- Release evidence that immutable Docker digests are recorded for user-facing artifacts.

## Required Inputs

| Input | Required value |
|---|---|
| PR run URL | Gitea Actions URL for a pull-request run on `ci-builder.yaml`. |
| Release/tag run URL | Gitea Actions URL for a version tag or approved manual release run. |
| Commit SHA | Commit tested by both static guard lint and live CI. |
| Runner labels | Labels used by each job, especially `matric-builder`, `titan`, or `gpu`. |
| Secret names expected | `BUILD_REPO_TOKEN`, `GH_PUBLISH_TOKEN`, and any release-specific token names. |
| Release image tags | Versioned API and bundle image tags for digest verification. |

## Preflight Static Checks

Run before collecting live evidence:

```bash
.aiwg/scripts/check-gitea-actions-live-evidence-preflight.sh
cd fortemi
scripts/ci/verify-release-job-guards.py
python3 - <<'PY'
from pathlib import Path
import yaml
for name in ['.gitea/workflows/ci-builder.yaml', '.gitea/workflows/publish-sidecar.yml']:
    with Path(name).open() as f:
        yaml.safe_load(f)
    print(f'ok {name}')
PY
git diff --check
```

Pass criteria:

- Release-job guard lint passes.
- Edited workflows parse.
- No whitespace errors.

## Pull-Request Run Evidence

Use a normal PR run that exercises the `pull_request` trigger.

Required observations:

| Check | Pass criteria |
|---|---|
| Guard lint ran | `Check release job guards` appears in the lint job and passes. |
| Build/test jobs ran | Non-secret build/test/container jobs run as expected. |
| Secret-bearing jobs skipped | Jobs that reference `${{ secrets.* }}` and publish images/releases do not run for the PR event. |
| No secret log exposure | Logs do not show token values, registry credentials, or Docker login passwords. |
| Runner labels match intent | PR build/test jobs use `matric-builder`; host labels are limited to intended integration jobs. |

Attach sanitized screenshots or log excerpts showing the skipped publish jobs and passing guard lint.

## Release/Tag Run Evidence

Use a version tag or approved manual dispatch run. Do not use this section for an unapproved test tag unless release owners accept the artifact impact.

Required observations:

| Check | Pass criteria |
|---|---|
| Event guard is correct | Publish jobs run only because the event is a version tag, push to the expected protected branch, or approved manual dispatch. |
| Secrets are used only in publish jobs | `BUILD_REPO_TOKEN` and `GH_PUBLISH_TOKEN` are referenced only in guarded jobs. |
| Registry login succeeds | Internal registry and GHCR login steps succeed without printing token values. |
| Versioned images publish | Versioned API and bundle images are pushed. |
| Mutable aliases are labeled convenience only | `latest` and `bundle-latest` are not treated as verification evidence. |
| Immutable digests are recorded | Release notes or evidence include `ghcr.io/fortemi/fortemi@sha256:...` references for API and bundle images. |

Digest capture commands:

```bash
VERSION=2026.6.1
docker pull ghcr.io/fortemi/fortemi:${VERSION}
docker pull ghcr.io/fortemi/fortemi:bundle-${VERSION}
docker image inspect ghcr.io/fortemi/fortemi:${VERSION} --format '{{index .RepoDigests 0}}'
docker image inspect ghcr.io/fortemi/fortemi:bundle-${VERSION} --format '{{index .RepoDigests 0}}'
```

## Evidence Receipt Template

Attach this receipt to `Fortemi/fortemi#1021`.

```markdown
## CI Live Evidence Receipt

- Date/time:
- Commit SHA:
- PR run URL:
- Release/tag run URL:
- Release version/tag:
- Runner labels observed:
- Static guard lint result:

### Pull-request run

| Check | Result | Evidence |
|---|---|---|
| Guard lint ran and passed | pass/fail | log link |
| Build/test jobs ran | pass/fail | run summary |
| Secret-bearing publish jobs skipped | pass/fail | skipped job evidence |
| No secret log exposure | pass/fail | sanitized log review note |
| Runner labels match intent | pass/fail | job summary |

### Release/tag run

| Check | Result | Evidence |
|---|---|---|
| Event guard correct | pass/fail | run summary |
| Secrets only in guarded jobs | pass/fail | workflow/job evidence |
| Registry login succeeded without token exposure | pass/fail | sanitized log link |
| Versioned images published | pass/fail | registry/release link |
| Immutable API image digest recorded | pass/fail | digest |
| Immutable bundle image digest recorded | pass/fail | digest |

### Decision

- CI live-evidence gate: pass/fail/deferred
- Follow-up issues:
```

Do not paste raw tokens, registry credentials, private runner registration tokens, or full secret-bearing log lines into the receipt.

## Failure Handling

If a PR run executes a secret-bearing publish job, treat `Fortemi/fortemi#1021` as not fixed and immediately disable or tighten the job guard before the next PR run.

If a release/tag run cannot record immutable digests, keep mutable Docker tags as convenience aliases only and do not claim release artifact verification until digest evidence exists.

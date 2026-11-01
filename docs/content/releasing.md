# Release Process

This document describes the release process for Fortémi. The executable release
policy is `.aiwg/release.config`; stable tags must use
`tools/release/cut-tag.sh` and its distinct OpenBao-backed release signing key.

## Versioning

Fortémi uses **CalVer** (Calendar Versioning):

- Format: `YYYY.M.PATCH`
- Examples: `2026.1.0`, `2026.1.5`, `2026.12.0`
- **No leading zeros** - npm/cargo semver rejects them
- PATCH resets to 0 each month
- Git tags use `v` prefix: `v2026.1.0`

## Release Checklist

### Pre-Release

- [ ] A green release validation run: ci-builder `workflow_dispatch` on the exact
      candidate commit on `main`, plus its
      comprehensive `test.yml` handoff. Record both run IDs and the candidate SHA
      in the release PR/notes **before `tools/release/cut-tag.sh`**.
- [ ] A green `docsite-build` `workflow_dispatch` on the same candidate (strict
      links and asset checks). Ordinary main pushes do not build the docsite;
      the tag-only chain reaches `docsite-deploy` only after release.
- [ ] Linting clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Format check: `cargo fmt --all -- --check`
- [ ] Dependency policy: `cargo deny check advisories bans licenses sources`
- [ ] Documentation contract: `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
- [ ] External issuer tests: `scripts/test-mcp-external-issuer.sh` and
      `scripts/test-keycloak-reference-realm.sh` pass with disposable Docker
      stacks. The Keycloak test imports the shipped realm and claim policy,
      checks PKCE/client credentials, audiences, role ceilings, MCP and metadata,
      and rejects secret leakage in logs.
- [ ] CI pipeline green on main branch
- [ ] Documentation updated for new features
- [ ] No critical open issues blocking release
- [ ] If the release touches the Intel overlay or provider routing:
      `./scripts/smoke-intel-vllm.sh` passes (see
      [Intel Arc / vLLM Deployment](#/operations-intel-arc-vllm))

### Prepare the native documentation shard

Finish the version bump, AsyncAPI corpus, changelog, announcement and documentation
sync first. Commit those reviewed source inputs as a source baseline before native
generation. The `docs-shard-preparation` hard-stop gate in `.aiwg/release.config`
then generates, verifies and stages both documentation artifacts before local
validation or delivery. Release validation must target the subsequent commit that
includes the genuine shard and receipt; tag-time regeneration cannot repair stale
artifacts already rejected by main lint.

Use the suite's detached bounded native runner with the existing target cache
explicitly bound by `--cache-dir /var/tmp/fortemi-release-target-2026.9.1`.
Keep two CPU, at most two Cargo jobs (one is recommended for the API), 8 GiB maximum,
private network/tmp/devices and no GPU or Docker socket. Build offline with locked
dependencies: `cargo build --locked --offline -j1 -p matric-api --bin matric-api
--message-format=json`. An AsyncAPI exporter build does not build this server.
Retain the successful Cargo compiler-artifact record, clean source commit/tree
and input inventory, executable length and SHA-256. Preserve the cache; do not
clean it or enable network to resolve a missing dependency.

Run the source-bound private `matric-api` copy and a disposable PostgreSQL fixture
in the same bounded unit. The suite's existing `with-ephemeral-postgres.mjs
--profile hosted-http` provides PostgreSQL 18, PostGIS, a private Unix socket and
20 connections. Native migrations also require the installed vector/trigram/UUID
extensions. Use the helper's database URL, private storage and loopback API port;
never use shared PostgreSQL, Redis or an operational API. The importer deletes
and recreates the `fortemi-docs` archive. Use the reviewed Community fixture's
explicit no-auth acknowledgement, disabled workers/scanner/rate limit and
unreachable inference endpoints. These fixture settings do not qualify hosted
authentication or broaden any ADR-102 profile claim.

Inside that fixture, provide `DOCS_SHARD_API_URL`, `DOCS_SHARD_SERVER_BINARY`
(the digest-verified private copy named `matric-api`),
`DOCS_SHARD_BINARY_SHA256` (from the completed build), and
`DOCS_SHARD_RUNTIME_HEALTH` (a new owned evidence path). Execute the preparation
steps in order. The gate observes `/health`, runs the canonical importer (which
checks tracked-source coverage), invokes the receipt writer with
`--server-binary`, `--runtime-health`, `--expected-binary-sha256` and
`--source-commit`, runs freshness verification, then stages exactly:

```bash
git add -- docker/seed-data/fortemi-docs.shard \
  docker/seed-data/fortemi-docs.shard.receipt.json
```

Health version, manifest producer version and workspace version must agree;
health `git_sha`, producer revision and receipt source must name the source
baseline. `MATRIC_GIT_SHA` is supplied at launch, so health alone does not prove
build provenance. Keep the completed build evidence separately. Commit the two
derived artifacts after acceptance, retaining their actual source baseline in the
receipt; do not replace it with the later artifact commit. If source inputs change,
regenerate from the new baseline. Never edit receipt identities or bypass freshness.

Require graceful owned API/PostgreSQL shutdown, stopped-fixture verification,
private-directory cleanup and terminal runner evidence before acceptance. Preserve
failure evidence and stop only owned units; never kill a discovered shared service.

### Dispatch release validation

After the source baseline and generated shard/receipt are merged, fetch main and
dispatch the exact candidate.
Use a Gitea token with workflow-dispatch permission (do not record the token):

```bash
git fetch origin main
CANDIDATE_SHA=$(git rev-parse origin/main)
jq -n --arg sha "$CANDIDATE_SHA" \
  '{ref: "main", inputs: {source_sha: $sha}}' \
  > /tmp/fortemi-release-validation.json
curl --fail-with-body --silent --show-error -X POST \
  -H "Authorization: token ${GITEA_TOKEN:?Set a workflow-dispatch token}" \
  -H 'Content-Type: application/json' \
  'https://git.integrolabs.net/api/v1/repos/Fortemi/fortemi/actions/workflows/ci-builder.yaml/dispatches' \
  --data-binary @/tmp/fortemi-release-validation.json
```

Dispatch the docsite build on the same candidate (it accepts no inputs, so run it
while `main` is still the candidate commit):

```bash
curl --fail-with-body --silent --show-error -X POST \
  -H "Authorization: token ${GITEA_TOKEN:?Set a workflow-dispatch token}" \
  -H 'Content-Type: application/json' \
  'https://git.integrolabs.net/api/v1/repos/Fortemi/fortemi/actions/workflows/docsite-build.yml/dispatches' \
  --data '{"ref":"main"}'
```

Alternatively, open **Actions → CI → Run workflow**, select `main`, enter the
full 40-character candidate commit in `source_sha`, and run it. In Actions, record the ci-builder run ID/URL and the
comprehensive **Tests** run ID/URL that its handoff creates with
`test_category=all` and the same `source_sha`. Wait for both runs to finish green.
Record `candidate SHA`, `ci-builder run ID/result`, and `test.yml run ID/result`
in the release PR/notes; an accepted dispatch or a skipped run is not validation.

The stale-source guard requires the candidate to remain the current main tip
at admission and at handoff. If main advances, validate the new candidate again;
a green superseded run with a skipped handoff does not satisfy the release gate.
`delivery.require_ci_green` also applies to post-merge main CI and its test
handoff. Fix red main forward on a new branch.

A main dispatch runs lint, audits, the Rust release build and tests, the
API/hosted images, container checks, kind Pod Security and GPU integration, then
hands off to the comprehensive Tests run. It publishes nothing: the publication,
signing, GHCR verification and finalization jobs are tag-only.

Some release build steps still run only on tags, inside the publication jobs:
the complete knowledge-shard matrix with remote verification, multiarch
API/hosted/MCP/bundle builds, docs-shard regeneration on the release image, arm64
health and both-architecture hosted smoke checks, and per-platform SBOMs. Until
the no-publish dry-run mode in Fortemi/fortemi#1251 lands, those are first
exercised by the tag run; a failure there requires a new patch version.

### Version Bump

1. Set the workspace version in `Cargo.toml` and refresh the corresponding
   workspace package entries in `Cargo.lock`.
2. Set the same version in `mcp-server/package.json` and
   `mcp-server/package-lock.json`. MCP version alignment is required for every
   server release. Set `version` and `appVersion` in
   `deploy/helm/fortemi/Chart.yaml` to the same version; the Helm chart lint
   job fails when they differ.
3. Regenerate and check the AsyncAPI YAML/checksum, then immediately regenerate
   and check the event corpus, including metadata-only version bumps:

   ```bash
   scripts/ci/asyncapi-contract.sh generate
   scripts/ci/asyncapi-contract.sh check
   scripts/ci/asyncapi-event-fixtures.sh generate
   scripts/ci/asyncapi-event-fixtures.sh check
   ```

   Commit the YAML, checksum, fixture manifest, producer receipt and all generated
   event fixtures together. The required core tests check the committed corpus's
   exact bytes and inventory against the raw YAML, runtime schemas and typed
   events. A stale corpus fails even when all 48 payload files are unchanged.
4. Move the selected `CHANGELOG.md` entries into the dated release section and
   update comparison links. Add `docs/releases/vYYYY.M.PATCH-announcement.md`.

### Create Release

Commit the reviewed release files with the configured OpenBao-backed commit
signing authority. Ordinary commits and release tags use distinct keys:

```bash
git add Cargo.toml Cargo.lock mcp-server/package.json mcp-server/package-lock.json \
  deploy/helm/fortemi/Chart.yaml \
  contracts/asyncapi/asyncapi.yaml contracts/asyncapi/asyncapi.sha256 \
  contracts/asyncapi/fixtures/manifest.json contracts/asyncapi/fixtures/events/ \
  contracts/asyncapi/producer-event-fixture-receipt.json \
  docker/seed-data/fortemi-docs.shard docker/seed-data/fortemi-docs.shard.receipt.json \
  CHANGELOG.md docs/releases/vYYYY.M.PATCH-announcement.md
git -c user.signingkey=62297562B1C7053088F405DB0117DAAA677A5BF2 \
  -c gpg.program=tools/git/gpg-from-openbao.sh commit -S \
  -m "chore: release vYYYY.M.PATCH"
git push origin main
```

After both recorded release validation runs pass, use a clean checkout at the exact current
`origin/main` commit. If the working checkout contains unrelated files, use an
isolated clean checkout and preserve the original work. The tag wrapper checks
workspace/MCP versions, changelog, announcement, clean status and `origin/main`,
then signs and verifies with the published release key:

```bash
tools/release/cut-tag.sh YYYY.M.PATCH -m "vYYYY.M.PATCH - Release title"
git push origin vYYYY.M.PATCH
```

Then push the same commit and tag to the GitHub backup mirror:

```bash
git push github main vYYYY.M.PATCH
```

`core.sshCommand` is set to `tools/git/ssh-route.sh`. It sends `origin`
(Gitea) traffic through the OpenBao-held deploy key and `github` traffic through
the operator's own GitHub SSH keys, so neither remote needs per-command
overrides. The routing values live in the local `aiwg.vault.*` Git config and
are listed in `.aiwg/aiwg.config` under `remotes.transport`. Run
`tools/git/push-origin.sh --check` to confirm the Gitea key resolves and is
accepted before pushing.

The wrapper accepts stable CalVer versions. It does not replace CI, lockfile or
publication verification. Do not bypass it with a raw annotated tag or publish
unrelated local tags. A failed gate leaves the release incomplete.

Shared-database integration and coverage test cases run serially. Hosted-role
checks inspect the entire archive catalog, and schema-changing fixtures and
child processes cannot be isolated by separate Rust mutexes. Tests that issue
concurrent requests internally still exercise that concurrency. Run reproductions
against a fresh disposable database; never point them at an operational database.

After a pushed tag fails a required gate, preserve it and its failure evidence.
Fix and validate the cause, then cut a new patch version. Do not move the old tag
or waive coverage to finish publication.

### CI/CD Automation

When you push the tag, the CI pipeline automatically:

1. **Runs all tests** (lint, unit tests, integration tests, container tests)
2. **Publishes Docker images** to both registries:
   - Internal: `ghcr.io/fortemi/fortemi:{version}`
   - Public: `ghcr.io/fortemi/fortemi:{version}`
3. **Creates releases** on both Gitea and GitHub with:
   - Changelog extracted from `CHANGELOG.md`
   - Docker installation instructions
   - Quick start commands

**Docker image tags published:**
- `{version}` - Specific version (e.g., `2026.2.0`)
- `latest` - Latest stable release
- `bundle-{version}` - All-in-one image with embedded PostgreSQL
- `bundle-latest` - Latest bundle image

Mutable `latest` tags are convenience aliases only. Release verification must resolve and record immutable digest references for versioned images:

```bash
VERSION=2026.6.1
docker pull ghcr.io/fortemi/fortemi:${VERSION}
docker pull ghcr.io/fortemi/fortemi:bundle-${VERSION}
docker image inspect ghcr.io/fortemi/fortemi:${VERSION} --format '{{index .RepoDigests 0}}'
docker image inspect ghcr.io/fortemi/fortemi:bundle-${VERSION} --format '{{index .RepoDigests 0}}'
```

Use `ghcr.io/fortemi/fortemi@sha256:...` references in production deployment records and rollback plans.
The release jobs upload registry-derived receipts that bind version and mutable
tags to these digests. Verify and retain them as described in
[Container Release Evidence](#/container-release-evidence).

The Gitea and GitHub release entries are finalized together only after the
tag-only `verify-ghcr-release` job repeats those checks with an anonymous
Docker configuration and verifies the published image revision/version labels.
This checks public container availability and keeps both entries under the same
container publication gate. Comprehensive `test.yml` runs afterward and gates
native binary publication. An entry and images can therefore exist while the
release remains incomplete. Require both configured workflows to pass and verify
the native assets, checksums and provenance before declaring release completion.

> **Sidecar images** (GLiNER, pyannote) are released independently with their own tags. See [CI/CD docs](#/operations-ci-cd) for details.

### Dispatch versus tag job audit (#1250)

The ci-builder push trigger selects `main` only. A `v*` push first runs
`build-builder.yaml`, which dispatches ci-builder with the tag ref and immutable
`source_sha`; the tag column below describes that chained ci-builder run.
These job guards are unchanged by #1250. All jobs also require successful
`needs` unless their guard explicitly uses `always()`.

On a main dispatch without `source_sha`, `CHAIN_SOURCE_SHA` is the dispatch's
`github.sha`. With it, that exact commit is fetched. Both take the same job path
if the SHA is still main's tip; an old SHA fails admission as superseded, and
heavy jobs/handoff skip. Without `source_sha` the dispatch uses the shared
concurrency group; with it, it uses the per-run group (#1234).

Guards (verbatim; G0 denotes the implicit default):

- **G0**: `success() (implicit)`
- **G1**: `needs.admit-source.outputs.current == 'true' && (github.event_name != 'pull_request')`
- **G2**: `needs.admit-source.outputs.current == 'true'`
- **G3**: `startsWith(github.ref, 'refs/tags/v')`
- **G4**: `startsWith(github.ref, 'refs/tags/v') && needs.publish-release.result == 'success' && needs.publish-github.result == 'success'`
- **G5**: `startsWith(github.ref, 'refs/tags/v') && needs.publish-github.result == 'success'`
- **G6**: `startsWith(github.ref, 'refs/tags/v') && needs.publish-release.result == 'success' && needs.publish-github.result == 'success' && needs.verify-ghcr-release.result == 'success' && needs.sign-release-images.result == 'success'`
- **G7**: `needs.admit-source.outputs.current == 'true' && (github.ref == 'refs/heads/main' && github.event_name != 'pull_request')`
- **G8**: `needs.admit-source.outputs.current == 'true' && (always() && github.event_name != 'pull_request' && ((github.ref == 'refs/heads/main' && needs.main-validation.result == 'success') || (startsWith(github.ref, 'refs/tags/v') && needs.finalize-releases.result == 'success')))`
- **G9**: `always()`

| ci-builder job | Guard | Main dispatch, with/without current `source_sha` | Tag chain |
|---|---|---|---|
| `runner-capacity` | G0 | Yes | Yes |
| `verify-release-ref` | G0 | Yes; signature check is a no-op | Yes; verifies signed tag |
| `sync-github-source` | G1 | Yes | Yes |
| `admit-source` | G0 | Checks current main tip | Admitted (no main-tip check) |
| `build-testdb` | G2 | Yes | Yes |
| `lint` | G2 | Yes | Yes |
| `knowledge-shard-matrix` | G2 | Yes | Yes |
| `helm-chart` | G2 | Yes | Yes |
| `audit` | G2 | Yes | Yes |
| `deny` | G2 | Yes | Yes |
| `auth-consumer` | G2 | Yes | Yes |
| `mcp-lockfile-sync` | G2 | Yes | Yes |
| `mcp-server-tests` | G2 | Yes | Yes |
| `build` | G2 | Yes | Yes |
| `build-image` | G2 | Yes | Yes |
| `hosted-image-smoke` | G2 | Yes | Yes |
| `pod-security-restricted` | G2 | Yes | Yes |
| `test-container` | G2 | Yes | Yes |
| `integration-test` | G2 | Yes | Yes |
| `publish-release` | G3 | Skipped | Yes |
| `publish-github` | G3 | Skipped | Yes |
| `sign-release-images` | G4 | Skipped | Yes |
| `verify-ghcr-release` | G5 | Skipped | Yes |
| `finalize-releases` | G6 | Skipped | Yes |
| `main-validation` | G7 | Yes | Skipped |
| `handoff-tests` | G8 | After main-validation | After finalize-releases |
| `cleanup-run-images` | G9 | Always | Always |

The comprehensive main handoff runs `runner-capacity`, `admit-source`,
`build-testdb`, `fast-tests`, `integration-tests`, `coverage`, `slow-tests`,
`validate-intel-overlay`, `test-summary` and cleanup. Its `handoff-sidecar` is
strictly tag-only. Native Linux/macOS release-asset builds therefore still run
only in the tag chain; they need their dedicated native runners and publication
handoff and are not represented as pre-tag native-platform qualification.

### Post-Release

- [ ] Verify the canonical [Gitea release](https://git.integrolabs.net/Fortemi/fortemi/releases) and required assets
- [ ] Verify the mirrored release appears on [GitHub Releases](https://github.com/fortemi/fortemi/releases)
- [ ] Verify Docker images on [ghcr.io](https://ghcr.io/fortemi/fortemi)
- [ ] Record immutable Docker digest references for `{version}` and `bundle-{version}`
- [ ] Update any deployment configurations
- [ ] Pull new image and restart production service if needed:
  ```bash
  docker pull ghcr.io/fortemi/fortemi:bundle-${VERSION}
  docker compose -f docker-compose.bundle.yml up -d
  ```
- [ ] Smoke test production endpoints:
  ```bash
  curl http://localhost:3000/health
  ```

## Release Documentation

Each release should have documentation in these locations:

| Location | Purpose |
|----------|---------|
| `CHANGELOG.md` | Technical changelog with highlights table |
| Gitea Release | Public release notes with install instructions |

### CHANGELOG.md Format

```markdown
## [YYYY.M.PATCH] - YYYY-MM-DD

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| Feature A | Benefit description |

### Added
- New features

### Changed
- Changes to existing features

### Fixed
- Bug fixes

### Deprecated
- Features to be removed

### Removed
- Removed features

### Security
- Security fixes
```

## Hotfix Releases

For urgent fixes:

1. Create fix on main branch
2. Increment PATCH: `2026.1.0` → `2026.1.1`
3. Abbreviated changelog entry
4. Follow normal release process

## Monthly Releases

For planned releases at month boundaries:

1. Reset PATCH to 0
2. New month number: `2026.1.5` → `2026.2.0`
3. Comprehensive changelog entry

## Breaking Changes

For breaking API changes:

1. Document in CHANGELOG.md under dedicated section
2. Provide migration guide in release notes
3. Consider deprecation period before removal
4. Update API version path if major incompatibility

## Rollback Procedure

If a release has critical issues:

```bash
# 1. Revert to previous version in production
git checkout v2026.1.0  # previous version
cargo build --release
sudo systemctl restart matric-api

# 2. Create hotfix release
# Follow hotfix procedure above
```

## Authentication Migration Guide

When enabling authentication on an existing deployment:

### Pre-Migration Checklist

- [ ] Deploy latest version with `REQUIRE_AUTH=true` (default)
- [ ] Register OAuth clients for all integrations: `POST /oauth/register`
- [ ] Create API keys for CLI/automation: `POST /api/v1/api-keys`
- [ ] Distribute credentials to all clients/users
- [ ] Test authentication with a sample request:
  ```bash
  curl -H "Authorization: Bearer <ACCESS_TOKEN>" http://localhost:3000/api/v1/notes
  ```

### Enable Authentication

1. Set `REQUIRE_AUTH=true` in `.env`
2. Restart: `docker compose -f docker-compose.bundle.yml up -d`
3. Verify public endpoints still work: `curl http://localhost:3000/health`
4. Verify auth is enforced: `curl http://localhost:3000/api/v1/notes` (should return 401)

### Rollback

For local sidecar/dev rollback only, set both `REQUIRE_AUTH=false` and `I_UNDERSTAND_NO_AUTH=true` in `.env` and restart to disable auth.

## Automation Status

Current automation (implemented):

- [x] Docker image publishing on tag push (ghcr.io and internal registry)
- [x] Multi-architecture (`linux/amd64`, `linux/arm64`) API, MCP and bundle images
- [x] GitHub Release creation with changelog
- [x] Gitea Release creation with changelog

Planned improvements:

- [ ] Automatic CHANGELOG generation from commits
- [ ] Release notification webhooks

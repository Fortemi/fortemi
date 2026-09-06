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

- [ ] All tests passing: `cargo test --workspace -- --test-threads=1` with a disposable test database
- [ ] Linting clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Format check: `cargo fmt --all -- --check`
- [ ] Dependency policy: `cargo deny check advisories bans licenses sources`
- [ ] Documentation contract: `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict`
- [ ] CI pipeline green on main branch
- [ ] Documentation updated for new features
- [ ] No critical open issues blocking release
- [ ] If the release touches the Intel overlay or provider routing:
      `./scripts/smoke-intel-vllm.sh` passes (see
      [Intel Arc / vLLM Deployment](#/operations-intel-arc-vllm))

### Version Bump

1. Set the workspace version in `Cargo.toml` and refresh the corresponding
   workspace package entries in `Cargo.lock`.
2. Set the same version in `mcp-server/package.json` and
   `mcp-server/package-lock.json`. MCP version alignment is required for every
   server release.
3. Move the selected `CHANGELOG.md` entries into the dated release section and
   update comparison links. Add `docs/releases/vYYYY.M.PATCH-announcement.md`.

### Create Release

Commit the reviewed release files with the configured OpenBao-backed commit
signing authority. Ordinary commits and release tags use distinct keys:

```bash
git add Cargo.toml Cargo.lock mcp-server/package.json mcp-server/package-lock.json \
  CHANGELOG.md docs/releases/vYYYY.M.PATCH-announcement.md
git -c user.signingkey=62297562B1C7053088F405DB0117DAAA677A5BF2 \
  -c gpg.program=tools/git/gpg-from-openbao.sh commit -S \
  -m "chore: release vYYYY.M.PATCH"
git push origin main
```

After the required gates pass, use a clean checkout at the exact current
`origin/main` commit. If the working checkout contains unrelated files, use an
isolated clean checkout and preserve the original work. The tag wrapper checks
workspace/MCP versions, changelog, announcement, clean status and `origin/main`,
then signs and verifies with the published release key:

```bash
tools/release/cut-tag.sh YYYY.M.PATCH -m "vYYYY.M.PATCH - Release title"
git push origin vYYYY.M.PATCH
```

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
- [x] GitHub Release creation with changelog
- [x] Gitea Release creation with changelog

Planned improvements:

- [ ] Automatic CHANGELOG generation from commits
- [ ] Release notification webhooks
- [ ] Multi-architecture Docker builds (arm64)

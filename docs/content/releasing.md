# Release Process

This document describes the release process for Fortémi.

## Versioning

Fortémi uses **CalVer** (Calendar Versioning):

- Format: `YYYY.M.PATCH`
- Examples: `2026.1.0`, `2026.1.5`, `2026.12.0`
- **No leading zeros** - npm/cargo semver rejects them
- PATCH resets to 0 each month
- Git tags use `v` prefix: `v2026.1.0`

## Release Checklist

### Pre-Release

- [ ] All tests passing: `cargo test --workspace`
- [ ] Linting clean: `cargo clippy -- -D warnings`
- [ ] Format check: `cargo fmt --check`
- [ ] CI pipeline green on main branch
- [ ] Documentation updated for new features
- [ ] No critical open issues blocking release

### Version Bump

1. **Update Cargo.toml**
   ```bash
   # Edit workspace version in Cargo.toml
   # Change: version = "0.1.0"
   # To:     version = "2026.1.0"
   ```

2. **Update CHANGELOG.md**
   - Move items from `[Unreleased]` to new version section
   - Add release date
   - Update comparison links at bottom

3. **Update mcp-server/package.json** (if applicable)
   ```json
   {
     "version": "2026.1.0"
   }
   ```

### Create Release

```bash
# 1. Ensure working directory is clean
git status

# 2. Commit version changes
git add Cargo.toml CHANGELOG.md mcp-server/package.json
git commit -m "chore: release v2026.1.0"

# 3. Create annotated tag
git tag -a v2026.1.0 -m "v2026.1.0 - First CalVer release

Highlights:
- Strict tag filtering for data segregation
- W3C SKOS tagging system
- Hybrid search with RRF fusion
- MCP server with 65+ tools
- PKE encryption support"

# 4. Push to remote
git push origin main --tags
```

### CI/CD Automation

When you push the tag, the CI pipeline automatically:

1. **Runs all tests** (lint, unit tests, integration tests, container tests)
2. **Publishes Docker images** to both registries:
   - Internal: `git.integrolabs.net/fortemi/fortemi:{version}`
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

### Post-Release

- [ ] Verify release appears on [GitHub Releases](https://github.com/fortemi/fortemi/releases)
- [ ] Verify Docker images on [ghcr.io](https://ghcr.io/fortemi/fortemi)
- [ ] Update any deployment configurations
- [ ] Pull new image and restart production service if needed:
  ```bash
  docker pull ghcr.io/fortemi/fortemi:latest
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

- [ ] Deploy latest version with `REQUIRE_AUTH=false` (default)
- [ ] Register OAuth clients for all integrations: `POST /oauth/register`
- [ ] Create API keys for CLI/automation: `POST /api/v1/api-keys`
- [ ] Distribute credentials to all clients/users
- [ ] Test authentication with a sample request:
  ```bash
  curl -H "Authorization: Bearer mm_at_xxxx" https://your-domain.com/api/v1/notes
  ```

### Enable Authentication

1. Set `REQUIRE_AUTH=true` in `.env`
2. Restart: `docker compose -f docker-compose.bundle.yml up -d`
3. Verify public endpoints still work: `curl https://your-domain.com/health`
4. Verify auth is enforced: `curl https://your-domain.com/api/v1/notes` (should return 401)

### Rollback

Set `REQUIRE_AUTH=false` in `.env` and restart to disable auth immediately.

## Automation Status

Current automation (implemented):

- [x] Docker image publishing on tag push (ghcr.io and internal registry)
- [x] GitHub Release creation with changelog
- [x] Gitea Release creation with changelog

Planned improvements:

- [ ] Automatic CHANGELOG generation from commits
- [ ] Release notification webhooks
- [ ] Multi-architecture Docker builds (arm64)

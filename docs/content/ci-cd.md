# CI/CD Pipeline

## Overview

Fortémi uses Gitea Actions for continuous integration and deployment. The pipeline is configured in `.gitea/workflows/ci-builder.yaml` and runs on self-hosted runners.

**Release targets:**
- **Internal**: Container registry - Development builds
- **Public**: GitHub Container Registry (`ghcr.io/fortemi/fortemi`) - Production releases

## Pipeline Stages

### 1. Lint (runs on: matric-builder)

Validates code quality and formatting:

```yaml
- cargo fmt --all -- --check      # Enforce consistent formatting
- cargo clippy --all-targets --all-features -- -D warnings  # Catch common mistakes
```

**Triggers**: On push to main, on pull requests to main, on version tags (`v*`)

**Exit criteria**: All formatting rules pass, no clippy warnings

### 2. Build & Unit Test (runs on: matric-builder)

Compiles and runs the test suite with a dedicated PostgreSQL container:

```yaml
- cargo build --release --workspace
- cargo test --package matric-jobs --test worker_integration_test -- --test-threads=1
- cargo test --workspace --exclude matric-jobs
- cargo test --doc
```

**Dependencies**: Requires `lint` job to pass

**Test Coverage**:
- matric-core (unit tests)
- matric-db (repository tests with real PostgreSQL)
- matric-search (hybrid search tests)
- matric-inference (mock tests; real tests in integration-test job)
- matric-jobs (worker integration tests, run serially)
- matric-api (API endpoint tests)

### 3. Build Docker Image (runs on: matric-builder)

Creates the Docker image for testing:

```yaml
- docker build -t Fortémi:test .
```

**Dependencies**: Requires `build` job to pass

### 4. Test Container (runs on: matric-builder)

Deploys the built image in an isolated Docker network and runs API tests:

- Starts PostgreSQL with pgvector
- Starts Redis for caching
- Runs database migrations
- Starts the API container
- Executes `scripts/container-api-tests.sh`

**Dependencies**: Requires `build-image` job to pass

### 5. Integration Tests (runs on: titan)

Tests AI/ML functionality with Ollama (GPU-enabled):

```yaml
- nvidia-smi                    # Verify GPU access
- curl http://localhost:11434/api/tags  # Verify Ollama available
- cargo test --package matric-inference --features integration
```

**Dependencies**: Requires `build` job to pass

**Environment**:
- OLLAMA_HOST: http://localhost:11434
- INFERENCE_BACKEND: ollama
- GPU access for embedding generation

**Timeout**: 30 minutes

### 6. Publish Dev (runs on: matric-builder)

Publishes development images to internal registry on every main branch push:

**Tags published**:
- `dev` - Latest dev build
- `sha-{commit}` - Specific commit SHA
- `main` - Latest from main branch
- `bundle`, `bundle-{sha}`, `bundle-main` - All-in-one images

**Triggers**: Push to main branch only

### 7. Publish Release (runs on: matric-builder)

Publishes release images to internal registry on version tags:

**Tags published**:
- `{version}` - Semantic version (e.g., `2026.2.0`)
- `latest` - Latest stable release
- `bundle-{version}`, `bundle-latest` - All-in-one images

**Triggers**: Version tags only (`v*`)

### 8. Create Gitea Release

Creates a release on the internal Gitea instance with changelog extraction from `CHANGELOG.md`.

**Dependencies**: Requires `publish-release` job to pass

### 9. Publish to GitHub (ghcr.io)

Publishes release images to GitHub Container Registry for public distribution:

```yaml
IMAGE: ghcr.io/fortemi/fortemi
Tags: {version}, latest, bundle-{version}, bundle-latest
```

**Dependencies**: Requires `test-container` and `integration-test` jobs to pass

**Triggers**: Version tags only (`v*`)

### 10. Create GitHub Release

Creates a public release on GitHub with:
- Changelog extracted from `CHANGELOG.md`
- Installation instructions for Docker
- Quick start commands

**Dependencies**: Requires `publish-github` job to pass

## Self-Hosted Runners

The pipeline uses two self-hosted runners:

### matric-builder (Docker-in-Docker)
- Lint, Build, Test
- Docker image builds
- Container testing
- Registry publishing

### titan (GPU-enabled)
- Integration tests with Ollama
- Requires GPU for embedding generation

## Secrets Required

### BUILD_REPO_TOKEN
Used for internal container registry authentication:

```yaml
echo "${{ secrets.BUILD_REPO_TOKEN }}" | docker login $REGISTRY -u ${{ gitea.actor }} --password-stdin
```

### GH_PUBLISH_TOKEN
Used for GitHub Container Registry and GitHub Releases. Create a GitHub Personal Access Token (classic) with these scopes:
- `write:packages` - Push images to ghcr.io
- `contents:write` - Create releases
- `repo` - Full repository access (for private repos)

```yaml
echo "${{ secrets.GH_PUBLISH_TOKEN }}" | docker login ghcr.io -u fortemi --password-stdin
```

**Setup instructions:**
1. Go to GitHub → Settings → Developer settings → Personal access tokens → Tokens (classic)
2. Generate new token with required scopes
3. Add to Gitea repository → Settings → Secrets → Add secret named `GH_PUBLISH_TOKEN`
4. The PAT must belong to a member of the `fortemi` GitHub organization

## Triggers

### Push to main
```yaml
on:
  push:
    branches: [main]
```

Runs full pipeline including internal dev publish. Does NOT publish to GitHub.

### Pull Requests
```yaml
on:
  pull_request:
    branches: [main]
```

Runs lint, build, test-container, and integration-test. Does NOT publish.

### Version Tags
```yaml
on:
  push:
    tags: ['v*']
```

Runs full pipeline including:
- Internal registry publish (release images)
- Gitea release creation
- GitHub Container Registry publish
- GitHub release creation

## Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md` with release notes
3. Commit: `git commit -m "chore: release v2026.2.0"`
4. Tag: `git tag -a v2026.2.0 -m "v2026.2.0"`
5. Push: `git push origin main --tags`

The CI pipeline will automatically:
- Run all tests
- Build and publish Docker images to both registries
- Create releases on both Gitea and GitHub

## Local Development

To match CI checks locally:

```bash
# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --workspace
cargo test --doc

# Build release
cargo build --release --workspace
```

## Docker Images

Two image variants are published:

### API-only (`Fortémi:{version}`)
- Requires external PostgreSQL database
- Suitable for Kubernetes/container orchestration
- Smaller image size

### Bundle (`Fortémi:bundle-{version}`)
- All-in-one with embedded PostgreSQL
- No external dependencies
- Suitable for quick starts and single-node deployments

## Troubleshooting

### Clippy Warnings Block CI

If clippy fails, the pipeline stops. Fix locally:

```bash
cargo clippy --all-targets --all-features -- -D warnings
# Fix all warnings, then commit
```

### Formatting Issues

```bash
cargo fmt --all
git add .
git commit -m "style: apply cargo fmt formatting"
```

### Integration Tests Timeout

GPU runner has 30-minute timeout. If Ollama tests exceed this:
- Check Ollama service health on titan runner
- Verify model availability: `curl http://localhost:11434/api/tags`
- Check for GPU memory issues: `nvidia-smi`

### GitHub Push Fails

Verify the `GH_PUBLISH_TOKEN` secret:
- Token must have `write:packages` scope
- Token must not be expired
- Token owner must be a member of the `fortemi` organization with push access

### GitHub Release Creation Fails

- Check if release already exists (HTTP 422 response)
- Verify token has `contents:write` or `repo` scope
- Check GitHub API rate limits

## Future Enhancements

- [ ] Add test coverage reporting
- [ ] Cache cargo dependencies between runs
- [ ] Multi-architecture Docker builds (arm64, armv7)
- [ ] Security scanning (cargo-audit, trivy)
- [ ] Automatic SBOM generation

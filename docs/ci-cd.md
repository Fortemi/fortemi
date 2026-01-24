# CI/CD Pipeline

## Overview

matric-memory uses Gitea Actions for continuous integration and deployment. The pipeline is configured in `.gitea/workflows/ci.yaml` and runs on self-hosted runners.

## Pipeline Stages

### 1. Lint (runs on: titan)

Validates code quality and formatting:

```yaml
- cargo fmt --all -- --check      # Enforce consistent formatting
- cargo clippy --all-targets --all-features -- -D warnings  # Catch common mistakes
```

**Triggers**: On push to main, on pull requests to main

**Exit criteria**: All formatting rules pass, no clippy warnings

### 2. Test (runs on: titan)

Runs the test suite:

```yaml
- cargo test --workspace   # Unit tests across all crates
- cargo test --doc         # Documentation tests
```

**Dependencies**: Requires `lint` job to pass

**Coverage**:
- matric-core
- matric-db
- matric-search
- matric-inference
- matric-jobs
- matric-api

### 3. Integration Tests (runs on: gpu)

Tests AI/ML functionality with Ollama:

```yaml
- nvidia-smi                    # Verify GPU access
- curl http://localhost:11434/api/tags  # Verify Ollama available
- cargo test --package matric-inference --features integration
```

**Dependencies**: Requires `lint` job to pass

**Environment**:
- OLLAMA_HOST: http://localhost:11434
- INFERENCE_BACKEND: ollama
- GPU access for embedding generation

**Timeout**: 30 minutes

### 4. Build (runs on: titan)

Creates release binaries:

```yaml
- cargo build --release --workspace
```

**Dependencies**: Requires `test` job to pass (integration tests optional)

**Artifacts**:
- target/release/matric-api
- target/release/matric-jobs (if applicable)

### 5. Publish (runs on: gpu)

Builds and publishes Docker images:

```yaml
- docker build -t git.integrolabs.net/roctinam/matric-memory:${SHORT_SHA} .
- docker push git.integrolabs.net/roctinam/matric-memory:${GITHUB_REF_NAME}
- docker push git.integrolabs.net/roctinam/matric-memory:sha-${SHORT_SHA}
```

**Dependencies**: Requires `build` job to pass

**Registry**: git.integrolabs.net

**Tags**:
- `main` - Latest from main branch
- `sha-{commit}` - Specific commit SHA (first 7 chars)

## Self-Hosted Runners

The pipeline uses two self-hosted runners:

### titan (CPU-only)
- Lint
- Test
- Build

### gpu (GPU-enabled)
- Integration tests (requires Ollama + GPU for embeddings)
- Publish (Docker build/push)

## Environment Variables

### Global (all jobs)
```yaml
CARGO_TERM_COLOR: always
RUST_BACKTRACE: 1
```

### Integration tests only
```yaml
OLLAMA_HOST: http://localhost:11434
INFERENCE_BACKEND: ollama
```

## Secrets Required

### BUILD_REPO_TOKEN
Used for Docker registry authentication in the publish job:

```yaml
echo "${{ secrets.BUILD_REPO_TOKEN }}" | docker login git.integrolabs.net -u ${{ gitea.actor }} --password-stdin
```

## Triggers

### Push to main
```yaml
on:
  push:
    branches: [main]
```

Runs full pipeline including publish step.

### Pull Requests
```yaml
on:
  pull_request:
    branches: [main]
```

Runs lint, test, integration-test, and build. Does NOT publish.

## Recent CI Compliance

Recent commits show CI compliance:

```
0b13bef docs(research): add comprehensive Ollama model performance testing and analysis
159fc51 fix: address clippy warnings for CI compliance
945dd1a style: apply cargo fmt formatting
```

The pipeline enforces:
- Zero clippy warnings (`-D warnings`)
- Consistent formatting (`cargo fmt --check`)
- All tests passing

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

## Docker Build

The Dockerfile at the repository root is used for containerization. See the main README for container usage.

## Monitoring

Check pipeline status:
- Gitea web UI: https://git.integrolabs.net/roctinam/matric-memory/actions
- Recent commits show if checks passed

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
- Check Ollama service health on gpu runner
- Verify model availability: `curl http://localhost:11434/api/tags`
- Check for GPU memory issues: `nvidia-smi`

### Docker Push Fails

Verify secrets:
- `BUILD_REPO_TOKEN` must be set in repository secrets
- Token must have registry push permissions

## Future Enhancements

Potential improvements for future versions:

- [ ] Add test coverage reporting
- [ ] Cache cargo dependencies between runs
- [ ] Deploy to staging/production automatically
- [ ] Add performance benchmarks
- [ ] Security scanning (cargo-audit, trivy)
- [ ] Multi-architecture Docker builds (arm64)

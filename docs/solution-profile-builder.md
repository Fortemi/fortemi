# Solution Profile: Docker Builder Pattern for CI/CD

## Summary

Implement a Docker-in-Docker builder pattern for matric-memory CI/CD, following the roko-builder approach. This creates a self-contained build environment that eliminates runner dependency issues and ensures consistent, reproducible builds.

## Problem Statement

### Current State

The matric-memory CI pipeline relies on tools installed directly on self-hosted runners (`titan`, `gpu`):

```yaml
# Current approach - fragile
runs-on: titan
steps:
  - run: rustup component add rustfmt clippy  # Assumes rustup exists
  - run: cargo fmt --all -- --check           # Assumes cargo exists
```

### Issues

1. **Runner Environment Drift**: Host-installed tools may differ between runners or change over time
2. **Inconsistent Builds**: Local dev environment may not match CI, causing "works on my machine" problems
3. **Setup Burden**: New runners require manual Rust toolchain installation and configuration
4. **Version Pinning**: Hard to ensure specific Rust version (1.92.0) across all environments
5. **Dependency Management**: sqlx-cli, protobuf, and other tools must be installed on hosts

## Proposed Solution

### Docker Builder Pattern ("Docker Building Docker")

Create a builder image that contains all build tools, then use that image to build deployment images:

```
┌─────────────────────────────────────────────────────────────┐
│                    Gitea Actions Runner                      │
│  ┌───────────────────────────────────────────────────────┐  │
│  │            matric-builder:latest container             │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  Rust 1.92.0 + clippy + rustfmt                 │  │  │
│  │  │  sqlx-cli for migrations                        │  │  │
│  │  │  Docker CLI (builds deployment images)          │  │  │
│  │  │  Node.js for MCP server build                   │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  │                         │                              │  │
│  │                         ▼                              │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  Builds matric-memory:dev deployment image      │  │  │
│  │  │  Runs cargo test, clippy, fmt inside container  │  │  │
│  │  │  Pushes to git.integrolabs.net registry         │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Key Benefits

1. **Reproducibility**: Same toolchain version everywhere
2. **Isolation**: Build environment completely independent of runner host
3. **Versioning**: Builder image tagged, can pin exact build environment
4. **Portability**: Any Docker-capable runner can build matric-memory
5. **Local Parity**: Developers can use same builder image locally

## Architecture

### Directory Structure

```
build/
├── Dockerfile.builder       # Builder image definition
├── Dockerfile              # Multi-stage deployment image
├── docker-compose.test.yml # Integration test infrastructure
├── README.md               # Usage documentation
├── RUNNER_SETUP.md         # Runner configuration guide
└── scripts/
    ├── entrypoint.sh       # Docker-in-Docker daemon manager
    └── build-and-push.sh   # Builder image management
```

### Builder Image Contents

| Component | Version | Purpose |
|-----------|---------|---------|
| Rust | 1.92.0 | Core toolchain (matches project's Cargo.lock requirements) |
| rustfmt | bundled | Code formatting |
| clippy | bundled | Linting |
| sqlx-cli | 0.8.x | Migration management (installed with --locked) |
| docker-cli | latest | Image building |
| Node.js | 22.x LTS | MCP server build |
| curl/jq | latest | CI operations |

### CI Workflow Changes

**Before** (current):
```yaml
jobs:
  lint:
    runs-on: titan
    steps:
      - run: rustup component add rustfmt clippy
      - run: cargo fmt --all -- --check
```

**After** (proposed):
```yaml
jobs:
  lint:
    runs-on: matric-builder
    steps:
      # No tool installation needed - everything in builder image
      - run: cargo fmt --all -- --check
```

### Runner Label Strategy

Register builder containers with specific labels:

| Label | Image | Use Case |
|-------|-------|----------|
| `matric-builder` | matric-builder:latest | Lint, test, build jobs |
| `matric-builder-gpu` | matric-builder:gpu | GPU-enabled integration tests |

## Implementation Plan

### Phase 1: Builder Image (Issues #186-187)

1. Create `build/Dockerfile.builder` with all build tools
2. Add `build/scripts/entrypoint.sh` for Docker-in-Docker support
3. Document builder image usage in `build/README.md`
4. Manual build and push to registry for initial testing

**Deliverables:**
- `build/Dockerfile.builder`
- `build/scripts/entrypoint.sh`
- `build/README.md`
- Image at `git.integrolabs.net/roctinam/matric-memory/builder:latest`

### Phase 2: Runner Configuration (Issue #188)

1. Create `build/RUNNER_SETUP.md` with runner registration guide
2. Document systemd service configuration
3. Register `matric-builder` label on titan runner
4. Test basic workflow execution

**Deliverables:**
- `build/RUNNER_SETUP.md`
- Working `matric-builder` runner label

### Phase 3: CI Migration (Issue #189)

1. Update `.gitea/workflows/ci.yaml` to use `matric-builder` runner
2. Remove explicit tool installation steps
3. Add workflow for builder image updates
4. Test full CI pipeline

**Deliverables:**
- Updated `ci.yaml`
- New `build-builder.yaml` workflow
- Green CI pipeline

### Phase 4: Integration Testing (Issue #190)

1. Create `build/docker-compose.test.yml` for local testing
2. Add integration test job that pulls built image and runs tests
3. Document end-to-end testing workflow

**Deliverables:**
- `build/docker-compose.test.yml`
- Integration test CI job
- Testing documentation

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Docker-in-Docker privilege issues | High | Test thoroughly on runner, document required permissions |
| GPU passthrough complexity | Medium | Separate gpu-enabled builder variant |
| Initial setup complexity | Medium | Comprehensive documentation, step-by-step guides |
| Builder image bloat | Low | Multi-stage build, minimal base image |

## Success Criteria

- [ ] Builder image builds and pushes successfully
- [ ] CI pipeline runs inside builder container
- [ ] All existing tests pass in containerized environment
- [ ] GPU integration tests work with builder
- [ ] Documentation complete for runner setup
- [ ] Local development can use builder image

## References

- roko/roko-builder - Reference implementation
- roctinam/asms#69 - Similar implementation for Python project
- Gitea Actions runner documentation
- Docker-in-Docker best practices

## Timeline

| Phase | Estimated Effort |
|-------|-----------------|
| Phase 1: Builder Image | Initial implementation |
| Phase 2: Runner Config | Runner registration |
| Phase 3: CI Migration | Workflow updates |
| Phase 4: Integration | Testing infrastructure |

## Appendix: Current vs Proposed CI Comparison

### Current CI Jobs

```
lint → test → integration-test → build → publish-dev
  ↓      ↓           ↓              ↓          ↓
titan  titan        gpu          titan       gpu
```

### Proposed CI Jobs

```
lint → test → integration-test → build → publish-dev
  ↓      ↓           ↓              ↓          ↓
matric-builder (all jobs in container, gpu passthrough for integration)
```

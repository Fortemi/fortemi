# Fortémi Build Infrastructure

This directory contains the Docker builder pattern infrastructure for Fortémi CI/CD.

## Overview

The builder pattern uses a dedicated Docker image containing all build tools:

```
┌─────────────────────────────────────────┐
│         matric-builder image            │
│  ┌───────────────────────────────────┐  │
│  │  Rust 1.82.0 + rustfmt + clippy   │  │
│  │  sqlx-cli for migrations          │  │
│  │  Docker CLI (builds app images)   │  │
│  │  Node.js 22.x for MCP server      │  │
│  │  PostgreSQL client                │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

## Quick Start

### Build the Builder Image

```bash
# From repository root
docker build -f build/Dockerfile.builder -t matric-builder .
```

### Use for Local Development

```bash
# Run tests in builder container
docker run --rm -v $(pwd):/build -w /build matric-builder cargo test

# Interactive shell
docker run --rm -it -v $(pwd):/build -w /build matric-builder

# Run clippy
docker run --rm -v $(pwd):/build -w /build matric-builder \
    cargo clippy --all-targets --all-features -- -D warnings
```

### Build Application Image (Docker-in-Docker)

```bash
# Mount Docker socket for image building
docker run --rm \
    -v $(pwd):/build \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -w /build \
    matric-builder \
    docker build -t fortemi:local .
```

## Directory Contents

```
build/
├── Dockerfile.builder       # Builder image definition
├── README.md               # This file
├── RUNNER_SETUP.md         # Gitea runner configuration guide
├── docker-compose.test.yml # Integration test infrastructure
└── scripts/
    └── entrypoint.sh       # Docker-in-Docker setup script
```

## Builder Image Contents

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.92.0 | Core toolchain |
| rustfmt | (bundled) | Code formatting |
| clippy | (bundled) | Linting |
| sqlx-cli | 0.8.x | Database migrations |
| docker-cli | latest | Image building |
| Node.js | 22.x | MCP server build |
| PostgreSQL client | latest | Database operations |

## CI/CD Integration

### Registry Location

```
ghcr.io/fortemi/fortemi/builder:latest
```

### Workflow Usage

```yaml
jobs:
  lint:
    runs-on: matric-builder  # Uses builder container
    steps:
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
```

### Updating the Builder Image

1. Modify `build/Dockerfile.builder`
2. Build and test locally:
   ```bash
   docker build -f build/Dockerfile.builder -t matric-builder:test .
   docker run --rm -v $(pwd):/build -w /build matric-builder:test cargo test
   ```
3. Push to registry:
   ```bash
   docker tag matric-builder:test \
       ghcr.io/fortemi/fortemi/builder:latest
   docker push ghcr.io/fortemi/fortemi/builder:latest
   ```

## Integration Testing

Use the docker-compose configuration to test built images:

```bash
# Start test environment
docker compose -f build/docker-compose.test.yml up -d

# Wait for services
docker compose -f build/docker-compose.test.yml ps

# Run health check
curl http://localhost:3001/health

# Cleanup
docker compose -f build/docker-compose.test.yml down -v
```

## Troubleshooting

### Permission Denied on Docker Socket

When running Docker-in-Docker, the builder needs access to the host's Docker socket:

```bash
# Check socket permissions
ls -la /var/run/docker.sock

# May need to add user to docker group or adjust permissions
sudo chmod 666 /var/run/docker.sock
```

### Cargo Cache

For faster builds, mount a cargo cache volume:

```bash
docker run --rm \
    -v $(pwd):/build \
    -v matric-cargo-cache:/root/.cargo/registry \
    -w /build \
    matric-builder cargo build
```

### Database Connection in Tests

Some tests require a PostgreSQL connection. Use the test compose file:

```bash
docker compose -f build/docker-compose.test.yml up -d db
export DATABASE_URL="postgres://matric:matric@localhost:5432/matric"
docker run --rm \
    -v $(pwd):/build \
    --network host \
    -e DATABASE_URL \
    -w /build \
    matric-builder cargo test
```

## References

- [Solution Profile](../docs/solution-profile-builder.md) - Architecture decisions
- [RUNNER_SETUP.md](./RUNNER_SETUP.md) - Gitea runner configuration
- [CI/CD Documentation](../docs/ci-cd.md) - Pipeline overview

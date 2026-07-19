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

### Build Application Image Through the Host Daemon (Trusted Opt-In)

The socket-backed example below grants the builder root-equivalent host control.
Use it only on a dedicated builder host or VM for trusted jobs. Do not co-locate
untrusted pull-request workloads or general user workloads on that host.
Mounting the socket `:ro` does not constrain Docker API operations and is not a
security boundary. Prefer a rootless daemon or an authenticated remote BuildKit
builder when the job does not require control of the host daemon.

```bash
# Trusted, dedicated builder host only.
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
    └── entrypoint.sh       # Optional host-daemon socket detection
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

Do not make the socket world-writable. Membership in the `docker` group is
root-equivalent host control, so grant it only to the dedicated trusted runner
account. If direct daemon access is not required, remove the socket mount and
use a rootless or authenticated remote builder instead.

```bash
# Inspect the daemon-owned socket and group.
stat -c '%A %U %G %n' /var/run/docker.sock
getent group docker
id runner

# On a dedicated trusted runner, add the service account and restart its session.
sudo usermod -aG docker runner
sudo systemctl restart act_runner
```

If the socket owner or mode is not the distribution default (normally
`root:docker` and `0660`), repair the Docker service/package configuration and
restart Docker instead of applying an ad hoc permissive mode. See
[RUNNER_SETUP.md](./RUNNER_SETUP.md) for the runner threat boundary and remote
builder alternatives.

This guidance covers trusted internal CI builders. End-user bundle access to
the Docker daemon is a separate product boundary tracked in issue #937; runner
configuration must not be copied into the user-facing bundle.

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

## fortemi-docs Shard Regeneration

The bundle image bakes `docker/seed-data/fortemi-docs.shard` (the in-product support memory archive) into `/app/seed-data` so first-boot seeding has a known-good corpus to import.

**CI rebuild (automatic):** `ci-builder.yaml`'s `publish-dev` and `publish-release` jobs run `scripts/ci/rebuild-shard-in-ci.sh ${IMAGE}:<tag>` after building the API-only image and before building the bundle. The helper stands up a transient Postgres + API stack on an isolated Docker network, waits for `/health` (the API auto-runs sqlx migrations on startup), runs `scripts/rebuild-docs-shard.sh` to import the current source tree, and tears the stack down. Bundle builds fail loudly if the rebuild fails — we never ship a stale shard. See issue #652.

**Manual rebuild (ad-hoc testing):** with a Fortémi instance already running locally:

```bash
scripts/rebuild-docs-shard.sh http://localhost:3000
git add docker/seed-data/fortemi-docs.shard
git commit -m "chore(seed): rebuild fortemi-docs shard"
```

The CI version doesn't commit; it just hands the freshly-written file to the immediately-following `docker build -f Dockerfile.bundle`.

**What the shard contains:** `docs/**/*.md`, `.aiwg/**/*.md`, `CHANGELOG.md`, `README.md` — imported as notes with `revision_mode: "none"` (no inference) and tagged from path-based rules in `scripts/rebuild-docs-shard.sh:69-230`.

## References

- [Solution Profile](../docs/solution-profile-builder.md) - Architecture decisions
- [RUNNER_SETUP.md](./RUNNER_SETUP.md) - Gitea runner configuration
- [CI/CD Documentation](../docs/ci-cd.md) - Pipeline overview

# Gitea Runner Setup for matric-builder

## Why Build Containers?

Use a dedicated runner host or VM for socket-backed build jobs. Build containers
provide:

1. **Isolation** - CI builds don't modify system Rust/toolchain versions
2. **Reproducibility** - Same environment across all builds
3. **No Version Conflicts** - Developers can use different Rust versions locally
4. **Clean Builds** - Each job starts fresh, no leftover state

These properties isolate the toolchain, not the host Docker control plane. A
job with Docker socket access has root-equivalent host control.

### Runner Label Strategy

| Label | Type | Purpose |
|-------|------|---------|
| `matric-builder` | Docker | Trusted builds with optional host-daemon access |
| `titan` | Host | Direct system access, local services |
| `gpu` | Host | GPU access for ML/inference tests |

**Rule**: Use `matric-builder` for trusted builds. Do not route untrusted fork
or public pull-request workloads to a socket-backed runner. Use `titan`/`gpu`
only when direct hardware or local service access is required.

---

This guide explains how to configure a Gitea Actions runner to use the matric-builder container.

## Prerequisites

- Docker installed on runner host
- Gitea Actions runner (act_runner) installed
- Access to `ghcr.io` container registry
- Dedicated runner host or VM and service account, with no general user
  workloads or untrusted CI co-tenancy

## Runner Registration

### 1. Pull the Builder Image

```bash
# Login to registry
docker login ghcr.io

# Pull latest builder
docker pull ghcr.io/fortemi/fortemi/builder:latest
```

### 2. Configure Runner Labels

Edit the runner configuration to add the `matric-builder` label:

```yaml
# ~/.config/act_runner/config.yaml (or /etc/act_runner/config.yaml)
runner:
  labels:
    - "matric-builder:docker://ghcr.io/fortemi/fortemi/builder:latest"
    - "titan:host"  # Keep existing labels
```

The label format is: `label-name:docker://image-name`

### 3. Register the Runner

```bash
# If not already registered
act_runner register \
    --instance https://github.com \
    --token <YOUR_RUNNER_TOKEN> \
    --labels "matric-builder:docker://ghcr.io/fortemi/fortemi/builder:latest"
```

### 4. Start/Restart the Runner

```bash
# Systemd service
sudo systemctl restart act_runner

# Or manually
act_runner daemon
```

## Systemd Service Configuration

Create or update `/etc/systemd/system/act_runner.service`:

```ini
[Unit]
Description=Gitea Actions Runner
After=docker.service
Requires=docker.service

[Service]
Type=simple
User=runner
Group=docker
WorkingDirectory=/home/runner
ExecStart=/usr/local/bin/act_runner daemon
Restart=always
RestartSec=10

# Environment
Environment=DOCKER_HOST=unix:///var/run/docker.sock

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable act_runner
sudo systemctl start act_runner
```

The `docker` group grants root-equivalent host control. Use this service
configuration only for the dedicated trusted builder. A runner configured only
for host execution, with no Docker-backed labels or jobs, should omit
`Group=docker` and `DOCKER_HOST`.

## Host Docker Daemon Configuration (Trusted Opt-In)

Only jobs that must build or inspect container images should receive host
daemon access. The following opt-in configuration is for a dedicated runner
host or VM that executes trusted workflows only.

### Runner Config

```yaml
# ~/.config/act_runner/config.yaml
container:
  options: |
    -v /var/run/docker.sock:/var/run/docker.sock
  privileged: false
  docker_host: unix:///var/run/docker.sock
```

### Security Considerations

Treat a Docker-socket runner as root-equivalent host control. Any workflow step
that can talk to `/var/run/docker.sock` can build, run, and mount host resources
through Docker. `privileged: false` constrains the job container itself but does
not constrain commands sent to the host daemon. A `:ro` socket mount still
allows mutating Docker API calls and is not a security boundary. Keep the
following posture:

- `matric-builder` is the default label for CI builds and pull-request tests.
  Limit repository write and pull-request access to trusted contributors; do
  not attach a socket-backed runner to public fork workloads. Publish jobs and
  other secret-bearing jobs must keep job-level `if:` guards that exclude
  pull-request execution. The lint step
  `scripts/ci/verify-release-job-guards.py` enforces this for workflows with a
  `pull_request` trigger.
- `titan` and `gpu` labels are hardware/local-service exceptions. Avoid using
  them for registry publish, package publish, or other secret-bearing jobs
  unless the workflow has been explicitly reviewed for host exposure.
- Keep runner registration tokens, PATs, deploy keys, and registry tokens out of
  the repository and out of runner labels. Rotate any token that appears in
  logs, config examples, or shell history.
- Prefer the repository workflow token for repository-scoped Gitea operations.
  Use PAT-style secrets only where Gitea package, release, or external registry
  APIs require them.
- `BUILD_REPO_TOKEN` is for internal Gitea registry/package publish flows. Keep
  it limited to Fortemi package and release publishing; do not reuse it as a
  general admin token.
- `GH_PUBLISH_TOKEN` is for GHCR and public GitHub release publishing. The
  expected minimum permissions are `write:packages` and `contents:write`; add
  broader repository access only when the target repository privacy model
  requires it.
- For high-assurance runners, pin the `matric-builder` label image to an
  immutable digest and rotate it intentionally. If the label uses `:latest`,
  treat that as an operator convenience alias and pull only from the trusted
  registry.
- Do not make the socket world-writable. The expected distribution default is
  normally `root:docker` with mode `0660`; repair the Docker service/package
  configuration if ownership or mode drifts.

```bash
# Add runner user to docker group
sudo usermod -aG docker runner

# Verify
groups runner
```

### Lower-Blast-Radius Alternatives

Prefer one of these designs when host-daemon control is unnecessary:

- Run rootless Docker under the dedicated runner account and expose only that
  account's Unix socket.
- Use a remote BuildKit builder over mutually authenticated TLS, with a
  dedicated builder identity, network allowlist, and no unauthenticated Docker
  TCP listener.
- Use an ephemeral VM per trusted build and destroy it after artifact
  publication.

A Docker API proxy is not automatically safe: the proxy itself still controls
the host socket. Use one only after enumerating and enforcing the exact API
methods required by the workflow.

## GPU Runner (matric-builder-gpu)

For GPU-enabled builds (integration tests with Ollama):

### 1. Install NVIDIA Container Toolkit

```bash
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/nvidia-docker/gpgkey | sudo apt-key add -
curl -s -L https://nvidia.github.io/nvidia-docker/$distribution/nvidia-docker.list | \
    sudo tee /etc/apt/sources.list.d/nvidia-docker.list

sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit
sudo systemctl restart docker
```

### 2. Configure GPU Label

```yaml
# ~/.config/act_runner/config.yaml
runner:
  labels:
    - "matric-builder-gpu:docker://ghcr.io/fortemi/fortemi/builder:gpu"

container:
  options: |
    --gpus all
```

GPU inference jobs do not receive the Docker socket by default. If a trusted GPU
job also needs to build images, use the dedicated opt-in runner boundary above
rather than adding a global socket mount to the GPU label.

## Verification

### Check Runner Status

```bash
# View runner status in Gitea UI
# Settings → Actions → Runners

# Or check systemd
systemctl status act_runner

# View logs
journalctl -u act_runner -f
```

### Test Job Execution

Create a test workflow:

```yaml
# .gitea/workflows/test-builder.yml
name: Test Builder
on: workflow_dispatch

jobs:
  test:
    runs-on: matric-builder
    steps:
      - run: rustc --version
      - run: cargo --version
      - run: docker --version
      - run: node --version
```

Trigger manually and verify all commands succeed.

## Troubleshooting

### Runner Not Picking Up Jobs

1. Check runner is online in Gitea UI
2. Verify label matches workflow `runs-on`
3. Check runner logs: `journalctl -u act_runner -f`

### Container Pull Failures

```bash
# Verify registry access
docker pull ghcr.io/fortemi/fortemi/builder:latest

# Check authentication
docker login ghcr.io
```

### Permission Issues

```bash
# Check Docker socket
stat -c '%A %U %G %n' /var/run/docker.sock
# Should be: srw-rw---- 1 root docker ...

# Verify user in docker group
id runner
# Should include: groups=...,docker(...)
```

Do not use a world-writable mode to bypass access errors. On the dedicated
trusted runner, repair Docker's `root:docker` ownership/mode through the service
or package configuration, add only the runner service account to the group, and
restart the runner session. Otherwise remove direct socket access and use a
rootless or mutually authenticated remote builder.

### Out of Disk Space

The primary CI, test, and Linux sidecar workflows run a capacity sentinel before
their build fan-out. It checks both the free blocks and free inodes on the
filesystem backing `GITHUB_WORKSPACE`. Defaults are:

- `RUNNER_MIN_FREE_KIB=41943040` (40 GiB)
- `RUNNER_MIN_FREE_INODES=1000000`

Set repository variables with those names only after measuring a successful
maximum-load run. Invalid, zero, or unavailable values fail closed. A failed
`Runner Capacity Preflight` is an infrastructure result; do not reinterpret it
as a test failure or bypass the dependency.

#### Diagnose

Use read-only diagnostics first:

```bash
df -hP /var/lib/docker /var/lib/act_runner
df -iP /var/lib/docker /var/lib/act_runner
docker system df -v
sudo journalctl -u act_runner --since '24 hours ago' |
  grep -E 'no space left|ENOSPC|disk quota'
```

Record free bytes, free inodes, Docker image/build-cache usage, active runner
jobs, and the largest runner work directories in the incident. A full
filesystem can prevent the preflight container from executing at all; the
sentinel job name still isolates the failure before the expensive build fan-out.

#### Drain Before Cleanup

1. Disable or drain the runner in Gitea so it accepts no new jobs.
2. Wait for every active job on that runner to finish or cancel it explicitly.
3. Capture the read-only diagnostics above.
4. Remove only resources proven stale, then rerun the diagnostics.
5. Re-enable the runner and dispatch the failed immutable commit SHA.

Do not run `docker system prune -a` or `docker volume prune` on an active
runner. Broad volume pruning can destroy test or service state, and an age
filter alone does not prove that a resource is unrelated to a running job.

For a drained dedicated runner, a bounded cache policy can use commands such as:

```bash
# Stopped containers older than one day.
docker container prune --force --filter 'until=24h'

# Dangling images older than seven days. This does not remove all unused images.
docker image prune --force --filter 'until=168h'

# BuildKit cache older than seven days, retaining up to 40 GiB.
docker builder prune --force --filter 'until=168h' --keep-storage 40GB
```

Review the candidates and retention values for the installed Docker/BuildKit
version before automating them. Do not automate volume deletion. Give
long-lived images and volumes ownership labels, and make cleanup select only
the runner-owned labels.

#### Continuous Monitoring

Install the repository guard on the runner and call it from the host scheduler
against the filesystem that backs both Actions workspaces and Docker:

```ini
# /etc/systemd/system/fortemi-runner-capacity.service
[Unit]
Description=Check Fortemi runner disk and inode capacity

[Service]
Type=oneshot
Environment=RUNNER_MIN_FREE_KIB=41943040
Environment=RUNNER_MIN_FREE_INODES=1000000
ExecStart=/opt/fortemi/scripts/ci/check-runner-capacity.sh --path /var/lib/docker
```

```ini
# /etc/systemd/system/fortemi-runner-capacity.timer
[Unit]
Description=Monitor Fortemi runner capacity every five minutes

[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
Persistent=true

[Install]
WantedBy=timers.target
```

Connect unit failures to the host's existing alerting. After installing or
updating the script:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now fortemi-runner-capacity.timer
systemctl list-timers fortemi-runner-capacity.timer
```

The scheduled monitor and workflow sentinel use the same thresholds. Cleanup
remains a separate, drain-required operator action so monitoring cannot delete
active-job state.

## Boundary With the User-Facing Bundle

This document applies only to trusted internal CI builders. It does not
authorize mounting the Docker socket into Fortemi's end-user bundle,
sidecars, or application services. Bundle daemon access and the `autoheal`
boundary are tracked separately in issue #937.

## Updating the Builder Image

When the builder image is updated:

```bash
# Pull new image
docker pull ghcr.io/fortemi/fortemi/builder:latest

# Restart runner to pick up changes
sudo systemctl restart act_runner
```

No configuration changes needed - the runner will use the new image automatically.

## References

- [Gitea Actions Documentation](https://docs.gitea.com/usage/actions/overview)
- [act_runner Documentation](https://gitea.com/gitea/act_runner)
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html)

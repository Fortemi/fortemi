# Gitea Runner Setup for matric-builder

## Why Build Containers?

At Integro Labs, our build servers are also development servers. Using build containers provides:

1. **Isolation** - CI builds don't modify system Rust/toolchain versions
2. **Reproducibility** - Same environment across all builds
3. **No Version Conflicts** - Developers can use different Rust versions locally
4. **Clean Builds** - Each job starts fresh, no leftover state

### Runner Label Strategy

| Label | Type | Purpose |
|-------|------|---------|
| `matric-builder` | Docker | Isolated Rust builds, Docker-in-Docker |
| `titan` | Host | Direct system access, local services |
| `gpu` | Host | GPU access for ML/inference tests |

**Rule**: Use `matric-builder` for builds. Use `titan`/`gpu` only when you need direct hardware or local service access.

---

This guide explains how to configure a Gitea Actions runner to use the matric-builder container.

## Prerequisites

- Docker installed on runner host
- Gitea Actions runner (act_runner) installed
- Access to `ghcr.io` container registry

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

## Docker-in-Docker Configuration

For jobs that need to build Docker images, mount the Docker socket:

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

Mounting the Docker socket grants significant privileges:

1. **User permissions**: Ensure the runner user is in the `docker` group
2. **Socket permissions**: The socket must be accessible to the container
3. **Network isolation**: Consider using Docker networks to isolate builds

```bash
# Add runner user to docker group
sudo usermod -aG docker runner

# Verify
groups runner
```

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
    -v /var/run/docker.sock:/var/run/docker.sock
```

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
ls -la /var/run/docker.sock
# Should be: srw-rw---- 1 root docker ...

# Verify user in docker group
id runner
# Should include: groups=...,docker(...)
```

### Out of Disk Space

Builder images and caches can consume significant space:

```bash
# Check Docker disk usage
docker system df

# Clean up
docker system prune -a
docker volume prune
```

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

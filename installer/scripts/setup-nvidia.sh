#!/usr/bin/env bash
set -euo pipefail

# Configure NVIDIA Container Toolkit for GPU profiles
# Only runs on Linux when COMPOSE_PROFILES includes "gpu"

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"

# Double-check we're on Linux (when condition should catch this, but be safe)
if [ "$(uname -s)" != "Linux" ]; then
    echo "Skipping NVIDIA setup — not on Linux."
    exit 0
fi

echo "Checking NVIDIA GPU setup..."

if ! command -v nvidia-smi &>/dev/null; then
    echo "ERROR: nvidia-smi not found. Install NVIDIA drivers first."
    echo "  https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html"
    exit 1
fi

echo "GPU detected:"
nvidia-smi --query-gpu=name,memory.total --format=csv,noheader

# Check if nvidia runtime is configured for Docker
if ! docker info 2>/dev/null | grep -q "Default Runtime.*nvidia"; then
    echo ""
    echo "WARNING: Docker is not configured with nvidia as default runtime."
    echo ""
    echo "To fix:"
    echo "  1. sudo cp ${INSTALL_DIR}/docker/daemon.json /etc/docker/daemon.json"
    echo "  2. sudo systemctl restart docker"
    echo "  3. Verify: docker info | grep 'Default Runtime'"
    echo ""
    echo "Without this, GPU containers will fail silently."
    exit 1
fi

echo "NVIDIA Container Toolkit configured correctly."

#!/bin/bash
# matric-builder entrypoint script
#
# Handles Docker-in-Docker setup when running in CI environments.
# For standard use, simply passes through to the command.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[builder]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[builder]${NC} $1"
}

log_error() {
    echo -e "${RED}[builder]${NC} $1"
}

# Check if Docker socket is available
if [ -S /var/run/docker.sock ]; then
    log_info "Docker socket detected - Docker-in-Docker mode enabled"

    # Verify Docker is accessible
    if docker info > /dev/null 2>&1; then
        log_info "Docker daemon accessible"
        DOCKER_VERSION=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo "unknown")
        log_info "Docker version: ${DOCKER_VERSION}"
    else
        log_warn "Docker socket present but daemon not accessible"
        log_warn "Check permissions on /var/run/docker.sock"
    fi
else
    log_info "No Docker socket - running in standalone mode"
fi

# Display Rust toolchain info
log_info "Rust version: $(rustc --version)"
log_info "Cargo version: $(cargo --version)"

# Execute command
exec "$@"

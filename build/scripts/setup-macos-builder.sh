#!/usr/bin/env bash
# Setup macOS build host for Fortemi CI sidecar builds.
#
# Run once on a fresh macOS host (e.g. mutsu M4 Mac mini) to install
# the same toolchain the Linux builder container provides.
#
# Usage:
#   bash build/scripts/setup-macos-builder.sh
#
# Prerequisites:
#   - macOS with Apple Silicon (aarch64)
#   - Xcode Command Line Tools (xcode-select --install)
#   - Homebrew (https://brew.sh)

set -euo pipefail

RUST_VERSION="1.92.0"

echo "=== Fortemi macOS Builder Setup ==="
echo "Target Rust version: ${RUST_VERSION}"
echo ""

# Xcode CLT check
if ! xcode-select -p &>/dev/null; then
  echo "ERROR: Xcode Command Line Tools not installed."
  echo "  Run: xcode-select --install"
  exit 1
fi
echo "  Xcode CLT: OK"

# Homebrew check
if ! command -v brew &>/dev/null; then
  echo "ERROR: Homebrew not installed."
  echo "  Run: /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
  exit 1
fi
echo "  Homebrew: OK"

# Install system dependencies (macOS equivalents of the Linux builder)
echo ""
echo "=== Installing system dependencies ==="
brew install --quiet \
  openssl@3 \
  pkg-config \
  libpq \
  jq \
  git

echo "  System deps: OK"

# Rust toolchain
echo ""
echo "=== Setting up Rust toolchain ==="
if ! command -v rustup &>/dev/null; then
  echo "Installing rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain "${RUST_VERSION}"
  source "$HOME/.cargo/env"
else
  echo "rustup already installed, pinning to ${RUST_VERSION}..."
  rustup default "${RUST_VERSION}"
fi

rustup component add rustfmt clippy
echo "  Rust ${RUST_VERSION}: OK"

# Verify
echo ""
echo "=== Verification ==="
echo "  rustc:    $(rustc --version)"
echo "  cargo:    $(cargo --version)"
echo "  openssl:  $(pkg-config --modversion openssl 2>/dev/null || echo 'not found via pkg-config')"
echo "  libpq:    $(pg_config --version 2>/dev/null || echo 'not found')"
echo "  git:      $(git --version)"
echo "  jq:       $(jq --version)"
echo ""
echo "=== macOS builder setup complete ==="
echo ""
echo "Ensure the Gitea Actions runner is configured with label 'mutsu'."

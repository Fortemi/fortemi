#!/usr/bin/env bash
# Install version- and checksum-pinned syft and cosign for release supply-chain evidence.
#
# Pins are recorded in ci/digests.txt. Bump a version by updating the version
# and the sha256 values below from the release's published checksums file in
# the same commit as the ci/digests.txt row.
set -euo pipefail

DEST="${1:?usage: install-supply-chain-tools.sh <bin-dir>}"

SYFT_VERSION="1.52.0"
SYFT_SHA256_AMD64="caeedb81fb0491615f1ebd1761e4145d41ee86dd2cc7bf80669f9f5ad9d6133d"
SYFT_SHA256_ARM64="c46d5e4c28e12aa4c5becfaa343ef1c7f89045b6b895f2c21d471c62db09c706"

COSIGN_VERSION="3.1.3"
COSIGN_SHA256_AMD64="4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71"
COSIGN_SHA256_ARM64="c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a"

case "$(uname -m)" in
  x86_64|amd64) arch=amd64; syft_sha="$SYFT_SHA256_AMD64"; cosign_sha="$COSIGN_SHA256_AMD64" ;;
  aarch64|arm64) arch=arm64; syft_sha="$SYFT_SHA256_ARM64"; cosign_sha="$COSIGN_SHA256_ARM64" ;;
  *) echo "install-supply-chain-tools: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$DEST"

fetch_verified() {
  local url="$1" expected="$2" out="$3" observed
  curl -fsSL --retry 3 --max-time 300 -o "$out" "$url"
  observed="$(sha256sum "$out" | awk '{print $1}')"
  echo "observed sha256 for ${url##*/}: ${observed}"
  if [[ "$observed" != "$expected" ]]; then
    echo "install-supply-chain-tools: sha256 drift for ${url##*/}; expected ${expected}; refusing to install" >&2
    exit 1
  fi
}

fetch_verified \
  "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_${arch}.tar.gz" \
  "$syft_sha" "$work/syft.tar.gz"
tar -xzf "$work/syft.tar.gz" -C "$work" syft
install -m 0755 "$work/syft" "$DEST/syft"

fetch_verified \
  "https://github.com/sigstore/cosign/releases/download/v${COSIGN_VERSION}/cosign-linux-${arch}" \
  "$cosign_sha" "$work/cosign"
install -m 0755 "$work/cosign" "$DEST/cosign"

"$DEST/syft" version | sed -n 's/^Version:/syft version:/p'
"$DEST/cosign" version 2>/dev/null | sed -n 's/^GitVersion:/cosign version:/p'

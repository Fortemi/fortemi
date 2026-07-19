#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UV="${UV:-uv}"
EXPECTED_UV_VERSION="0.9.26"
PYTHON_VERSION="3.12"
EXCLUDE_NEWER="2026-07-19T00:00:00Z"

if [[ "$("$UV" --version)" != "uv ${EXPECTED_UV_VERSION}" ]]; then
  echo "uv ${EXPECTED_UV_VERSION} is required to regenerate sidecar locks" >&2
  exit 1
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

compile_lock() {
  local input="$1"
  local output="$2"
  local platform="$3"
  local backend="$4"

  "$UV" pip compile "$input" \
    --output-file "$output" \
    --python-version "$PYTHON_VERSION" \
    --python-platform "$platform" \
    --generate-hashes \
    --only-binary :all: \
    --exclude-newer "$EXCLUDE_NEWER" \
    --index-strategy first-index \
    --torch-backend "$backend" \
    --emit-index-url \
    --emit-index-annotation \
    --custom-compile-command scripts/lock-python-sidecars.sh \
    >/dev/null

  # pip merges the primary and PyTorch indexes but cannot select an exact local
  # version specifier when the public release is also present. Keep only the
  # reviewed backend hashes while matching the public version; the installed
  # wheel still reports its +cpu/+cu126 local version.
  sed -E -i \
    's/^(torch|torchaudio)==([^[:space:]+]+)\+(cpu|cu126)([[:space:]]*\\)$/\1==\2\4/' \
    "$output"
  # pyannote uses TorchCodec's CPU decoder with a CUDA-capable PyTorch runtime.
  # Preserve === so pip cannot prefer a same-version accelerator wheel.
  if grep -q '^torchcodec==' "$output"; then
    sed -i 's/^torchcodec==/torchcodec===/' "$output"
  fi
  sed -i \
    "s|^--index-url https://pypi.org/simple$|--index-url https://download.pytorch.org/whl/${backend}\\
--extra-index-url https://pypi.org/simple|" \
    "$output"
}

compile_lock \
  build/gliner/requirements.txt \
  "$tmpdir/gliner-amd64.lock" \
  x86_64-unknown-linux-gnu \
  cpu
compile_lock \
  build/gliner/requirements.txt \
  "$tmpdir/gliner-arm64.lock" \
  aarch64-unknown-linux-gnu \
  cpu

if ! cmp -s "$tmpdir/gliner-amd64.lock" "$tmpdir/gliner-arm64.lock"; then
  echo "GLiNER resolution differs between linux/amd64 and linux/arm64" >&2
  diff -u "$tmpdir/gliner-amd64.lock" "$tmpdir/gliner-arm64.lock" || true
  exit 1
fi

compile_lock \
  build/pyannote/requirements.txt \
  "$tmpdir/pyannote-amd64.lock" \
  x86_64-unknown-linux-gnu \
  cu126

install -m 0644 "$tmpdir/gliner-amd64.lock" build/gliner/requirements.lock
install -m 0644 "$tmpdir/pyannote-amd64.lock" build/pyannote/requirements.lock

echo "Regenerated GLiNER CPU and pyannote CUDA 12.6 dependency locks"

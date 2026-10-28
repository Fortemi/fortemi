#!/bin/sh
# BuildKit sharing=locked serializes this cache mount. Cached bytes are optional;
# Cargo --locked still resolves/compiles and failure never falls back to a binary.
set -eu
# System package identity matters for native linked dependencies, even with the
# same Rust toolchain and Cargo.lock. Source edits remain Cargo's responsibility.
key=$({
    rustc -vV
    dpkg-query -W -f='${Package}=${Version}\n'
    # Include workspace manifests, SQLx metadata and migrations. Cargo also
    # checks source fingerprints; build-script external inputs need explicit keys.
    find . -type f \( -name Cargo.lock -o -name Cargo.toml \) ! -path './target/*' -print0 | sort -z | xargs -0 sha256sum
    for inputs in .sqlx migrations; do
        if [ -d "$inputs" ]; then
            find "$inputs" -type f -print0 | sort -z | xargs -0 -r sha256sum
        fi
    done
    # Core reads SHA/date at runtime; package version is in Cargo manifests.
    # Stamp-only changes retain dependencies. Cargo still runs and fingerprints
    # source, env!/option_env! and declared build-script environment inputs.
    printf '%s\n' "${SQLX_OFFLINE:-}"
    printf '%s\n' "${TARGETARCH}" "${FORTEMI_API_FEATURES}" "${RUSTFLAGS:-}" "${CARGO_ENCODED_RUSTFLAGS:-}" "${FORTEMI_CACHE_BUILD_PKE:-0}" release
} | sha256sum | cut -d' ' -f1)
export CARGO_TARGET_DIR="/cargo-target/${key}"
# --no-cache alone does not clear mutable mounts. This explicit escape hatch
# uses a non-mounted target and registry; existing shared caches are untouched.
if [ "${FORTEMI_CACHE_DISABLE:-0}" = 1 ]; then
    export CARGO_TARGET_DIR=/app/clean-target CARGO_HOME=/app/clean-cargo
fi
set -- --package matric-api
if [ "${FORTEMI_CACHE_BUILD_PKE:-0}" = 1 ]; then
    set -- "$@" --package matric-crypto
fi
rust-cross-build cargo build --locked --release --jobs "${CARGO_BUILD_JOBS}" \
    "$@" --features "${FORTEMI_API_FEATURES}"
cp "${CARGO_TARGET_DIR}/$(rust-cross-build triple)/release/matric-api" /app/matric-api
if [ "${FORTEMI_CACHE_BUILD_PKE:-0}" = 1 ]; then
    cp "${CARGO_TARGET_DIR}/$(rust-cross-build triple)/release/matric-pke" /app/matric-pke
fi
# Keep at most the current input generation. It is safe to discard mount
# contents after the binaries have been copied into the image layer.
if [ "${FORTEMI_CACHE_DISABLE:-0}" != 1 ]; then
    # This mount is feature/PKE-specific. Remove only known generation names;
    # never traverse symlinks or remove foreign mount children.
    for old in /cargo-target/*; do
        name=${old##*/}
        [ "${#name}" = 64 ] || continue
        case "$name" in *[!0-9a-f]*) continue ;; esac
        [ "$name" != "$key" ] && [ ! -L "$old" ] && [ -d "$old" ] || continue
        [ "$(stat -c %u "$old")" = "$(id -u)" ] || continue
        rm -rf -- "$old"
    done
    # Retention ceiling, not a disk quota during compilation. An oversized
    # generation is discarded, never treated as a successful missing binary.
    if [ "$(du -sk /cargo-target | cut -f1)" -gt 12582912 ]; then
        rm -rf -- "$CARGO_TARGET_DIR"
    fi
fi

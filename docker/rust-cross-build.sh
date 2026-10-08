#!/bin/sh
# Cross-compilation helper for the Rust builder stages of Dockerfile and
# Dockerfile.bundle (#623).
#
# The builder stages run on the build host's platform (BuildKit
# BUILDPLATFORM) and compile for the image platform (TARGETARCH) with a
# Debian bookworm cross toolchain, so linux/arm64 release images never compile
# Rust under QEMU. Native builds use the same path with the host target.
#
#   rust-cross-build.sh install-deps     apt toolchain + rustup target (root)
#   rust-cross-build.sh triple           print the Rust target triple
#   rust-cross-build.sh cargo <args...>  run cargo for the target
#
# TARGETARCH must be set (BuildKit provides it as a build argument).
set -eu

case "${TARGETARCH:?TARGETARCH is required (BuildKit build argument)}" in
    amd64)
        triple=x86_64-unknown-linux-gnu
        gnu=x86_64-linux-gnu
        ;;
    arm64)
        triple=aarch64-unknown-linux-gnu
        gnu=aarch64-linux-gnu
        ;;
    *)
        echo "rust-cross-build: unsupported TARGETARCH=${TARGETARCH}" >&2
        exit 1
        ;;
esac

build_arch="$(dpkg --print-architecture)"
cross=false
if [ "$build_arch" != "$TARGETARCH" ]; then
    cross=true
fi

install_deps() {
    packages="pkg-config libssl-dev curl"
    if [ "$cross" = true ]; then
        dpkg --add-architecture "$TARGETARCH"
        packages="$packages gcc-$gnu g++-$gnu libc6-dev-$TARGETARCH-cross libssl-dev:$TARGETARCH"
    fi
    # Retry the verified update/install transaction to tolerate short-lived
    # repository/proxy failures without disabling APT signature or TLS checks.
    attempt=1
    while true; do
        # shellcheck disable=SC2086 # package list is intentionally word-split
        if apt-get -o Acquire::Retries=3 update &&
            apt-get -o Acquire::Retries=3 install -y --no-install-recommends $packages; then
            break
        fi
        if [ "$attempt" -ge 3 ]; then
            echo "APT build dependency installation failed after ${attempt} attempts" >&2
            exit 1
        fi
        rm -rf /var/lib/apt/lists/*
        sleep $((attempt * 5))
        attempt=$((attempt + 1))
    done
    rm -rf /var/lib/apt/lists/*
    # The pinned rust image already carries the host target; rustup verifies
    # the cross standard library against the pinned toolchain's manifest.
    rustup target add "$triple"
}

run_cargo() {
    if [ "$cross" = true ]; then
        upper="$(printf '%s' "$triple" | tr 'a-z-' 'A-Z_')"
        lower="$(printf '%s' "$triple" | tr '-' '_')"
        export "CARGO_TARGET_${upper}_LINKER=${gnu}-gcc"
        export "CC_${lower}=${gnu}-gcc"
        export "CXX_${lower}=${gnu}-g++"
        export "AR_${lower}=${gnu}-ar"
        export PKG_CONFIG_ALLOW_CROSS=1
        export PKG_CONFIG_LIBDIR="/usr/lib/${gnu}/pkgconfig:/usr/share/pkgconfig"
    fi
    exec cargo "$@" --target "$triple"
}

command="${1:?usage: rust-cross-build.sh install-deps|triple|cargo ...}"
shift
case "$command" in
    install-deps) install_deps ;;
    triple) printf '%s\n' "$triple" ;;
    cargo) run_cargo "$@" ;;
    *)
        echo "rust-cross-build: unknown command ${command}" >&2
        exit 2
        ;;
esac

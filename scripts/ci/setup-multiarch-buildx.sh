#!/usr/bin/env bash
# Prepare a digest-pinned BuildKit builder that can produce linux/amd64 and
# linux/arm64 images on the amd64 release runner (#623).
#
# Rust is cross-compiled natively in the builder stages (docker/rust-cross-build.sh);
# QEMU (binfmt_misc) is needed only for the arm64 runtime stages, which run
# package installs and file copies, and for arm64 smoke tests.
#
# Usage: scripts/ci/setup-multiarch-buildx.sh
# Prints nothing to stdout except buildx diagnostics; exits non-zero when buildx
# is missing or the builder cannot target both platforms.
set -euo pipefail

# Pins are recorded in ci/digests.txt.
BINFMT_IMAGE="tonistiigi/binfmt:qemu-v10.2.3-68@sha256:400a4873b838d1b89194d982c45e5fb3cda4593fbfd7e08a02e76b03b21166f0"
BUILDKIT_IMAGE="moby/buildkit:v0.33.1@sha256:cec9f139f45e93c5c69c60f8b07cfad9f43f4ef6b6a6cd917527fea5ff2e3dea"
# The builder name encodes the BuildKit pin so a builder created from another
# image (for example by an older workflow) is never silently reused.
BUILDER_NAME="${FORTEMI_BUILDX_BUILDER:-fortemi-multiarch-bk0331}"
REQUIRED_PLATFORMS=(linux/amd64 linux/arm64)

if ! docker buildx version >/dev/null 2>&1; then
    echo "ERROR: docker buildx is unavailable; multi-platform release images cannot be built" >&2
    exit 1
fi

if [[ ! -e /proc/sys/fs/binfmt_misc/qemu-aarch64 ]]; then
    echo "Registering QEMU arm64 binfmt handler from ${BINFMT_IMAGE}"
    docker run --privileged --rm "$BINFMT_IMAGE" --install arm64
fi
if [[ ! -e /proc/sys/fs/binfmt_misc/qemu-aarch64 ]]; then
    echo "ERROR: qemu-aarch64 binfmt handler is not registered" >&2
    exit 1
fi

if docker buildx inspect "$BUILDER_NAME" >/dev/null 2>&1; then
    docker buildx use "$BUILDER_NAME"
else
    docker buildx create \
        --name "$BUILDER_NAME" \
        --driver docker-container \
        --driver-opt "image=${BUILDKIT_IMAGE}" \
        --use
fi

inspect="$(docker buildx inspect --bootstrap "$BUILDER_NAME")"
printf '%s\n' "$inspect"
for platform in "${REQUIRED_PLATFORMS[@]}"; do
    if ! grep -E '^Platforms:' <<<"$inspect" | grep -q "$platform"; then
        echo "ERROR: builder ${BUILDER_NAME} cannot target ${platform}" >&2
        exit 1
    fi
done
echo "buildx builder ${BUILDER_NAME} ready for ${REQUIRED_PLATFORMS[*]}"

#!/usr/bin/env bash
# Verify that a pushed image reference is a linux/amd64 + linux/arm64 index and
# that every platform image carries the expected OCI revision/version labels
# (#623). Reads only registry manifests and configs, so it works for foreign
# platforms and for anonymous clients.
#
# Usage: scripts/ci/verify-multiarch-image.sh <reference> <git-sha> <version>
set -euo pipefail

REFERENCE="${1:?usage: verify-multiarch-image.sh <reference> <git-sha> <version>}"
EXPECTED_REVISION="${2:?expected git revision is required}"
EXPECTED_VERSION="${3:?expected image version is required}"
EXPECTED_PLATFORMS='["linux/amd64","linux/arm64"]'

raw="$(docker buildx imagetools inspect --raw "$REFERENCE")"
platforms="$(jq -c '
    [(.manifests // [])[]
     | select(.platform.os != null and .platform.os != "unknown")
     | "\(.platform.os)/\(.platform.architecture)"] | sort' <<<"$raw")"
if [[ "$platforms" != "$EXPECTED_PLATFORMS" ]]; then
    echo "ERROR: ${REFERENCE} platforms ${platforms} != ${EXPECTED_PLATFORMS}" >&2
    exit 1
fi
attestations="$(jq '[(.manifests // [])[] | select(.platform.os == "unknown")] | length' <<<"$raw")"
if [[ "$attestations" != "0" ]]; then
    echo "ERROR: ${REFERENCE} carries ${attestations} BuildKit attestation manifests;" \
        "release attestations are attached by sign-release-images instead" >&2
    exit 1
fi

configs="$(docker buildx imagetools inspect --format '{{json .Image}}' "$REFERENCE")"
for platform in linux/amd64 linux/arm64; do
    revision="$(jq -r --arg p "$platform" \
        '.[$p].config.Labels["org.opencontainers.image.revision"] // ""' <<<"$configs")"
    version="$(jq -r --arg p "$platform" \
        '.[$p].config.Labels["org.opencontainers.image.version"] // ""' <<<"$configs")"
    architecture="$(jq -r --arg p "$platform" '.[$p].architecture // ""' <<<"$configs")"
    if [[ "$architecture" != "${platform#linux/}" ]]; then
        echo "ERROR: ${REFERENCE} ${platform} config reports architecture ${architecture:-<missing>}" >&2
        exit 1
    fi
    if [[ "$revision" != "$EXPECTED_REVISION" ]]; then
        echo "ERROR: ${REFERENCE} ${platform} revision ${revision:-<missing>} does not match ${EXPECTED_REVISION}" >&2
        exit 1
    fi
    if [[ "$version" != "$EXPECTED_VERSION" ]]; then
        echo "ERROR: ${REFERENCE} ${platform} version ${version:-<missing>} does not match ${EXPECTED_VERSION}" >&2
        exit 1
    fi
done

digest="$(docker buildx imagetools inspect --format '{{json .Manifest}}' "$REFERENCE" | jq -r .digest)"
echo "verified ${REFERENCE} (${digest}): linux/amd64, linux/arm64 at ${EXPECTED_REVISION}"

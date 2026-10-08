#!/usr/bin/env bash
# Verify a Fortemi GHCR release through an anonymous Docker client.
set -euo pipefail

TARGET_IMAGE="${TARGET_IMAGE:?TARGET_IMAGE is required}"
MCP_TARGET_IMAGE="${MCP_TARGET_IMAGE:?MCP_TARGET_IMAGE is required}"
VERSION="${VERSION:?VERSION is required}"
GITHUB_SHA="${GITHUB_SHA:?GITHUB_SHA is required}"
OUTPUT_DIR="${OUTPUT_DIR:-container-release-evidence}"

if [[ ! "$VERSION" =~ ^[0-9]{4}\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "ERROR: VERSION must be an unprefixed Fortemi release version" >&2
    exit 1
fi
if [[ ! "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]; then
    echo "ERROR: GITHUB_SHA must be a full lowercase Git revision" >&2
    exit 1
fi

public_docker_config="$(mktemp -d)"
cleanup() {
    rm -rf "$public_docker_config"
}
trap cleanup EXIT

# An empty Docker configuration proves the package is publicly readable. The
# publishing job runs separately and does not pass its GHCR credential here.
export DOCKER_CONFIG="$public_docker_config"
mkdir -p "$OUTPUT_DIR"

python3 scripts/ci/capture-container-release-evidence.py \
    --family api \
    --source-revision "$GITHUB_SHA" \
    --channel release \
    --immutable-ref "${TARGET_IMAGE}:${VERSION}" \
    --alias "${TARGET_IMAGE}:latest" \
    --output "${OUTPUT_DIR}/ghcr-api-public-release.json"

python3 scripts/ci/capture-container-release-evidence.py \
    --family bundle \
    --source-revision "$GITHUB_SHA" \
    --channel release \
    --immutable-ref "${TARGET_IMAGE}:bundle-${VERSION}" \
    --alias "${TARGET_IMAGE}:bundle-latest" \
    --output "${OUTPUT_DIR}/ghcr-bundle-public-release.json"

python3 scripts/ci/capture-container-release-evidence.py \
    --family mcp \
    --source-revision "$GITHUB_SHA" \
    --channel release \
    --immutable-ref "${MCP_TARGET_IMAGE}:${VERSION}" \
    --alias "${MCP_TARGET_IMAGE}:latest" \
    --output "${OUTPUT_DIR}/ghcr-mcp-public-release.json"

verify_labels() {
    local reference="$1"
    local revision
    local image_version

    if ! docker pull --quiet --platform linux/amd64 "$reference" >/dev/null; then
        echo "ERROR: anonymous pull of ${reference} failed. A newly created GHCR package" \
            "is private until an organization owner sets its visibility to public" \
            "(docs/deployment/image-promotion.md#public-ghcr-packages)." >&2
        return 1
    fi
    revision="$(docker image inspect \
        --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' \
        "$reference")"
    image_version="$(docker image inspect \
        --format '{{index .Config.Labels "org.opencontainers.image.version"}}' \
        "$reference")"

    if [[ "$revision" != "$GITHUB_SHA" ]]; then
        echo "ERROR: ${reference} revision ${revision:-<missing>} does not match ${GITHUB_SHA}" >&2
        return 1
    fi
    if [[ "$image_version" != "$VERSION" ]]; then
        echo "ERROR: ${reference} version ${image_version:-<missing>} does not match ${VERSION}" >&2
        return 1
    fi
}

verify_labels "${TARGET_IMAGE}:${VERSION}"
verify_labels "${TARGET_IMAGE}:bundle-${VERSION}"
verify_labels "${MCP_TARGET_IMAGE}:${VERSION}"

# Every public family is a linux/amd64 + linux/arm64 index whose platform
# images both carry the release revision and version (#623).
for reference in \
    "${TARGET_IMAGE}:${VERSION}" \
    "${TARGET_IMAGE}:bundle-${VERSION}" \
    "${MCP_TARGET_IMAGE}:${VERSION}"; do
    scripts/ci/verify-multiarch-image.sh "$reference" "$GITHUB_SHA" "$VERSION"
done

echo "Public GHCR release verified (linux/amd64, linux/arm64)"
echo "  API:    ${TARGET_IMAGE}:${VERSION}, ${TARGET_IMAGE}:latest"
echo "  Bundle: ${TARGET_IMAGE}:bundle-${VERSION}, ${TARGET_IMAGE}:bundle-latest"
echo "  MCP:    ${MCP_TARGET_IMAGE}:${VERSION}, ${MCP_TARGET_IMAGE}:latest"

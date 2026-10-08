#!/usr/bin/env bash
# Promote exact-revision internal release images to GHCR by digest (#623).
#
# publish-release builds every family once, as a linux/amd64 + linux/arm64
# index, in the internal Gitea registry. This job copies each index by digest,
# so GHCR serves byte-identical manifests (same index and per-platform digests)
# and nothing is rebuilt. Each family is described by three variables:
#
#   <FAMILY>_SOURCE_TAG   tag in SOURCE_IMAGE (or MCP_SOURCE_IMAGE for MCP)
#   <FAMILY>_TARGET_TAGS  space-separated tags to create in the target repository
#
# Families: API, HOSTED, BUNDLE (SOURCE_IMAGE -> TARGET_IMAGE) and MCP
# (MCP_SOURCE_IMAGE -> MCP_TARGET_IMAGE).
set -euo pipefail

SOURCE_IMAGE="${SOURCE_IMAGE:?SOURCE_IMAGE is required}"
TARGET_IMAGE="${TARGET_IMAGE:?TARGET_IMAGE is required}"
MCP_SOURCE_IMAGE="${MCP_SOURCE_IMAGE:?MCP_SOURCE_IMAGE is required}"
MCP_TARGET_IMAGE="${MCP_TARGET_IMAGE:?MCP_TARGET_IMAGE is required}"
API_SOURCE_TAG="${API_SOURCE_TAG:?API_SOURCE_TAG is required}"
BUNDLE_SOURCE_TAG="${BUNDLE_SOURCE_TAG:?BUNDLE_SOURCE_TAG is required}"
MCP_SOURCE_TAG="${MCP_SOURCE_TAG:?MCP_SOURCE_TAG is required}"
HOSTED_SOURCE_TAG="${HOSTED_SOURCE_TAG:?HOSTED_SOURCE_TAG is required}"
API_TARGET_TAGS="${API_TARGET_TAGS:?API_TARGET_TAGS is required}"
BUNDLE_TARGET_TAGS="${BUNDLE_TARGET_TAGS:?BUNDLE_TARGET_TAGS is required}"
MCP_TARGET_TAGS="${MCP_TARGET_TAGS:?MCP_TARGET_TAGS is required}"
HOSTED_TARGET_TAGS="${HOSTED_TARGET_TAGS:?HOSTED_TARGET_TAGS is required}"
VERSION="${VERSION:?VERSION is required}"
GITHUB_SHA="${GITHUB_SHA:?GITHUB_SHA is required}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

retry() {
    local attempt
    for attempt in 1 2 3; do
        echo "Attempt ${attempt}/3: $*"
        if "$@"; then
            return 0
        fi
        sleep $((attempt * 5))
    done
    echo "ERROR: command failed after 3 attempts: $*" >&2
    return 1
}

index_digest() {
    docker buildx imagetools inspect --format '{{json .Manifest}}' "$1" | jq -r .digest
}

promote() {
    local family="$1" source_ref="$2" target_repo="$3" target_tags="$4"
    local -a tags tag_args
    local tag source_digest target_digest

    read -r -a tags <<<"$target_tags"
    if (( ${#tags[@]} == 0 )); then
        echo "ERROR: ${family} needs at least one target tag" >&2
        return 1
    fi

    echo "Promoting ${family}: ${source_ref}"
    "${SCRIPT_DIR}/verify-multiarch-image.sh" "$source_ref" "$GITHUB_SHA" "$VERSION"
    source_digest="$(index_digest "$source_ref")"
    if [[ ! "$source_digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
        echo "ERROR: cannot resolve the ${family} source index digest" >&2
        return 1
    fi

    tag_args=()
    for tag in "${tags[@]}"; do
        tag_args+=(--tag "${target_repo}:${tag}")
    done
    # A single index source is copied unchanged (children and index by digest).
    retry docker buildx imagetools create "${tag_args[@]}" \
        "${source_ref%:*}@${source_digest}"

    for tag in "${tags[@]}"; do
        target_digest="$(index_digest "${target_repo}:${tag}")"
        if [[ "$target_digest" != "$source_digest" ]]; then
            echo "ERROR: ${target_repo}:${tag} is ${target_digest}, expected ${source_digest}" >&2
            return 1
        fi
    done
    "${SCRIPT_DIR}/verify-multiarch-image.sh" "${target_repo}:${tags[0]}" "$GITHUB_SHA" "$VERSION"
    echo "${family}: ${target_repo} ${target_tags} -> ${source_digest}"
}

promote api "${SOURCE_IMAGE}:${API_SOURCE_TAG}" "$TARGET_IMAGE" "$API_TARGET_TAGS"
promote hosted "${SOURCE_IMAGE}:${HOSTED_SOURCE_TAG}" "$TARGET_IMAGE" "$HOSTED_TARGET_TAGS"
promote bundle "${SOURCE_IMAGE}:${BUNDLE_SOURCE_TAG}" "$TARGET_IMAGE" "$BUNDLE_TARGET_TAGS"
promote mcp "${MCP_SOURCE_IMAGE}:${MCP_SOURCE_TAG}" "$MCP_TARGET_IMAGE" "$MCP_TARGET_TAGS"

echo "Promoted ${TARGET_IMAGE} and ${MCP_TARGET_IMAGE} at ${GITHUB_SHA} (linux/amd64, linux/arm64)"

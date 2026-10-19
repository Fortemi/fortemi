#!/usr/bin/env bash
# Partial reruns may retain build-image's result after its local image was reaped.
set -euo pipefail
[[ "${CHAIN_SOURCE_SHA:?}" =~ ^[0-9a-f]{40}$ ]]
test "$(git rev-parse HEAD)" = "$CHAIN_SOURCE_SHA"
if ! docker image inspect "${TEST_API_IMAGE:?}" >/dev/null 2>&1; then
  docker build \
    --build-arg "VERSION=test-${CHAIN_SOURCE_SHA:0:7}" \
    --build-arg "GIT_SHA=${CHAIN_SOURCE_SHA}" \
    --build-arg "BUILD_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:?}" \
    --label "fortemi.ci.attempt=${CI_RESOURCE_ATTEMPT:?}" \
    -t "$TEST_API_IMAGE" .
fi
actual="$(docker image inspect "$TEST_API_IMAGE" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')"
if [[ "$actual" != "$CHAIN_SOURCE_SHA" ]]; then
  echo '::error::Run API image source label does not match the exact checkout.' >&2
  exit 1
fi

#!/usr/bin/env bash
# Start one platform member of a published Fortemi image and prove matric-api
# binds and /health responds (#623). On the amd64 runner a linux/arm64 member
# runs under QEMU (binfmt), so this is emulated evidence, not native hardware.
#
# Usage:
#   scripts/ci/smoke-image-platform.sh --kind api|bundle --platform linux/arm64 \
#     --image <registry/repo:tag> --output <receipt.json>
#
# api:    starts the disposable test database (build/Dockerfile.testdb, native
#         platform) and the API image on a run-scoped network.
# bundle: starts the all-in-one bundle (embedded PostgreSQL) on its own.
# The receipt records the resolved index digest and the digest of the platform
# image that actually ran, for the release signing follow-up (#1158).
#
# Environment: TESTDB_IMAGE (default matric-testdb:local, built when missing),
# SMOKE_TIMEOUT_SECONDS (default 1200; emulated first boots run migrations).
set -euo pipefail

KIND="" PLATFORM="" IMAGE="" OUTPUT=""
while (( $# )); do
    case "$1" in
        --kind) KIND="$2"; shift 2 ;;
        --platform) PLATFORM="$2"; shift 2 ;;
        --image) IMAGE="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ "$KIND" == api || "$KIND" == bundle ]] || { echo "--kind api|bundle is required" >&2; exit 2; }
[[ "$PLATFORM" =~ ^linux/(amd64|arm64)$ ]] || { echo "--platform linux/amd64|linux/arm64 is required" >&2; exit 2; }
[[ -n "$IMAGE" && -n "$OUTPUT" ]] || { echo "--image and --output are required" >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TESTDB_IMAGE="${TESTDB_IMAGE:-matric-testdb:local}"
TIMEOUT="${SMOKE_TIMEOUT_SECONDS:-1200}"
RUN_ID="fortemi-smoke-${KIND}-${PLATFORM#linux/}-$$-$(date +%s)"
NETWORK="${RUN_ID}-net"
DB="${RUN_ID}-db"
APP="${RUN_ID}-app"
SECRET="smoke-$(od -An -N12 -tx1 /dev/urandom | tr -d ' \n')"

log() { printf '[image-smoke] %s\n' "$*"; }
cleanup() {
    docker rm -f "$APP" "$DB" >/dev/null 2>&1 || true
    docker network rm "$NETWORK" >/dev/null 2>&1 || true
}
trap cleanup EXIT

HOST_ARCH="$(docker version --format '{{.Server.Arch}}')"
EMULATED=false
if [[ "${PLATFORM#linux/}" != "$HOST_ARCH" ]]; then
    EMULATED=true
    case "${PLATFORM#linux/}" in arm64) qemu=aarch64 ;; amd64) qemu=x86_64 ;; esac
    if [[ ! -e "/proc/sys/fs/binfmt_misc/qemu-${qemu}" ]]; then
        echo "ERROR: no binfmt handler for ${PLATFORM}; run scripts/ci/setup-multiarch-buildx.sh" >&2
        exit 1
    fi
fi

index_digest="$(docker buildx imagetools inspect --format '{{json .Manifest}}' "$IMAGE" | jq -r .digest)"
platform_digest="$(docker buildx imagetools inspect --raw "$IMAGE" | jq -r --arg arch "${PLATFORM#linux/}" \
    '[.manifests[]? | select(.platform.os == "linux" and .platform.architecture == $arch)][0].digest // empty')"
[[ "$platform_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || { echo "ERROR: ${IMAGE} has no ${PLATFORM} member" >&2; exit 1; }
REF="${IMAGE%:*}@${platform_digest}"
log "running ${REF} (${PLATFORM}) from index ${index_digest}"
docker pull --quiet --platform "$PLATFORM" "$REF" >/dev/null

"${ROOT}/scripts/ci/create-explicit-docker-network.sh" "$NETWORK" "$$" >/dev/null
started=$(date +%s)
if [[ "$KIND" == api ]]; then
    if ! docker image inspect "$TESTDB_IMAGE" >/dev/null 2>&1; then
        docker build -q -f "${ROOT}/build/Dockerfile.testdb" -t "$TESTDB_IMAGE" "$ROOT" >/dev/null
    fi
    docker run -d --name "$DB" --network "$NETWORK" \
        -e POSTGRES_USER=matric -e POSTGRES_PASSWORD="$SECRET" -e POSTGRES_DB=matric \
        "$TESTDB_IMAGE" >/dev/null
    # TCP readiness: the image's first-boot init server listens on the socket
    # only, so this succeeds only once the final server is up.
    for _ in $(seq 1 60); do
        docker exec "$DB" pg_isready -h 127.0.0.1 -U matric -d matric >/dev/null 2>&1 && break
        sleep 2
    done
    docker exec "$DB" pg_isready -h 127.0.0.1 -U matric -d matric >/dev/null || { docker logs "$DB" | tail -30; exit 1; }
    docker run -d --name "$APP" --network "$NETWORK" --platform "$PLATFORM" \
        -p 127.0.0.1::3000 \
        -e DATABASE_URL="$(printf '%s%s:%s@%s:5432/%s' 'postgres://' matric "$SECRET" "$DB" matric)" \
        -e ISSUER_URL=http://localhost:3000 -e FORTEMI_ALLOW_LOCAL_ISSUER=true \
        -e RATE_LIMIT_ENABLED=false -e MATRIC_ATTACHMENT_SCAN_MODE=disabled \
        -e DISABLE_SUPPORT_MEMORY=1 \
        "$REF" >/dev/null
else
    docker run -d --name "$APP" --network "$NETWORK" --platform "$PLATFORM" \
        -p 127.0.0.1::3000 --shm-size 1g \
        -e POSTGRES_PASSWORD="$SECRET" \
        -e ISSUER_URL=http://localhost:3000 -e FORTEMI_ALLOW_LOCAL_ISSUER=true \
        -e MATRIC_ATTACHMENT_SCAN_MODE=disabled -e RENDERER_ENABLED=false \
        -e API_STARTUP_TIMEOUT_SECONDS="$TIMEOUT" \
        "$REF" >/dev/null
fi

port="$(docker port "$APP" 3000/tcp | head -1 | sed 's/.*://')"
health=""
while true; do
    if health="$(curl -fsS --max-time 5 "http://127.0.0.1:${port}/health" 2>/dev/null)"; then
        break
    fi
    if [[ "$(docker inspect -f '{{.State.Running}}' "$APP")" != true ]]; then
        docker logs "$APP" 2>&1 | tail -80
        echo "ERROR: ${KIND} container exited before /health responded" >&2
        exit 1
    fi
    if (( $(date +%s) - started > TIMEOUT )); then
        docker logs "$APP" 2>&1 | tail -80
        echo "ERROR: ${KIND} /health did not respond within ${TIMEOUT}s" >&2
        exit 1
    fi
    sleep 5
done
elapsed=$(( $(date +%s) - started ))
arch="$(docker exec "$APP" uname -m)"
log "${KIND} ${PLATFORM}: /health responded after ${elapsed}s (uname -m ${arch})"

mkdir -p "$(dirname "$OUTPUT")"
jq -n --arg kind "$KIND" --arg platform "$PLATFORM" --arg image "$IMAGE" \
    --arg index "$index_digest" --arg member "$platform_digest" --arg arch "$arch" \
    --argjson elapsed "$elapsed" --arg health "$health" \
    --argjson emulated "$EMULATED" \
    --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '{
        schema: "fortemi.image-platform-smoke.v1", kind: $kind, platform: $platform,
        tagged_reference: $image, index_digest: $index, platform_digest: $member,
        container_uname_m: $arch, emulated: $emulated,
        health_seconds: $elapsed, health_body: ($health | fromjson? // $health),
        completed_at: $at }' > "$OUTPUT"
log "receipt: ${OUTPUT}"

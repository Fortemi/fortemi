#!/usr/bin/env bash
set -euo pipefail

# Hands-free local workstation installer and validation runner.
#
# Typical use from a checked-out Fortemi repository:
#   FORTEMI_INSTALL_MODE=local-no-auth ./installer/scripts/hands-free-local.sh
#
# Important environment:
#   INSTALL_DIR                 repository/install directory (default: cwd)
#   FORTEMI_INSTALL_MODE        secure | local-no-auth (default: local-no-auth)
#   FORTEMI_HARDWARE_PROFILE    auto | cpu | intel | amd | nvidia (default: auto)
#   INFERENCE_PROVIDER          ollama | openai | openrouter | llamacpp (default: ollama)
#   FORTEMI_VALIDATE_CHAT       true | false (default: true)
#   FORTEMI_REQUIRE_CHAT        true | false (default: false)
#   FORTEMI_SKIP_DEPLOY         true | false (default: false)

INSTALL_DIR="${INSTALL_DIR:-$(pwd)}"
FORTEMI_INSTALL_MODE="${FORTEMI_INSTALL_MODE:-local-no-auth}"
FORTEMI_HARDWARE_PROFILE="${FORTEMI_HARDWARE_PROFILE:-auto}"
INFERENCE_PROVIDER="${INFERENCE_PROVIDER:-ollama}"
FORTEMI_VALIDATE_CHAT="${FORTEMI_VALIDATE_CHAT:-true}"
FORTEMI_REQUIRE_CHAT="${FORTEMI_REQUIRE_CHAT:-false}"
FORTEMI_SKIP_DEPLOY="${FORTEMI_SKIP_DEPLOY:-false}"
API_URL="${FORTEMI_API_URL:-http://127.0.0.1:3000}"
VALIDATION_TIMEOUT_SECS="${FORTEMI_VALIDATION_TIMEOUT_SECS:-240}"
HEALTH_TIMEOUT_SECS="${FORTEMI_HEALTH_TIMEOUT_SECS:-180}"
AUTH_HEADER=()
if [ -n "${FORTEMI_API_TOKEN:-}" ]; then
    AUTH_HEADER=(-H "Authorization: Bearer ${FORTEMI_API_TOKEN}")
fi
HOST_OS="$(uname -s 2>/dev/null || echo unknown)"
HOST_ARCH="$(uname -m 2>/dev/null || echo unknown)"

cd "${INSTALL_DIR}"

PASS=()
WARNINGS=()
FAIL=()
COMPOSE_FILES=(-f docker-compose.bundle.yml)
LOCAL_OVERRIDE=".fortemi-local.override.yml"

record_pass() {
    PASS+=("$1")
    echo "PASS: $1"
}

record_warn() {
    WARNINGS+=("$1")
    echo "WARN: $1"
}

record_fail() {
    FAIL+=("$1")
    echo "FAIL: $1"
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        record_fail "missing required command: $1"
        return 1
    fi
    record_pass "$1 available"
}

json_field() {
    local field="$1"
    python3 -c 'import json,sys
field=sys.argv[1]
data=json.load(sys.stdin)
value=data
for part in field.split("."):
    value=value[part]
print(value)' "$field"
}

json_status_ok() {
    python3 -c 'import json,sys
data=json.load(sys.stdin)
status=str(data.get("status", "")).lower()
sys.exit(0 if status == "completed" else 1)'
}

api_get() {
    curl -fsS "${AUTH_HEADER[@]}" "${API_URL}$1"
}

api_post() {
    local path="$1"
    local body="$2"
    curl -fsS "${AUTH_HEADER[@]}" -X POST "${API_URL}${path}" \
        -H "Content-Type: application/json" \
        -d "${body}"
}

detect_profile() {
    if [ "${FORTEMI_HARDWARE_PROFILE}" != "auto" ]; then
        echo "${FORTEMI_HARDWARE_PROFILE}"
        return
    fi

    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
        if docker info --format '{{json .Runtimes}}' 2>/dev/null | grep -qi nvidia; then
            echo "nvidia"
            return
        fi
    fi

    if command -v lspci >/dev/null 2>&1; then
        if lspci | grep -Eiq 'VGA|3D|Display' && lspci | grep -Eiq 'Intel|Arc'; then
            echo "intel"
            return
        fi
        if lspci | grep -Eiq 'VGA|3D|Display' && lspci | grep -Eiq 'AMD|Advanced Micro Devices'; then
            echo "amd"
            return
        fi
    fi

    if [ -e /dev/dri ]; then
        echo "cpu"
    else
        echo "cpu"
    fi
}

write_local_override() {
    cat > "${LOCAL_OVERRIDE}" <<'EOF'
services:
  fortemi:
    deploy:
      resources:
        reservations:
          devices: !reset []
    environment:
      - RENDERER_ENABLED=${RENDERER_ENABLED:-false}
      - OPEN3D_ENABLED=${OPEN3D_ENABLED:-false}
      - NVIDIA_VISIBLE_DEVICES=${NVIDIA_VISIBLE_DEVICES:-}
      - NVIDIA_DRIVER_CAPABILITIES=${NVIDIA_DRIVER_CAPABILITIES:-}
EOF
    COMPOSE_FILES+=(-f "${LOCAL_OVERRIDE}")
    record_pass "generated ${LOCAL_OVERRIDE} for non-NVIDIA local profile"
}

compose() {
    docker compose "${COMPOSE_FILES[@]}" "$@"
}

wait_for_health() {
    local deadline=$((SECONDS + HEALTH_TIMEOUT_SECS))
    until api_get /health >/dev/null 2>&1; do
        if [ "${SECONDS}" -ge "${deadline}" ]; then
            record_fail "API did not become healthy within ${HEALTH_TIMEOUT_SECS}s"
            compose logs --tail 80 fortemi || true
            return 1
        fi
        sleep 3
    done
    record_pass "API health endpoint is reachable"
}

wait_for_job() {
    local job_id="$1"
    local deadline=$((SECONDS + VALIDATION_TIMEOUT_SECS))
    while true; do
        local job_json
        job_json="$(api_get "/api/v1/jobs/${job_id}")"
        if printf '%s' "${job_json}" | json_status_ok; then
            record_pass "embedding job completed"
            return 0
        fi
        local status
        status="$(printf '%s' "${job_json}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))')"
        if [ "${status}" = "failed" ] || [ "${status}" = "cancelled" ]; then
            record_fail "embedding job ended with status ${status}"
            return 1
        fi
        if [ "${SECONDS}" -ge "${deadline}" ]; then
            record_fail "embedding job did not complete within ${VALIDATION_TIMEOUT_SECS}s"
            return 1
        fi
        sleep 3
    done
}

run_search() {
    local mode="$1"
    local response
    response="$(api_get "/api/v1/search?q=fortemi-hands-free-validation&mode=${mode}&limit=5")"
    local count
    count="$(printf '%s' "${response}" | python3 -c 'import json,sys
data=json.load(sys.stdin)
items=data.get("results", data.get("hits", data.get("notes", [])))
print(len(items) if isinstance(items, list) else 0)')"
    record_pass "${mode} search returned HTTP 200 (${count} result item(s))"
}

run_chat_if_configured() {
    case "$(printf '%s' "${FORTEMI_VALIDATE_CHAT}" | tr '[:upper:]' '[:lower:]')" in
        0|false|no|off)
            record_warn "chat validation skipped by FORTEMI_VALIDATE_CHAT=false"
            return 0
            ;;
    esac

    if api_post /api/v1/chat '{"input":"Reply with ok for installer validation."}' >/dev/null; then
        record_pass "chat endpoint returned HTTP 200"
        return 0
    fi

    if [ "$(printf '%s' "${FORTEMI_REQUIRE_CHAT}" | tr '[:upper:]' '[:lower:]')" = "true" ]; then
        record_fail "chat endpoint did not return HTTP 200"
        return 1
    fi
    record_warn "chat endpoint not available; continuing because FORTEMI_REQUIRE_CHAT=false"
}

print_report() {
    echo ""
    echo "=== Fortemi local install report ==="
    echo "Install dir: ${INSTALL_DIR}"
    echo "API URL:     ${API_URL}"
    echo "Host:        ${HOST_OS}/${HOST_ARCH}"
    echo "Profile:     ${PROFILE:-unknown}"
    echo "Provider:    ${INFERENCE_PROVIDER}"
    echo "Mode:        ${FORTEMI_INSTALL_MODE}"
    echo ""
    echo "Passed: ${#PASS[@]}"
    printf '  - %s\n' "${PASS[@]}"
    if [ "${#WARNINGS[@]}" -gt 0 ]; then
        echo "Warnings: ${#WARNINGS[@]}"
        printf '  - %s\n' "${WARNINGS[@]}"
    fi
    if [ "${#FAIL[@]}" -gt 0 ]; then
        echo "Failures: ${#FAIL[@]}"
        printf '  - %s\n' "${FAIL[@]}"
        echo ""
        echo "Next actions:"
        echo "  docker compose ${COMPOSE_FILES[*]} logs -f fortemi"
        echo "  Re-run this script after fixing the failures."
        return 1
    fi
    echo ""
    echo "Next actions:"
    echo "  Open ${API_URL}/health to confirm health from the host."
    echo "  Inspect ${INSTALL_DIR}/.env before exposing the service beyond localhost."
}

if [ ! -f docker-compose.bundle.yml ] || [ ! -x installer/scripts/configure.sh ]; then
    record_fail "run from a Fortemi checkout or set INSTALL_DIR to one"
    print_report
    exit 1
fi

if [ "${FORTEMI_INSTALL_MODE}" = "secure" ] && [ -z "${FORTEMI_API_TOKEN:-}" ]; then
    record_warn "secure mode selected without FORTEMI_API_TOKEN; protected API validation may fail"
fi

require_command docker || true
require_command curl || true
require_command python3 || true
if [ "${#FAIL[@]}" -gt 0 ]; then
    print_report
    exit 1
fi
record_pass "host platform detected: ${HOST_OS}/${HOST_ARCH}"

if ! docker compose version >/dev/null 2>&1; then
    record_fail "Docker Compose v2 plugin is not available"
    print_report
    exit 1
fi
record_pass "Docker Compose: $(docker compose version --short 2>/dev/null || docker compose version)"

PROFILE="$(detect_profile)"
case "${PROFILE}" in
    nvidia)
        record_pass "hardware profile: nvidia"
        ;;
    cpu|intel|amd)
        record_pass "hardware profile: ${PROFILE}"
        write_local_override
        ;;
    *)
        record_fail "unsupported FORTEMI_HARDWARE_PROFILE=${PROFILE}"
        print_report
        exit 1
        ;;
esac

if docker run --rm --add-host=host.docker.internal:host-gateway alpine:3.20 \
    getent hosts host.docker.internal >/dev/null 2>&1; then
    record_pass "host.docker.internal resolves from containers"
else
    record_warn "host.docker.internal probe failed; Docker may need host-gateway support"
fi

export INSTALL_DIR FORTEMI_INSTALL_MODE INFERENCE_PROVIDER
if [ "${INFERENCE_PROVIDER}" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    export OPENAI_API_KEY=local-openai-compatible
fi

installer/scripts/configure.sh
record_pass "generated .env"

if [ "$(printf '%s' "${FORTEMI_SKIP_DEPLOY}" | tr '[:upper:]' '[:lower:]')" != "true" ]; then
    compose pull
    compose up -d
    record_pass "compose stack started"
else
    record_warn "deploy skipped by FORTEMI_SKIP_DEPLOY=true"
fi

wait_for_health

api_get /api/v1/inference/config >/dev/null
record_pass "inference config endpoint returned HTTP 200"
api_get /api/v1/embedding-configs/default >/dev/null
record_pass "default embedding config endpoint returned HTTP 200 before queueing embeddings"

NOTE_BODY='{"title":"Fortemi hands-free validation","content":"fortemi-hands-free-validation semantic embedding smoke note","format":"markdown","revision_mode":"none","pipeline":[],"tags":["validation/hands-free"]}'
NOTE_JSON="$(api_post /api/v1/notes "${NOTE_BODY}")"
NOTE_ID="$(printf '%s' "${NOTE_JSON}" | json_field id)"
record_pass "created validation note ${NOTE_ID}"

JOB_JSON="$(api_post /api/v1/jobs "{\"note_id\":\"${NOTE_ID}\",\"job_type\":\"embedding\",\"deduplicate\":false}")"
JOB_ID="$(printf '%s' "${JOB_JSON}" | json_field id)"
record_pass "queued embedding job ${JOB_ID}"
wait_for_job "${JOB_ID}"

run_search fts
run_search semantic
run_search hybrid
run_chat_if_configured

SHARD_PATH="${INSTALL_DIR}/fortemi-validation.shard"
api_get '/api/v1/backup/knowledge-shard?include=notes,embedding_sets' > "${SHARD_PATH}"
if [ -s "${SHARD_PATH}" ]; then
    record_pass "exported Knowledge Shard to ${SHARD_PATH}"
else
    record_fail "Knowledge Shard export was empty"
fi

print_report

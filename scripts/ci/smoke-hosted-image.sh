#!/usr/bin/env bash
# Start a hosted Fortemi image (built with kms-vault) in hosted mode against a
# disposable OpenBao Transit server and prove /readyz reports the key provider
# ready (#1172).
#
# Usage:
#   scripts/ci/smoke-hosted-image.sh --image <ref> [--platform linux/arm64] \
#     [--output receipt.json]
#
# Disposable stack, all on 127.0.0.1 with per-run ports and credentials:
#   * OpenBao (digest-pinned, ci/digests.txt) with TLS from a per-run CA,
#     in-memory storage, a derived aes256-gcm96 Transit key and a runtime token
#     limited to keys/encrypt/decrypt on that key;
#   * PostgreSQL (testdb image) with the hosted two-role split
#     (docs/deployment/hosted-postgresql-role.md) and Redis for hosted quota;
#   * the image under test: `--migrate-only` with the migration role, then the
#     API with FORTEMI_MULTI_TENANT=true, FORTEMI_KEY_PROVIDER=vault-transit and
#     the runtime role. The API container runs with the caller's UID so the
#     bind-mounted 0600 Vault token passes the provider's owner check.
# The run fails unless /readyz returns HTTP 200 with key_provider.status
# "ready". All key material lives in a mktemp directory removed on exit.
#
# Environment: TESTDB_IMAGE (default matric-testdb:local, built when missing),
# SMOKE_TIMEOUT_SECONDS (default 600), KEEP_HOSTED_SMOKE=1 keeps the stack.
set -euo pipefail

IMAGE="" PLATFORM="" OUTPUT=""
while (( $# )); do
    case "$1" in
        --image) IMAGE="$2"; shift 2 ;;
        --platform) PLATFORM="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$IMAGE" ]] || { echo "--image is required" >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OPENBAO_IMAGE="quay.io/openbao/openbao@sha256:41dc3e47da01575e1ffea70aa635180b57ff999264398313334c259a697bf7a2"
REDIS_IMAGE="redis:7-alpine@sha256:6ab0b6e7381779332f97b8ca76193e45b0756f38d4c0dcda72dbb3c32061ab99"
TESTDB_IMAGE="${TESTDB_IMAGE:-matric-testdb:local}"
TIMEOUT="${SMOKE_TIMEOUT_SECONDS:-600}"
TRANSIT_KEY=fortemi-hosted-smoke
RUN_ID="fortemi-hosted-smoke-$$-$(date +%s)"
BAO="${RUN_ID}-bao" DB="${RUN_ID}-db" REDIS="${RUN_ID}-redis" APP="${RUN_ID}-api"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fortemi-hosted-smoke.XXXXXX")"
chmod 0700 "$WORK"
PLATFORM_ARGS=()
[[ -n "$PLATFORM" ]] && PLATFORM_ARGS=(--platform "$PLATFORM")

log() { printf '[hosted-smoke] %s\n' "$*"; }
fail() { printf '[hosted-smoke] FAIL: %s\n' "$*" >&2; exit 1; }
cleanup() {
    if [[ "${KEEP_HOSTED_SMOKE:-0}" == 1 ]]; then
        log "kept ${BAO} ${DB} ${REDIS} ${APP} and ${WORK}"
        return
    fi
    docker rm -f "$APP" "$BAO" "$DB" "$REDIS" >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT

secret() { od -An -N16 -tx1 /dev/urandom | tr -d ' \n'; }
private_write() { (umask 077; printf '%s' "$2" > "$1"); }
bao_api() {
    # bao_api <method> <path> [json] ; token from $WORK/root.token when present
    local method="$1" path="$2" data="${3:-}" args=()
    [[ -s "${WORK}/root.token" ]] && args+=(-H "X-Vault-Token: $(cat "${WORK}/root.token")")
    [[ -n "$data" ]] && args+=(-H 'Content-Type: application/json' --data "$data")
    curl -fsS --cacert "${WORK}/ca.pem" -X "$method" "${args[@]}" "${BAO_ADDR}/v1/${path}"
}

for tool in docker curl jq openssl; do
    command -v "$tool" >/dev/null || fail "${tool} is required"
done

# --- Per-run TLS trust -------------------------------------------------------------
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=fortemi-hosted-smoke CA" \
    -keyout "${WORK}/ca.key" -out "${WORK}/ca.pem" \
    -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=localhost" \
    -keyout "${WORK}/server.key" -out "${WORK}/server.csr" 2>/dev/null
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' \
    > "${WORK}/server.ext"
openssl x509 -req -in "${WORK}/server.csr" -CA "${WORK}/ca.pem" -CAkey "${WORK}/ca.key" \
    -CAcreateserial -days 1 -extfile "${WORK}/server.ext" -out "${WORK}/server.pem" 2>/dev/null
rm -f "${WORK}/ca.key" "${WORK}/server.csr"
cat > "${WORK}/bao.hcl" <<'HCL'
disable_mlock = true
api_addr = "https://localhost:8200"
storage "inmem" {}
listener "tcp" {
  address = "0.0.0.0:8200"
  tls_cert_file = "/smoke/server.pem"
  tls_key_file = "/smoke/server.key"
}
HCL

# --- OpenBao Transit ----------------------------------------------------------------
log "starting OpenBao"
docker run -d --name "$BAO" --user "$(id -u):$(id -g)" --cap-drop ALL \
    --security-opt no-new-privileges -p 127.0.0.1::8200 -v "${WORK}:/smoke:ro" \
    --entrypoint bao "$OPENBAO_IMAGE" server -config=/smoke/bao.hcl >/dev/null
BAO_ADDR="https://localhost:$(docker port "$BAO" 8200/tcp | head -1 | sed 's/.*://')"
for _ in $(seq 1 120); do
    bao_api GET sys/init >/dev/null 2>&1 && break
    sleep 0.5
done
bao_api GET sys/init >/dev/null || { docker logs "$BAO" | tail -20; fail "OpenBao unreachable"; }
init="$(bao_api PUT sys/init '{"secret_shares":1,"secret_threshold":1}')"
private_write "${WORK}/root.token" "$(jq -r .root_token <<<"$init")"
unseal="$(jq -r '.keys_base64[0]' <<<"$init")"
bao_api PUT sys/unseal "$(jq -n --arg k "$unseal" '{key:$k}')" >/dev/null
unset init unseal
bao_api POST sys/mounts/transit '{"type":"transit"}' >/dev/null
# FORTEMI_KEY_STRATEGY defaults to per-purpose: the provider uses
# <FORTEMI_VAULT_TRANSIT_KEY>-<purpose>, and the startup canary uses user_secret.
key="${TRANSIT_KEY}-user_secret"
bao_api POST "transit/keys/${key}" \
    '{"type":"aes256-gcm96","derived":true,"exportable":false,"allow_plaintext_backup":false}' >/dev/null
policy="path \"transit/keys/${key}\" { capabilities = [\"read\"] }
path \"transit/encrypt/${key}\" { capabilities = [\"update\"] }
path \"transit/decrypt/${key}\" { capabilities = [\"update\"] }
path \"auth/token/lookup-self\" { capabilities = [\"read\"] }
path \"auth/token/renew-self\" { capabilities = [\"update\"] }"
bao_api PUT sys/policies/acl/fortemi-runtime "$(jq -n --arg p "$policy" '{policy:$p}')" >/dev/null
runtime="$(bao_api POST auth/token/create \
    '{"policies":["fortemi-runtime"],"ttl":"30m","renewable":true,"no_default_policy":true}')"
private_write "${WORK}/runtime.token" "$(jq -r .auth.client_token <<<"$runtime")"
unset runtime
rm -f "${WORK}/root.token"

# --- PostgreSQL (two roles) and Redis -----------------------------------------------
if ! docker image inspect "$TESTDB_IMAGE" >/dev/null 2>&1; then
    log "building ${TESTDB_IMAGE}"
    docker build -q -f "${ROOT}/build/Dockerfile.testdb" -t "$TESTDB_IMAGE" "$ROOT" >/dev/null
fi
OWNER_PASSWORD="$(secret)" RUNTIME_PASSWORD="$(secret)"
docker run -d --name "$DB" -p 127.0.0.1::5432 \
    -e POSTGRES_USER=matric -e POSTGRES_PASSWORD="$OWNER_PASSWORD" -e POSTGRES_DB=matric \
    "$TESTDB_IMAGE" >/dev/null
docker run -d --name "$REDIS" -p 127.0.0.1::6379 "$REDIS_IMAGE" >/dev/null
DB_PORT="$(docker port "$DB" 5432/tcp | head -1 | sed 's/.*://')"
REDIS_PORT="$(docker port "$REDIS" 6379/tcp | head -1 | sed 's/.*://')"
# TCP readiness: the image's first-boot init server listens on the socket only.
for _ in $(seq 1 90); do
    docker exec "$DB" pg_isready -h 127.0.0.1 -U matric -d matric >/dev/null 2>&1 && break
    sleep 1
done
docker exec "$DB" pg_isready -h 127.0.0.1 -U matric -d matric >/dev/null ||
    { docker logs "$DB" 2>&1 | tail -20; fail "postgres not ready"; }
psql_admin() { docker exec -i "$DB" psql -qtAU matric -d matric -v ON_ERROR_STOP=1 "$@"; }
psql_admin -c "CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis;" >/dev/null
MIGRATION_DATABASE_URL="$(printf '%s%s:%s@127.0.0.1:%s/%s' 'postgres://' matric "$OWNER_PASSWORD" "$DB_PORT" matric)"
DATABASE_URL="$(printf '%s%s:%s@127.0.0.1:%s/%s' 'postgres://' fortemi_runtime "$RUNTIME_PASSWORD" "$DB_PORT" matric)"

common_env=(
    -e FORTEMI_MULTI_TENANT=true -e REQUIRE_AUTH=true -e I_UNDERSTAND_NO_AUTH=false
    -e "MIGRATION_DATABASE_URL=${MIGRATION_DATABASE_URL}" -e "DATABASE_URL=${DATABASE_URL}"
)
log "applying migrations with ${IMAGE} --migrate-only"
docker run --rm --network host "${PLATFORM_ARGS[@]}" "${common_env[@]}" \
    "$IMAGE" /app/matric-api --migrate-only > "${WORK}/migrate.log" 2>&1 ||
    { tail -30 "${WORK}/migrate.log"; fail "--migrate-only failed"; }

# Hardened runtime role per docs/deployment/hosted-postgresql-role.md.
psql_admin <<SQL >/dev/null
CREATE ROLE fortemi_runtime LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION
  PASSWORD '${RUNTIME_PASSWORD}';
GRANT CONNECT ON DATABASE matric TO fortemi_runtime;
GRANT USAGE ON SCHEMA public TO fortemi_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO fortemi_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO fortemi_runtime;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO fortemi_runtime;
REVOKE CREATE ON SCHEMA public FROM fortemi_runtime;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public.tenant_registry FROM fortemi_runtime;
SQL

# --- Hosted API ------------------------------------------------------------------------
API_PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
log "starting hosted API on 127.0.0.1:${API_PORT}"
docker run -d --name "$APP" --network host "${PLATFORM_ARGS[@]}" \
    --user "$(id -u):$(id -g)" --cap-drop ALL --security-opt no-new-privileges \
    -v "${WORK}/ca.pem:/run/fortemi/ca.pem:ro" \
    -v "${WORK}/runtime.token:/run/fortemi/vault-token:ro" \
    "${common_env[@]}" \
    -e HOST=127.0.0.1 -e PORT="$API_PORT" \
    -e ISSUER_URL=https://issuer.hosted-smoke.invalid/realms/fortemi \
    -e FORTEMI_AUTH_AUDIENCE=https://api.hosted-smoke.invalid \
    -e "FORTEMI_QUOTA_REDIS_URL=redis://127.0.0.1:${REDIS_PORT}" \
    -e FORTEMI_KEY_PROVIDER=vault-transit -e "FORTEMI_VAULT_ADDR=${BAO_ADDR}" \
    -e FORTEMI_VAULT_TRANSIT_KEY="$TRANSIT_KEY" -e FORTEMI_VAULT_AUTH_METHOD=token-file \
    -e FORTEMI_VAULT_TOKEN_FILE=/run/fortemi/vault-token \
    -e FORTEMI_VAULT_CA_BUNDLE=/run/fortemi/ca.pem \
    -e WORKER_ENABLED=false -e FORTEMI_ATTACHMENTS_ENABLED=false -e DISABLE_SUPPORT_MEMORY=1 \
    -e FILE_STORAGE_PATH=/tmp/files -e LOG_FORMAT=json -e RUST_LOG=info \
    "$IMAGE" >/dev/null

started=$(date +%s)
status="" body=""
while true; do
    status="$(curl -s -o "${WORK}/readyz.json" -w '%{http_code}' --max-time 5 \
        "http://127.0.0.1:${API_PORT}/readyz" 2>/dev/null)" || true
    body=""
    [[ -s "${WORK}/readyz.json" ]] && body="$(cat "${WORK}/readyz.json")"
    if [[ "$status" == 200 ]]; then
        break
    fi
    if [[ "$(docker inspect -f '{{.State.Running}}' "$APP")" != true ]]; then
        docker logs "$APP" 2>&1 | tail -60
        fail "hosted API exited before /readyz was ready"
    fi
    if (( $(date +%s) - started > TIMEOUT )); then
        docker logs "$APP" 2>&1 | tail -60
        fail "/readyz not ready within ${TIMEOUT}s (last HTTP ${status:-none}: ${body})"
    fi
    sleep 2
done
key_status="$(jq -r '.key_provider.status // empty' <<<"$body")"
[[ "$key_status" == ready ]] || fail "/readyz key_provider is '${key_status:-absent}': ${body}"
elapsed=$(( $(date +%s) - started ))
arch="$(docker exec "$APP" uname -m)"
log "PASS: /readyz 200 with key_provider ready after ${elapsed}s (${arch})"

if [[ -n "$OUTPUT" ]]; then
    digest="$(docker image inspect --format '{{index .RepoDigests 0}}' "$IMAGE" 2>/dev/null || true)"
    mkdir -p "$(dirname "$OUTPUT")"
    jq -n --arg image "$IMAGE" --arg digest "$digest" --arg platform "${PLATFORM:-native}" \
        --arg arch "$arch" --arg bao "$OPENBAO_IMAGE" --argjson readyz "$body" \
        --argjson elapsed "$elapsed" --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '{
            schema: "fortemi.hosted-image-smoke.v1", image: $image, repo_digest: $digest,
            platform: $platform, container_uname_m: $arch, key_provider: "vault-transit",
            kms_stand_in: $bao, readyz: $readyz, ready_seconds: $elapsed, completed_at: $at }' \
        > "$OUTPUT"
    log "receipt: ${OUTPUT}"
fi

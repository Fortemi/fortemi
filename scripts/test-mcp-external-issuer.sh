#!/usr/bin/env bash
# test-mcp-external-issuer.sh - MCP over HTTP with externally issued OIDC tokens (#1151).
#
# Runs a disposable hosted stack and drives the MCP server with tokens from a
# disposable OIDC provider fixture:
#   * PostgreSQL (testdb image) and Redis (digest-pinned, ci/digests.txt);
#   * the TLS fixture issuer shared with the scoped-search tests
#     (crates/matric-api/src/scoped_search_tests/fixture-issuer.mjs), serving
#     discovery + JWKS and minting tokens with chosen iss/aud/exp/scope/tenant;
#   * a minimal Transit stand-in for the hosted KMS startup canary;
#   * matric-api --features hosted-auth,kms-vault in FORTEMI_MULTI_TENANT mode,
#     tenants provisioned with `matric-api admin bootstrap` (no hand-written SQL;
#     the suspended-tenant case flips one status column);
#   * the MCP server with MCP_TRANSPORT=http.
# scripts/mcp-external-issuer/run-cases.mjs then asserts: a token for the
# configured audience with `mcp` scope initializes a session and calls a tool;
# wrong issuer, wrong audience, unknown tenant, suspended tenant, missing `mcp`
# scope, expired, refresh tokens and a missing bearer are rejected with 401/403
# and a WWW-Authenticate challenge; protected-resource metadata advertises the
# external issuer and audience; no presented token reaches the API or MCP logs.
#
# All key material (TLS CA, JWT signing keys, Transit key and token) is
# generated per run inside a mktemp directory and removed on exit.
#
# Environment:
#   TESTDB_IMAGE           PostgreSQL+pgvector+PostGIS image (default: matric-testdb:local,
#                          built from build/Dockerfile.testdb when missing)
#   KEEP_MCP_OIDC_IT=1     keep containers and the work directory for inspection

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REDIS_IMAGE="redis:7-alpine@sha256:6ab0b6e7381779332f97b8ca76193e45b0756f38d4c0dcda72dbb3c32061ab99"
TESTDB_IMAGE="${TESTDB_IMAGE:-matric-testdb:local}"
RUN_ID="mcpoidc-$$-$(date +%s)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fortemi-mcp-oidc.XXXXXX")"
EVIDENCE="${WORK}/evidence"
DB_CONTAINER="${RUN_ID}-db"
REDIS_CONTAINER="${RUN_ID}-redis"
AUDIENCE="https://mcp-e2e.fortemi.invalid/mcp"
PIDS=()

log() { printf '[mcp-oidc-it] %s\n' "$*"; }
fail() { printf '[mcp-oidc-it] FAIL: %s\n' "$*" >&2; exit 1; }

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

cleanup() {
  local pid
  for pid in "${PIDS[@]}"; do
    kill -TERM "${pid}" 2>/dev/null || true
  done
  for pid in "${PIDS[@]}"; do
    wait "${pid}" 2>/dev/null || true
  done
  if [[ "${KEEP_MCP_OIDC_IT:-0}" == "1" ]]; then
    log "kept containers ${DB_CONTAINER} ${REDIS_CONTAINER} and ${WORK}"
    return
  fi
  docker rm -f "${DB_CONTAINER}" "${REDIS_CONTAINER}" >/dev/null 2>&1 || true
  rm -rf "${WORK}"
}
trap cleanup EXIT

wait_for_file() {
  local file="$1" pid="$2" what="$3"
  for _ in $(seq 1 100); do
    [[ -s "${file}" ]] && return 0
    kill -0 "${pid}" 2>/dev/null || fail "${what} exited during startup"
    sleep 0.1
  done
  fail "${what} did not start"
}

wait_for_http() {
  local url="$1" pid="$2" what="$3" logfile="$4"
  for _ in $(seq 1 120); do
    curl -fsS -o /dev/null "${url}" 2>/dev/null && return 0
    kill -0 "${pid}" 2>/dev/null || { tail -40 "${logfile}" >&2; fail "${what} exited"; }
    sleep 1
  done
  tail -40 "${logfile}" >&2
  fail "${what} never became ready"
}

for tool in docker python3 curl node openssl; do
  command -v "${tool}" >/dev/null || fail "${tool} is required"
done
mkdir -p "${EVIDENCE}" "${WORK}/files"
chmod 0700 "${WORK}"

if ! docker image inspect "${TESTDB_IMAGE}" >/dev/null 2>&1; then
  log "building ${TESTDB_IMAGE} from build/Dockerfile.testdb"
  docker build -q -f "${ROOT}/build/Dockerfile.testdb" -t "${TESTDB_IMAGE}" "${ROOT}" >/dev/null
fi

log "building matric-api --features hosted-auth,kms-vault"
(cd "${ROOT}" && cargo build -q --locked -p matric-api --features hosted-auth,kms-vault)
API_BIN="${ROOT}/target/debug/matric-api"
if [[ ! -d "${ROOT}/mcp-server/node_modules" ]]; then
  (cd "${ROOT}/mcp-server" && npm ci --ignore-scripts --no-fund --no-audit >/dev/null)
fi

# --- PostgreSQL and Redis ----------------------------------------------------
DB_PORT="$(free_port)"
REDIS_PORT="$(free_port)"
DB_PASSWORD="mcp-oidc-$$-$RANDOM"
RUNTIME_PASSWORD="mcp-oidc-rt-$$-$RANDOM"
docker run -d --name "${DB_CONTAINER}" -p "127.0.0.1:${DB_PORT}:5432" \
  -e POSTGRES_USER=matric -e POSTGRES_PASSWORD="${DB_PASSWORD}" -e POSTGRES_DB=matric_mcp_oidc \
  "${TESTDB_IMAGE}" >/dev/null
docker run -d --name "${REDIS_CONTAINER}" -p "127.0.0.1:${REDIS_PORT}:6379" "${REDIS_IMAGE}" >/dev/null
psql_admin() { docker exec -i "${DB_CONTAINER}" psql -qtAU matric -d matric_mcp_oidc -v ON_ERROR_STOP=1 "$@"; }
for _ in $(seq 1 60); do
  docker exec "${DB_CONTAINER}" pg_isready -U matric -d matric_mcp_oidc >/dev/null 2>&1 && break
  sleep 1
done
docker exec "${DB_CONTAINER}" pg_isready -U matric -d matric_mcp_oidc >/dev/null || fail "postgres not ready"
sleep 2
psql_admin -c "CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis;" >/dev/null
MIGRATION_DATABASE_URL="$(printf '%s%s:%s@127.0.0.1:%s/%s' 'postgres://' matric "${DB_PASSWORD}" "${DB_PORT}" matric_mcp_oidc)"
DATABASE_URL="$(printf '%s%s:%s@127.0.0.1:%s/%s' 'postgres://' fortemi_runtime "${RUNTIME_PASSWORD}" "${DB_PORT}" matric_mcp_oidc)"

# --- Tenants (migrations run as part of the first bootstrap) ------------------
bootstrap_tenant() {
  MIGRATION_DATABASE_URL="${MIGRATION_DATABASE_URL}" "${API_BIN}" admin bootstrap --slug "$1" --json \
    2>>"${WORK}/bootstrap.log" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tenant_id"])'
}
log "provisioning tenants with matric-api admin bootstrap"
ACTIVE_TENANT="$(bootstrap_tenant mcp-e2e-active)" || { tail -20 "${WORK}/bootstrap.log"; fail "bootstrap failed"; }
SUSPENDED_TENANT="$(bootstrap_tenant mcp-e2e-suspended)" || fail "bootstrap failed"
psql_admin -c "UPDATE tenant_registry SET status='suspended' WHERE id='${SUSPENDED_TENANT}'" >/dev/null

# Hardened runtime role per docs/deployment/hosted-postgresql-role.md.
psql_admin <<SQL >/dev/null
CREATE ROLE fortemi_runtime LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION
  PASSWORD '${RUNTIME_PASSWORD}';
GRANT CONNECT ON DATABASE matric_mcp_oidc TO fortemi_runtime;
GRANT USAGE ON SCHEMA public TO fortemi_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO fortemi_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO fortemi_runtime;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO fortemi_runtime;
REVOKE CREATE ON SCHEMA public FROM fortemi_runtime;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public.tenant_registry FROM fortemi_runtime;
SQL

# --- Ephemeral TLS trust, OIDC issuer and Transit stand-in --------------------
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=fortemi-mcp-oidc-it CA" \
  -keyout "${WORK}/ca.key" -out "${WORK}/ca.pem" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=127.0.0.1" \
  -keyout "${WORK}/server.key" -out "${WORK}/server.csr" 2>/dev/null
printf 'subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' >"${WORK}/server.ext"
openssl x509 -req -in "${WORK}/server.csr" -CA "${WORK}/ca.pem" -CAkey "${WORK}/ca.key" \
  -CAcreateserial -days 1 -extfile "${WORK}/server.ext" -out "${WORK}/server.pem" 2>/dev/null
rm -f "${WORK}/ca.key" "${WORK}/server.csr"

FORTEMI_FIXTURE_PROFILE=mcp-external-issuer node \
  "${ROOT}/crates/matric-api/src/scoped_search_tests/fixture-issuer.mjs" "${WORK}" "${EVIDENCE}" \
  >"${WORK}/issuer.log" 2>&1 &
PIDS+=($!)
wait_for_file "${WORK}/issuer.json" "${PIDS[-1]}" "fixture issuer"
ISSUER="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["issuer"])' "${WORK}/issuer.json")"

node "${ROOT}/scripts/mcp-external-issuer/transit-fixture.mjs" "${WORK}" >"${WORK}/transit.log" 2>&1 &
PIDS+=($!)
wait_for_file "${WORK}/transit.json" "${PIDS[-1]}" "transit fixture"
VAULT_ADDR_FIXTURE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["addr"])' "${WORK}/transit.json")"

# --- API (hosted mode) ---------------------------------------------------------
API_PORT="$(free_port)"
API_LOG="${WORK}/api.log"
log "starting hosted API on ${API_PORT} (issuer ${ISSUER})"
env -i PATH="${PATH}" HOME="${HOME}" \
  HOST=127.0.0.1 PORT="${API_PORT}" \
  DATABASE_URL="${DATABASE_URL}" MIGRATION_DATABASE_URL="${MIGRATION_DATABASE_URL}" \
  FORTEMI_MULTI_TENANT=true REQUIRE_AUTH=true I_UNDERSTAND_NO_AUTH=false \
  ISSUER_URL="${ISSUER}" FORTEMI_ALLOW_LOCAL_ISSUER=true \
  FORTEMI_AUTH_AUDIENCE="${AUDIENCE}" FORTEMI_AUTH_CA_BUNDLE="${WORK}/ca.pem" \
  FORTEMI_AUTH_CLOCK_SKEW_SECONDS=0 FORTEMI_AUTH_HTTP_TIMEOUT_SECONDS=2 \
  FORTEMI_QUOTA_REDIS_URL="redis://127.0.0.1:${REDIS_PORT}" \
  REDIS_URL="redis://127.0.0.1:${REDIS_PORT}" \
  FORTEMI_KEY_PROVIDER=vault-transit FORTEMI_VAULT_ADDR="${VAULT_ADDR_FIXTURE}" \
  FORTEMI_VAULT_TRANSIT_KEY=fortemi-mcp-oidc-it FORTEMI_VAULT_AUTH_METHOD=token-file \
  FORTEMI_VAULT_TOKEN_FILE="${WORK}/transit-token" FORTEMI_VAULT_CA_BUNDLE="${WORK}/ca.pem" \
  WORKER_ENABLED=false FORTEMI_ATTACHMENTS_ENABLED=false DISABLE_SUPPORT_MEMORY=1 \
  FILE_STORAGE_PATH="${WORK}/files" \
  LOG_FORMAT=text LOG_ANSI=false RUST_LOG="info,matric_api=debug,fortemi.security=debug" \
  "${API_BIN}" >"${API_LOG}" 2>&1 &
PIDS+=($!)
wait_for_http "http://127.0.0.1:${API_PORT}/health" "${PIDS[-1]}" "API" "${API_LOG}"

# --- MCP server ------------------------------------------------------------------
MCP_PORT="$(free_port)"
MCP_LOG="${WORK}/mcp.log"
log "starting MCP server on ${MCP_PORT}"
env -i PATH="${PATH}" HOME="${HOME}" \
  MCP_TRANSPORT=http MCP_PORT="${MCP_PORT}" MCP_BASE_URL="http://127.0.0.1:${MCP_PORT}" \
  FORTEMI_URL="http://127.0.0.1:${API_PORT}" ISSUER_URL="${ISSUER}" \
  FORTEMI_MULTI_TENANT=true MCP_RESOURCE_URI="${AUDIENCE}" FORTEMI_AUTH_AUDIENCE="${AUDIENCE}" \
  node "${ROOT}/mcp-server/index.js" >"${MCP_LOG}" 2>&1 &
PIDS+=($!)
wait_for_http "http://127.0.0.1:${MCP_PORT}/health" "${PIDS[-1]}" "MCP server" "${MCP_LOG}"

# --- Cases -------------------------------------------------------------------------
FORTEMI_TEST_ISSUER="${ISSUER}" FORTEMI_TEST_CA="${WORK}/ca.pem" \
  node "${ROOT}/scripts/mcp-external-issuer/run-cases.mjs" \
  "http://127.0.0.1:${MCP_PORT}" "${AUDIENCE}" "${ACTIVE_TENANT}" "${SUSPENDED_TENANT}" \
  "${API_LOG}" "${MCP_LOG}" \
  || { log "API log tail:"; tail -30 "${API_LOG}" >&2; log "MCP log tail:"; tail -30 "${MCP_LOG}" >&2; fail "case matrix failed"; }
log "PASS"

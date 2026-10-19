#!/usr/bin/env bash
# Qualify the shipped reference realm and claim policy against real Keycloak.
# Credentials and TLS keys are generated per run; output contains case names only.
# TESTDB_IMAGE defaults to the local image built from build/Dockerfile.testdb.
# KEEP_KEYCLOAK_IT=1 keeps containers/network and the private work/evidence dir.
set -euo pipefail

remove_docker() { bash "$(dirname "${BASH_SOURCE[0]}")/ci/remove-run-docker.sh" "$@"; }

# Fail before building or starting containers if bind mounts are reintroduced.
if grep -qE -- '(^|[[:space:]])(-v|--volume|--mount)[[:space:]=]' "${BASH_SOURCE[0]}"; then
  printf '%s\n' 'FAIL: Keycloak harness must copy files instead of bind-mounting them' >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEYCLOAK_IMAGE="quay.io/keycloak/keycloak:26.7.3@sha256:ff4257d0d64efbe99ed1ddfaf07765cc3c36dc7518bf8324d41961327f441c54"
TESTDB_IMAGE="${TESTDB_IMAGE:-matric-testdb:local}"
RUN_ID="keycloak-it-${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}-${CI_RESOURCE_ATTEMPT:-1}-$$-$(date +%s)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fortemi-keycloak.XXXXXX")"
EVIDENCE="${WORK}/evidence"
NETWORK="${RUN_ID}"
KEYCLOAK_NAME="${RUN_ID}-kc"
DB_CONTAINER="${RUN_ID}-db"
API_CONTAINER="${RUN_ID}-api"
PIDS=()
mkdir -p "${EVIDENCE}" "${WORK}/tls" "${WORK}/files"
chmod 0700 "${WORK}"
# tee records all script output for the driver's secret-leak assertions.
exec > >(tee -a "${WORK}/output.log") 2>&1
log() { printf '[keycloak-it] %s\n' "$*"; }
fail() { log "FAIL: $*"; exit 1; }
cleanup() {
  local pid
  for pid in "${PIDS[@]}"; do kill -TERM "${pid}" 2>/dev/null || true; done
  for pid in "${PIDS[@]}"; do wait "${pid}" 2>/dev/null || true; done
  if [[ "${KEEP_KEYCLOAK_IT:-0}" == "1" ]]; then
    log "kept ${KEYCLOAK_NAME} ${DB_CONTAINER} ${API_CONTAINER}, network ${NETWORK}, evidence ${EVIDENCE}"
    return
  fi
  remove_docker rm -fv "${KEYCLOAK_NAME}" "${DB_CONTAINER}" "${API_CONTAINER}" || true
  remove_docker network rm "${NETWORK}" || true
  bash "$ROOT/scripts/ci/cleanup-local-temp.sh" "${WORK}"
}
trap cleanup EXIT
free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}
wait_http() {
  local url="$1" what="$2" process="$3"
  for _ in $(seq 1 120); do
    curl --connect-timeout 2 --max-time 5 -fsS -o /dev/null "${url}" 2>/dev/null && return 0
    if [[ "${what}" == "API" ]]; then
      [[ "$(docker inspect --format '{{.State.Running}}' "${process}")" == "true" ]] || fail "API exited during startup"
    else
      kill -0 "${process}" 2>/dev/null || fail "${what} exited during startup"
    fi
    sleep 1
  done
  fail "${what} never became ready (logs are in the private work directory)"
}
for tool in docker python3 curl node openssl cargo ldd tar; do
  command -V "${tool}" >/dev/null || fail "${tool} is required"
done
if ! docker image inspect "${TESTDB_IMAGE}" >/dev/null 2>&1; then
  log "building testdb image"
  docker build -q -f "${ROOT}/build/Dockerfile.testdb" -t "${TESTDB_IMAGE}" "${ROOT}" >/dev/null
fi
log "building matric-api --features hosted-auth"
(cd "${ROOT}" && cargo build -q --locked -p matric-api --features hosted-auth)
TARGET_DIR="$(cd "${ROOT}" && cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
API_BIN="${TARGET_DIR}/debug/matric-api"
# Carry the executable's host libraries and loader into the disposable runtime.
# Developer hosts may use newer glibc than the Debian testdb image. This small
# directory keeps the host-built binary runnable without an extra mutable image.
python3 - "${API_BIN}" "${WORK}" <<'PYLIBS'
from pathlib import Path
import re, shutil, subprocess, sys
binary, work = sys.argv[1], Path(sys.argv[2])
listing = subprocess.check_output(["ldd", binary], text=True)
paths = re.findall(r"(?:=>\s+|^\s*)(/[^\s]+)\s+\(", listing, re.MULTILINE)
loader = next(p for p in paths if Path(p).name.startswith("ld-linux"))
runtime = work / "runtime"; runtime.mkdir()
for path in paths:
    shutil.copy2(path, runtime / Path(path).name)
(work / "loader").write_text(Path(loader).name)
PYLIBS
LOADER="$(cat "${WORK}/loader")"
if [[ ! -d "${ROOT}/mcp-server/node_modules" ]]; then
  (cd "${ROOT}/mcp-server" && npm ci --ignore-scripts --no-fund --no-audit >/dev/null)
fi

# Private generated credentials are read directly by the driver and docker's
# env-file option. No password or token goes into a command argument or output.
python3 - "${WORK}" <<'PY'
import json, secrets, sys, uuid
from pathlib import Path
work = Path(sys.argv[1])
c = {key: secrets.token_urlsafe(32) for key in (
    "admin_password", "tester_password", "admin-tester_password", "client_secret", "db_password",
    "runtime_db_password")}
c["admin_username"] = "bootstrap-" + secrets.token_hex(8)
(work / "credentials.json").write_text(json.dumps(c))
(work / "keycloak.env").write_text(
    f"KC_BOOTSTRAP_ADMIN_USERNAME={c['admin_username']}\nKC_BOOTSTRAP_ADMIN_PASSWORD={c['admin_password']}\n")
(work / "db.env").write_text(f"POSTGRES_USER=matric\nPOSTGRES_PASSWORD={c['db_password']}\nPOSTGRES_DB=matric_keycloak\n")
(work / "tenant").write_text(str(uuid.uuid4()))
PY
chmod 0600 "${WORK}/"*.json "${WORK}/"*.env
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=Fortemi Keycloak test CA" \
  -keyout "${WORK}/tls/ca.key" -out "${WORK}/tls/ca.pem" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=${KEYCLOAK_NAME}" \
  -keyout "${WORK}/tls/server.key" -out "${WORK}/tls/server.csr" 2>/dev/null
printf 'subjectAltName=DNS:%s\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' \
  "${KEYCLOAK_NAME}" >"${WORK}/tls/server.ext"
openssl x509 -req -in "${WORK}/tls/server.csr" -CA "${WORK}/tls/ca.pem" -CAkey "${WORK}/tls/ca.key" \
  -CAcreateserial -days 1 -extfile "${WORK}/tls/server.ext" -out "${WORK}/tls/server.pem" 2>/dev/null
rm -f "${WORK}/tls/ca.key" "${WORK}/tls/server.csr"
# Keep the leaf key private on the host and readable only by its container owner.
chmod 0700 "${WORK}/tls"
chmod 0400 "${WORK}/tls/server.key"
docker network create --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}" "${NETWORK}" >/dev/null
KEYCLOAK_PORT="$(free_port)"
export KEYCLOAK_NAME KEYCLOAK_PORT
# No bind mounts: the CI runner's private /tmp (and possibly the checkout) is
# invisible to the Docker daemon, so host paths would arrive empty. Copy files
# into each disposable container before starting it instead.
docker create --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}" --name "${KEYCLOAK_NAME}" --network "${NETWORK}" \
  -p "127.0.0.1:${KEYCLOAK_PORT}:8443" --env-file "${WORK}/keycloak.env" \
  -e "KC_HOSTNAME=https://${KEYCLOAK_NAME}:8443" \
  "${KEYCLOAK_IMAGE}" start-dev \
  --https-certificate-file=/tls/server.pem --https-certificate-key-file=/tls/server.key >/dev/null
# The image runs as uid 1000. Set archive ownership without changing host owners;
# docker cp -a preserves that ownership and the private directory/key modes.
tar --owner=1000 --group=0 -C "${WORK}" -cf - tls \
  | docker cp -a - "${KEYCLOAK_NAME}:/"
docker start "${KEYCLOAK_NAME}" >/dev/null
log "importing shipped realm through the admin API"
python3 "${ROOT}/scripts/keycloak-reference-realm/run-cases.py" prepare "${WORK}" \
  "${ROOT}/deploy/identity/keycloak/realm-example.json"

# Reuse the testdb image as the API runtime with the executable's host libraries.
# Both containers use Docker DNS for the
# Keycloak hostname; only the API/Keycloak test ports are published on loopback.
docker run --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}" -d --name "${DB_CONTAINER}" --network "${NETWORK}" \
  --env-file "${WORK}/db.env" "${TESTDB_IMAGE}" >/dev/null
# The init server is socket-only and shuts down before the final TCP server.
for _ in $(seq 1 60); do
  docker exec "${DB_CONTAINER}" pg_isready -h 127.0.0.1 -U matric -d matric_keycloak >/dev/null 2>&1 && break
  sleep 1
done
docker exec "${DB_CONTAINER}" pg_isready -h 127.0.0.1 -U matric -d matric_keycloak >/dev/null || fail "PostgreSQL not ready"
# External-OIDC startup requires a non-bypass runtime login and separate
# migration credentials (docs/content/external-oidc-upgrade.md). Apply the
# documented pre-start grants so this run also exercises that procedure.
# Only a client-computed SCRAM-SHA-256 verifier reaches the server (on stdin),
# so the plaintext cannot appear in process arguments or server logs.
{ python3 - "${WORK}/credentials.json" <<'PY'
import base64, hashlib, hmac, json, os, sys
pw = json.load(open(sys.argv[1]))["runtime_db_password"].encode()
salt, iters = os.urandom(16), 4096
salted = hashlib.pbkdf2_hmac("sha256", pw, salt, iters)
client = hmac.new(salted, b"Client Key", "sha256").digest()
server = hmac.new(salted, b"Server Key", "sha256").digest()
b64 = lambda b: base64.b64encode(b).decode()
print("\\set runtime_verifier 'SCRAM-SHA-256$%d:%s$%s:%s'" % (
    iters, b64(salt), b64(hashlib.sha256(client).digest()), b64(server)))
PY
  cat <<'SQL'
ALTER ROLE matric BYPASSRLS;
GRANT CREATE ON DATABASE matric_keycloak TO matric;
GRANT USAGE,CREATE ON SCHEMA public TO matric;
CREATE ROLE fortemi_runtime LOGIN NOSUPERUSER NOBYPASSRLS
    NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD :'runtime_verifier';
GRANT CONNECT ON DATABASE matric_keycloak TO fortemi_runtime;
REVOKE CREATE ON DATABASE matric_keycloak FROM fortemi_runtime;
REVOKE CREATE ON SCHEMA public FROM PUBLIC,fortemi_runtime;
DO $$
DECLARE s RECORD;
BEGIN
    FOR s IN SELECT nspname FROM pg_namespace
        WHERE nspname='public' OR nspname LIKE 'archive\_%' ESCAPE '\' LOOP
        EXECUTE format('GRANT USAGE ON SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('ALTER DEFAULT PRIVILEGES FOR ROLE matric IN SCHEMA %I GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO fortemi_runtime',s.nspname);
        EXECUTE format('ALTER DEFAULT PRIVILEGES FOR ROLE matric IN SCHEMA %I GRANT USAGE,SELECT ON SEQUENCES TO fortemi_runtime',s.nspname);
    END LOOP;
END $$;
SQL
} | docker exec -i "${DB_CONTAINER}" psql -qtA -h 127.0.0.1 -U matric -d matric_keycloak --set=ON_ERROR_STOP=1 >/dev/null \
  || fail "runtime role setup failed"
API_PORT="$(free_port)"
python3 - "${WORK}" "${DB_CONTAINER}" <<'PY'
import json, sys
from pathlib import Path
work = Path(sys.argv[1]); c = json.loads((work / "credentials.json").read_text())
(work / "api.env").write_text(
    f"MIGRATION_DATABASE_URL=postgres://matric:{c['db_password']}@{sys.argv[2]}:5432/matric_keycloak\n"
    f"DATABASE_URL=postgres://fortemi_runtime:{c['runtime_db_password']}@{sys.argv[2]}:5432/matric_keycloak\n"
    f"FORTEMI_AUTH_DEFAULT_TENANT={(work / 'tenant').read_text()}\n")
PY
chmod 0600 "${WORK}/api.env"
mkdir -p "${WORK}/api-root/fortemi" "${WORK}/api-root/work/tls"
cp "${API_BIN}" "${WORK}/api-root/fortemi/matric-api"
cp "${ROOT}/deploy/identity/claim-policy.example.json" "${WORK}/api-root/fortemi/claim-policy.json"
cp -a "${WORK}/runtime" "${WORK}/files" "${WORK}/api-root/work/"
cp "${WORK}/tls/ca.pem" "${WORK}/api-root/work/tls/ca.pem"
chmod 0700 "${WORK}/api-root/work/files"
docker create --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}" --name "${API_CONTAINER}" --network "${NETWORK}" \
  --user "$(id -u):$(id -g)" \
  -p "127.0.0.1:${API_PORT}:3000" --entrypoint "/work/runtime/${LOADER}" \
  --env-file "${WORK}/api.env" \
  -e HOST=0.0.0.0 -e PORT=3000 -e REQUIRE_AUTH=true -e FORTEMI_MULTI_TENANT=false \
  -e FORTEMI_AUTH_MODE=external-oidc -e FORTEMI_AUTH_ALLOW_MULTI_AUDIENCE=true -e ISSUER_URL=https://fortemi.example.org \
  -e "FORTEMI_AUTH_ISSUER=https://${KEYCLOAK_NAME}:8443/realms/example" \
  -e FORTEMI_AUTH_AUDIENCES=https://fortemi.example.org,https://fortemi.example.org/mcp \
  -e FORTEMI_AUTH_CLAIM_POLICY_FILE=/fortemi/claim-policy.json \
  -e FORTEMI_ALLOW_LOCAL_ISSUER=true -e FORTEMI_AUTH_CA_BUNDLE=/work/tls/ca.pem \
  -e WORKER_ENABLED=false -e FORTEMI_ATTACHMENTS_ENABLED=false -e DISABLE_SUPPORT_MEMORY=1 \
  -e FILE_STORAGE_PATH=/work/files -e LOG_FORMAT=text -e LOG_ANSI=false \
  -e RUST_LOG=info,matric_api=debug,fortemi.security=debug "${TESTDB_IMAGE}" \
  --library-path /work/runtime /fortemi/matric-api >/dev/null
# Match the copied files' owner so the API can execute the binary/loader and
# write its storage directory. Credentials and the TLS key stay out of this copy.
docker cp -a "${WORK}/api-root/." "${API_CONTAINER}:/"
docker start "${API_CONTAINER}" >/dev/null
docker logs -f "${API_CONTAINER}" >"${WORK}/api.log" 2>&1 &
PIDS+=($!)
wait_http "http://127.0.0.1:${API_PORT}/health" "API" "${API_CONTAINER}"
MCP_PORT="$(free_port)"
env -i PATH="${PATH}" HOME="${HOME}" \
  MCP_TRANSPORT=http MCP_PORT="${MCP_PORT}" MCP_HOST=127.0.0.1 \
  MCP_RESOURCE_URI=https://fortemi.example.org/mcp MCP_BASE_URL=https://fortemi.example.org/mcp \
  FORTEMI_URL="http://127.0.0.1:${API_PORT}" ISSUER_URL=https://fortemi.example.org \
  REQUIRE_AUTH=true FORTEMI_AUTH_MODE=external-oidc \
  FORTEMI_AUTH_ISSUER="https://${KEYCLOAK_NAME}:8443/realms/example" \
  node "${ROOT}/mcp-server/index.js" >"${WORK}/mcp.log" 2>&1 &
PIDS+=($!)
wait_http "http://127.0.0.1:${MCP_PORT}/health" "MCP server" "${PIDS[-1]}"
python3 "${ROOT}/scripts/keycloak-reference-realm/run-cases.py" run "${WORK}" \
  "http://127.0.0.1:${API_PORT}" "http://127.0.0.1:${MCP_PORT}" || fail "case matrix failed"
log "PASS: Keycloak reference realm"

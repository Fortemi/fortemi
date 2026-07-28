#!/usr/bin/env bash
set -euo pipefail

export CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}"

ORCHESTRATOR_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="${SUITE_PLATFORM_MANIFEST:-${ORCHESTRATOR_ROOT}/contracts/suite-conformance/platform-matrix.json}"
OUTPUT_INPUT="${1:-${ORCHESTRATOR_ROOT}/target/suite-platform-contract}"
if [[ "$OUTPUT_INPUT" == /* ]]; then
  OUTPUT_DIR="$OUTPUT_INPUT"
else
  OUTPUT_DIR="${PWD}/${OUTPUT_INPUT}"
fi
WORK_ROOT="${SUITE_PLATFORM_WORK_ROOT:-${ORCHESTRATOR_ROOT}/../suite-platform-work}"
mkdir -p "$WORK_ROOT"
WORK_DIR="$(mktemp -d "${WORK_ROOT}/run.XXXXXX")"
mkdir -p "${WORK_DIR}/tmp"
export TMPDIR="${WORK_DIR}/tmp"
AUTHORITY_DIR="${WORK_DIR}/fortemi"
REACT_DIR="${WORK_DIR}/fortemi-react"
HOTM_DIR="${WORK_DIR}/hotm"
DB_CONTAINER=""
DB_IMAGE=""
DB_EXTENSION_SQL=""
NATIVE_PG_BIN=""
NATIVE_PG_DATA=""
NATIVE_PG_PORT=""
API_PID=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ "$status" -ne 0 ]]; then
    mkdir -p "${OUTPUT_DIR}/diagnostics"
    for log in "${WORK_DIR}/authority-api.log" "${WORK_DIR}/authority-health.json"; do
      if [[ -f "$log" ]]; then
        cp "$log" "${OUTPUT_DIR}/diagnostics/"
      fi
    done
    if [[ -d "${HOTM_DIR}/ui/test-results/live-asset-ci-receipt" ]]; then
      mkdir -p "${OUTPUT_DIR}/hotm"
      cp -R "${HOTM_DIR}/ui/test-results/live-asset-ci-receipt/." \
        "${OUTPUT_DIR}/hotm/" || true
    fi
  fi
  if [[ -n "$API_PID" ]]; then
    kill "$API_PID" >/dev/null 2>&1 || true
    wait "$API_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DB_CONTAINER" ]]; then
    docker rm -f "$DB_CONTAINER" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DB_IMAGE" ]]; then
    docker image rm -f "$DB_IMAGE" >/dev/null 2>&1 || true
  fi
  if [[ -n "$NATIVE_PG_DATA" && -n "$NATIVE_PG_BIN" ]]; then
    "$NATIVE_PG_BIN/pg_ctl" -D "$NATIVE_PG_DATA" stop -m fast \
      >/dev/null 2>&1 || true
  fi
  if [[ "${SUITE_PLATFORM_PRESERVE_WORK:-0}" != "1" ]]; then
    rm -rf "${WORK_DIR}"
  fi
  exit "${status}"
}
trap cleanup EXIT INT TERM

json_value() {
  python3 - "$MANIFEST" "$1" <<'PY'
import json
import sys

value = json.load(open(sys.argv[1], encoding="utf-8"))
for segment in sys.argv[2].split("."):
    value = value[segment]
print(value)
PY
}

sha256_file() {
  python3 - "$1" <<'PY'
import hashlib
import sys

with open(sys.argv[1], "rb") as handle:
    print(hashlib.file_digest(handle, "sha256").hexdigest())
PY
}

clone_exact() {
  local repository="$1"
  local commit="$2"
  local destination="$3"
  git init -q "$destination"
  git -C "$destination" remote add origin "https://git.integrolabs.net/${repository}.git"
  git -C "$destination" fetch --quiet --no-tags --depth 1 origin "$commit"
  git -C "$destination" checkout --quiet --detach FETCH_HEAD
  [[ "$(git -C "$destination" rev-parse HEAD)" == "$commit" ]]
  [[ -z "$(git -C "$destination" status --porcelain)" ]]
}

find_port() {
  python3 <<'PY'
import socket

with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
}

provision_database() {
  if [[ "$PLATFORM_OS" == "macos" ]]; then
    provision_native_macos_database
    return
  fi

  local image port password ready
  command -v docker >/dev/null 2>&1 || {
    echo "docker is required to provision the isolated suite database" >&2
    exit 2
  }
  docker info >/dev/null
  image="fortemi-suite-testdb:${GITHUB_RUN_ID:-local-$$}"
  DB_IMAGE="$image"
  DB_CONTAINER="fortemi-suite-db-${GITHUB_RUN_ID:-$$}"
  port="$(find_port)"
  password="$(python3 -c 'import secrets; print(secrets.token_hex(24))')"

  docker build -q -f "$AUTHORITY_DIR/build/Dockerfile.testdb" \
    -t "$image" "$AUTHORITY_DIR" >/dev/null
  docker rm -f "$DB_CONTAINER" >/dev/null 2>&1 || true
  docker run -d --name "$DB_CONTAINER" \
    -p "127.0.0.1:${port}:5432" \
    -e POSTGRES_USER=matric \
    -e "POSTGRES_PASSWORD=${password}" \
    -e POSTGRES_DB=matric_suite \
    "$image" >/dev/null

  ready=false
  for _ in $(seq 1 60); do
    if docker exec "$DB_CONTAINER" pg_isready \
      -h 127.0.0.1 -p 5432 -U matric -d matric_suite >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 1
  done
  [[ "$ready" == "true" ]] || {
    docker logs "$DB_CONTAINER" >&2
    return 1
  }
  DATABASE_URL="postgres://matric:${password}@127.0.0.1:${port}/matric_suite"
  export DATABASE_URL
  SUITE_DB_PROVISIONING="managed-docker"
  SUITE_DB_ARCHITECTURE="$(
    docker image inspect "$image" --format '{{.Architecture}}'
  )"
  SUITE_DB_VERSION="$(
    docker exec "$DB_CONTAINER" postgres --version | sed 's/^postgres (PostgreSQL) //'
  )"
  SUITE_DB_EXTENSIONS="$(
    docker exec "$DB_CONTAINER" psql -At -U matric -d matric_suite \
      -c "SELECT string_agg(extname, ',' ORDER BY extname) FROM pg_extension"
  )"
  DB_EXTENSION_SQL="$(
    docker exec "$DB_CONTAINER" psql -At -U matric -d matric_suite \
      -c "SELECT format('CREATE EXTENSION IF NOT EXISTS %I;', extname)
          FROM pg_extension
          WHERE extname <> 'plpgsql'
          ORDER BY extname"
  )"
  export SUITE_DB_PROVISIONING SUITE_DB_ARCHITECTURE \
    SUITE_DB_VERSION SUITE_DB_EXTENSIONS
}

provision_native_macos_database() {
  local formula port ready
  command -v brew >/dev/null 2>&1 || {
    echo "Homebrew is required to provision the native macOS database" >&2
    exit 2
  }
  for formula in postgresql@18 postgis pgvector; do
    if ! brew list --versions "$formula" >/dev/null 2>&1; then
      brew install "$formula"
    fi
  done

  NATIVE_PG_BIN="$(brew --prefix postgresql@18)/bin"
  NATIVE_PG_DATA="${WORK_DIR}/postgres"
  NATIVE_PG_PORT="$(find_port)"
  export PATH="${NATIVE_PG_BIN}:$PATH"
  "$NATIVE_PG_BIN/initdb" -D "$NATIVE_PG_DATA" \
    --username=matric --auth=trust --no-locale --encoding=UTF8 >/dev/null
  "$NATIVE_PG_BIN/pg_ctl" -D "$NATIVE_PG_DATA" \
    -l "${WORK_DIR}/postgres.log" \
    -o "-h 127.0.0.1 -p ${NATIVE_PG_PORT} -c max_locks_per_transaction=256" \
    start >/dev/null

  ready=false
  for _ in $(seq 1 60); do
    if "$NATIVE_PG_BIN/pg_isready" -h 127.0.0.1 \
      -p "$NATIVE_PG_PORT" -U matric >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 1
  done
  [[ "$ready" == "true" ]] || {
    tail -n 100 "${WORK_DIR}/postgres.log" >&2 || true
    return 1
  }

  "$NATIVE_PG_BIN/createdb" -h 127.0.0.1 -p "$NATIVE_PG_PORT" \
    -U matric -O matric matric_suite
  for formula in vector postgis pg_trgm; do
    "$NATIVE_PG_BIN/psql" -v ON_ERROR_STOP=1 -h 127.0.0.1 \
      -p "$NATIVE_PG_PORT" -U matric -d matric_suite \
      -c "CREATE EXTENSION IF NOT EXISTS ${formula};" >/dev/null
  done

  DATABASE_URL="postgres://matric@127.0.0.1:${NATIVE_PG_PORT}/matric_suite"
  export DATABASE_URL
  SUITE_DB_PROVISIONING="managed-native"
  SUITE_DB_ARCHITECTURE="$(uname -m)"
  SUITE_DB_VERSION="$("$NATIVE_PG_BIN/postgres" --version |
    sed 's/^postgres (PostgreSQL) //')"
  SUITE_DB_EXTENSIONS="$(
    "$NATIVE_PG_BIN/psql" -At -h 127.0.0.1 -p "$NATIVE_PG_PORT" \
      -U matric -d matric_suite \
      -c "SELECT string_agg(extname, ',' ORDER BY extname) FROM pg_extension"
  )"
  DB_EXTENSION_SQL="$(
    "$NATIVE_PG_BIN/psql" -At -h 127.0.0.1 -p "$NATIVE_PG_PORT" \
      -U matric -d matric_suite \
      -c "SELECT format('CREATE EXTENSION IF NOT EXISTS %I;', extname)
          FROM pg_extension
          WHERE extname <> 'plpgsql'
          ORDER BY extname"
  )"
  export SUITE_DB_PROVISIONING SUITE_DB_ARCHITECTURE \
    SUITE_DB_VERSION SUITE_DB_EXTENSIONS
}

reset_database() {
  if [[ -n "$NATIVE_PG_DATA" ]]; then
    "$NATIVE_PG_BIN/dropdb" --force -h 127.0.0.1 -p "$NATIVE_PG_PORT" \
      -U matric matric_suite >/dev/null
    "$NATIVE_PG_BIN/createdb" -h 127.0.0.1 -p "$NATIVE_PG_PORT" \
      -U matric -O matric matric_suite >/dev/null
    if [[ -n "$DB_EXTENSION_SQL" ]]; then
      printf '%s\n' "$DB_EXTENSION_SQL" |
        "$NATIVE_PG_BIN/psql" -v ON_ERROR_STOP=1 -h 127.0.0.1 \
          -p "$NATIVE_PG_PORT" -U matric -d matric_suite >/dev/null
    fi
    return
  fi
  [[ -n "$DB_CONTAINER" ]] || {
    echo "suite phase isolation requires the managed database container" >&2
    return 1
  }
  docker exec "$DB_CONTAINER" dropdb \
    --force -U matric matric_suite >/dev/null
  docker exec "$DB_CONTAINER" createdb \
    -U matric -O matric matric_suite >/dev/null
  if [[ -n "$DB_EXTENSION_SQL" ]]; then
    printf '%s\n' "$DB_EXTENSION_SQL" |
      docker exec -i "$DB_CONTAINER" psql \
        -v ON_ERROR_STOP=1 -U matric -d matric_suite >/dev/null
  fi
}

start_authority_server() {
  local api_root ready registration client_id client_secret token_response
  API_PORT="$(find_port)"
  api_root="http://127.0.0.1:${API_PORT}"
  node - "${WORK_DIR}/signing-key.json" "${WORK_DIR}/trusted-keys.json" <<'NODE'
const crypto = require('node:crypto');
const fs = require('node:fs');
const [privatePath, trustPath] = process.argv.slice(2);
const seed = Buffer.alloc(32, 29);
const prefix = Buffer.from('302e020100300506032b657004220420', 'hex');
const privateKey = crypto.createPrivateKey({
  key: Buffer.concat([prefix, seed]),
  format: 'der',
  type: 'pkcs8',
});
const publicDer = crypto.createPublicKey(privateKey).export({
  format: 'der',
  type: 'spki',
});
const publicKey = publicDer.subarray(publicDer.length - 32).toString('base64url');
fs.writeFileSync(privatePath, `${JSON.stringify({
  key_id: 'suite-platform-contract-1',
  private_key: seed.toString('base64url'),
})}\n`, { mode: 0o600 });
fs.writeFileSync(trustPath, `${JSON.stringify([
  {
    key_id: 'suite-platform-contract-1',
    public_key: publicKey,
    revoked: false,
  },
  {
    key_id: 'fortemi-fixture-1',
    public_key: '6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw',
    revoked: false,
  },
])}\n`, { mode: 0o600 });
NODE

  env \
    DATABASE_URL="$DATABASE_URL" \
    MATRIC_GIT_SHA="$AUTHORITY_RUNTIME_COMMIT" \
    HOST=127.0.0.1 \
    PORT="$API_PORT" \
    REQUIRE_AUTH=true \
    FORTEMI_ALLOW_LOCAL_ISSUER=true \
    ISSUER_URL="$api_root" \
    ALLOWED_ORIGINS=http://localhost:1420,http://127.0.0.1:1420 \
    RATE_LIMIT_ENABLED=false \
    REDIS_URL=redis://127.0.0.1:1 \
    MATRIC_ATTACHMENT_SCAN_MODE=disabled \
    DISABLE_SUPPORT_MEMORY=true \
    FILE_STORAGE_PATH="${WORK_DIR}/authority-storage" \
    TUS_STAGING_DIR="${WORK_DIR}/authority-tus" \
    FORTEMI_SHARD_SIGNING_KEY_FILE="${WORK_DIR}/signing-key.json" \
    FORTEMI_SHARD_TRUSTED_KEYS_FILE="${WORK_DIR}/trusted-keys.json" \
    LOG_FORMAT=json \
    RUST_LOG=info \
    "${AUTHORITY_DIR}/target/release/matric-api" \
    >"${WORK_DIR}/authority-api.log" 2>&1 &
  API_PID=$!

  ready=false
  for _ in $(seq 1 120); do
    if curl -fsS "${api_root}/health" >"${WORK_DIR}/authority-health.json" \
      2>/dev/null; then
      ready=true
      break
    fi
    if ! kill -0 "$API_PID" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  if [[ "$ready" != "true" ]]; then
    tail -n 100 "${WORK_DIR}/authority-api.log" >&2 || true
    return 1
  fi
  [[ "$(
    node -e 'process.stdout.write(require(process.argv[1]).git_sha)' \
      "${WORK_DIR}/authority-health.json"
  )" == "$AUTHORITY_RUNTIME_COMMIT" ]]

  registration="$(
    curl -fsS -X POST "${api_root}/oauth/register" \
      -H 'Content-Type: application/json' \
      -d '{"client_name":"React core platform contract","grant_types":["client_credentials"],"scope":"read write"}'
  )"
  client_id="$(
    node -e 'process.stdout.write(JSON.parse(process.argv[1]).client_id)' \
      "$registration"
  )"
  client_secret="$(
    node -e 'process.stdout.write(JSON.parse(process.argv[1]).client_secret)' \
      "$registration"
  )"
  token_response="$(
    curl -fsS -X POST "${api_root}/oauth/token" \
      -H 'Content-Type: application/x-www-form-urlencoded' \
      --data-urlencode 'grant_type=client_credentials' \
      --data-urlencode "client_id=${client_id}" \
      --data-urlencode "client_secret=${client_secret}" \
      --data-urlencode 'scope=read write'
  )"
  FORTEMI_PLATFORM_SERVER_URL="$api_root"
  FORTEMI_PLATFORM_SERVER_TOKEN="$(
    node -e 'process.stdout.write(JSON.parse(process.argv[1]).access_token)' \
      "$token_response"
  )"
  export FORTEMI_PLATFORM_SERVER_URL FORTEMI_PLATFORM_SERVER_TOKEN
}

seed_authority_fixture() {
  local dry_run fixture http_status response response_file
  fixture="${AUTHORITY_DIR}/tests/fixtures/shards/full-v1-integrated-candidate.shard"
  response_file="${WORK_DIR}/authority-fixture-response.json"
  for dry_run in true false; do
    http_status="$(
      curl -sS -o "$response_file" -w '%{http_code}' -X POST \
        "${FORTEMI_PLATFORM_SERVER_URL}/api/v1/backup/knowledge-shard/upload?dry_run=${dry_run}&on_conflict=replace&verify_signature=require" \
        -H "Authorization: Bearer ${FORTEMI_PLATFORM_SERVER_TOKEN}" \
        -F "file=@${fixture};type=application/gzip"
    )"
    if [[ "$http_status" -lt 200 || "$http_status" -ge 300 ]]; then
      printf 'Signed authority fixture import returned HTTP %s: ' \
        "$http_status" >&2
      cat "$response_file" >&2
      printf '\n' >&2
      return 1
    fi
    response="$(cat "$response_file")"
    node -e '
      const value = JSON.parse(process.argv[1]);
      const expectedDryRun = process.argv[2] === "true";
      if (value.status !== "success" || value.dry_run !== expectedDryRun) {
        process.exit(1);
      }
    ' "$response" "$dry_run"
  done
}

stop_authority_server() {
  if [[ -n "$API_PID" ]]; then
    kill "$API_PID" >/dev/null 2>&1 || true
    wait "$API_PID" >/dev/null 2>&1 || true
    API_PID=""
  fi
  unset FORTEMI_PLATFORM_SERVER_TOKEN
}

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)
    PLATFORM_ID="linux-x86_64"
    PLATFORM_OS="linux"
    PLATFORM_ARCH="x86_64"
    FILESYSTEM="$(stat -f -c '%T' "$ORCHESTRATOR_ROOT")"
    ;;
  Darwin/arm64)
    PLATFORM_ID="macos-arm64"
    PLATFORM_OS="macos"
    PLATFORM_ARCH="arm64"
    FILESYSTEM="$(diskutil info "$ORCHESTRATOR_ROOT" 2>/dev/null |
      awk -F: '/File System Personality/ {gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}' ||
      true)"
    FILESYSTEM="${FILESYSTEM:-unknown-macos-filesystem}"
    ;;
  *)
    echo "Unsupported suite platform: $(uname -s)/$(uname -m)" >&2
    exit 2
    ;;
esac

python3 "${ORCHESTRATOR_ROOT}/scripts/ci/verify-suite-platform-matrix.py" \
  --manifest "$MANIFEST" manifest
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

AUTHORITY_REPOSITORY="$(json_value authority.repository)"
AUTHORITY_SCHEMA_COMMIT="$(json_value authority.schema_commit)"
AUTHORITY_RUNTIME_COMMIT="$(json_value authority.runtime_commit)"
REACT_REPOSITORY="$(json_value participants.fortemi_react.repository)"
REACT_COMMIT="$(json_value participants.fortemi_react.commit)"
HOTM_REPOSITORY="$(json_value participants.hotm.repository)"
HOTM_COMMIT="$(json_value participants.hotm.commit)"

clone_exact "$AUTHORITY_REPOSITORY" "$AUTHORITY_RUNTIME_COMMIT" "$AUTHORITY_DIR"
clone_exact "$REACT_REPOSITORY" "$REACT_COMMIT" "$REACT_DIR"
clone_exact "$HOTM_REPOSITORY" "$HOTM_COMMIT" "$HOTM_DIR"

git -C "$AUTHORITY_DIR" fetch --quiet --no-tags --depth 1 \
  origin "$AUTHORITY_SCHEMA_COMMIT"
SCHEMA_CONTRACT_FILE="${WORK_DIR}/schema-authority-contract.json"
git -C "$AUTHORITY_DIR" show \
  "${AUTHORITY_SCHEMA_COMMIT}:contracts/knowledge-shard/2.0.0/contract.json" \
  >"$SCHEMA_CONTRACT_FILE"
SCHEMA_CONTRACT_FILE="$SCHEMA_CONTRACT_FILE" MANIFEST="$MANIFEST" python3 <<'PY'
import hashlib
import json
import os

manifest = json.load(open(os.environ["MANIFEST"], encoding="utf-8"))
authority = manifest["authority"]
raw = open(os.environ["SCHEMA_CONTRACT_FILE"], "rb").read()
contract = json.loads(raw)
if hashlib.sha256(raw).hexdigest() != authority["contract_sha256"]:
    raise SystemExit("schema authority contract SHA-256 drift")
if contract["contractRevision"] != authority["contract_revision"]:
    raise SystemExit("schema authority revision drift")
if contract["schemaBundle"]["sha256"] != authority["schema_bundle_sha256"]:
    raise SystemExit("schema authority bundle drift")
PY

if [[ -n "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL must be unset; suite proof owns isolated phase databases" >&2
  exit 2
fi
provision_database

(
  cd "$AUTHORITY_DIR"
  scripts/ci/openapi-contract.sh check
  scripts/ci/asyncapi-contract.sh check
  python3 scripts/ci/verify-knowledge-shard-presence.py
  python3 -m unittest \
    tests/test_verify_knowledge_shard_matrix.py \
    tests/test_verify_knowledge_shard_presence.py
  python3 scripts/ci/verify-knowledge-shard-matrix.py \
    --output "$OUTPUT_DIR/knowledge-shard-matrix.json"
  cargo build --release --workspace
)

start_authority_server
stop_authority_server

(
  cd "$AUTHORITY_DIR"
  cargo test --package matric-jobs --test worker_integration_test -- --test-threads=1
  cargo test --workspace --exclude matric-jobs
  cargo test --doc
  cargo test --manifest-path tests/fortemi-auth-consumer/Cargo.toml --locked
)

python3 "${ORCHESTRATOR_ROOT}/scripts/ci/verify-suite-platform-matrix.py" \
  --manifest "$MANIFEST" authority \
  --platform-id "$PLATFORM_ID" \
  --filesystem "$FILESYSTEM" \
  --database-provisioning "$SUITE_DB_PROVISIONING" \
  --database-architecture "$SUITE_DB_ARCHITECTURE" \
  --database-version "$SUITE_DB_VERSION" \
  --database-extensions "$SUITE_DB_EXTENSIONS" \
  --runtime-checkout "$AUTHORITY_DIR" \
  --schema-contract "$SCHEMA_CONTRACT_FILE" \
  --output "$OUTPUT_DIR/authority-receipt.json"

reset_database
start_authority_server
seed_authority_fixture
(
  cd "$REACT_DIR"
  package_manager="$(node -p "require('./package.json').packageManager")"
  if [[ "$package_manager" =~ ^pnpm@([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    pnpm_version="${BASH_REMATCH[1]}"
  else
    printf 'Expected an exact pnpm packageManager declaration, got %s\n' \
      "$package_manager" >&2
    exit 2
  fi

  if command -v pnpm >/dev/null 2>&1 \
    && [[ "$(pnpm --version)" == "$pnpm_version" ]]; then
    pnpm_command=(pnpm)
  else
    command -v npm >/dev/null 2>&1 || {
      echo "npm is required to provision the pinned React/core pnpm version" >&2
      exit 2
    }
    npm install --prefix "${WORK_DIR}/pnpm-tools" \
      --no-save --ignore-scripts --no-audit --no-fund \
      "pnpm@${pnpm_version}" >/dev/null
    export PATH="${WORK_DIR}/pnpm-tools/node_modules/.bin:${PATH}"
    pnpm_command=(pnpm)
  fi
  [[ "$("${pnpm_command[@]}" --version)" == "$pnpm_version" ]]
  "${pnpm_command[@]}" install --frozen-lockfile
  VITEST_MAX_WORKERS="${VITEST_MAX_WORKERS:-2}" \
    node packages/core/scripts/run-platform-contract.mjs \
      --output "$OUTPUT_DIR/react-receipt.json"
  node packages/core/scripts/verify-platform-contract-receipt.mjs \
    "$OUTPUT_DIR/react-receipt.json"
  mkdir -p "$OUTPUT_DIR/react-package"
  "${pnpm_command[@]}" --dir packages/core pack \
    --pack-destination "$OUTPUT_DIR/react-package"
)
REACT_PACKAGE_TARBALL="$(find "$OUTPUT_DIR/react-package" -maxdepth 1 \
  -type f -name '*.tgz' -print)"
if [[ "$(printf '%s\n' "$REACT_PACKAGE_TARBALL" | sed '/^$/d' | wc -l | tr -d ' ')" != "1" ]]; then
  printf 'Expected exactly one packed @fortemi/core tarball\n' >&2
  exit 1
fi
REACT_PACKAGE_SHA256="$(sha256_file "$REACT_PACKAGE_TARBALL")"
EXPECTED_REACT_PACKAGE_SHA256="$(json_value \
  participants.fortemi_react.package_tarball_sha256)"
if [[ "$REACT_PACKAGE_SHA256" != "$EXPECTED_REACT_PACKAGE_SHA256" ]]; then
  printf 'Packed @fortemi/core SHA-256 drift: expected %s, got %s\n' \
    "$EXPECTED_REACT_PACKAGE_SHA256" "$REACT_PACKAGE_SHA256" >&2
  exit 1
fi
stop_authority_server
reset_database

(
  cd "$HOTM_DIR/ui"
  npm ci
  npx playwright install chromium
)
CI=true \
HOTM_LIVE_EXPECT_CLEAN=1 \
HOTM_LIVE_DATABASE_URL="$DATABASE_URL" \
  "$HOTM_DIR/scripts/ci/run-live-asset-receipt.sh"
mkdir -p "$OUTPUT_DIR/hotm"
cp -R "$HOTM_DIR/ui/test-results/live-asset-ci-receipt/." \
  "$OUTPUT_DIR/hotm/"
cp "$HOTM_DIR/ui/test-results/live-asset-ci-receipt/receipt.json" \
  "$OUTPUT_DIR/hotm-receipt.json"
node "$HOTM_DIR/ui/scripts/verify-live-asset-ci-receipt.cjs" \
  "$OUTPUT_DIR/hotm/receipt.json"

python3 "${ORCHESTRATOR_ROOT}/scripts/ci/verify-suite-platform-matrix.py" \
  --manifest "$MANIFEST" platform \
  --platform-id "$PLATFORM_ID" \
  --filesystem "$FILESYSTEM" \
  --authority-receipt "$OUTPUT_DIR/authority-receipt.json" \
  --react-receipt "$OUTPUT_DIR/react-receipt.json" \
  --hotm-receipt "$OUTPUT_DIR/hotm-receipt.json" \
  --output "$OUTPUT_DIR/platform-receipt.json"

printf 'Suite platform contract passed: %s (%s/%s)\n' \
  "$PLATFORM_ID" "$PLATFORM_OS" "$PLATFORM_ARCH"

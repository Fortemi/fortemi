#!/usr/bin/env bash
set -euo pipefail

ORCHESTRATOR_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="${SUITE_PLATFORM_MANIFEST:-${ORCHESTRATOR_ROOT}/contracts/suite-conformance/platform-matrix.json}"
OUTPUT_DIR="${1:-${ORCHESTRATOR_ROOT}/target/suite-platform-contract}"
WORK_DIR="$(mktemp -d)"
AUTHORITY_DIR="${WORK_DIR}/fortemi"
REACT_DIR="${WORK_DIR}/fortemi-react"
HOTM_DIR="${WORK_DIR}/hotm"
DB_CONTAINER=""
API_PID=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$API_PID" ]]; then
    kill "$API_PID" >/dev/null 2>&1 || true
    wait "$API_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DB_CONTAINER" ]]; then
    docker rm -f "$DB_CONTAINER" >/dev/null 2>&1 || true
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
  local image port password ready migration
  command -v docker >/dev/null 2>&1 || {
    echo "docker is required to provision the isolated suite database" >&2
    exit 2
  }
  docker info >/dev/null
  image="fortemi-suite-testdb:${GITHUB_RUN_ID:-local}"
  DB_CONTAINER="fortemi-suite-db-${GITHUB_RUN_ID:-$$}"
  port="$(find_port)"
  password="$(python3 -c 'import secrets; print(secrets.token_hex(24))')"

  docker build -q -f "$ORCHESTRATOR_ROOT/build/Dockerfile.testdb" \
    -t "$image" "$ORCHESTRATOR_ROOT" >/dev/null
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
      -U matric -d matric_suite >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 1
  done
  [[ "$ready" == "true" ]] || {
    docker logs "$DB_CONTAINER" >&2
    return 1
  }
  for migration in "$ORCHESTRATOR_ROOT"/migrations/*.sql; do
    docker exec -i "$DB_CONTAINER" psql \
      -v ON_ERROR_STOP=1 -U matric -d matric_suite <"$migration" >/dev/null
  done

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
  export SUITE_DB_PROVISIONING SUITE_DB_ARCHITECTURE \
    SUITE_DB_VERSION SUITE_DB_EXTENSIONS
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
      awk -F: '/File System Personality/ {gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}')"
    FILESYSTEM="${FILESYSTEM:-unknown-macos-filesystem}"
    ;;
  *)
    echo "Unsupported suite platform: $(uname -s)/$(uname -m)" >&2
    exit 2
    ;;
esac

if [[ -z "${DATABASE_URL:-}" ]]; then
  provision_database
else
  : "${SUITE_DB_ARCHITECTURE:?required for an externally provisioned database}"
  : "${SUITE_DB_VERSION:?required for an externally provisioned database}"
  : "${SUITE_DB_EXTENSIONS:?required for an externally provisioned database}"
  SUITE_DB_PROVISIONING="${SUITE_DB_PROVISIONING:-external}"
  export SUITE_DB_PROVISIONING
fi

AUTHORITY_REPOSITORY="$(json_value authority.repository)"
AUTHORITY_SCHEMA_COMMIT="$(json_value authority.schema_commit)"
AUTHORITY_RUNTIME_COMMIT="$(json_value authority.runtime_commit)"
REACT_REPOSITORY="$(json_value participants.fortemi_react.repository)"
REACT_COMMIT="$(json_value participants.fortemi_react.commit)"
HOTM_REPOSITORY="$(json_value participants.hotm.repository)"
HOTM_COMMIT="$(json_value participants.hotm.commit)"

python3 "${ORCHESTRATOR_ROOT}/scripts/ci/verify-suite-platform-matrix.py" \
  --manifest "$MANIFEST" manifest
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

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

(
  cd "$REACT_DIR"
  corepack enable
  pnpm install --frozen-lockfile
  VITEST_MAX_WORKERS="${VITEST_MAX_WORKERS:-2}" \
    node packages/core/scripts/run-platform-contract.mjs \
      --output "$OUTPUT_DIR/react-receipt.json"
  node packages/core/scripts/verify-platform-contract-receipt.mjs \
    "$OUTPUT_DIR/react-receipt.json"
)

(
  cd "$HOTM_DIR/ui"
  npm ci
  npx playwright install chromium
)
HOTM_LIVE_EXPECT_CLEAN=1 \
HOTM_LIVE_DATABASE_URL="$DATABASE_URL" \
  "$HOTM_DIR/scripts/ci/run-live-asset-receipt.sh"
cp "$HOTM_DIR/ui/test-results/live-asset-ci-receipt/receipt.json" \
  "$OUTPUT_DIR/hotm-receipt.json"
node "$HOTM_DIR/ui/scripts/verify-live-asset-ci-receipt.cjs" \
  "$OUTPUT_DIR/hotm-receipt.json"

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

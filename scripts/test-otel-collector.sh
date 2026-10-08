#!/usr/bin/env bash
# test-otel-collector.sh - OTLP export integration test against a real collector (#1156).
#
# Starts a disposable PostgreSQL (testdb image) and an OpenTelemetry Collector
# (digest-pinned, see ci/digests.txt) whose file exporter writes everything it
# receives to a mounted directory. Runs matric-api built with `--features otel`
# pointed at the collector, issues requests carrying credential/content
# sentinels, then asserts:
#   * server spans named by route template arrived and continue the inbound
#     W3C traceparent;
#   * HTTP, DB-pool and job-queue metrics arrived;
#   * the queued job payload carries the request's traceparent (API -> jobs);
#   * stdout JSON logs carry trace_id;
#   * no sentinel value reached the collector.
#
# Usage:
#   scripts/test-otel-collector.sh                 # both protocols
#   OTEL_IT_PROTOCOLS=grpc scripts/test-otel-collector.sh
#
# Environment:
#   TESTDB_IMAGE       PostgreSQL+pgvector+PostGIS image (default: matric-testdb:local,
#                      built from build/Dockerfile.testdb when missing)
#   OTEL_IT_PROTOCOLS  space-separated subset of "http/protobuf grpc"
#   KEEP_OTEL_IT=1     keep containers and output directory for inspection

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COLLECTOR_IMAGE="otel/opentelemetry-collector:0.161.0@sha256:b6d2b9a85b1029d05b5ad913150c1f014eed4ae99be81a1813ca5ade4a191913"
TESTDB_IMAGE="${TESTDB_IMAGE:-matric-testdb:local}"
PROTOCOLS="${OTEL_IT_PROTOCOLS:-http/protobuf grpc}"
RUN_ID="otelit-$$-$(date +%s)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fortemi-otel-it.XXXXXX")"
DB_CONTAINER="${RUN_ID}-db"
COLLECTOR_CONTAINER="${RUN_ID}-collector"
API_PID=""

TOKEN_SENTINEL="mm_at_OTELITTOKENSENTINEL0123456789"
NOTE_SENTINEL="OTELITNOTESENTINEL confidential body"
QUERY_SENTINEL="OTELITQUERYSENTINEL"
INBOUND_TRACE_ID="0af7651916cd43dd8448eb211c80319c"
INBOUND_TRACEPARENT="00-${INBOUND_TRACE_ID}-b7ad6b7169203331-01"

log() { printf '[otel-it] %s\n' "$*"; }
fail() { printf '[otel-it] FAIL: %s\n' "$*" >&2; exit 1; }

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

cleanup() {
  if [[ -n "${API_PID}" ]] && kill -0 "${API_PID}" 2>/dev/null; then
    kill -TERM "${API_PID}" 2>/dev/null || true
    wait "${API_PID}" 2>/dev/null || true
  fi
  if [[ "${KEEP_OTEL_IT:-0}" == "1" ]]; then
    log "kept containers ${DB_CONTAINER} ${COLLECTOR_CONTAINER} and ${WORK}"
    return
  fi
  docker rm -f "${DB_CONTAINER}" "${COLLECTOR_CONTAINER}" >/dev/null 2>&1 || true
  rm -rf "${WORK}"
}
trap cleanup EXIT

command -v docker >/dev/null || fail "docker is required"
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required"

if ! docker image inspect "${TESTDB_IMAGE}" >/dev/null 2>&1; then
  log "building ${TESTDB_IMAGE} from build/Dockerfile.testdb"
  docker build -q -f "${ROOT}/build/Dockerfile.testdb" -t "${TESTDB_IMAGE}" "${ROOT}" >/dev/null
fi

# --- PostgreSQL --------------------------------------------------------------
DB_PORT="$(free_port)"
DB_PASSWORD="otel-it-$$"
docker run -d --name "${DB_CONTAINER}" -p "127.0.0.1:${DB_PORT}:5432" \
  -e POSTGRES_USER=matric -e POSTGRES_PASSWORD="${DB_PASSWORD}" -e POSTGRES_DB=matric_otel \
  "${TESTDB_IMAGE}" >/dev/null
for _ in $(seq 1 60); do
  docker exec "${DB_CONTAINER}" pg_isready -U matric -d matric_otel >/dev/null 2>&1 && break
  sleep 1
done
docker exec "${DB_CONTAINER}" pg_isready -U matric -d matric_otel >/dev/null || fail "postgres not ready"
sleep 2
docker exec "${DB_CONTAINER}" psql -qU matric -d matric_otel -c "CREATE EXTENSION IF NOT EXISTS vector;" >/dev/null
docker exec "${DB_CONTAINER}" psql -qU matric -d matric_otel -c "CREATE EXTENSION IF NOT EXISTS postgis;" >/dev/null
DATABASE_URL="$(printf '%s%s:%s@127.0.0.1:%s/%s' 'postgres://' matric "${DB_PASSWORD}" "${DB_PORT}" matric_otel)"
export DATABASE_URL

log "bootstrapping schema"
(cd "${ROOT}" && FORTEMI_CI_DISPOSABLE_DATABASE=1 cargo run -q --locked -p matric-db \
  --features migrations --example ci_database_bootstrap) >"${WORK}/bootstrap.log" 2>&1 \
  || { tail -20 "${WORK}/bootstrap.log"; fail "schema bootstrap failed"; }

log "building matric-api --features otel"
(cd "${ROOT}" && cargo build -q --locked -p matric-api --features otel)
API_BIN="${ROOT}/target/debug/matric-api"

# --- Collector ---------------------------------------------------------------
GRPC_PORT="$(free_port)"
HTTP_PORT="$(free_port)"
mkdir -p "${WORK}/out"
# docker cp keeps mode bits, so the non-root collector can write here.
chmod 0777 "${WORK}/out"
cat >"${WORK}/collector.yaml" <<'YAML'
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318
exporters:
  file:
    path: /out/telemetry.jsonl
    flush_interval: 200ms
service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [file]
    metrics:
      receivers: [otlp]
      exporters: [file]
YAML
# No bind mounts: the CI runner's private /tmp is not visible to the Docker
# daemon, so a host path would arrive as an empty directory. Copy the config in
# and the export out instead.
docker create --name "${COLLECTOR_CONTAINER}" \
  -p "127.0.0.1:${GRPC_PORT}:4317" -p "127.0.0.1:${HTTP_PORT}:4318" \
  "${COLLECTOR_IMAGE}" --config /etc/otelcol/config.yaml >/dev/null
docker cp "${WORK}/collector.yaml" "${COLLECTOR_CONTAINER}:/etc/otelcol/config.yaml"
docker cp "${WORK}/out" "${COLLECTOR_CONTAINER}:/out"
docker start "${COLLECTOR_CONTAINER}" >/dev/null
sleep 2
docker ps --filter "name=${COLLECTOR_CONTAINER}" --format '{{.Status}}' | grep -q Up \
  || { docker logs "${COLLECTOR_CONTAINER}" | tail -20; fail "collector did not start"; }

fetch_telemetry() {
  docker cp "${COLLECTOR_CONTAINER}:/out/telemetry.jsonl" "${WORK}/out/telemetry.jsonl" >/dev/null 2>&1 \
    || : >"${WORK}/out/telemetry.jsonl"
}

run_protocol() {
  local protocol="$1" endpoint api_port out
  out="${WORK}/out/telemetry.jsonl"
  if [[ "${protocol}" == "grpc" ]]; then
    endpoint="http://127.0.0.1:${GRPC_PORT}"
  else
    endpoint="http://127.0.0.1:${HTTP_PORT}"
  fi
  api_port="$(free_port)"
  log "protocol=${protocol}: starting API on ${api_port}"
  local before_lines
  fetch_telemetry
  before_lines="$(wc -l <"${out}" 2>/dev/null || echo 0)"

  HOST=127.0.0.1 PORT="${api_port}" REQUIRE_AUTH=false I_UNDERSTAND_NO_AUTH=true \
    WORKER_ENABLED=false MATRIC_ATTACHMENT_SCAN_MODE=disabled LOG_FORMAT=json RUST_LOG=info DISABLE_SUPPORT_MEMORY=1 \
    FILE_STORAGE_PATH="${WORK}/files" \
    OTEL_SERVICE_NAME=fortemi-otel-it \
    OTEL_EXPORTER_OTLP_ENDPOINT="${endpoint}" \
    OTEL_EXPORTER_OTLP_PROTOCOL="${protocol}" \
    OTEL_METRIC_EXPORT_INTERVAL=1000 OTEL_BSP_SCHEDULE_DELAY=200 \
    "${API_BIN}" >"${WORK}/api-${protocol//\//-}.log" 2>&1 &
  API_PID=$!

  for _ in $(seq 1 120); do
    curl -fsS "http://127.0.0.1:${api_port}/health" >/dev/null 2>&1 && break
    kill -0 "${API_PID}" 2>/dev/null || { tail -30 "${WORK}/api-${protocol//\//-}.log"; fail "API exited"; }
    sleep 1
  done
  curl -fsS "http://127.0.0.1:${api_port}/health" >/dev/null || fail "API never became healthy"

  local note_json
  note_json="$(curl -fsS -X POST "http://127.0.0.1:${api_port}/api/v1/notes" \
    -H 'Content-Type: application/json' \
    -H "Cookie: fortemi_session=${TOKEN_SENTINEL}" \
    -H "traceparent: ${INBOUND_TRACEPARENT}" \
    -d "{\"content\": \"${NOTE_SENTINEL}\"}")" || fail "note creation failed"
  local note_id
  note_id="$(printf '%s' "${note_json}" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("id") or d.get("note_id") or "")')"
  curl -fsS "http://127.0.0.1:${api_port}/api/v1/notes?token=${QUERY_SENTINEL}" \
    -H "Cookie: fortemi_session=${TOKEN_SENTINEL}" >/dev/null || fail "note list failed"
  # A rejected bearer credential must not reach telemetry either.
  curl -sS -o /dev/null "http://127.0.0.1:${api_port}/api/v1/notes" \
    -H "Authorization: Bearer ${TOKEN_SENTINEL}" || true

  # Let the periodic reader and the 15s queue sampler run at least once.
  sleep 17
  kill -TERM "${API_PID}"
  wait "${API_PID}" 2>/dev/null || true
  API_PID=""
  sleep 1

  [[ -n "${note_id}" ]] || fail "note id missing from create response"
  local job_trace
  job_trace="$(docker exec "${DB_CONTAINER}" psql -qtAU matric -d matric_otel -c \
    "SELECT count(*) FROM job_queue WHERE note_id='${note_id}' AND payload->>'_fortemi_traceparent' LIKE '00-${INBOUND_TRACE_ID}-%'")"
  [[ "${job_trace// /}" -ge 1 ]] || fail "queued job payloads did not carry the request traceparent"

  grep -q "\"trace_id\":\"${INBOUND_TRACE_ID}\"" "${WORK}/api-${protocol//\//-}.log" \
    || fail "stdout JSON logs lack trace_id correlation"

  fetch_telemetry
  python3 - "${out}" "${before_lines}" "${INBOUND_TRACE_ID}" \
    "${TOKEN_SENTINEL}" "${NOTE_SENTINEL}" "${QUERY_SENTINEL}" <<'PY'
import json, sys
path, skip, trace_id, *sentinels = sys.argv[1:]
lines = open(path, encoding="utf-8").read().splitlines()[int(skip):]
raw = "\n".join(lines)
for s in sentinels:
    if s in raw or s.split()[0] in raw:
        sys.exit(f"sentinel leaked to collector: {s.split()[0]}")
spans, metrics, services = [], set(), set()
for line in lines:
    doc = json.loads(line)
    for rs in doc.get("resourceSpans", []):
        for a in rs["resource"]["attributes"]:
            if a["key"] == "service.name":
                services.add(a["value"]["stringValue"])
        for ss in rs.get("scopeSpans", []):
            spans.extend(ss.get("spans", []))
    for rm in doc.get("resourceMetrics", []):
        for sm in rm.get("scopeMetrics", []):
            metrics.update(m["name"] for m in sm.get("metrics", []))
names = {s["name"] for s in spans}
assert "fortemi-otel-it" in services, services
for expected in ("GET /health", "POST /api/v1/notes", "GET /api/v1/notes"):
    assert expected in names, (expected, sorted(names)[:20])
post = [s for s in spans if s["name"] == "POST /api/v1/notes"]
assert any(s["traceId"] == trace_id for s in post), "inbound traceparent not continued"
for expected in ("http.server.request.duration", "http.server.active_requests",
                 "fortemi.db.pool.connections", "fortemi.db.pool.max_connections",
                 "fortemi.job.queue.depth", "fortemi.job.queue.oldest_pending_age"):
    assert expected in metrics, (expected, sorted(metrics))
print(f"[otel-it] ok: {len(spans)} spans, metrics={sorted(metrics)}")
PY
}

for protocol in ${PROTOCOLS}; do
  run_protocol "${protocol}"
done
log "PASS (${PROTOCOLS})"

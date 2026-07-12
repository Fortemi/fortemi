#!/usr/bin/env bash
# smoke-intel-vllm.sh — runtime smoke test for the Intel Arc / host-vLLM profile
#
# Proves the split-provider routing of docker-compose.intel.yml (#1044) without
# any Intel hardware: a stub OpenAI-compatible server stands in for host vLLM,
# and the test asserts that
#   1. Fortemi's chat route calls the configured OPENAI_BASE_URL with the
#      configured OPENAI_GEN_MODEL (vLLM --served-model-name semantics: the
#      stub rejects any other model name, like vLLM does)
#   2. the embedding route stays on the configured embedding provider — the
#      stub must never receive a /v1/embeddings call
#   3. /api/v1/inference/config reports generation=openai and
#      embedding_backend=ollama so operators can verify routing
#
# This starts the full Docker bundle and is intended for release validation on
# a workstation/CI host with the bundle images available — not the default CI
# path (see scripts/validate-intel-overlay.sh for the render-only CI gate).
#
# Usage:
#   ./scripts/smoke-intel-vllm.sh
#
# Environment:
#   FORTEMI_REGISTRY / FORTEMI_TAG   image source (compose defaults otherwise)
#   SMOKE_API_PORT      host port for the API        (default 13000)
#   SMOKE_MCP_PORT      host port for the MCP server (default 13001)
#   SMOKE_VLLM_PORT     host port for the vLLM stub  (default 18000)
#   SMOKE_MODEL         served model name            (default qwen3.5:9b)
#   SMOKE_TIMEOUT_SECS  health-wait timeout          (default 600)
#   SMOKE_KEEP=1        skip teardown for debugging

set -euo pipefail

cd "$(dirname "$0")/.."

API_PORT="${SMOKE_API_PORT:-13000}"
MCP_PORT="${SMOKE_MCP_PORT:-13001}"
STUB_PORT="${SMOKE_VLLM_PORT:-18000}"
MODEL="${SMOKE_MODEL:-qwen3.5:9b}"
TIMEOUT="${SMOKE_TIMEOUT_SECS:-600}"
PROJECT="fortemi-intel-smoke"
WORKDIR="$(mktemp -d)"
STUB_LOG="${WORKDIR}/stub-requests.jsonl"
STUB_PID=""

compose() {
    docker compose -p "$PROJECT" \
        --env-file /dev/null \
        -f docker-compose.bundle.yml \
        -f docker-compose.intel.yml "$@"
}

cleanup() {
    local rc=$?
    if [ "${SMOKE_KEEP:-0}" = "1" ]; then
        echo "SMOKE_KEEP=1 — leaving stack up (project ${PROJECT}, stub pid ${STUB_PID})"
        return $rc
    fi
    [ -n "$STUB_PID" ] && kill "$STUB_PID" 2>/dev/null || true
    compose down -v --remove-orphans >/dev/null 2>&1 || true
    rm -rf "$WORKDIR"
    return $rc
}
trap cleanup EXIT

# ── 1. Stub OpenAI-compatible server (stands in for host vLLM) ──────────────
cat >"${WORKDIR}/stub.py" <<'EOF'
import json, os, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MODEL = os.environ["STUB_MODEL"]
LOG = os.environ["STUB_LOG"]

def record(entry):
    with open(LOG, "a") as f:
        f.write(json.dumps(entry) + "\n")

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        record({"method": "GET", "path": self.path})
        if self.path == "/v1/models":
            self._json(200, {"object": "list",
                             "data": [{"id": MODEL, "object": "model"}]})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        payload = json.loads(self.rfile.read(length) or b"{}")
        record({"method": "POST", "path": self.path,
                "model": payload.get("model")})
        if self.path == "/v1/chat/completions":
            # vLLM semantics: only the served model name is accepted.
            if payload.get("model") != MODEL:
                self._json(404, {"error": {"message":
                    f"The model `{payload.get('model')}` does not exist.",
                    "type": "NotFoundError"}})
                return
            self._json(200, {
                "id": "cmpl-smoke", "object": "chat.completion",
                "model": MODEL,
                "choices": [{"index": 0, "finish_reason": "stop",
                             "message": {"role": "assistant",
                                         "content": "smoke-ok"}}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1,
                          "total_tokens": 2}})
        elif self.path == "/v1/embeddings":
            # vLLM generation deployments do not serve Fortemi's embeddings.
            self._json(404, {"error": {"message":
                "This vLLM deployment does not serve embeddings.",
                "type": "NotFoundError"}})
        else:
            self._json(404, {"error": "not found"})

ThreadingHTTPServer(("0.0.0.0", int(sys.argv[1])), Handler).serve_forever()
EOF

: >"$STUB_LOG"
STUB_MODEL="$MODEL" STUB_LOG="$STUB_LOG" \
    python3 "${WORKDIR}/stub.py" "$STUB_PORT" &
STUB_PID=$!
sleep 1
curl -fsS "http://127.0.0.1:${STUB_PORT}/v1/models" >/dev/null \
    || { echo "ERROR: stub server failed to start"; exit 1; }
echo "Stub vLLM endpoint up on :${STUB_PORT} serving model '${MODEL}'"

# ── 2. Start the bundle with the Intel overlay pointed at the stub ──────────
export API_HOST_PORT="$API_PORT" MCP_HOST_PORT="$MCP_PORT"
export OPENAI_BASE_URL="http://host.docker.internal:${STUB_PORT}/v1"
export OPENAI_GEN_MODEL="$MODEL"
# Smoke-only relaxations: anonymous API + plain-http issuer.
export REQUIRE_AUTH=false I_UNDERSTAND_NO_AUTH=true
export FORTEMI_ALLOW_LOCAL_ISSUER=true
export ISSUER_URL="http://localhost:${API_PORT}"

echo "Starting bundle (project ${PROJECT}, API :${API_PORT})..."
compose up -d --scale gliner=0 fortemi

# ── 3. Wait for health ───────────────────────────────────────────────────────
echo "Waiting up to ${TIMEOUT}s for /health..."
deadline=$((SECONDS + TIMEOUT))
until curl -fsS "http://127.0.0.1:${API_PORT}/health" >/dev/null 2>&1; do
    if [ $SECONDS -ge $deadline ]; then
        echo "ERROR: API did not become healthy within ${TIMEOUT}s"
        compose logs --tail 50 fortemi || true
        exit 1
    fi
    sleep 5
done
echo "API healthy."

# ── 4. Assert routing configuration ──────────────────────────────────────────
curl -fsS "http://127.0.0.1:${API_PORT}/api/v1/inference/config" \
    >"${WORKDIR}/config.json"
python3 - "${WORKDIR}/config.json" <<'EOF'
import json, sys
cfg = json.load(open(sys.argv[1]))
text = json.dumps(cfg)

def sourced(v):
    return v.get("value") if isinstance(v, dict) and "value" in v else v

failures = []
default_backend = sourced(cfg.get("default_backend") or cfg.get("default"))
if default_backend != "openai":
    failures.append(f"default backend is {default_backend!r}, expected 'openai'")
emb = sourced(cfg.get("embedding_backend"))
if emb != "ollama":
    failures.append(f"embedding_backend is {emb!r}, expected 'ollama'")
if "host.docker.internal" not in text:
    failures.append("configured OpenAI base_url does not point at the host stub")
if failures:
    print("Routing config assertions FAILED:")
    [print("  -", f) for f in failures]
    sys.exit(1)
print("Routing config OK: generation=openai (host stub), embeddings=ollama")
EOF

# ── 5. Chat request must reach the stub with the served model name ──────────
chat_response=$(curl -fsS -X POST "http://127.0.0.1:${API_PORT}/api/v1/chat" \
    -H "Content-Type: application/json" \
    -d '{"input": "intel smoke test"}')
echo "$chat_response" | grep -q "smoke-ok" \
    || { echo "ERROR: chat response did not come from the stub: ${chat_response}"; exit 1; }

python3 - "$STUB_LOG" "$MODEL" <<'EOF'
import json, sys
entries = [json.loads(l) for l in open(sys.argv[1])]
model = sys.argv[2]
chats = [e for e in entries if e["path"] == "/v1/chat/completions"]
embeds = [e for e in entries if e["path"] == "/v1/embeddings"]
failures = []
if not chats:
    failures.append("stub never received /v1/chat/completions")
elif any(e.get("model") != model for e in chats):
    failures.append(f"chat used wrong model: {[e.get('model') for e in chats]}, expected {model!r}")
if embeds:
    failures.append(f"stub received {len(embeds)} /v1/embeddings call(s) — embeddings leaked to vLLM")
if failures:
    print("Stub traffic assertions FAILED:")
    [print("  -", f) for f in failures]
    sys.exit(1)
print(f"Stub traffic OK: {len(chats)} chat call(s) with model '{model}', 0 embedding calls")
EOF

echo ""
echo "✅ Intel host-vLLM profile smoke test passed"

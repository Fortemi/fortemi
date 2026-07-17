#!/usr/bin/env bash
# validate-intel-overlay.sh — render-time validation of docker-compose.intel.yml
#
# Asserts that the Intel Arc / host-vLLM overlay (#1043):
#   1. renders cleanly on top of docker-compose.bundle.yml
#   2. clears the fortemi service's NVIDIA device reservation (via `!reset`)
#   3. sets the split-provider environment (generation → OpenAI-compatible
#      host vLLM, embeddings → Ollama) and NVIDIA env blanks
#
# This is compose-render validation only — no Intel hardware, no containers
# started. Runs in CI on every push and locally via:
#   ./scripts/validate-intel-overlay.sh
#
# Requires Docker Compose v2.17.0+ (the overlay's `!reset` YAML tag).

set -euo pipefail

cd "$(dirname "$0")/.."

# Keep the copy/paste host service and guide aligned with the security contract
# from #1045. These checks run before Compose so failures remain actionable even
# on hosts where the Docker plugin is unavailable.
service_unit=deploy/vllm-intel-xpu.service.example
deployment_guide=docs/content/intel-arc-vllm.md

required_unit_patterns=(
    'EnvironmentFile=%h/.config/fortemi/vllm.env'
    'NoNewPrivileges=true'
    'PrivateTmp=true'
    'ProtectSystem=full'
    'CapabilityBoundingSet='
)
for pattern in "${required_unit_patterns[@]}"; do
    if ! grep -Fq -- "$pattern" "$service_unit"; then
        echo "ERROR: $service_unit is missing hardening contract: $pattern" >&2
        exit 1
    fi
done
if grep -Fq -- '--api-key ${VLLM_API_KEY}' "$service_unit"; then
    echo "ERROR: do not expose VLLM_API_KEY through the process command line" >&2
    exit 1
fi

if ! grep -Fq -- 'OPENAI_API_KEY=${OPENAI_API_KEY:?' docker-compose.intel.yml; then
    echo "ERROR: docker-compose.intel.yml must require an explicit vLLM API key" >&2
    exit 1
fi

required_guide_patterns=(
    'you must restrict TCP port 8000 with the host firewall'
    'only a Fortemi-side placeholder'
    'pinned, locally audited snapshot'
    'Network exposure check'
    'this must time out or be refused'
)
for pattern in "${required_guide_patterns[@]}"; do
    if ! grep -Fq -- "$pattern" "$deployment_guide"; then
        echo "ERROR: $deployment_guide is missing security guidance: $pattern" >&2
        exit 1
    fi
done

echo "Intel host-vLLM security guidance validation passed"

if ! docker compose version >/dev/null 2>&1; then
    echo "ERROR: 'docker compose' plugin not available. The Intel overlay" >&2
    echo "requires Docker Compose v2.17.0+ (see docs/content/intel-arc-vllm.md)." >&2
    exit 1
fi

compose_version=$(docker compose version --short 2>/dev/null || echo "unknown")
echo "Docker Compose version: ${compose_version}"

# Render with a pinned-empty env file so a developer's local .env cannot
# change the asserted defaults. CI has no .env; locally this keeps the
# validation deterministic.
rendered=$(mktemp)
missing_key_error=$(mktemp)
trap 'rm -f "$rendered" "$missing_key_error"' EXIT

if docker compose \
    --env-file /dev/null \
    -f docker-compose.bundle.yml \
    -f docker-compose.intel.yml \
    config --format json >/dev/null 2>"$missing_key_error"; then
    echo "ERROR: Intel overlay rendered without the required OPENAI_API_KEY" >&2
    exit 1
fi
if ! grep -Fq -- "OPENAI_API_KEY" "$missing_key_error"; then
    echo "ERROR: keyless render failed for an unexpected reason:" >&2
    cat "$missing_key_error" >&2
    exit 1
fi
echo "Intel overlay rejects a missing vLLM API key"

if ! OPENAI_API_KEY=validation-only-vllm-key docker compose \
    --env-file /dev/null \
    -f docker-compose.bundle.yml \
    -f docker-compose.intel.yml \
    config --format json >"$rendered" 2>/tmp/intel-overlay-render.err; then
    echo "ERROR: compose render failed — likely a Compose release older than" >&2
    echo "v2.17.0 (no '!reset' support) or a syntax error in the overlay:" >&2
    cat /tmp/intel-overlay-render.err >&2
    exit 1
fi

python3 - "$rendered" <<'EOF'
import json, sys

with open(sys.argv[1]) as f:
    rendered = json.load(f)

svc = rendered["services"]["fortemi"]
failures = []

# 1. NVIDIA device reservation must be cleared by `!reset`.
devices = (
    svc.get("deploy", {})
    .get("resources", {})
    .get("reservations", {})
    .get("devices")
)
if devices:
    failures.append(f"deploy.resources.reservations.devices not cleared: {devices!r}")

# 2. Split-provider environment: generation via host vLLM, embeddings on
#    Ollama, NVIDIA env blanked, Open3D renderer disabled.
env = svc["environment"]
expected = {
    "MATRIC_INFERENCE_DEFAULT": "openai",
    "OPENAI_BASE_URL": "http://host.docker.internal:8000/v1",
    "OPENAI_API_KEY": "validation-only-vllm-key",
    "MATRIC_EMBEDDING_PROVIDER": "ollama",
    "RENDERER_ENABLED": "false",
    "OPEN3D_ENABLED": "false",
    "NVIDIA_VISIBLE_DEVICES": "",
    "NVIDIA_DRIVER_CAPABILITIES": "",
}
for key, want in expected.items():
    got = env.get(key)
    if got != want:
        failures.append(f"environment[{key}] = {got!r}, expected {want!r}")

if failures:
    print("Intel overlay validation FAILED:")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)

print("Intel overlay validation passed:")
print("  - NVIDIA device reservation cleared")
for key in expected:
    print(f"  - {key}={env[key]!r}")
EOF

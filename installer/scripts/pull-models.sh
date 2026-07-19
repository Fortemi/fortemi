#!/usr/bin/env bash
set -euo pipefail

# Pull Ollama models on the host (Ollama runs on the host, not in the Docker bundle)
# Params: INSTALL_DIR, OLLAMA_GEN_MODEL, OLLAMA_EMBED_MODEL

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"
OLLAMA_GEN_MODEL="${OLLAMA_GEN_MODEL:-qwen3.5:9b}"
OLLAMA_EMBED_MODEL="${OLLAMA_EMBED_MODEL:-nomic-embed-text}"

# Check if Ollama is running on the host
if ! curl -sf http://localhost:11434/api/version &>/dev/null; then
    if command -v ollama &>/dev/null; then
        echo "Ollama is installed but not running. Starting..."
        ollama serve &>/dev/null &
        sleep 3
        if ! curl -sf http://localhost:11434/api/version &>/dev/null; then
            echo "WARNING: Could not start Ollama — skipping model pull."
            echo "Start Ollama manually and pull models:"
            echo "  ollama pull ${OLLAMA_GEN_MODEL}"
            echo "  ollama pull ${OLLAMA_EMBED_MODEL}"
            exit 0
        fi
    else
        echo "WARNING: Ollama not found — skipping model pull."
        echo "Install Ollama from https://ollama.com and then pull models:"
        echo "  ollama pull ${OLLAMA_GEN_MODEL}"
        echo "  ollama pull ${OLLAMA_EMBED_MODEL}"
        exit 0
    fi
fi

echo "Pulling ${OLLAMA_GEN_MODEL} (this may take several minutes)..."
ollama pull "${OLLAMA_GEN_MODEL}"

# Also pull the embedding model if different
if [ "${OLLAMA_EMBED_MODEL}" != "${OLLAMA_GEN_MODEL}" ]; then
    echo "Pulling embedding model ${OLLAMA_EMBED_MODEL}..."
    ollama pull "${OLLAMA_EMBED_MODEL}"
fi

echo "Models ready."
ollama list

# Container-reachability check (Docker bundle): on Linux with the systemd
# Ollama service, the default OLLAMA_HOST=127.0.0.1 means containers can't
# reach the daemon through the host gateway. Probe and warn so operators
# don't end up with a healthy bundle reporting inference.available=false.
if [ "$(uname -s)" = "Linux" ] \
    && command -v systemctl &>/dev/null \
    && systemctl is-active --quiet ollama 2>/dev/null; then
    HOST_BIND=$(ss -tlnp 2>/dev/null | awk '/:11434 /{print $4}' | head -1)
    if echo "${HOST_BIND}" | grep -Eq '^(127\.|\[?::1\]?:)'; then
        echo ""
        echo "WARNING: Ollama is bound to ${HOST_BIND} — not reachable from Docker containers."
        echo "         The bundle will start but inference.available will be false."
        echo ""
        echo "No host settings were changed."
        echo "Preferred least-exposure fix for the headless bundle:"
        echo "  HOST_GATEWAY_IP=\"\$(docker network inspect bridge \\"
        echo "    --format '{{(index .IPAM.Config 0).Gateway}}')\""
        echo "  test -n \"\${HOST_GATEWAY_IP}\""
        echo "  printf 'Docker host gateway: %s\\n' \"\${HOST_GATEWAY_IP}\""
        echo "  # Review the printed address before changing the listener."
        echo "  sudo mkdir -p /etc/systemd/system/ollama.service.d"
        echo "  printf '[Service]\\nEnvironment=\"OLLAMA_HOST=%s:11434\"\\n' \\"
        echo "    \"\${HOST_GATEWAY_IP}\" |"
        echo "    sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null"
        echo "  sudo systemctl daemon-reload"
        echo "  sudo systemctl restart ollama"
        echo ""
        echo "This listens only on Docker's configured host-gateway address."
        echo "For rootless/custom Docker or shared Ollama, follow:"
        echo "  docs/content/ollama-connectivity.md"
        echo "On macOS the Ollama desktop app handles this automatically."
    fi
fi

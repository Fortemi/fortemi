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
    if echo "${HOST_BIND}" | grep -q '^127\.'; then
        echo ""
        echo "WARNING: Ollama is bound to ${HOST_BIND} — not reachable from Docker containers."
        echo "         The bundle will start but inference.available will be false."
        echo ""
        echo "Fix (run as a user with sudo):"
        echo "  sudo mkdir -p /etc/systemd/system/ollama.service.d"
        echo "  sudo tee /etc/systemd/system/ollama.service.d/override.conf <<'EOF'"
        echo "  [Service]"
        echo "  Environment=\"OLLAMA_HOST=0.0.0.0\""
        echo "  EOF"
        echo "  sudo systemctl daemon-reload"
        echo "  sudo systemctl restart ollama"
        echo ""
        echo "On macOS the Ollama desktop app handles this automatically."
    fi
fi

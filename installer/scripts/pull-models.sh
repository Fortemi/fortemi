#!/usr/bin/env bash
set -euo pipefail

# Pull Ollama models after deployment
# Params: INSTALL_DIR, OLLAMA_GEN_MODEL

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"
OLLAMA_GEN_MODEL="${OLLAMA_GEN_MODEL:-qwen3.5:9b}"

cd "${INSTALL_DIR}"

# Find the ollama container
OLLAMA_CONTAINER=$(docker compose -f docker-compose.bundle.yml ps -q ollama 2>/dev/null || true)

if [ -z "${OLLAMA_CONTAINER}" ]; then
    echo "WARNING: Ollama container not found — skipping model pull."
    echo "Models will be downloaded on first API request."
    exit 0
fi

# Wait for Ollama to be ready
echo "Waiting for Ollama to be ready..."
RETRIES=15
until docker exec "${OLLAMA_CONTAINER}" ollama list &>/dev/null; do
    RETRIES=$((RETRIES - 1))
    if [ "${RETRIES}" -le 0 ]; then
        echo "WARNING: Ollama not responding — skipping model pull."
        echo "Models will be downloaded on first API request."
        exit 0
    fi
    sleep 2
done

echo "Pulling ${OLLAMA_GEN_MODEL} (this may take several minutes)..."
docker exec "${OLLAMA_CONTAINER}" ollama pull "${OLLAMA_GEN_MODEL}"

# Also pull the embedding model if different
EMBED_MODEL="${OLLAMA_EMBED_MODEL:-nomic-embed-text}"
if [ "${EMBED_MODEL}" != "${OLLAMA_GEN_MODEL}" ]; then
    echo "Pulling embedding model ${EMBED_MODEL}..."
    docker exec "${OLLAMA_CONTAINER}" ollama pull "${EMBED_MODEL}"
fi

echo "Models ready."
docker exec "${OLLAMA_CONTAINER}" ollama list

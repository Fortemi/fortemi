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

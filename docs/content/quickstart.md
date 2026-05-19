# Quickstart: Deploy Fortemi

> ## 🖥️ Looking for a desktop app?
>
> **You probably want [HotM](https://git.integrolabs.net/Fortemi/HotM/releases/latest), not this guide.** HotM is the first-party desktop application — a `.deb` / `.msi` / `.dmg` / `.AppImage` that bundles this Fortemi backend as a sidecar. One prereq script gets you running with no Docker, no Postgres setup, no backend ops:
>
> - **Linux:** [`bash scripts/setup-linux.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-linux.sh) → [install guide](https://git.integrolabs.net/Fortemi/HotM/src/branch/main/docs/installation/desktop-linux.md)
> - **macOS:** [`bash scripts/setup-macos.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-macos.sh) → [install guide](https://git.integrolabs.net/Fortemi/HotM/src/branch/main/docs/installation/desktop-macos.md)
>
> Both scripts install Postgres 18 + pgvector + PostGIS, create the `matric` role + database, and (unless `--no-ollama`) install Ollama with `nomic-embed-text` + `qwen3.5:9b`.
>
> **This Quickstart is for the headless server path** — Docker bundle for agents over MCP, custom UIs, multi-user, air-gapped backends. If that's what you want, keep reading.

> ## 💻 Building features on a dev box?
>
> **You probably want the [Local Workstation stack](https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/QUICKSTART.md), not this guide.** The workstation gives you Fortemi + HotM UI + Ollama in containers with a single command:
>
> ```bash
> ./workstation up                   # full stack with UI at http://localhost:4180
> ./workstation up --backend-only    # just ollama + postgres + matric-api
> ```
>
> Use a different LLM than Ollama:
>
> ```bash
> ./workstation configure-llm        # interactive picker: ollama / vllm / openai / openrouter / llamacpp
> ```
>
> Walkthrough (no Docker experience required): **[QUICKSTART.md](https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/QUICKSTART.md)**
> Operations reference: **[WORKSTATION-SETUP.md](https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/WORKSTATION-SETUP.md)**
>
> Pick the headless-server Quickstart below if you're deploying to a real server. Pick the workstation if you're hacking on Fortemi or HotM locally.

Deploy a fully functional Fortemi instance using published container images from GHCR. This guide covers three progressive tiers — each self-contained, each building on the previous:

1. **Core** — Full-text search, tagging, graph linking (no AI, no GPU)
2. **+AI** — Add Ollama for semantic search, auto-linking, and NLP extraction
3. **+Full Stack** — Add extraction sidecars (GLiNER NER, Whisper transcription)

Time estimate: **2 minutes** for Core (one command), 15-20 minutes through Full Stack with AI.

This guide is designed for both humans and AI agents. Agent-parseable markers (`<!-- agent:step -->`) annotate each step with check commands, expected output, and failure actions.

---

## Prerequisites

### Required

<!-- agent:step id="check-docker" required="true" -->

**Docker Engine 24.0+ with Compose v2**

```bash
docker --version
# Expected: Docker version 24.x.x or higher

docker compose version
# Expected: Docker Compose version v2.x.x or higher
```

On failure: Install Docker Engine from https://docs.docker.com/engine/install/

<!-- agent:step id="check-curl" required="true" -->

**curl**

```bash
curl --version
# Expected: curl 7.x or 8.x
```

On failure: Install via your package manager (`apt install curl`, `brew install curl`, etc.)

<!-- agent:step id="check-ports" required="true" -->

**Ports 3000 and 3001 available**

```bash
# Linux/macOS
ss -tlnp | grep -E ':300[01]\b' || echo "Ports available"

# Alternative
curl -sf http://localhost:3000 > /dev/null 2>&1 && echo "FAIL: Port 3000 in use" || echo "OK: Port 3000 free"
curl -sf http://localhost:3001 > /dev/null 2>&1 && echo "FAIL: Port 3001 in use" || echo "OK: Port 3001 free"
```

On failure: Stop the service occupying the port, or change the port mapping in docker-compose.bundle.yml.

<!-- agent:step id="check-disk" required="true" -->

**10 GB free disk space** (minimum; 20 GB+ recommended with AI models)

```bash
df -h / | awk 'NR==2 {print $4}'
# Expected: 10G or more
```

On failure: Free disk space before proceeding.

<!-- agent:step id="check-ram" required="true" -->

**4 GB RAM** (minimum; 8 GB+ recommended)

```bash
# Linux
free -g | awk '/^Mem:/ {print $2 "GB total"}'

# macOS
sysctl -n hw.memsize | awk '{print int($1/1024/1024/1024) "GB total"}'
```

On failure: Fortemi Core runs in 4 GB. AI features need 8 GB+. Upgrade RAM or use a larger machine.

### Optional (for AI features)

<!-- agent:step id="detect-gpu" required="false" -->

**NVIDIA GPU detection**

```bash
nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null \
  && echo "GPU: detected" \
  || echo "GPU: not detected (CPU-only mode)"
```

<!-- agent:step id="detect-nvidia-toolkit" required="false" depends="detect-gpu" -->

**NVIDIA Container Toolkit** (only if GPU detected)

```bash
docker info 2>/dev/null | grep -i nvidia \
  && echo "NVIDIA Container Toolkit: installed" \
  || echo "NVIDIA Container Toolkit: not installed"
```

On failure (with GPU): Install from https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html

<!-- agent:step id="detect-ollama" required="false" -->

**Ollama** (for AI features in Tier 2+)

```bash
ollama --version 2>/dev/null \
  && echo "Ollama: installed" \
  || echo "Ollama: not installed"
```

---

## Step 1: Download and Start

<!-- agent:step id="download-config" required="true" depends="check-docker,check-curl,check-ports,check-disk,check-ram" -->

Download the compose file and start Fortemi:

```bash
mkdir -p fortemi && cd fortemi

# Download compose file
curl -fsSL -o docker-compose.bundle.yml \
  https://raw.githubusercontent.com/fortemi/fortemi/main/docker-compose.bundle.yml
```

**Alternative** — clone the repository:

```bash
git clone https://github.com/fortemi/fortemi.git
cd fortemi
```

Verify the compose file is valid:

```bash
docker compose -f docker-compose.bundle.yml config --quiet \
  && echo "OK: compose file valid" \
  || echo "FAIL: compose file invalid"
```

On failure: Re-download the file. If using a proxy, ensure it's not modifying the download.

> **No `.env` file required for basic usage.** The compose file defaults to pulling published images from GHCR (`ghcr.io/fortemi/fortemi`). All sidecars (GLiNER, Whisper, pyannote) are optional and won't block startup.

### Optional: Configure Environment

Create a `.env` file only if you need to customize settings:

```bash
# Download the template (optional)
curl -fsSL -o .env.example \
  https://raw.githubusercontent.com/fortemi/fortemi/main/.env.example
cp .env.example .env
```

Common customizations:

| Setting | When to configure |
|---------|------------------|
| `ISSUER_URL=https://your-domain.com` | Deploying behind a reverse proxy or domain |
| `OLLAMA_VISION_MODEL=` | CPU-only host (disables vision model) |
| `FORTEMI_REGISTRY=git.integrolabs.net` | Internal development (Gitea registry) |

### GPU or CPU-only?

<!-- agent:step id="configure-gpu" required="false" depends="detect-gpu,download-config" -->

**GPU detected** — no changes needed. The default compose file enables GPU acceleration for Whisper and pyannote sidecars via `deploy.resources.reservations.devices`.

**No GPU detected** — that's fine. Sidecars requiring GPU (Whisper, pyannote) are marked `required: false` and won't block startup. Fortemi Core and GLiNER work on CPU. For CPU transcription, use:

```bash
docker compose -f docker-compose.bundle.yml --profile whisper-cpu up -d
```

---

## Step 2: Start Core Services

<!-- agent:step id="start-core" required="true" depends="download-config" -->

Start the core stack (Fortemi + Redis). Sidecars (Whisper, GLiNER, pyannote) start automatically but are non-blocking — Fortemi works without them.

```bash
docker compose -f docker-compose.bundle.yml up -d matric redis
```

This pulls ~1 GB of images on first run and starts:
- PostgreSQL 18 with pgvector + PostGIS (bundled in the matric container)
- The API server on port 3000
- The MCP server on port 3001
- Redis for search query caching

### Wait for healthy

<!-- agent:step id="verify-health" required="true" depends="start-core" -->

Poll until the health check passes (allows up to 90 seconds for first-time initialization):

```bash
# Poll health endpoint
for i in $(seq 1 18); do
  status=$(curl -sf http://localhost:3000/health | grep -o '"status":"[^"]*"' | head -1)
  if echo "$status" | grep -q "healthy"; then
    echo "OK: Fortemi is healthy"
    break
  fi
  echo "Waiting... ($i/18)"
  sleep 5
done
```

Or just wait and check once:

```bash
sleep 30 && curl -s http://localhost:3000/health
```

Expected output includes:

```json
{
  "status": "healthy",
  "database": "connected"
}
```

On failure:
- Check logs: `docker compose -f docker-compose.bundle.yml logs matric`
- Check container status: `docker compose -f docker-compose.bundle.yml ps`
- Verify port isn't in use: `ss -tlnp | grep :3000`

### Verify endpoints

```bash
# API docs (Swagger UI)
curl -sf http://localhost:3000/docs > /dev/null \
  && echo "OK: API docs available at http://localhost:3000/docs" \
  || echo "FAIL: API docs not reachable"

# MCP endpoint
curl -sf http://localhost:3001/ > /dev/null 2>&1; echo "MCP server on port 3001"
```

At this point, **Tier 1 (Core) is complete**. You have full-text search, tagging, collections, version history, graph linking, and the MCP server. No AI/GPU required.

---

## Step 3: Add AI Features (Optional)

This section adds Ollama for semantic search, embeddings, auto-linking, and NLP extraction.

### Install Ollama

<!-- agent:step id="install-ollama" required="false" depends="verify-health" -->

Skip if Ollama is already installed (check: `ollama --version`).

```bash
curl -fsSL https://ollama.ai/install.sh | sh
```

Verify:

```bash
ollama --version
# Expected: ollama version 0.x.x

# Ensure the service is running
ollama list > /dev/null 2>&1 \
  && echo "OK: Ollama running" \
  || echo "Starting Ollama..." && ollama serve &
```

### Detect Hardware and Select Models

<!-- agent:step id="select-models" required="false" depends="install-ollama,detect-gpu" -->

Detect available VRAM and RAM to select appropriate models:

```bash
# Detect VRAM (GB)
VRAM=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1)
if [ -n "$VRAM" ]; then
  VRAM_GB=$((VRAM / 1024))
  echo "GPU VRAM: ${VRAM_GB}GB"
else
  VRAM_GB=0
  echo "GPU VRAM: none"
fi

# Detect RAM (GB)
if [ -f /proc/meminfo ]; then
  RAM_GB=$(awk '/MemTotal/ {print int($2/1024/1024)}' /proc/meminfo)
else
  RAM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{print int($1/1024/1024/1024)}')
fi
echo "System RAM: ${RAM_GB}GB"
```

Use this table to select models based on your hardware:

| VRAM | RAM | Generation | Fast | Vision | Embedding |
|---------|-------|-------------------|------------------|-----------------|------------------|
| 24 GB+  | any   | `qwen3.5:27b`    | `qwen3.5:9b`    | `qwen3.5:9b`   | `nomic-embed-text` |
| 12-23 GB| any   | `qwen2.5:7b`     | `qwen3.5:9b`    | _(disable)_     | `nomic-embed-text` |
| 6-11 GB | any   | `llama3.2:3b`    | _(disable)_      | _(disable)_     | `nomic-embed-text` |
| none    | 32 GB+| `qwen2.5:7b`     | `qwen3.5:9b`    | _(disable)_     | `nomic-embed-text` |
| none    | 16 GB+| `qwen2.5:7b`     | `llama3.2:3b`   | _(disable)_     | `nomic-embed-text` |
| none    | 8-15 GB| `llama3.2:3b`   | _(disable)_      | _(disable)_     | `nomic-embed-text` |

_(disable)_ means set the corresponding env var to empty string. See [Hardware Planning](./hardware-planning.md) for full quality benchmarks and cost analysis.

### Pull Models

<!-- agent:step id="pull-models" required="false" depends="select-models" -->

Pull the models you selected. At minimum, pull the embedding model:

```bash
# Always needed for semantic search
ollama pull nomic-embed-text

# Pull your selected generation model (example for 12-23GB VRAM)
ollama pull qwen2.5:7b

# Pull fast model if your hardware supports it (also serves as vision model — natively multimodal)
ollama pull qwen3.5:9b

# Pull vision model: qwen3.5:9b is natively multimodal; pull separately only if using a dedicated vision model
# ollama pull qwen3.5:9b  # already pulled above for 24GB+ VRAM tier
```

Verify models are available:

```bash
ollama list
# Expected: nomic-embed-text and your selected models listed
```

### Update .env with Model Selections

<!-- agent:step id="configure-models" required="false" depends="pull-models" -->

Create a `.env` file (if you don't already have one) with your model selections. Example for a 12-23 GB VRAM system:

```bash
cat >> .env << 'EOF'

# ── Ollama Model Configuration ──────────────────────────────────────────
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=qwen2.5:7b
MATRIC_FAST_GEN_MODEL=qwen3.5:9b
OLLAMA_VISION_MODEL=
EOF
```

Adjust the model names to match your hardware tier from the table above. For models marked _(disable)_, set the variable to empty (e.g., `MATRIC_FAST_GEN_MODEL=`).

### Restart and Verify

<!-- agent:step id="restart-with-ai" required="false" depends="configure-models" -->

Restart the matric service to pick up new model configuration:

```bash
docker compose -f docker-compose.bundle.yml up -d matric
```

Wait for healthy (same poll as Step 2), then verify Ollama connectivity:

```bash
# Check Fortemi can reach Ollama
curl -s http://localhost:3000/health | grep -o '"ollama[^}]*}'
```

Test embedding generation:

```bash
# Create a test note
NOTE_ID=$(curl -sf -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content":"Test note for embedding verification."}' \
  | grep -o '"id":"[^"]*"' | cut -d'"' -f4)

echo "Created note: $NOTE_ID"

# Wait for background embedding job (5-10 seconds)
sleep 10

# Verify embeddings exist
curl -sf "http://localhost:3000/api/v1/notes/$NOTE_ID" \
  | grep -o '"has_embedding":[^,]*'
# Expected: "has_embedding":true
```

On failure: Check that Ollama is reachable from Docker. The compose file uses `host.docker.internal` — on Linux, this requires the `extra_hosts` mapping (already configured in the compose file). Verify with:

```bash
docker exec $(docker compose -f docker-compose.bundle.yml ps -q matric) \
  curl -sf http://host.docker.internal:11434/api/tags > /dev/null \
  && echo "OK: Ollama reachable from container" \
  || echo "FAIL: Cannot reach Ollama from container"
```

**Tier 2 (+AI) is complete.** You now have semantic search, auto-linking, and NLP extraction.

---

## Step 4: Enable Extraction Sidecars (Optional)

Sidecars provide specialized NLP capabilities that run as separate containers alongside Fortemi.

### GLiNER (Named Entity Recognition)

<!-- agent:step id="start-gliner" required="false" depends="verify-health" -->

GLiNER is a zero-shot NER model that extracts entities from text. It's CPU-only and adds rich concept tagging to the extraction pipeline.

```bash
# Start GLiNER alongside existing services
docker compose -f docker-compose.bundle.yml up -d gliner
```

Wait for GLiNER to be healthy (first start downloads the model, ~1-2 minutes):

```bash
for i in $(seq 1 12); do
  curl -sf http://localhost:8090/health > /dev/null 2>&1 \
    && echo "OK: GLiNER healthy" && break
  echo "Waiting for GLiNER... ($i/12)"
  sleep 10
done
```

### Whisper (Audio Transcription)

<!-- agent:step id="start-whisper" required="false" depends="verify-health,detect-gpu" -->

Whisper transcribes audio and video attachments. Choose GPU or CPU mode:

**GPU mode** (default, fast, requires NVIDIA Container Toolkit):

```bash
docker compose -f docker-compose.bundle.yml up -d whisper
```

**CPU mode** (slower, works everywhere):

```bash
docker compose -f docker-compose.bundle.yml --profile whisper-cpu up -d
```

Wait for Whisper (first start downloads the model, ~2-5 minutes):

```bash
for i in $(seq 1 30); do
  curl -sf http://localhost:8000/health > /dev/null 2>&1 \
    && echo "OK: Whisper healthy" && break
  echo "Waiting for Whisper... ($i/30)"
  sleep 10
done
```

### pyannote (Speaker Diarization)

<!-- agent:step id="start-pyannote" required="false" depends="verify-health,detect-gpu" -->

pyannote identifies and labels individual speakers in audio. Requires a HuggingFace token for the gated pyannote model.

```bash
# Add your HuggingFace token (required for model download)
echo 'HF_TOKEN=hf_your_token_here' >> .env

# GPU mode (default, requires NVIDIA Container Toolkit):
docker compose -f docker-compose.bundle.yml up -d pyannote

# CPU mode (slower, works everywhere):
docker compose -f docker-compose.bundle.yml --profile pyannote-cpu up -d
```

Wait for pyannote (first start downloads the model, ~2-5 minutes):

```bash
for i in $(seq 1 30); do
  curl -sf http://localhost:8001/health > /dev/null 2>&1 \
    && echo "OK: pyannote healthy" && break
  echo "Waiting for pyannote... ($i/30)"
  sleep 10
done
```

### Verify Capabilities

<!-- agent:step id="verify-capabilities" required="false" depends="start-gliner,start-whisper" -->

Check which extraction strategies are active:

```bash
curl -s http://localhost:3000/health | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3000/health
```

The `capabilities.extraction_strategies` array in the health response shows all registered adapters. Expected entries depending on what you enabled:

| Sidecar | Extraction Strategy |
|---------|-------------------|
| GLiNER | `gliner_ner` |
| Whisper | `audio_transcription` |
| Ollama Vision | `image_vision`, `video_multimodal` |

On failure: Restart the matric service to re-detect sidecars: `docker compose -f docker-compose.bundle.yml restart matric`

**Tier 3 (+Full Stack) is complete.**

---

## Step 5: Connect an AI Agent (MCP)

<!-- agent:step id="configure-mcp" required="false" depends="verify-health" -->

Fortemi's MCP server enables AI agents (Claude Code, etc.) to read, search, and manage your knowledge base.

### Claude Code

Add to your project's `.mcp.json` (or `~/.claude/mcp.json` for global access):

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "http://localhost:3001/mcp"
    }
  }
}
```

For remote deployments behind a domain:

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "https://memory.example.com/mcp"
    }
  }
}
```

### Verify MCP Tools

After restarting Claude Code, verify tools are available:

```bash
# Quick test: list notes via MCP-backed API
curl -sf http://localhost:3000/api/v1/notes | head -c 200
```

In Claude Code, the `fortemi` MCP tools (e.g., `capture_knowledge`, `search`, `manage_tags`) should appear in the tool list. See [MCP Server](./mcp.md) for full tool documentation.

---

## Verification Checklist

| Feature | Check Command | Expected Result |
|---------|--------------|-----------------|
| API health | `curl -sf http://localhost:3000/health` | `"status":"healthy"` |
| API docs | `curl -sf http://localhost:3000/docs -o /dev/null -w '%{http_code}'` | `200` |
| MCP server | `curl -sf http://localhost:3001/ -o /dev/null -w '%{http_code}'` | `200` or connection accepted |
| Full-text search | `curl -sf 'http://localhost:3000/api/v1/search?q=test'` | JSON response with `results` array |
| Ollama connectivity | `curl -sf http://localhost:11434/api/tags` | JSON with model list |
| Embeddings working | Create note, wait 10s, check `has_embedding` | `true` |
| GLiNER healthy | `curl -sf http://localhost:8090/health` | `200` |
| Whisper healthy | `curl -sf http://localhost:8000/health` | `200` |
| Extraction strategies | `curl -sf http://localhost:3000/health \| grep extraction` | Lists active strategies |

---

## Troubleshooting

### Container fails to start on CPU-only host

**Symptom**: `docker compose up` fails with `nvidia` runtime error.

**Cause**: A sidecar (whisper, pyannote) requests GPU resources via `deploy.resources.reservations.devices`.

**Fix**: These sidecars are already `required: false` in the compose file. Start only the services you need:

```bash
docker compose -f docker-compose.bundle.yml up -d matric redis
```

Or use CPU profiles for transcription:

```bash
docker compose -f docker-compose.bundle.yml --profile whisper-cpu up -d
```

### Port 3000 already in use

```bash
# Find what's using the port
ss -tlnp | grep :3000
# Kill it or change the port mapping in docker-compose.bundle.yml
```

### Ollama not reachable from container

**Symptom**: Health shows `ollama: disconnected` or embeddings never generate.

**Fix**: Verify `host.docker.internal` resolves inside the container:

```bash
docker exec $(docker compose -f docker-compose.bundle.yml ps -q matric) \
  getent hosts host.docker.internal
```

If it doesn't resolve (some older Docker versions on Linux), add to your `.env`:

```bash
OLLAMA_BASE=http://172.17.0.1:11434
OLLAMA_HOST=http://172.17.0.1:11434
```

### Slow first startup

First-time initialization runs all database migrations and creates extensions. This can take 30-60 seconds. Subsequent starts are faster (~10 seconds).

### MCP tools not loading in Claude Code

1. Verify MCP server is running: `curl -sf http://localhost:3001/`
2. Check `.mcp.json` syntax (must be valid JSON)
3. Restart Claude Code after editing `.mcp.json`
4. See [MCP Troubleshooting](./mcp-troubleshooting.md) for detailed diagnostics

### Image pull fails from GHCR

```bash
# Verify GHCR is reachable
docker pull ghcr.io/fortemi/fortemi:bundle-latest
```

If you get authentication errors, GHCR public images should not require login. Check your Docker daemon configuration and network connectivity.

### Data persistence across restarts

All data is stored in Docker volumes (`matric-pgdata`, `matric-files`, `matric-backups`, `matric-redis`). Stopping and starting containers preserves data. Only `docker compose down -v` deletes volumes.

---

## What's Next?

| Goal | Guide |
|------|-------|
| Explore features (notes, search, tags, graph) | [Getting Started](./getting-started.md) |
| Configure search and AI in depth | [Search Guide](./search-guide.md), [Inference Backends](./inference-backends.md) |
| Plan hardware for production | [Hardware Planning](./hardware-planning.md) |
| Set up OAuth authentication | [Authentication](./authentication.md) |
| Configure reverse proxy (nginx) | [Deployment and Migrations](./deployment-and-migrations.md) |
| Connect AI assistants | [MCP Server](./mcp.md) |
| Manage multiple memories | [Multi-Memory Guide](./multi-memory.md) |
| Troubleshoot MCP issues | [MCP Troubleshooting](./mcp-troubleshooting.md) |

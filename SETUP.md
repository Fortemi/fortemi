# SETUP.md — Agentic Installation Guide

**Audience:** AI coding agents (Claude Code, Codex, Cursor, Windsurf, Copilot, etc.) installing Fortémi on a user's behalf. Read this end-to-end before running any commands.

**Companion docs:** [`README.md`](README.md) (human overview), [`QUICKSTART.md`](QUICKSTART.md) (full workstation walkthrough including HotM UI), [`docs/deployment/realtime-providers.md`](docs/deployment/realtime-providers.md) (Twilio Voice + Deepgram realtime setup).

---

## 1. What Fortémi Is

Fortémi is a Rust + PostgreSQL knowledge base with a semantic search engine, automatic knowledge graph, and MCP server exposing 43 agent tools. The deliverable from this repo is a Rust API + database backend; it does not include a UI.

There are **three install paths**. Pick the right one for the user's intent before running anything.

| User intent (signal) | Path | Where to install |
|---|---|---|
| "I want an app to take notes / talk to my notes / a desktop app" | **HotM (NOT this repo)** | Redirect to [Fortemi/HotM](https://git.integrolabs.net/Fortemi/HotM/releases/latest) — UI + this API bundled in one installer. Do not proceed in this repo. |
| "I want the headless API / I want to run MCP tools against my own backend / I want a Docker stack" | **Docker bundle** (this repo) | Section 3 |
| "I want to develop on Fortémi / build from source / contribute" | **Dev install** (this repo) | Section 4 |

If the user's intent is ambiguous, ask one clarifying question:
> "Do you want a finished desktop app to use Fortémi (HotM), or are you setting up the backend API for your own UI / agents?"

Do not guess. Installing the wrong path wastes the user's time.

---

## 2. Environment Detection

Before any install action, run these checks and note results. Do not skip — the install path forks on them.

### 2.1 Platform detection

```bash
uname -s   # Darwin = macOS, Linux = Linux, *NT* / MINGW* = Windows
uname -m   # x86_64, aarch64, arm64
```

| `uname -s` | Continue with |
|---|---|
| `Darwin` | macOS branch in §3 |
| `Linux` | Linux branch in §3 |
| `MINGW*` / `MSYS*` / `CYGWIN*` | **Windows native shell — switch to WSL2** before continuing. PowerShell directly is not supported by this repo's tooling. |

### 2.2 Docker availability

```bash
docker --version 2>&1
docker info 2>&1 | grep -E "Server Version|Default Runtime"
```

| Result | Action |
|---|---|
| Command not found | Install Docker Desktop (macOS / Windows / WSL2) or Docker Engine (Linux) before proceeding. Confirm with user, don't auto-install. |
| `Cannot connect to the Docker daemon` | Ask user to start Docker Desktop (macOS / Windows) or `sudo systemctl start docker` (Linux). |
| Version present, daemon reachable | Proceed. |

### 2.3 GPU detection (optional, gates GPU profiles)

```bash
nvidia-smi --query-gpu=name,memory.total --format=csv 2>&1 || echo "no-nvidia-gpu"
```

| Result | Profile to use |
|---|---|
| `no-nvidia-gpu` or command not found | **Default profile** (CPU). Whisper / pyannote / Open3D will use CPU; AI chat works but slower. |
| GPU detected, VRAM ≥ 6 GB and < 12 GB | Default profile is correct for this tier. |
| GPU detected, VRAM ≥ 12 GB and < 24 GB | `COMPOSE_PROFILES=gpu-12gb` |
| GPU detected, VRAM ≥ 24 GB | `COMPOSE_PROFILES=gpu-24gb` |

If GPU is present but the **NVIDIA Container Toolkit** is not installed, the user will get silent CUDA failures. Verify:

```bash
docker info | grep "Default Runtime"   # should show "nvidia" for GPU profiles
```

If missing, point the user at [NVIDIA's install guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) and the daemon config at `docker/daemon.json` in this repo. Do not silently install kernel-level packages.

### 2.4 Disk and RAM floor

```bash
# Linux / macOS
df -h .   # need ≥ 20 GB free on this volume
# macOS / Linux
free -h 2>/dev/null || vm_stat 2>/dev/null | head -5
```

Minimum: **16 GB RAM, 20 GB free disk**. If lower, tell the user — don't proceed silently. Most of the disk is model weights.

### 2.5 Port collision check

```bash
ss -tln 2>/dev/null | grep -E ':(3000|3001) ' || \
  netstat -an 2>/dev/null | grep -E '(3000|3001).*LISTEN'
```

If something is already on 3000 or 3001, set `API_HOST_PORT` / `MCP_HOST_PORT` in the `.env` file before `up` (see §3.3).

---

## 3. Docker Bundle Install (most common path)

The bundle is one container running Rust API + Postgres 18 (pgvector) + MCP server under supervisord. Optional sidecars (Whisper, pyannote, Redis) attach via profile.

### 3.1 Clone

```bash
git clone https://git.integrolabs.net/Fortemi/fortemi.git
cd fortemi
```

### 3.2 Create `.env`

```bash
cp .env.example .env
```

Then open `.env` and confirm:

- `ISSUER_URL` — set to `http://localhost:3000` for local dev, or the public URL if this is a server install.
- `COMPOSE_PROFILES` — set per §2.3 GPU tier (omit for default CPU profile).
- `API_HOST_PORT` / `MCP_HOST_PORT` — change if §2.5 found a collision.
- `OLLAMA_BASE` / `OLLAMA_HOST` — leave default (`http://host.docker.internal:11434`) if running Ollama on the host. Override only if Ollama lives in a sibling container or remote host.

**Do not** put secrets in `.env` if the project is shared. Don't commit `.env`.

### 3.3 Bring up the stack

```bash
docker compose -f docker-compose.bundle.yml up -d
```

For internal builds from the private registry:

```bash
FORTEMI_REGISTRY=git.integrolabs.net FORTEMI_TAG=bundle-main \
  docker compose -f docker-compose.bundle.yml pull
FORTEMI_REGISTRY=git.integrolabs.net FORTEMI_TAG=bundle-main \
  docker compose -f docker-compose.bundle.yml up -d --no-build
```

### 3.4 Wait for health

```bash
# Poll for up to 90 seconds (first-boot includes initdb on fresh volumes)
for i in $(seq 1 30); do
  curl -fs http://localhost:3000/health >/dev/null && { echo "healthy"; break; }
  sleep 3
done
```

If `/health` does not return `200` within ~90 seconds, see §6 Troubleshooting.

### 3.5 Set up Ollama (host install path)

If Ollama is not already on the host:

```bash
# macOS / Linux
curl -fsSL https://ollama.com/install.sh | sh

# Pull the default models Fortémi expects
ollama pull qwen3.5:9b
ollama pull nomic-embed-text
```

Verify Fortémi can reach it:

```bash
curl -s http://localhost:3000/api/v1/inference/providers | head
```

### 3.6 First-run smoke test

```bash
# Create a note via MCP-compatible REST
curl -s -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content": "Fortémi setup test note", "revision_mode": "none"}'

# Search it
curl -s 'http://localhost:3000/search?q=setup&limit=5'
```

If both succeed, the install is complete.

---

## 4. Dev Install (build from source)

Use this path only when the user wants to develop on Fortémi itself — modify Rust code, run tests, add features.

### 4.1 Prerequisites

| Tool | Min version | Install hint |
|---|---|---|
| Rust toolchain | 1.83+ | `curl https://sh.rustup.rs -sSf \| sh` |
| PostgreSQL 18 + pgvector | 18.x | OS package manager; ensure `pgvector` extension is available |
| `sqlx-cli` | latest | `cargo install sqlx-cli --no-default-features --features postgres` |
| Node.js (for MCP server) | 20+ | `nvm install 20` |

Do **not** install Postgres 17 or earlier — the project hard-requires PG 18 (see commit `10d2601`).

### 4.2 Database setup

```bash
# Create database
createdb -U postgres matric_memory
psql -U postgres -d matric_memory -c "CREATE EXTENSION IF NOT EXISTS vector;"

# Set DATABASE_URL
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/matric_memory"

# Run migrations
sqlx migrate run
```

### 4.3 Build and run

```bash
cargo build --release -p matric-api
./target/release/matric-api
```

API listens on `:3000`. For the MCP server:

```bash
cd mcp-server
npm install
npm start
```

### 4.4 Test suite

```bash
cargo check --workspace --tests
cargo test --workspace
```

Use this as the "is the dev environment correctly set up?" check.

---

## 5. Configuration Reference

| File | Purpose | Edit when |
|---|---|---|
| `.env` | Compose-level env (host ports, profile, registry, OLLAMA_BASE) | Per-install |
| `docker-compose.bundle.yml` | Service topology — do not edit; override via `.env` or compose override file | Never (use override) |
| `docker-compose.workstation.yml` | All-in-one workstation stack (Fortémi + HotM + auth proxy + nginx) | Only if using the workstation walkthrough in `QUICKSTART.md` |
| `crates/matric-core/src/defaults.rs` | Compile-time defaults (e.g. `OLLAMA_GEN_MODEL`, `REVISION_CHUNK_MAX_CHARS`) | Source modifications only |
| Per-doc-type chunking | `revision_chunking` columns on `note_types` table; seeded by migration | Runtime SQL update |

Key environment variables (full list in `.env.example`):

```
OLLAMA_GEN_MODEL=qwen3.5:9b              # Generation model
OLLAMA_EMBED_MODEL=nomic-embed-text      # Embedding model
OLLAMA_EMBED_DIM=768                     # Must match embed model output dim
REVISION_CHUNK_MAX_CHARS                 # Override revision chunking (issue #573)
GPU_EXCLUSIVE_MODE=true                  # Stop sidecars during Ollama tiers (issue #576)
CHAT_MAX_CONCURRENT=4                    # /chat semaphore (issue #549)
```

---

## 6. Troubleshooting

### `/health` never returns 200 within 90 seconds

```bash
docker compose -f docker-compose.bundle.yml logs --tail=100 matric
```

Common causes:

- **`initdb` still running on a fresh volume** — wait 30 more seconds. Look for `database system is ready to accept connections` in logs.
- **Port already bound** — see §2.5; change `API_HOST_PORT` in `.env`, then `down && up -d`.
- **GPU profile selected but Container Toolkit missing** — drop the profile (`COMPOSE_PROFILES=` empty) or install the toolkit.

### Ollama unreachable from container

```bash
docker compose -f docker-compose.bundle.yml exec matric \
  curl -s http://host.docker.internal:11434/api/version
```

- On Linux, this requires `extra_hosts: ["host.docker.internal:host-gateway"]` (already in the compose file).
- On Docker Desktop (macOS/Windows), works natively.
- If Ollama is in a sibling container, set `OLLAMA_BASE` to the container's network alias instead.

### `pgvector extension not available`

The image bundles `pgvector/pgvector:pg18`. If you see this error, the volume mount may be from an older Postgres data directory — wipe with `docker compose -f docker-compose.bundle.yml down -v` and start fresh. **This destroys data.** Confirm with the user before wiping.

### AI revision fails on long documents

Hit by users on small-GPU systems before #576 (GPU sidecar lifecycle). Confirm `GPU_EXCLUSIVE_MODE=true` in `.env`. If sidecars are needed continuously (multi-GPU rig), set to `false` and accept higher VRAM pressure.

### `/chat` returns 503

Either no inference backend is reachable (Ollama down — see Ollama troubleshooting), or the concurrency semaphore is full. Tune via `CHAT_MAX_CONCURRENT`.

### Migrations fail with "relation already exists"

The database was partially initialized from a prior install. Either:

```bash
docker compose -f docker-compose.bundle.yml down -v   # wipe — destructive
```

or manually inspect with `docker compose ... exec matric psql -U matric -d matric_memory -c "\dt"` and resolve.

### Other

Check the GitHub-style issues at [git.integrolabs.net/Fortemi/fortemi/issues](https://git.integrolabs.net/Fortemi/fortemi/issues) before opening a new one. Many install-time problems already have closed issues with solutions.

---

## 7. Verification Checklist

Before reporting success to the user:

- [ ] `docker compose -f docker-compose.bundle.yml ps` shows `matric` container `Up` and `(healthy)`
- [ ] `curl -fs http://localhost:3000/health` returns `200`
- [ ] `curl -s http://localhost:3000/api/v1/inference/providers` lists Ollama as available
- [ ] A test note can be created and searched (see §3.6)
- [ ] If GPU profile was selected, `nvidia-smi` shows the Ollama / whisper processes consuming VRAM

If any item fails, do not declare the install complete. Walk back through §6.

---

## 8. Agent Conduct Notes

- **Do not** install system packages without confirming with the user (e.g., NVIDIA Container Toolkit, Docker Desktop, Postgres).
- **Do not** wipe Docker volumes (`down -v`) without explicit user confirmation — this destroys data.
- **Do not** write to `.env` programmatically without showing the user the diff.
- **Do** read this entire document before running commands. The decision tree forks early (§1).
- **Do** prefer the **HotM** install path for non-technical users. Most people who land on this repo wanted the desktop app and didn't know it.

If the user asks for something this document doesn't cover, check `QUICKSTART.md` (for the workstation walkthrough), `README.md` (for the backend overview), or the [issue tracker](https://git.integrolabs.net/Fortemi/fortemi/issues) before improvising.

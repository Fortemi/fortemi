# Workstation Stack — Setup & Operations

> **New to Docker?** Start with **[QUICKSTART.md](./QUICKSTART.md)** instead. It walks you through the same setup with no jargon and step-by-step expected output. This document is the reference manual — all the commands and troubleshooting beyond the happy path.
>
> The friendly wrapper is `./workstation` — run `./workstation help` for the command list.

A unified Docker stack for HotM + Fortemi + Ollama, designed for end-to-end UI verification on a developer box.

This document covers the one-time host preparation (removing the native ollama install) and the day-to-day commands for spinning the stack up and down with or without the UI.

---

## One-time host prep

### 1. Stop and remove the native ollama install

The native ollama systemd service is owned by root (system service, not user service). The commands below need `sudo` and must be run interactively by you — agents cannot type sudo passwords.

Copy-paste in order:

```bash
# Stop the running service and disable it from coming back at boot
sudo systemctl stop ollama.service
sudo systemctl disable ollama.service

# Confirm port 11434 is now free
ss -tlnp 2>/dev/null | grep 11434 || echo "port 11434 is free"

# Remove the systemd unit + drop-in directory
sudo rm -f /etc/systemd/system/ollama.service
sudo rm -rf /etc/systemd/system/ollama.service.d

# Remove the binary
sudo rm -f /usr/local/bin/ollama

# Reload systemd so it forgets the unit
sudo systemctl daemon-reload

# Verify
which ollama || echo "native ollama binary removed"
```

**Preserved**: `~/.ollama/models/` (6.4GB of `qwen3.5:9b` + `nomic-embed-text`). The compose stack bind-mounts this directory into the container — no redownload.

If you ever want to nuke the models too: `rm -rf ~/.ollama` (~6.4GB).

### 2. Verify GPU passthrough is wired

```bash
# nvidia-container-toolkit should already be installed if previous Docker+GPU work has been done.
# Quick check:
docker run --rm --gpus all nvidia/cuda:12.2.0-base-ubuntu22.04 nvidia-smi
```

You should see your `NVIDIA GeForce RTX 3060 Laptop GPU` in the output. If not, install nvidia-container-toolkit per <https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html>.

---

## Day-to-day commands

All commands assume you are in the `fortemi/` repo directory — the one that contains `workstation`, `docker-compose.workstation.yml`, and this file. The wrapper expects HotM cloned as a sibling at `../HotM/` (per QUICKSTART step 1) for the `hotm` and `ui` profiles; backend-only mode does not require it.

> Prefer the wrapper. Every section below shows the raw `docker compose` form for reference, but `./workstation <subcommand>` will be shorter, will set the right `--profile` flag, and will print health output. Run `./workstation help` for the full list.

### Spin up — core (no UI)

For backend workstation, raw API calls, agent-proxy validation:

```bash
docker compose -f docker-compose.workstation.yml up -d
```

Brings up:
- `workstation-ollama` (port 11434) — bind-mounted models
- `workstation-postgres` (port 5434) — pgvector
- `workstation-matric-api` (port 3000) — Fortemi backend with `REQUIRE_AUTH=false`
- `workstation-agent-proxy` (port 3011) — HotM sidecar

### Spin up — with UI

For end-to-end UI verification:

```bash
docker compose -f docker-compose.workstation.yml --profile ui up -d
```

Adds:
- `workstation-hotm-ui` (port 4180) — React SPA

Open <http://localhost:4180> to verify the UI.

### Spin up — LLM only

For raw ollama API workstation without anything else:

```bash
docker compose -f docker-compose.workstation.yml --profile llm-only up -d ollama
```

### Stop — keep data

```bash
docker compose -f docker-compose.workstation.yml down
```

Containers stop; postgres data and ollama models persist.

### Stop — wipe everything

```bash
docker compose -f docker-compose.workstation.yml down -v
```

Containers stop; `workstation_pgdata` volume is wiped. Ollama models survive (bind-mounted, not in a docker volume).

### Tail logs

```bash
# Everything
docker compose -f docker-compose.workstation.yml logs -f

# One service
docker compose -f docker-compose.workstation.yml logs -f matric-api
docker compose -f docker-compose.workstation.yml logs -f ollama
```

### Rebuild a single service after code changes

```bash
docker compose -f docker-compose.workstation.yml up -d --build matric-api
docker compose -f docker-compose.workstation.yml up -d --build agent-proxy
docker compose -f docker-compose.workstation.yml up -d --build hotm-ui
```

---

## Verification checklist

After `docker compose ... up -d`, run:

```bash
# 1. Ollama responds and sees the bind-mounted models
curl -s localhost:11434/api/tags | jq '.models[].name'
# Expected: "qwen3.5:9b", "nomic-embed-text:latest"

# 2. Postgres is healthy
docker exec workstation-postgres pg_isready -U matric
# Expected: accepting connections

# 3. matric-api is up
curl -s localhost:3000/health | jq
# Expected: HTTP 200, healthy

# 4. agent-proxy is up
curl -s localhost:3011/health 2>&1
# Expected: HTTP 200 (response shape varies)

# 5. UI loads (only with --profile ui)
curl -sI localhost:4180/ | head -3
# Expected: HTTP/1.1 200 OK

# 6. End-to-end: matric-api can reach ollama
curl -s localhost:3000/api/v1/inference/providers | jq
# Expected: includes ollama with status accessible
```

---

## LLM backend selection

By default the workstation uses **containerized Ollama** with `qwen3.5:9b` and `nomic-embed-text`. The matric-api inference router also speaks OpenAI-compatible APIs (vLLM, llama.cpp, real OpenAI, OpenRouter, LocalAI, LiteLLM), and you can swap which one is active without editing `docker-compose.workstation.yml` or rebuilding any image.

### How it works

The matric-api service in the compose file declares `env_file: .env.workstation` with `required: false`. If the file exists, its values override the inline `environment:` defaults; if it doesn't, the workstation behaves exactly like the no-config Ollama path. `.env.workstation` is gitignored — API keys never get committed.

The compose file also wires `extra_hosts: host.docker.internal:host-gateway` so the same URL (`http://host.docker.internal:<port>`) reaches the host from inside the container on Linux, macOS, and Windows. No more `172.17.0.1` vs `host.docker.internal` juggling.

### The wizard (recommended for new users)

```bash
./workstation configure-llm
```

Walks through five backends:

1. **ollama-local** — Docker-managed Ollama (default, no setup needed)
2. **vllm-local** — vLLM running on your host machine
3. **openai-cloud** — Real OpenAI API (requires API key)
4. **openrouter** — OpenRouter cloud router (requires API key)
5. **llamacpp-local** — llama.cpp server running on your host machine

For cloud providers, the wizard prompts for the API key with no terminal echo and writes `.env.workstation` mode 600. For local-on-host providers (vLLM, llama.cpp), it prompts for the host port and writes the right `host.docker.internal:<port>` URL.

After writing the file, the next `./workstation up` picks it up automatically.

### Edit by hand (power-user path)

```bash
cp .env.workstation.example .env.workstation
$EDITOR .env.workstation        # uncomment one provider block
chmod 600 .env.workstation
```

`.env.workstation.example` carries five copy-paste-ready blocks with inline guidance on the served-name-vs-HF-path distinction (vLLM), the embedding-pairing pattern (cloud providers route embeddings through the containerized Ollama), and hardware-friendly model recommendations for 8GB to 24GB+ GPUs.

### vLLM specifics

vLLM has two names for any model:

- the **HF path** it loads (e.g. `Qwen/Qwen2.5-7B-Instruct`) — passed as the first positional arg to `vllm serve`
- the **served-model-name** it exposes (e.g. `qwen3.5:9b`) — passed to `--served-model-name`

`OPENAI_GEN_MODEL` in `.env.workstation` must match the **served-model-name**, not the HF path. matric-api sends `OPENAI_GEN_MODEL` verbatim in the OpenAI `model` field; vLLM compares it against `--served-model-name` and returns 404 on mismatch.

The default served name we recommend is `qwen3.5:9b` — same string as Fortemi's Ollama default, so the chat UI and any agent configuration sees the same model name regardless of which backend you've picked. Start vLLM with whatever HF path you want and label it `qwen3.5:9b`:

```bash
# Light: fits on most consumer GPUs
vllm serve Qwen/Qwen2.5-7B-Instruct \
  --host 0.0.0.0 --port 8000 \
  --served-model-name qwen3.5:9b

# Heavy: 35B-on-3×A100 pattern (see qwen36_vllm_autodeploy_basic.sh)
# port 11436, served as qwen3.6:35b
```

For the heavy-iron path, the `qwen36_vllm_autodeploy_basic.sh` pattern (auto-selects free A100-class GPUs, tries largest context first, picks an open port from 11436+, runs OpenAI-compatible) is a reasonable template. With that script:

```bash
./workstation configure-llm
# → 2 (vllm-local)
# → port: 11436
# → served model name: qwen3.6:35b
```

### Doctor reports the configured backend

```bash
./workstation doctor
```

Reports an 8th check, `LLM backend`, that:

- reads `MATRIC_INFERENCE_DEFAULT` from `.env.workstation` (or reports "ollama containerized default" if the file is absent)
- probes the configured endpoint from the host (rewriting `host.docker.internal` → `localhost` for the host-side probe; the container sees the same URL on the docker network)
- surfaces friendly remediation when the probe fails ("is vLLM running on the host?", "is the port right?")

This catches the common first-attempt mistakes — wrong port, vLLM not started yet, wrong served-model-name — before you waste time on `up` and chat-call debugging.

### Switching backends

Re-run `./workstation configure-llm` and pick a different option, or edit `.env.workstation` by hand. The change takes effect on the next `./workstation up` (Docker reads `env_file:` at container start). For a running stack:

```bash
./workstation configure-llm   # write new .env.workstation
./workstation down            # stop matric-api
./workstation up              # restart with new env
```

The `matric-api/v1/inference/config` endpoint also supports runtime hot-swap without a restart (POST `{"openrouter": {"api_key": "..."}}` etc.). That path is for already-running deployments and is documented in `docs/content/inference-providers.md` — the file-based wizard is the easier choice for "before I bring it up the first time."

### Per-backend reminders

| Backend | Common gotcha |
|---|---|
| ollama-local | No setup. Ensure `nomic-embed-text` + `qwen3.5:9b` are pulled via `./workstation models pull`. |
| vllm-local | `OPENAI_GEN_MODEL` must match `--served-model-name`, not the HF path. Embeddings still route through containerized Ollama (vLLM has no embedding endpoint). |
| openai-cloud | Embeddings are billed the same as generation. Set `MATRIC_EMBEDDING_PROVIDER=ollama` to keep embeddings free and local (the wizard does this for you unless you explicitly opt out). |
| openrouter | No embedding endpoint at all — must pair with Ollama for embeddings. Wizard wires this for you. |
| llamacpp-local | llama-server's embedding mode is finicky (the embedding model and generation model must match). Pair with Ollama for embeddings unless you have a specific reason. |

---

## Troubleshooting

### "port 11434 already in use"

The native ollama service is still running. Run the one-time prep above.

### "no NVIDIA driver / GPU device requested"

The compose file requests GPU passthrough. If you don't have a GPU or `nvidia-container-toolkit` isn't installed, edit `docker-compose.workstation.yml` and remove the `deploy.resources.reservations.devices` block under the `ollama` service. Models will run on CPU (very slow for qwen3.5:9b).

### "matric-api healthy but UI shows 'cannot reach API'"

The UI baked-in API URL is `http://localhost:3000`. This is correct when accessing the UI from your browser. If you proxy the UI through a tunnel or different hostname, rebuild with `VITE_API_BASE_URL=<your URL>` set as a build arg.

### Ollama can see models but matric-api errors

matric-api's `OLLAMA_GEN_MODEL=qwen3.5:9b` must match a model name that `curl localhost:11434/api/tags` returns. If the env defaults drift from what's installed, override via:

```bash
OLLAMA_GEN_MODEL=qwen3.5:9b docker compose -f docker-compose.workstation.yml up -d matric-api
```

### "permission denied" on the bind-mounted ~/.ollama

The container runs as root by default and writes new models with root ownership. If you later run native ollama again, you may see permission issues on `~/.ollama/models/`. Fix:

```bash
sudo chown -R $USER:$USER ~/.ollama
```

---

## What this stack is NOT

- **Not production**: `REQUIRE_AUTH=false`, CORS wide open, no TLS. Localhost workstation only.
- **Not the fortemi-auth crate's test environment**: the v0.1.0 fortemi-auth deliverable (JWT verification, tenant isolation) is a separate test surface. This stack uses `REQUIRE_AUTH=false` to bypass that path entirely.
- **Not for offline/airgapped use**: first build pulls images from Docker Hub + ghcr.io.

## References

- `docker-compose.workstation.yml` — this stack's source of truth
- `fortemi/docker-compose.yml` — per-repo Fortemi compose (production-shaped)
- `HotM/docker-compose.prod.yml` — per-repo HotM compose (ghcr.io images)
- `fortemi-auth/.aiwg/reports/construction-ready-brief.md` — the v0.1.0 auth crate that will eventually replace `REQUIRE_AUTH=false`

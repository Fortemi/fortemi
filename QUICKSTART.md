# Fortemi Workstation — Quickstart

This is a first-run walkthrough for setting up Fortemi + HotM + Ollama on your own machine using Docker. If you have never used Docker before, this is the right page to start on.

You don't need to know Docker or Rust to follow this. The setup is one command per step.

---

## What you'll end up with

A web app at **http://localhost:4180** where you can:
- Capture and organize notes
- Search them
- Chat with a local AI model that runs on your machine (your data never leaves your computer)

Plus the API and AI backend that powers it. All running in Docker containers — easy to start, stop, and remove cleanly.

---

## What you need

1. **A computer.** Linux, macOS, or Windows with WSL2.
2. **Docker.** If you don't have it, install Docker Desktop from <https://www.docker.com/products/docker-desktop>. Open it once after install to make sure it's running.
3. **Disk space.** About 15 GB for the AI models and database. Less if you skip the larger model.
4. *(Nice to have, not required)* **An NVIDIA GPU.** Makes AI chat much faster. Works on CPU too, just slower.

That's it. You do not need to install Ollama, PostgreSQL, Rust, Node.js, or anything else separately.

---

## Step 1 — Clone both repos side-by-side

The workstation needs **two repos cloned as siblings**: this one (`fortemi`) and `HotM`. The compose file looks for HotM at `../HotM/` relative to this repo.

```bash
# Pick a directory to hold both
mkdir -p ~/dev/fortemi && cd ~/dev/fortemi

git clone https://git.integrolabs.net/Fortemi/fortemi.git
git clone https://git.integrolabs.net/Fortemi/HotM.git

# You should end up with this layout:
#   ~/dev/fortemi/
#     fortemi/   ← this repo (you'll run commands from here)
#     HotM/      ← the UI + agent-proxy
```

**Backend-only?** If you only want the API (no HotM UI, no agent-proxy), you can skip cloning HotM and run `./workstation up --backend-only` instead. Everything below stays the same except you'll use the `--backend-only` flag on `up` and `doctor`.

## Step 2 — Open a terminal in the fortemi/ directory

Open Terminal (macOS / Linux) or PowerShell (Windows). Change into the `fortemi/` directory. You should see this file (`QUICKSTART.md`) in the directory.

```bash
cd /path/to/fortemi
ls
# you should see: workstation  QUICKSTART.md  docker-compose.workstation.yml  fortemi/  HotM/  ...
```

---

## Step 3 — Run the doctor

This checks your machine and tells you what's ready and what's missing. **Nothing is installed or changed by this step** — it's read-only.

```bash
./workstation doctor
```

Expected output (a green checkmark next to each line means good):

```
Pre-flight checks
✓ Docker installed and running
✓ docker compose available
✓ compose file found
✓ required ports free or already held by workstation containers
✓ no native ollama install detected
✓ GPU passthrough working: NVIDIA GeForce RTX 3060, 6144 MiB
⚠ ~/.ollama/models exists but contains no models
    Run: workstation models pull

All checks passed — ready to run: workstation up
```

If you see any **red ✗** marks, the message tells you exactly what to do. The most common ones:

- **Docker not running.** Open Docker Desktop (macOS/Windows) or run `sudo systemctl start docker` (Linux). Then try again.
- **Native ollama detected.** If you previously installed Ollama directly on your computer, follow the cleanup commands the doctor prints. The Docker version replaces it.
- **Port conflicts.** Some other software is using the same network ports. Stop that software, or open `docker-compose.workstation.yml` and change the port numbers.

If you see a **yellow ⚠** about models, that's expected on first run — we'll fix it in step 4.

---

## Step 3.5 — (Optional) Pick a different LLM backend

The default is **Ollama running in Docker** — fully self-contained, no API keys, no extra setup. If that's what you want, **skip this step** and go to Step 4.

You'd choose a different backend if you have:

| You have… | Pick |
|---|---|
| Just want to try Fortemi end-to-end (no API keys, no extra setup) | **Stay on Ollama** — skip this step |
| vLLM already running on your machine | **vllm-local** |
| An OpenAI API key | **openai-cloud** |
| An OpenRouter API key (multi-provider, including Anthropic models) | **openrouter** |
| A llama.cpp server you've started yourself | **llamacpp-local** |

Run the wizard:

```bash
./workstation configure-llm
```

It will ask which backend you want, prompt for an API key (silently) or a port, and write `.env.workstation` with the right env vars. The compose file picks it up automatically on the next `./workstation up` — no Dockerfile or compose edits needed.

If you prefer editing by hand instead of using the wizard:

```bash
cp .env.workstation.example .env.workstation
$EDITOR .env.workstation        # uncomment one provider block
chmod 600 .env.workstation       # keep API keys readable to you only
```

**Networking note (vLLM / llama.cpp users):** When the backend runs on your host machine and the workstation runs in Docker, the container has to reach the host via `host.docker.internal`. The compose file wires this up for Linux too (Docker Desktop on macOS/Windows handles it natively). The wizard prefills this for you — you only need to know the host port your LLM is listening on.

**Verify the choice took effect:**

```bash
./workstation doctor
# Look for: "✓ LLM backend: <your choice> (from .env.workstation)"
```

You can switch backends any time. Re-run `configure-llm` (or edit `.env.workstation`) and then `./workstation up` to apply.

---

## Step 4 — Start everything

One command brings up the whole stack:

```bash
./workstation up
```

What this does:
- Starts the AI backend (Ollama) — listens on port 11434
- Starts the database (PostgreSQL with vector search) — port 5434
- Starts the Fortemi API — port 3000
- Starts the agent proxy — port 3011
- Starts the web UI — port 4180
- Waits for each service to report "healthy"

First run takes 5–10 minutes because Docker is downloading the base images and compiling the Rust API. Later runs take about 30 seconds because everything is cached.

When it's done, you'll see:

```
Containers
... (5 services listed)

Service health
✓ ollama: running (health: healthy)
✓ postgres: running (health: healthy)
✓ matric-api: running (health: healthy)
✓ agent-proxy: running (health: n/a)
✓ hotm-ui: running (health: healthy)

Endpoints
✓ UI: HTTP 200 (http://localhost:4180)
✓ matric-api: HTTP 200 (http://localhost:3000)
✓ agent-proxy: HTTP 200 (http://localhost:3011)
✓ ollama: HTTP 200 (http://localhost:11434)

→ Open in browser: http://localhost:4180
→ Or run: workstation open
```

---

## Step 5 — Get the AI models (first run only)

The AI models are large files (a few GB each). The Docker setup stores them in your home directory at `~/.ollama/`, so they survive across restarts and only need to be downloaded once.

```bash
./workstation models pull
```

This pulls the two default models:
- `qwen3.5:9b` — the chat/generation model (~6.6 GB, ~10 min on a 50 Mbps connection)
- `nomic-embed-text` — used for searching across your notes (~274 MB)

Watch the progress bars. When both finish:

```bash
./workstation models list
```

You should see both names with a SIZE column.

If you already had Ollama installed before this setup, your existing models in `~/.ollama/` are already available — no download needed.

---

## Step 6 — Open the UI

```bash
./workstation open
```

This opens **http://localhost:4180** in your browser. You should see:
- "Hall of the Mind" header
- "API Connected" (green) in the left sidebar
- An empty "Notes Workspace"

If it says **"Offline Mode"** in red, hit **Ctrl+Shift+R** (Cmd+Shift+R on Mac) to force-refresh the page. Modern browsers cache aggressively and the first load can be stale.

You can now create notes, search them, and chat with the AI.

---

## Day-to-day operations

Once setup is done, you only need a few commands:

| What you want | Command |
|---|---|
| Start everything (full stack with UI) | `./workstation up` |
| Start API + agent-proxy, no UI | `./workstation up --no-ui` |
| Start backend only (no HotM repo needed) | `./workstation up --backend-only` |
| Stop everything | `./workstation down` |
| Open the UI | `./workstation open` |
| Check what's running | `./workstation status` |
| Watch logs (all services) | `./workstation logs` |
| Watch one service | `./workstation logs matric-api` |
| Wipe the database and start fresh | `./workstation reset` |
| See all available commands | `./workstation help` |

---

## What if something breaks?

### "API Connected" turns red / shows "Offline Mode"

1. `./workstation status` — check if all services are healthy
2. If any service is red or restarting, `./workstation logs <service>` to see why
3. If everything looks healthy, hit **Ctrl+Shift+R** in the browser

### Port already in use

Some software on your machine is using the same port as a workstation service. Either:
- Stop the conflicting software, or
- Open `docker-compose.workstation.yml` and change the host-side port number (the number BEFORE the `:`, e.g. `"3000:3000"` → `"3005:3000"`)

### AI chat is very slow

If you don't have an NVIDIA GPU (or the GPU isn't passing through to Docker), the model runs on CPU. The 9B model is slow on CPU — try a smaller one:

```bash
./workstation models pull qwen3.5:3b
# then update OLLAMA_GEN_MODEL in docker-compose.workstation.yml to qwen3.5:3b
./workstation up
```

### "Permission denied" on `./workstation`

Mark the script executable:

```bash
chmod +x ./workstation
```

### Container won't start: "no NVIDIA driver / GPU device requested"

You don't have GPU support set up (or don't have an NVIDIA GPU). Open `docker-compose.workstation.yml` and remove the `deploy.resources.reservations.devices` block under the `ollama:` service. The stack will run on CPU.

### Disk filling up

The biggest space users are the AI models. Check what you have:

```bash
./workstation models list
du -sh ~/.ollama
```

Remove models you're not using:

```bash
./workstation models rm qwen3.5:9b
```

---

## Going deeper

When you outgrow this quickstart:

- **WORKSTATION-SETUP.md** — full operations reference, including troubleshooting beyond what this doc covers, manual `docker compose` commands, and architecture notes
- **docker-compose.workstation.yml** — the stack definition itself, fully commented
- **`./workstation help`** — every available command

If something doesn't work and the doctor + logs don't help, the entries in WORKSTATION-SETUP.md → Troubleshooting are the next place to look.

---

## Removing the workstation entirely

If you decide this isn't for you:

```bash
./workstation reset                    # stop everything, wipe the database
rm -rf ~/.ollama                       # remove AI models (~6.4 GB)
docker rmi $(docker images -q workstation-pg18-pgvector fortemi-matric-api fortemi-agent-proxy fortemi-hotm-ui 2>/dev/null)
                                       # remove the built images
docker volume prune                    # remove any leftover volumes
```

Then if you want, uninstall Docker Desktop. That returns your machine to its pre-Fortemi state.

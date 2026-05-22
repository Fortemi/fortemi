# BT6 Arsenal Desktop — Bollard Deployment Contract

**Audience:** BT6 Arsenal Desktop Tauri backend (`bollard`-driven `DockerSupervisor`)
**Companion issue:** [Fortemi/fortemi#587](https://git.integrolabs.net/Fortemi/fortemi/issues/587)
**Stability:** Per-release. Pin to a release tag of this repo; do not track `:latest` in shipped builds.

## 1. Topology

Fortemi Full ships as **one bundle container** plus optional sidecars. The original issue assumed a three-container topology (API + Postgres + Ollama); the actual contract is:

| Container | Image | Role | Required |
|---|---|---|---|
| `fortemi` (bundle) | `ghcr.io/fortemi/fortemi:<tag>` | Rust API + Postgres 18 (pgvector) + MCP server, supervised by `supervisord` in one image | **Yes** |
| Ollama | `ollama/ollama:<tag>` or BYO | LLM inference. **Not bundled.** Runs on the host or in a separate container; Fortemi reaches it via `OLLAMA_BASE`. | Yes (functional) |
| Whisper | `ghcr.io/speaches-ai/speaches:latest-cpu` (CPU) or `…:latest-cuda-12.6.3` (GPU) | Speech-to-text | Optional |
| pyannote | `ghcr.io/fortemi/fortemi:pyannote-<tag>` | Speaker diarization | Optional |
| Redis | `redis:7-alpine` | Search-result cache | Optional |

### Why one container, not three

The `Dockerfile.bundle` produces a `pgvector/pgvector:pg18`-derived image that runs Postgres + matric-api + MCP under `supervisord`, with `bundle-entrypoint.sh` initializing Postgres on first run and applying migrations. This is intentional: single-process orchestration ships predictably across Docker Desktop (macOS/Windows) and Linux Engine, and the desktop bundle has no need for the operator-facing knobs of a multi-service compose deployment.

Bollard-driven orchestration is fine — Fortemi does **not** require `docker compose` semantics.

## 2. Image Names and Tags

### Canonical public path (authoritative)

```
ghcr.io/fortemi/fortemi:<release-tag>          # bundle (API + Postgres + MCP)
ghcr.io/fortemi/fortemi:pyannote-<release-tag> # pyannote sidecar
ghcr.io/fortemi/fortemi:gliner-<release-tag>   # gliner NER sidecar (currently unused by BT6)
```

`ghcr.io/bt-6/fortemi-*` is reserved for **BT-6-internal mirrors or forks** and MUST NOT be bundled in upstream-tracking releases. This matches the position BT6 stated in [#587 comment 2026-04-08](https://git.integrolabs.net/Fortemi/fortemi/issues/587#issuecomment-50058).

### Pinning policy

Pin to a release tag on this repo. Releases follow CalVer (`vYYYY.M.PATCH`, e.g. `v2026.5.11`). There is no separate stable channel. `:latest` is published but unsuitable for shipped desktop builds.

For `apps/desktop/src-tauri/tools.toml`, encode pins as:

```toml
[tools.fortemi]
image = "ghcr.io/fortemi/fortemi"
tag   = "v2026.5.11"          # bump deliberately, never use 'latest'
```

## 3. Container Spec — `fortemi` (bundle)

### Ports

| Container port | Purpose | Notes |
|---|---|---|
| `3000` | REST API + health | Required. Bind to a host port chosen by `DockerSupervisor`. |
| `3001` | MCP server (OAuth) | Required if HotM agent-proxy or other MCP consumers attach. |

Bind to localhost on the host side; do not expose to `0.0.0.0` from a desktop bundle.

### Volumes (named, managed by `DockerSupervisor`)

| Container path | Purpose | Persistence |
|---|---|---|
| `/var/lib/postgresql/data` | Postgres data directory (created on first run by `bundle-entrypoint.sh`) | Must survive app upgrades |
| `/var/lib/matric/files` | Attachment / blob store | Must survive app upgrades |
| `/var/backups/matric-memory` | On-demand backups | Optional; recommended |

Recommended host-side anchor (per BT6's stated layout):

- macOS: `~/Library/Application Support/BT6-Arsenal/fortemi/{pg-data,files,backups}/`
- Windows: `%APPDATA%\BT6-Arsenal\fortemi\{pg-data,files,backups}\`
- Linux: `~/.local/share/BT6-Arsenal/fortemi/{pg-data,files,backups}/`

Mount these as **named Docker volumes** with bind-source set to the host paths above so a desktop reinstall preserves user data.

### Environment

Minimum environment for a desktop deployment:

```
# Inference target (host-mounted Ollama)
MATRIC_INFERENCE_DEFAULT=ollama
OLLAMA_BASE=http://host.docker.internal:11434
OLLAMA_HOST=http://host.docker.internal:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_GEN_MODEL=qwen3.5:9b
OLLAMA_EMBED_DIM=768

# Optional sidecars (omit if not started)
WHISPER_BASE_URL=http://whisper:8000
DIARIZATION_BASE_URL=http://pyannote:8001

# Bundle internals (defaults fine for desktop)
POSTGRES_PASSWORD=matric          # internal-only, listens on localhost in the container
```

`host.docker.internal` resolves natively on Docker Desktop (macOS/Windows). On Linux Engine, add `extra_hosts: ["host.docker.internal:host-gateway"]` when creating the container.

### Healthcheck

The image declares:

```
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1
```

`bollard` can use the image-declared healthcheck or override it. Treat the container as healthy once `/health` returns `200`. First-boot is the long tail: `start-period=60s` covers Postgres `initdb` on fresh data volumes; budget up to **90 seconds** for cold first-run on slow disks.

### Resource floor

- Disk: 2 GB for the bundle image; Postgres data grows with use; **plus model weights** which live in the Ollama volume, not here.
- RAM: 4 GB minimum, 8 GB recommended (the bundle itself is lean — RAM headroom is for Ollama on the host).
- CPU: 2 cores minimum, 4 recommended.

## 4. Container Spec — Ollama (host or sibling container)

Ollama is **out of band** for Fortemi. Two deployment modes:

### Mode A — Ollama on the host (recommended for desktop)

User installs Ollama via the Ollama installer; Fortemi reaches it via `host.docker.internal:11434`. BT6's bollard supervisor does not need to manage Ollama in this mode.

### Mode B — Ollama as a sibling container

```
Image: ollama/ollama:<tag>
Ports: 11434 (container) → bind to localhost
Volume: ollama-models → /root/.ollama (model weights, large)
```

`OLLAMA_BASE`/`OLLAMA_HOST` then point at the container's network alias.

### First-run model download

Fortemi does **not** today expose a typed `/api/init/progress` endpoint. BT6's stated plan — scrape Ollama's `/api/pull` SSE stream directly — is correct. Endpoint:

```
POST http://<ollama-host>:11434/api/pull
Body: {"model": "qwen3.5:9b", "stream": true}
```

Emits a stream of JSON events with `total`/`completed` byte counts. Aggregating that for the user-facing progress UI is the right approach until Fortemi grows a first-class init endpoint (filed as a Fortemi follow-up).

## 5. Startup Order

For BT6 `DockerSupervisor`:

1. **Ollama** (host mode: no-op; container mode: start + wait for `/api/version`)
2. **Pull `qwen3.5:9b`** (or whichever `OLLAMA_GEN_MODEL` BT6 chose) — surface progress to the user
3. **Pull `nomic-embed-text`** (embedding model, small, fast)
4. **Whisper / pyannote** (only if BT6 enables them; required only for audio-transcript features)
5. **Fortemi bundle** — start, then poll `GET http://<bound-host>:3000/health` until `200`
6. **Reveal WebView** — HotM (the consumer UI) connects to the Fortemi API at the bound port

No specific dependency ordering inside the bundle — `bundle-entrypoint.sh` and `supervisord` handle Postgres → matric-api → MCP sequencing internally.

## 6. Graceful Shutdown

Fortemi handles `SIGTERM` cleanly:

- `matric-api` uses axum's graceful-shutdown hook; the sqlx pool drains.
- Postgres receives `SIGINT` from `supervisord` (smart shutdown) and persists WAL.
- MCP server (`mcp-server`) exits cleanly on stdin EOF.

The 10-second `SIGTERM` window BT6 plans is **sufficient for typical workloads**. Under heavy load (many in-flight long-context generations or active migrations), allow up to **15 seconds** before escalating to `SIGKILL`. Document this knob in `DockerSupervisor` if it's exposed to BT6 operators.

## 7. Container Labels

No conflict with BT6's label scheme. Fortemi does not claim:

- `bt6-arsenal.managed=true`
- `bt6-arsenal.tool=fortemi`

Fortemi's own compose deployments use `autoheal=true` on Redis/sidecars; the bundle container deliberately omits this label because Postgres lives inside it and an autoheal restart could corrupt WAL.

## 8. Cross-Platform Validation

| Platform | Bundle (API+PG+MCP) | Whisper sidecar | pyannote sidecar | Ollama |
|---|---|---|---|---|
| Docker Desktop (macOS) | Validated | Validated (CPU) | Validated (CPU) | Host install recommended |
| Docker Desktop (Windows) | Validated | Validated (CPU) | Validated (CPU) | Host install recommended |
| Docker Engine (Linux) | Validated | Validated (CPU + GPU profiles) | Validated (CPU + GPU profiles) | Host or sibling container |

### Platform quirks BT6 should expect

- **GPU passthrough**: CPU-only works everywhere. NVIDIA GPU passthrough requires the NVIDIA Container Toolkit on Linux; Docker Desktop on Windows supports it via WSL2 + recent NVIDIA driver; macOS has no GPU passthrough path (CPU-only Whisper/pyannote on Mac).
- **Host networking on macOS**: `host.docker.internal` works natively. No special config.
- **Host networking on Linux**: BT6's bollard code must add `extra_hosts: ["host.docker.internal:host-gateway"]` when creating the container — Linux Docker does not auto-resolve this name.
- **File-path mounts on Windows**: Use forward slashes in the bind-source path passed to bollard (`/c/Users/...`), not backslashes.

## 9. Acceptance Mapping (from issue body)

| Question | Resolution |
|---|---|
| Canonical GHCR image | `ghcr.io/fortemi/fortemi:<release-tag>`; `ghcr.io/bt-6/*` is BT-6-internal only. §2 |
| Multi-container topology | One bundle container + optional sidecars. Three-container assumption is incorrect. §1, §3 |
| First-run Ollama download | Poll Ollama `/api/pull` SSE directly. No upstream Fortemi changes required. §4 |
| Image pinning | Release tags on this repo (CalVer). §2 |
| Cross-platform Docker | Validated on Docker Desktop (mac/win) and Linux Engine. §8 |
| Graceful shutdown | SIGTERM handled cleanly; 10s sufficient, 15s recommended for heavy load. §6 |
| Container labels | No conflict with `bt6-arsenal.*`. §7 |
| Resource footprint | 4 GB RAM min / 8 GB rec, 2 GB disk + model weights, 2–4 cores. §3 |

## 10. Follow-ups

The following Fortemi-side work is not blocking for BT6 Iter 2 but is on the roadmap:

- First-class `/api/init/progress` endpoint that proxies Ollama's pull stream and adds Fortemi-side init phases (DB migrations, embedding-model warmup). Until then, BT6 polls Ollama directly per §4.
- GPU-passthrough validation matrix for Docker Desktop on Windows + recent NVIDIA driver.

## References

- Compose contract: [`docker-compose.bundle.yml`](../../docker-compose.bundle.yml)
- Bundle image: [`Dockerfile.bundle`](../../Dockerfile.bundle)
- Entrypoint: [`docker/bundle-entrypoint.sh`](../../docker/bundle-entrypoint.sh)
- Process supervision: [`docker/supervisord.conf`](../../docker/supervisord.conf)
- Chat endpoint contract: [`docs/api/chat.md`](../api/chat.md) (or `crates/matric-api/src/handlers/chat.rs`)
- Companion issue: [Fortemi/fortemi#587](https://git.integrolabs.net/Fortemi/fortemi/issues/587)
- Downstream: [bt6/BT6-ARSENAL#23](https://git.integrolabs.net/bt6/BT6-ARSENAL/issues/23)

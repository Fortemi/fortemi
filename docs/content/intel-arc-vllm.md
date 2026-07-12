# Intel Arc / XPU Deployment with Host vLLM

Fortemi can run its Docker bundle on Intel Arc/XPU hosts by keeping inference
outside the Fortemi container and pointing Fortemi at a host vLLM
OpenAI-compatible endpoint.

This is the recommended Intel path because the default Docker bundle is tuned
for NVIDIA CUDA/EGL devices. Intel vLLM/PyTorch XPU stacks are usually installed
directly on the host, where the Intel compute runtime, Level Zero, and driver
versions are easiest to keep aligned.

## Topology

```text
Fortemi Docker bundle ── OpenAI API ──> host vLLM on Intel Arc/XPU
Fortemi Docker bundle ── embeddings ──> Ollama or another embedding provider
```

vLLM serves generation only. Keep `MATRIC_EMBEDDING_PROVIDER=ollama` unless you
have another embedding-capable provider configured.

## Requirements

- **Docker Compose v2.17.0 or newer.** The Intel overlay
  (`docker-compose.intel.yml`) uses the Compose `!reset` YAML tag to clear the
  bundle's NVIDIA device reservation. `!reset` was introduced in Docker Compose
  v2.17.0; older releases reject the overlay at render time. The overlay is
  validated in CI with a current Compose release on every push. Check your
  version with `docker compose version`.
- A host vLLM build with Intel XPU support (Intel compute runtime + Level Zero
  installed on the host).
- Host Ollama (or another embedding-capable provider) for the embedding route.

## 1. Start vLLM on the host

Install or activate a vLLM environment with XPU support, then start vLLM with a
served model name. Fortemi must use the served model name, not the filesystem
path.

```bash
source ~/ai/vllm-xpu/bin/activate

python -m vllm.entrypoints.openai.api_server \
  --host 0.0.0.0 \
  --port 8000 \
  --model ~/models/huggingface/Qwen--Qwen3.5-9B \
  --served-model-name qwen3.5:9b \
  --trust-remote-code \
  --max-model-len 8192 \
  --gpu-memory-utilization 0.85 \
  --dtype bfloat16 \
  --enforce-eager \
  --no-enable-log-requests
```

Validate the OpenAI-compatible endpoint:

```bash
curl http://127.0.0.1:8000/v1/models
```

If startup hangs during first-run torch compilation on Intel XPU, keep
`--enforce-eager`. After the cache/runtime is proven stable on your hardware,
you can experiment without it.

An example user systemd unit is provided in the repository at
`deploy/vllm-intel-xpu.service.example`.

## 2. Configure Fortemi

Create `.env` from `.env.example` and add the Intel/vLLM settings:

```dotenv
COMPOSE_PROFILES=edge

MATRIC_INFERENCE_DEFAULT=openai
OPENAI_BASE_URL=http://host.docker.internal:8000/v1
OPENAI_API_KEY=local-vllm
OPENAI_GEN_MODEL=qwen3.5:9b
MATRIC_FAST_GEN_MODEL=qwen3.5:9b

MATRIC_EMBEDDING_PROVIDER=ollama
OLLAMA_BASE=http://host.docker.internal:11434
OLLAMA_HOST=http://host.docker.internal:11434
OLLAMA_EMBED_MODEL=nomic-embed-text
OLLAMA_EMBED_DIM=768

# Intel profile: do not ask Docker for NVIDIA devices.
OPEN3D_CPU_RENDERING=true
NVIDIA_VISIBLE_DEVICES=
NVIDIA_DRIVER_CAPABILITIES=
```

`OPENAI_GEN_MODEL` must match vLLM's `--served-model-name` exactly — vLLM
rejects requests for model names it does not serve.

On Linux, `docker-compose.bundle.yml` already maps `host.docker.internal` to the
host gateway for the Fortemi container.

## 3. Start Fortemi with the Intel overlay

```bash
docker compose \
  -f docker-compose.bundle.yml \
  -f docker-compose.intel.yml \
  up -d
```

Check health:

```bash
curl http://127.0.0.1:3000/health
```

## Sidecars under COMPOSE_PROFILES=edge

The `.env` block above uses `COMPOSE_PROFILES=edge`, which is the bundle's
CPU-sidecar profile. On an Intel host it starts:

| Service | Profile | Purpose | Runs on |
|---------|---------|---------|---------|
| `whisper` (CPU) | `edge` | Audio transcription | CPU |
| `pyannote` (CPU) | `edge` | Speaker diarization | CPU |
| `gliner` | *(always)* | Zero-shot entity extraction | CPU |
| `open3d` | *(always)* | 3D model rendering (`OPEN3D_CPU_RENDERING=true`) | CPU |

The GPU Whisper/pyannote profiles (`gpu-12gb`, `gpu-24gb`) remain NVIDIA/CUDA
oriented — do not use them on Intel hosts unless you provide separate
Intel-compatible services and point `WHISPER_BASE_URL` / `DIARIZATION_BASE_URL`
at them.

### Disabling sidecars for generation-only deployments

If you only want host-vLLM generation and no CPU extraction sidecars:

- **Whisper + pyannote**: leave `COMPOSE_PROFILES` unset (or empty) so the
  `edge` profile containers never start, and set `WHISPER_BASE_URL=` and
  `DIARIZATION_BASE_URL=` (empty) in `.env` so the API disables transcription
  and diarization instead of probing dead endpoints.
- **GLiNER**: the `gliner` container is not behind a profile and starts with
  the bundle. Set `GLINER_BASE_URL=` (empty) in `.env` to disable NER in the
  extraction cascade; to avoid running the container at all, add
  `--scale gliner=0` to your `docker compose up` command.
- **Embeddings**: `MATRIC_EMBEDDING_PROVIDER` may point at any registered
  provider with the Embedding capability. Do not point embeddings at vLLM —
  typical vLLM generation deployments do not serve Fortemi's embedding route.

## Verifying routing

Confirm chat is routed to vLLM and embeddings to your embedding provider:

```bash
# Provider configuration with source attribution (default/env/db_override)
curl http://127.0.0.1:3000/api/v1/inference/config

# Provider catalog: server_configured + supports_embeddings per provider
curl http://127.0.0.1:3000/api/v1/inference/providers
```

In the config output, the default backend should be `openai` with
`base_url` pointing at your vLLM endpoint, and the embedding backend override
should name your embedding provider (`ollama` in the setup above). The
`/health` payload additionally reports extraction capabilities so you can
confirm which sidecars are active.

A scripted smoke test for this profile — using a stub OpenAI-compatible
endpoint, so no Intel hardware is required — ships at
`scripts/smoke-intel-vllm.sh`. Run it as part of release validation for
changes touching the Intel overlay or provider routing.

## Notes and tradeoffs

- The overlay clears the bundle's NVIDIA
  `deploy.resources.reservations.devices` block for the Fortemi service via the
  Compose `!reset` tag (see Requirements above for the minimum Compose
  version).
- vLLM model names matter: `OPENAI_GEN_MODEL` must match vLLM
  `--served-model-name`.
- If the host vLLM port is not `8000`, update `OPENAI_BASE_URL` accordingly.

## Related documentation

- [Inference Providers](#/core-systems-inference) — provider catalog and
  configuration
- [Hardware Planning](#/operations-hardware) — GPU sizing guidance, including
  Intel Arc
- [Configuration Reference](#/operations-configuration) — all environment
  variables

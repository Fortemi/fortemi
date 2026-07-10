# Intel Arc / XPU deployment with host vLLM

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

An example user systemd unit is provided at
[`deploy/vllm-intel-xpu.service.example`](../../deploy/vllm-intel-xpu.service.example).

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

The health payload should show chat configured with your vLLM model and
embeddings configured through your embedding provider.

## Notes and tradeoffs

- The overlay clears the bundle's NVIDIA `deploy.resources.reservations.devices`
  block for the Fortemi service.
- GPU Whisper/pyannote profiles remain NVIDIA/CUDA-oriented. Keep
  `COMPOSE_PROFILES=edge` on Intel unless you provide separate Intel-compatible
  extraction services and set `WHISPER_BASE_URL` / `DIARIZATION_BASE_URL`.
- vLLM model names matter: `OPENAI_GEN_MODEL` must match vLLM
  `--served-model-name`.
- If the host vLLM port is not `8000`, update `OPENAI_BASE_URL` accordingly.

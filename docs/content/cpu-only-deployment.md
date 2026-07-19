# CPU-Only Deployment Guide

This guide covers deploying and running Fortemi without a GPU. Every core feature works on CPU — some AI-powered features run slower, and a few require configuration changes or alternative services.

## What Works Without a GPU

Fortemi is designed with graceful degradation. The table below shows what each feature needs and how it behaves on CPU-only hardware.

### Fully CPU-Native (No Configuration Needed)

These features work identically on CPU and GPU hardware:

| Feature | Description |
|---------|-------------|
| **Full-text search** | PostgreSQL FTS with multilingual stemming, emoji/CJK support |
| **Semantic search** | Vector similarity via pgvector (search itself is CPU — embedding generation is the variable) |
| **Knowledge graph** | Automatic linking, PFNET sparsification, community detection, SNN scoring |
| **Collections & tags** | Hierarchy, SKOS semantic tagging, bulk operations |
| **Multi-memory archives** | Schema-isolated parallel knowledge bases with federated search |
| **Backup & restore** | Full database backup/restore with shard migration |
| **Real-time events** | SSE, WebSocket, webhooks |
| **File storage** | Content-addressable deduplication, BLAKE3 hashing, virus scan hooks |
| **Text extraction** | Plaintext, Markdown, HTML, JSON, YAML, TOML, CSV, XML |
| **PDF text extraction** | pdftotext-based extraction (bundled in Docker image) |
| **Code analysis** | Syntax-aware extraction for 30+ languages |
| **Authentication** | OAuth2, API keys, MCP integration |
| **Redis cache** | Search query caching |

### CPU-Capable (Slower Than GPU)

These features use LLM inference. They work on CPU via Ollama but are significantly slower:

| Feature | What It Does | GPU Latency | CPU Latency | Notes |
|---------|-------------|-------------|-------------|-------|
| **Embeddings** | Semantic vectors for search & linking | ~50ms | 2-5s | nomic-embed-text is small and CPU-friendly |
| **Title generation** | Auto-generates note titles | ~1-3s | 8-15s | Uses fast model |
| **AI revision** | Enhances note content with context | ~3-8s | 15-45s | Uses standard model |
| **Concept tagging (LLM)** | Extracts topics when GLiNER needs supplementing | ~2-5s | 8-20s | GLiNER handles most notes without LLM |
| **Context updates** | Enriches notes with related context | ~2-5s | 10-20s | Uses standard model |

### CPU-Only by Design

| Feature | Description |
|---------|-------------|
| **GLiNER NER** | 0.5B BERT model for named entity recognition. Runs exclusively on CPU. Sub-300ms per document, 100-200x faster than LLM-based NER. Enabled by default in the Docker bundle. |

### GPU-Dependent (Disabled or Degraded on CPU)

| Feature | Requires | CPU Alternative |
|---------|----------|-----------------|
| **Image description** | Ollama + vision model (qwen3.5:9b, natively multimodal) | No CPU vision models available. Disable or use cloud API. |
| **Audio transcription** | Whisper server | CPU variant available (INT8 quantized). Slower but functional. |
| **Video multimodal** | FFmpeg + Whisper + vision model | Partial: transcription works on CPU Whisper, but keyframe description needs vision model. |
| **3D model understanding** | Open3D GPU renderer + vision model | No CPU fallback. Disable on CPU-only deployments. |

## Quick Start: CPU-Only Docker Bundle

### Step 1: Install Ollama

```bash
curl -fsSL https://ollama.ai/install.sh | sh
```

Force CPU-only mode:

```bash
# Add to /etc/systemd/system/ollama.service.d/override.conf
# or export before running ollama serve
export OLLAMA_NUM_GPU=0
```

### Step 2: Pull Models

Start with the minimum set — you can add more later:

```bash
# Required: embeddings (0.5GB, very fast on CPU)
ollama pull nomic-embed-text

# Recommended: fast model for extraction pipeline (3B, good CPU performance)
ollama pull granite3.1-dense:8b

# Optional: standard model for AI revision and complex tasks
# Choose ONE based on your RAM:
ollama pull qwen2.5:7b     # 16GB+ RAM — best quality
# OR
ollama pull llama3.2:3b    # 8GB RAM — faster, lower quality
```

### Step 3: Create `.env`

```bash
cd /path/to/fortemi
cp .env.example .env
```

Edit `.env`:

```bash
# External URL (required for OAuth/MCP)
ISSUER_URL=https://your-domain.com

# ── CPU-Optimized Inference ────────────────────────────────
MATRIC_INFERENCE_DEFAULT=ollama
OLLAMA_BASE=http://host.docker.internal:11434

# Embedding model (fast on CPU, always needed)
OLLAMA_EMBED_MODEL=nomic-embed-text

# Generation model — pick one matching your RAM
OLLAMA_GEN_MODEL=qwen2.5:7b        # 16GB+ RAM
# OLLAMA_GEN_MODEL=llama3.2:3b     # 8GB RAM

# Fast model for extraction pipeline
MATRIC_FAST_GEN_MODEL=granite3.1-dense:8b

# ── Disable GPU-Dependent Services ─────────────────────────
# No vision model on CPU (no viable CPU vision models exist)
OLLAMA_VISION_MODEL=

# Disable 3D renderer (requires GPU)
RENDERER_URL=

# ── Increase Timeouts for CPU ──────────────────────────────
# CPU inference is 5-10x slower than GPU — adjust accordingly
JOB_TIMEOUT_SECS=1800
MATRIC_GEN_TIMEOUT_SECS=300
MATRIC_FAST_GEN_TIMEOUT_SECS=120
MATRIC_EMBED_TIMEOUT_SECS=60
```

### Step 4: Start the Bundle

**Without audio transcription** (simplest):

```bash
docker compose -f docker-compose.bundle.yml up -d
```

The default `whisper` service requires an NVIDIA GPU. Without one, it will fail to start — but `required: false` in the compose file means Fortemi starts anyway with audio transcription disabled.

**With CPU audio transcription**:

```bash
docker compose -f docker-compose.bundle.yml --profile whisper-cpu up -d
```

This starts the INT8-quantized Whisper variant. First startup downloads the model (~1.5GB) and takes a few minutes.

### Step 5: Verify

```bash
curl -s http://localhost:3000/health | python3 -m json.tool
```

Check the `capabilities` section:

```json
{
  "status": "healthy",
  "capabilities": {
    "extraction_strategies": [
      "text_native",
      "pdf_text",
      "code_ast",
      "structured_extract",
      "audio_transcribe"
    ],
    "vision": false,
    "audio_transcription": true,
    "ner": true
  }
}
```

- `vision: false` — expected on CPU (no vision model)
- `audio_transcription: true` — if you used `--profile whisper-cpu`
- `ner: true` — GLiNER is CPU-native, always available

## Ollama CPU Optimization

Ollama runs on CPU by default when no GPU is detected. These settings improve CPU performance:

```bash
# /etc/systemd/system/ollama.service.d/override.conf
[Service]
Environment="OLLAMA_NUM_GPU=0"
Environment="OLLAMA_NUM_PARALLEL=1"
Environment="OLLAMA_FLASH_ATTENTION=1"
Environment="OLLAMA_KEEP_ALIVE=10m"
```

Then reload:

```bash
sudo systemctl daemon-reload
sudo systemctl restart ollama
```

### Key Settings

| Variable | Recommended | Why |
|----------|-------------|-----|
| `OLLAMA_NUM_GPU=0` | Required | Forces CPU inference (prevents CUDA errors on non-GPU systems) |
| `OLLAMA_NUM_PARALLEL=1` | Recommended | One request at a time — CPU can't efficiently parallelize model inference |
| `OLLAMA_FLASH_ATTENTION=1` | Recommended | Flash attention is supported on CPU and reduces memory usage |
| `OLLAMA_KEEP_ALIVE=10m` | Recommended | Keeps model loaded in RAM between requests. Set longer if you have spare RAM. |

### Fortemi Worker Concurrency

Since CPU inference serializes through `OLLAMA_NUM_PARALLEL=1`, reduce Fortemi's concurrent jobs to avoid queueing bottlenecks:

```bash
# In .env
JOB_MAX_CONCURRENT=2
```

This lets one job run inference while another does CPU-bound work (text extraction, PDF parsing, etc.).

## Model Selection for CPU

### Embedding Models

Embedding models are small and run well on CPU. Stick with the default:

| Model | Size | CPU Latency (500 tokens) | Quality |
|-------|------|--------------------------|---------|
| `nomic-embed-text` | 0.5GB | 2-5s | 85% |
| `mxbai-embed-large` | 1.0GB | 4-8s | 88% |

`nomic-embed-text` is the best balance of speed and quality on CPU.

### Generation Models

Pick based on available RAM (model + OS + PostgreSQL + Fortemi overhead):

| RAM | Model | Size | CPU Latency | Quality | Context |
|-----|-------|------|-------------|---------|---------|
| **8GB** | `llama3.2:3b` | 3.2GB | 1-3s | 78% | 128K |
| **8GB** | `phi4-mini:3.8b` | 3.8GB | 2-4s | 80% | 128K |
| **16GB** | `qwen2.5:7b` | 7.6GB | 5-10s | 89% | 128K |
| **16GB** | `mistral:7b` | 7.2GB | 4-8s | 86% | 32K |
| **32GB** | `llama3.1:8b` | 8.0GB | 5-10s | 87% | 128K |

Latency estimates assume 8-core x86 CPU. ARM (Apple Silicon) is typically 2-3x faster for the same model.

### Fast Model (Extraction Pipeline)

The fast model handles concept tagging, reference extraction, and title generation. It processes most notes and only escalates to the standard model when needed. Choose a small, fast model:

| Model | Size | CPU tok/s | Best For |
|-------|------|-----------|----------|
| `granite3.1-dense:8b` | 8.0GB | ~10-15 | Best JSON adherence for extraction tasks |
| `qwen2.5:3b` | 3.2GB | ~20-30 | Fastest, good for high-volume note taking |
| `llama3.2:3b` | 3.2GB | ~20-30 | Good general purpose |

Set in `.env`:

```bash
MATRIC_FAST_GEN_MODEL=granite3.1-dense:8b
```

Set to empty to disable the fast model entirely (extraction falls through to standard model for everything):

```bash
MATRIC_FAST_GEN_MODEL=
```

## Cloud API as GPU Alternative

If you need GPU-quality results without GPU hardware, use a cloud inference provider. Fortemi supports OpenAI-compatible APIs and OpenRouter (100+ models).

### Option A: OpenAI

```bash
# In .env
MATRIC_INFERENCE_DEFAULT=openai
OPENAI_API_KEY=<OPENAI_API_KEY>
OPENAI_EMBED_MODEL=text-embedding-3-small
OPENAI_GEN_MODEL=gpt-4o-mini
OPENAI_EMBED_DIM=1536
```

**Cost estimate**: ~$0.01-0.05 per note (embedding + generation). A 10,000-note knowledge base costs roughly $100-500 for initial processing, then pennies per day for incremental notes.

### Option B: OpenRouter

Access to 100+ models (Llama, Mistral, Claude, GPT) through a single API:

```bash
# In .env
OPENROUTER_API_KEY=<OPENROUTER_API_KEY>
```

OpenRouter is configured as an additional provider — you can use it alongside local Ollama.

### Option C: Hybrid (Recommended for CPU Deployments)

Run embeddings locally (fast, cheap, private) and generation via cloud (quality):

```bash
# Local embeddings via Ollama (CPU is fine for small embedding models)
MATRIC_INFERENCE_DEFAULT=ollama
OLLAMA_EMBED_MODEL=nomic-embed-text

# Cloud generation via OpenAI for quality-sensitive tasks
OPENAI_API_KEY=<OPENAI_API_KEY>
OPENAI_GEN_MODEL=gpt-4o-mini
```

Configure the inference routing in `inference.toml`:

```toml
[default]
provider = "ollama"

[providers.ollama]
base_url = "http://localhost:11434"
embedding_model = "nomic-embed-text"

[providers.openai]
api_key_env = "OPENAI_API_KEY"
generation_model = "gpt-4o-mini"

[routing]
embedding = "ollama"
generation = "openai"
```

This keeps your note content private (embeddings stay local) while leveraging cloud quality for generation tasks.

## Audio Transcription on CPU

The Docker bundle includes a CPU Whisper variant. It uses INT8 quantization and runs about 3-5x slower than GPU but produces identical accuracy.

### Enable CPU Whisper

```bash
docker compose -f docker-compose.bundle.yml --profile whisper-cpu up -d
```

### Performance Expectations

| Audio Length | GPU (RTX 3090) | CPU (8-core) | CPU (16-core) |
|-------------|----------------|--------------|---------------|
| 1 minute | ~3s | ~15s | ~10s |
| 10 minutes | ~20s | ~2.5min | ~1.5min |
| 1 hour | ~2min | ~15min | ~10min |

### Whisper Model Selection

The default model (`Systran/faster-distil-whisper-large-v3`) works on CPU. For faster CPU transcription at slight quality cost:

```bash
# In .env — smaller model, faster on CPU
WHISPER_MODEL=Systran/faster-distil-whisper-small.en
```

### Disable Audio Transcription

If you don't need audio/video transcription:

```bash
# In .env
WHISPER_BASE_URL=
```

## What You Lose Without a GPU

Be explicit about what's unavailable so you can plan accordingly:

### Image Description (Vision)

**Status**: Not available on CPU.

Vision models (qwen3.5:9b, llava) require GPU for practical inference speeds. On CPU, a single image description would take 30-60+ seconds, making it impractical.

**Impact**: Images uploaded as attachments are stored and served correctly, but won't get AI-generated descriptions. The `ai_description` field will be empty. Full-text search won't find images by their visual content.

**Workaround**: Use meaningful filenames and add manual descriptions in the note content. Tag images with relevant concepts for discoverability.

### Video Multimodal Extraction

**Status**: Partially available.

- Audio extraction and transcription: Works with CPU Whisper
- Keyframe extraction: Works (FFmpeg is CPU-native)
- Keyframe description: Not available (requires vision model)

**Impact**: Video attachments get audio transcription but no visual scene descriptions. The extracted text contains the transcript but not frame-by-frame visual context.

**Workaround**: The transcript alone is often sufficient for searchability. Add manual scene notes if visual content is important.

### 3D Model Understanding

**Status**: Not available on CPU.

The Open3D renderer requires GPU for multi-view rendering, and the vision model needs GPU for describing the rendered views.

**Impact**: GLB/GLTF/STL files are stored but not analyzed. No AI description is generated.

**Workaround**: Add manual descriptions when uploading 3D models.

## Hardware Recommendations

### Minimum (Personal Use)

- **CPU**: 4 cores / 8 threads
- **RAM**: 8GB (tight — use 3B models only)
- **Storage**: 20GB SSD for system + data
- **OS**: Linux (Ubuntu 22.04+, Debian 12+)

```bash
OLLAMA_GEN_MODEL=llama3.2:3b
MATRIC_FAST_GEN_MODEL=llama3.2:3b
OLLAMA_VISION_MODEL=
WHISPER_BASE_URL=
MAX_MEMORIES=5
```

### Recommended (Team Use)

- **CPU**: 8 cores / 16 threads
- **RAM**: 16-32GB
- **Storage**: 100GB SSD
- **OS**: Linux

```bash
OLLAMA_GEN_MODEL=qwen2.5:7b
MATRIC_FAST_GEN_MODEL=granite3.1-dense:8b
WHISPER_BASE_URL=http://whisper:8000   # with --profile whisper-cpu
OLLAMA_VISION_MODEL=
MAX_MEMORIES=20
```

### Recommended (Heavy Use, Cloud Hybrid)

- **CPU**: 8+ cores
- **RAM**: 16GB (less needed since generation is cloud)
- **Storage**: 100GB+ SSD

```bash
# Local embeddings only — generation via cloud
OLLAMA_EMBED_MODEL=nomic-embed-text
MATRIC_INFERENCE_DEFAULT=ollama
OPENAI_API_KEY=<OPENAI_API_KEY>
OPENAI_GEN_MODEL=gpt-4o-mini
WHISPER_BASE_URL=http://whisper:8000   # with --profile whisper-cpu
MAX_MEMORIES=50
```

## Troubleshooting

### "Connection refused" from Ollama

Ollama isn't running or isn't accessible from Docker:

```bash
# Check Ollama is running
curl http://localhost:11434/api/tags

# From inside Docker, it's accessed via host.docker.internal
# Verify with:
docker exec fortemi-matric-1 curl http://host.docker.internal:11434/api/tags
```

On Linux, ensure `extra_hosts` is set in compose (it is by default):

```yaml
extra_hosts:
  - "host.docker.internal:host-gateway"
```

The mapping supplies a route but does not configure the host listener. Bind
Ollama to Docker's resolved host-gateway address only, following
[Ollama Connectivity](#/operations-ollama-connectivity).

### Jobs timing out

CPU inference is slower. Increase timeouts:

```bash
JOB_TIMEOUT_SECS=1800           # 30 min
MATRIC_GEN_TIMEOUT_SECS=300     # 5 min
MATRIC_FAST_GEN_TIMEOUT_SECS=120  # 2 min
MATRIC_EMBED_TIMEOUT_SECS=60    # 1 min
```

### High memory usage

Ollama loads models into RAM. With `OLLAMA_KEEP_ALIVE=10m`, the model stays resident. If RAM is tight:

```bash
# Shorter keep-alive (model unloads faster)
OLLAMA_KEEP_ALIVE=2m

# Or use a smaller model
OLLAMA_GEN_MODEL=llama3.2:3b
```

### Slow first request after idle

The first inference request after model unload triggers a reload (5-15 seconds for 7B models on CPU). This is normal. Set `OLLAMA_KEEP_ALIVE` longer if this is disruptive.

### Whisper CPU out of memory

The default Whisper model uses ~4GB RAM. If you're constrained:

```bash
WHISPER_MODEL=Systran/faster-distil-whisper-small.en
```

This uses ~1GB and is faster, but English-only and slightly lower accuracy.

## Feature Matrix Summary

| Feature | No Config | CPU Ollama | CPU Whisper | Cloud API |
|---------|-----------|------------|-------------|-----------|
| Full-text search | Yes | | | |
| Semantic search | | Yes | | Yes |
| Auto-linking | | Yes | | Yes |
| Title generation | | Yes | | Yes |
| AI revision | | Yes | | Yes |
| Concept tagging (GLiNER) | Yes | | | |
| Concept tagging (LLM) | | Yes | | Yes |
| Graph maintenance | Yes | | | |
| Audio transcription | | | Yes | |
| Image description | | | | Yes* |
| Video transcription | | | Yes | |
| Video scene description | | | | Yes* |
| 3D model analysis | | | | No** |
| PDF text extraction | Yes | | | |
| Code analysis | Yes | | | |

\* Cloud API supports vision via OpenAI `gpt-4o` — requires custom routing configuration.
\** 3D model rendering requires GPU hardware regardless of inference provider.

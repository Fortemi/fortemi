# Fortémi v2026.4.0 Release

*Released: 2026-04-03*

## What's New

### Synchronous Chat API

A first-class chat endpoint is now available at `POST /api/v1/chat`. It provides direct LLM conversation with multi-turn history, model selection, and a GPU concurrency semaphore so chat never starves background jobs. `GET /api/v1/chat/models` lists all available models with metadata. The health endpoint exposes `capabilities.chat` so clients can detect availability before attempting a request.

### Chunked Audio Transcription

Long audio files are now automatically split into parallel chunks via fan-out `AudioChunkTranscription` jobs, with atomic dual fan-in for video assembly. This removes the previous wall-clock ceiling on transcription and enables reliable processing of feature-length recordings.

### Content-Type-Aware AI Revision

The revision pipeline is now document-type-aware. Meeting notes get Decisions and Action Items sections; movies get Synopsis and Cast. Chunking is adaptive per document type, and revision budgets scale with chunk count (10 chunks × 60s = 600s). Chunk size and overlap are user-configurable per request via `chunk_max_chars` and `chunk_overlap`.

### Decomposed 3D Model Extraction

3D model processing now uses atomic per-view vision jobs with `RENDER_GPU` tier scheduling, Open3D EGL GPU support, and AI revision after view assembly. Each view can be checkpointed, retried, and processed independently.

### Video Keyframe Vision Improvements

Two new job types — `KeyframeCharacterVision` and `KeyframeSettingVision` — improve scene-dialog interleaving and keyframe merging prompts for richer video understanding output.

### MMR Diversity Search and Access Analytics

Search results can now use Maximal Marginal Relevance (MMR) to reduce redundancy and surface more diverse results. Access frequency tracking and cold-spot detection provide insight into which notes are being used and which are drifting out of reach. A new `agent-reflection` document type supports AI self-reflection notes.

### Inference Runtime Config API and llama.cpp Provider

`GET/PUT /api/v1/config/inference` lets you change inference settings at runtime without restarting the server. Every PUT rebuilds the full provider registry immediately.

llama.cpp is now a first-class provider. Set `LLAMACPP_BASE_URL` and route jobs to it using provider-qualified slugs (`llamacpp:model-name`). It uses the existing `OpenAIBackend` under the hood — no new backend code. `response_format` (JSON mode) is also now supported on both OpenAI and llama.cpp backends.

### Inference Resilience

Inference requests now retry with exponential backoff and a circuit breaker for fail-fast detection. Memory limits are enforced for Whisper, pyannote, and GLiNER sidecar services to prevent OOM cascades.

### Edge-First Hardware Profiles

`COMPOSE_PROFILES` selects your deployment tier:

| Profile | GPU VRAM | Audio/Diarization | Default Gen Model |
|---------|----------|-------------------|-------------------|
| *(default / edge)* | 6-8GB | CPU | qwen3.5:9b |
| `gpu-12gb` | 12-16GB | GPU | qwen3.5:9b |
| `gpu-24gb` | 24GB+ | GPU | configurable |

This makes Fortémi usable on consumer hardware (RTX 3060 8GB, 4060, 5060) without sacrificing GPU VRAM to CPU-side sidecar services.

### Qwen3.5 as Default Model Family

`qwen3.5:9b` is now the default generation model. It supports 262K context, is natively multimodal (unified generation and vision in one model), and fits in ~6.5GB VRAM — a single load serves generation, fast extraction, and vision tasks. 24GB+ deployments can switch to `qwen3.5:27b` via `OLLAMA_GEN_MODEL`.

### Installer Scripts

`installer/scripts/` provides 8 guided deployment scripts: `clone.sh`, `configure.sh`, `deploy.sh`, `pull-models.sh`, `check-ports.sh`, `setup-nvidia.sh`, `verify.sh`, and `reset.sh`. A `setup.manifest.yaml` machine-readable manifest supports the AIWG installer framework.

## Breaking Changes

**GPU sidecar defaults changed.** Whisper and pyannote now run on CPU by default to preserve GPU VRAM for inference. If your existing deployment runs GPU-accelerated sidecars, add `COMPOSE_PROFILES=gpu-12gb` (or `gpu-24gb`) to your `.env` before upgrading.

## Bug Fixes

- **AI revision on non-default archives** — `reprocess_note` was missing the archive schema in the job payload, causing "Failed to fetch note" on non-default memory archives.
- **Video revision budget** — Revision budget now sums per-chunk adaptive timeouts; the loop exits cleanly before deadline rather than timing out mid-chunk. `REVISION_VIDEO_CHUNK_SIZE_MAX` reduced from 60K to 20K chars; `JOB_TIMEOUT_SECS` default raised to 1800s.
- **3D rendering** — Normalize extreme-scale models to prevent blank renders; validate render quality to prevent grey thumbnails; fix thumbnail MIME type.
- **Inference** — Disable thinking mode for Qwen3.5 generation (prevents empty responses from thinking models).
- **TUS uploads** — Add GET handler for upload finalization and `DefaultBodyLimit` on TUS routes.
- **Migrations** — snake_case enum values for `job_type`; correct singular `document_type` table name.
- **Archives** — Sync column drift in archive schemas on auto-migration; exclude identity columns from archive clone.
- **Docker** — Add NVIDIA EGL ICD for GPU rendering; nvidia default runtime migration.
- **GLiNER OOM** — Default memory limit raised from 2GB to 4GB.
- **Security** — Patched vulnerabilities in npm and Rust dependencies.

## Upgrade Notes

```bash
# Pull latest images
docker compose -f docker-compose.bundle.yml pull

# If you use GPU sidecars, add to .env before restarting:
# COMPOSE_PROFILES=gpu-12gb   (or gpu-24gb)

# Restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

Migrations run automatically on startup. No manual database changes required.

## Full Changelog

See [CHANGELOG.md](../../CHANGELOG.md) for the complete list of changes.

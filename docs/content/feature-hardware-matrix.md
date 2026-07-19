# Fortemi Feature & Hardware Matrix

Complete inventory of all operations, their external dependencies, and hardware requirements.

---

## Hardware Profiles

Three Docker Compose profiles map to GPU VRAM tiers:

| Profile | GPU VRAM | Sidecars | Gen Model | Example GPUs |
|---------|----------|----------|-----------|--------------|
| **edge** (default) | 6-8 GB | CPU (Whisper, pyannote) | qwen3.5:9b | RTX 3060 8GB, 4060, 5060 |
| **gpu-12gb** | 12-16 GB | GPU (Whisper, pyannote) | qwen3.5:9b | RTX 3060 12GB, 4070, 5070 |
| **gpu-24gb** | 24 GB+ | GPU (Whisper, pyannote) | configurable | RTX 3090, 4090, 5090 |

### System Requirements by Tier

| Tier | GPU VRAM | CPU | RAM | Storage | Notes |
|------|----------|-----|-----|---------|-------|
| Budget | 4-6 GB | 4 cores | 8 GB | 50 GB SSD | CPU sidecars, 7B quantized models |
| Mainstream | 12-16 GB | 8 cores | 16 GB | 100 GB NVMe | GPU sidecars, 7B-14B models |
| Performance | 24 GB | 16+ cores | 32 GB | 500 GB NVMe | GPU sidecars, 14B-32B models |
| Professional | 48 GB+ | 16+ cores | 32 GB+ | 500 GB+ NVMe | 70B+ models |
| Cloud-only | None | Any | Any | Any | OpenAI/OpenRouter, no local GPU |

---

## Core Services (Always Running)

| Service | Container | GPU | Memory | Port | Notes |
|---------|-----------|-----|--------|------|-------|
| **API + MCP** | matric | Optional | Unlimited | 3000, 3001 | Axum HTTP + MCP server |
| **PostgreSQL 18** | Embedded in matric | No | — | 5432 (internal) | pgvector + PostGIS + pg_trgm |
| **Redis** | redis | No | 256 MB | — | Query result cache |

## Sidecar Services (Optional)

| Service | Container | Profile | GPU | Memory | Port | Disable Via |
|---------|-----------|---------|-----|--------|------|-------------|
| **Autoheal** | autoheal | ops-autoheal | No | — | — | Omit `ops-autoheal` (default); enabling grants root-equivalent host control through the Docker socket |
| **Whisper (CPU)** | whisper | edge | No | 4 GB | 8000 | `WHISPER_BASE_URL=` |
| **Whisper (GPU)** | whisper-gpu | gpu-12gb, gpu-24gb | Yes (all) | 8 GB | 8000 | `WHISPER_BASE_URL=` |
| **pyannote (CPU)** | pyannote | edge | No | 4 GB | 8001 | `DIARIZATION_BASE_URL=` |
| **pyannote (GPU)** | pyannote-gpu | gpu-12gb, gpu-24gb | Yes (all) | 6 GB | 8001 | `DIARIZATION_BASE_URL=` |
| **GLiNER NER** | gliner | all | No | 4 GB | 8090 | `GLINER_BASE_URL=` |
| **Open3D Renderer** | Embedded in matric | all | Yes (EGL) | — | 8080 | CPU fallback auto |

---

## Inference Providers

| Provider | Type | Config | GPU | Required? |
|----------|------|--------|-----|-----------|
| **Ollama** | Local (default) | `OLLAMA_BASE` | Recommended | Optional (default) |
| **llama.cpp** | Local HTTP | `LLAMACPP_BASE_URL` | Recommended | Optional (opt-in) |
| **OpenAI** | Cloud API | `OPENAI_API_KEY` | No | Optional |
| **OpenRouter** | Cloud gateway | `OPENROUTER_API_KEY` | No | Optional |

Provider-qualified model slugs route requests: `ollama:qwen3.5:9b`, `openai:gpt-4o`, `openrouter:anthropic/claude-sonnet-4-20250514`, `llamacpp:my-model`.

---

## Feature Matrix

### Legend

| Symbol | Meaning |
|--------|---------|
| **Required** | Feature cannot work without this |
| **Recommended** | Works without, but significantly degraded |
| **Optional** | Enhances feature when available |
| **N/A** | Not applicable |
| CPU | Runs on CPU, no GPU needed |
| GPU | Needs GPU (or cloud API equivalent) |

### Knowledge Management (Core)

| Feature | GPU | External Service | External Tool | Hardware Notes |
|---------|-----|------------------|---------------|----------------|
| Note CRUD | N/A | None | None | CPU only, PostgreSQL |
| Full-text search (FTS) | N/A | None | None | CPU only, PostgreSQL FTS |
| Multilingual FTS | N/A | None | None | CPU only; `FTS_SCRIPT_DETECTION=true` |
| Emoji/symbol search | N/A | None | None | CPU only; pg_trgm; `FTS_TRIGRAM_FALLBACK=true` |
| CJK search | N/A | None | None | CPU only; pg_bigm; `FTS_BIGRAM_CJK=true` |
| Tag management (SKOS) | N/A | None | None | CPU only |
| Collections/folders | N/A | None | None | CPU only |
| Note templates | N/A | None | None | CPU only |
| Graph exploration | N/A | None | None | CPU only, recursive CTE |
| Export (Markdown + YAML) | N/A | None | None | CPU only |
| PKE encryption | N/A | None | None | CPU only |
| Event streaming (SSE/WS) | N/A | None | None | CPU only |
| OAuth2 / API keys | N/A | None | None | CPU only |
| Multi-memory archives | N/A | None | None | CPU only; scale `MAX_MEMORIES` with RAM |
| Federated cross-archive search | N/A | None | None | CPU only |
| TUS resumable uploads | N/A | None | None | CPU only |
| HTTP Range requests | N/A | None | None | CPU only |

### AI-Powered Features

| Feature | GPU | External Service | External Tool | Hardware Notes |
|---------|-----|------------------|---------------|----------------|
| Semantic search (vector) | Recommended | Ollama (embeddings) | None | pgvector search is CPU; embedding generation benefits from GPU |
| Automatic semantic linking | Recommended | Ollama (embeddings) | None | >70% similarity threshold |
| Embedding sets (filter/full) | Recommended | Ollama (embeddings) | None | MRL for 12x storage savings |
| AI revision | Recommended | Ollama or OpenAI | None | Type-aware; 5-10x slower on CPU |
| Title generation | Recommended | Ollama or OpenAI | None | 5-10x slower on CPU |
| Concept extraction (GLiNER) | CPU | GLiNER sidecar | None | CPU-native, ~300ms/doc; tier-0 in cascade |
| Concept extraction (LLM) | Recommended | Ollama | None | Tier-1/2 escalation if GLiNER < target |
| Synchronous chat | Recommended | Ollama or OpenAI | None | `CHAT_MAX_CONCURRENT` gates GPU access |
| Graph maintenance pipeline | Recommended | Ollama (embeddings) | None | SNN scoring, PFNET sparsification |
| Louvain community detection | N/A | None | None | CPU only, SKOS-derived labels |
| Knowledge health dashboard | N/A | None | None | CPU only |

### Extraction Pipeline

| Feature | GPU | External Service | External Tool | Hardware Notes |
|---------|-----|------------------|---------------|----------------|
| Plain text extraction | N/A | None | None | Pure Rust |
| PDF text extraction | N/A | None | None | Pure Rust (pdfium-render) |
| PDF OCR (scanned docs) | N/A | None | `pdftoppm`, `tesseract` | CPU only; triggered when text <50 chars |
| Office documents (DOCX, PPTX, etc.) | N/A | None | `pandoc` | CPU only; 120s timeout |
| Email extraction (.eml, .mbox) | N/A | None | None | Pure Rust (mailparse) |
| Spreadsheet extraction (.xlsx, .xls, .ods) | N/A | None | None | Pure Rust (calamine) |
| Archive extraction (.zip, .tar, .gz) | N/A | None | None | Pure Rust; configurable limits via env vars |
| Code AST parsing | N/A | None | None | Pure Rust |

### Media Processing

| Feature | GPU | External Service | External Tool | Hardware Notes |
|---------|-----|------------------|---------------|----------------|
| Audio transcription | Recommended | Whisper sidecar | `ffmpeg` | CPU variant available (INT8); GPU 5-10x faster |
| Speaker diarization | Recommended | pyannote sidecar | `ffmpeg` | CPU variant available; GPU recommended |
| Image description (vision) | **Required** | Ollama (vision model) | None | No viable CPU vision models |
| Video multimodal extraction | **Required** | Whisper + Ollama vision | `ffmpeg` | Transcription CPU-ok, keyframes need GPU |
| Video keyframe extraction | N/A | None | `ffmpeg` | CPU only (ffmpeg scene detect) |
| Media optimization (faststart) | N/A | None | `ffmpeg`, `ffprobe` | CPU only; copy-only remux |
| Audio extraction from video | N/A | None | `ffmpeg` | CPU only |
| 720p preview generation | N/A | None | `ffmpeg` | CPU only; transcode for files >100MB |
| Thumbnail sprite sheets | N/A | None | `ffmpeg` | CPU only; WebVTT maps |
| 3D model rendering | **Required** | Open3D renderer | None | GPU (EGL); CPU fallback (slow, test only) |
| 3D model understanding | **Required** | Open3D + Ollama vision | None | Multi-view render + vision description |

### Temporal-Spatial Features

| Feature | GPU | External Service | External Tool | Hardware Notes |
|---------|-----|------------------|---------------|----------------|
| Location-based search | N/A | None | None | PostGIS; CPU only |
| Time range queries | N/A | None | None | CPU only |
| W3C PROV provenance | N/A | None | None | CPU only |

---

## External Tool Dependencies

| Tool | Used By | Bundled in Docker? | Required? |
|------|---------|-------------------|-----------|
| `ffmpeg` | Audio transcription, video extraction, media optimization | Yes | Optional (media features) |
| `ffprobe` | Media metadata probing | Yes (with ffmpeg) | Optional |
| `pandoc` | Office document conversion | Yes | Optional (office docs) |
| `pdftoppm` | PDF page rendering for OCR | Yes (poppler-utils) | Optional (scanned PDFs) |
| `tesseract` | OCR text extraction | Yes | Optional (scanned PDFs) |

All tools are pre-installed in `Dockerfile.bundle`. Standalone deployments must install separately.

---

## CPU-Only Deployment Summary

All core knowledge management features work without GPU. AI features degrade gracefully:

| Category | CPU-Only Status |
|----------|----------------|
| Search, tags, collections, graph | Full functionality |
| Embeddings / semantic search | Works, generation 5-10x slower |
| AI revision / title generation | Works, 5-10x slower |
| Concept extraction (GLiNER) | Full speed (CPU-native) |
| Audio transcription | Works (INT8 Whisper), slower |
| Speaker diarization | Works (CPU pyannote), slower |
| Image/video vision | **Not available** (no CPU vision models) |
| 3D model rendering | **Not available** (EGL requires GPU) |
| PDF/Office/Email/Spreadsheet/Archive | Full functionality |
| Media optimization (ffmpeg) | Full functionality |

Recommended CPU-only hardware: 8-16 cores, 16-32 GB RAM, SSD storage.

---

## Memory Scaling

| MAX_MEMORIES | Recommended RAM | Recommended Disk |
|--------------|-----------------|------------------|
| 10 (default) | 8 GB | 50 GB |
| 50 | 16 GB | 100 GB |
| 200 | 32 GB | 500 GB |
| 500 | 64 GB+ | 1 TB+ |

Each memory archive is a separate PostgreSQL schema with 41 tables.

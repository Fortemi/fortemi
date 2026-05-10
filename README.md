<div align="center">

# Fortémi

*Pronounced: for-TAY-mee*

**Memory that understands.**

An intelligent knowledge base that comprehends what you store — the meaning behind your notes, the relationships between ideas, and the context that connects them. Semantic search, automatic knowledge graphs, multimodal media processing, and 43 MCP agent tools. Built in Rust. Runs on a single GPU.

```bash
docker compose -f docker-compose.bundle.yml up -d
```

[![License](https://img.shields.io/badge/license-BSL--1.1-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021_edition-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-18-336791?style=flat-square&logo=postgresql)](https://www.postgresql.org)
[![MCP](https://img.shields.io/badge/MCP-43_tools-purple?style=flat-square)](#mcp-server)
[![Docker](https://img.shields.io/badge/Docker-Bundle-2496ED?style=flat-square&logo=docker)](#quick-start)

[**Get Started**](#quick-start) · [**Features**](#features) · [**Architecture**](#architecture) · [**MCP Server**](#mcp-server) · [**API**](#api-endpoints) · [**Documentation**](#documentation) · [**🖥️ Desktop App (HotM)**](https://git.integrolabs.net/Fortemi/HotM/releases/latest)

</div>

---

## 🖥️ Looking for a desktop app, not just an API?

**You're probably here because you want to use this — install [HotM](https://git.integrolabs.net/Fortemi/HotM/releases/latest) instead.**

| If you want… | Use | Install |
|---|---|---|
| **A native desktop app** with editor, knowledge graph, capture, search, attachments — no Docker, no Postgres setup, no backend ops | **HotM** (`.deb` / `.msi` / `.dmg` / `.AppImage`) — UI + Fortemi API bundled in one package | [Download HotM](https://git.integrolabs.net/Fortemi/HotM/releases/latest) → run [`setup-linux.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-linux.sh) or [`setup-macos.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-macos.sh) → install the bundle |
| **A headless server** for agents over MCP, custom UIs, multi-user deployments, or air-gapped backends | **Fortemi** (this repo) — Docker bundle | `docker compose -f docker-compose.bundle.yml up -d` (see [Quick Start](#quick-start) below) |

HotM ships the same `matric-api` from this repo as a bundled sidecar, so the two stay in lockstep on features. **Single user with a laptop?** HotM is the right answer. **Team, fleet of agents, or backend service?** Stay here.

---

## What Fortémi Is

Fortémi is a self-hosted knowledge base that goes beyond storage. Most systems hold your data and wait for exact queries. Fortémi actively understands content: it finds conceptually relevant results even when you can't remember the right terminology, automatically discovers how new knowledge connects to everything else, and extracts searchable intelligence from images, audio, video, 3D models, emails, and spreadsheets.

If you've ever wished your notes could talk back — surfacing forgotten connections, answering questions from accumulated knowledge, and growing smarter with every piece of information — Fortémi is that system.

Built for privacy-first, edge-first deployment. No cloud dependency. Runs on commodity hardware with 8GB GPU VRAM. ~160k lines of Rust + 18k lines of MCP server (Node.js).

---

## What Problems Does Fortémi Solve?

### 1. Search That Misses the Point

Traditional search requires you to guess the right keywords. If you stored a note about "retrieval-augmented generation" but search for "using AI to answer questions from documents," you get nothing.

**Without Fortémi**: Keyword-only search. You find things only when you remember exactly how you phrased them.

**With Fortémi**: Hybrid retrieval fuses BM25 full-text search with dense vector similarity and Reciprocal Rank Fusion (Cormack et al., 2009). Semantic search finds conceptually related content regardless of terminology. Multilingual support covers English, German, French, Spanish, Portuguese, Russian, CJK, emoji, and more — each with language-appropriate tokenization.

### 2. Knowledge Without Connections

Notes accumulate in folders. Ideas that should be connected sit in isolation. You know the answer is "somewhere in your notes" but can't find the thread.

**Without Fortémi**: Manual linking, tagging by memory, or grep-and-hope. Connections exist only in your head.

**With Fortémi**: Automatic semantic linking at >70% embedding similarity. A knowledge graph with recursive exploration, SNN similarity scoring, PFNET sparsification, and Louvain community detection — all with SKOS-derived labels. W3C SKOS vocabularies provide hierarchical concept organization. The graph grows organically as you add content.

### 3. Media Trapped in Files

A video recording contains knowledge — decisions, explanations, demonstrations — locked inside an opaque binary. An audio meeting has action items buried in hours of conversation. An email thread has attachments with critical context.

**Without Fortémi**: Media files are dark matter. Unsearchable. Undiscoverable. You re-watch entire recordings to find one moment.

**With Fortémi**: 13 extraction adapters process images (vision), audio (Whisper transcription + pyannote speaker diarization), video (keyframe extraction + scene detection + transcript alignment), 3D models (multi-view rendering + vision description), emails (RFC 2822/MIME parsing), spreadsheets (xlsx/xls/ods), and archives (ZIP/tar/gz). Every piece of media becomes searchable knowledge with derived attachments (thumbnails, transcripts, caption files, sprite sheets).

### 4. One-Size-Fits-All Storage

Notes, meeting minutes, code documentation, research papers, and movie reviews all get the same treatment. A meeting note should emphasize decisions and action items; a research paper should highlight methodology and findings.

**Without Fortémi**: Everything processed identically. No content awareness.

**With Fortémi**: 131 document types with auto-detection from filename patterns and content analysis. Each type has tailored chunking strategies (syntactic for code, semantic for prose), content-specific revision prompts (meetings get Decisions/Action Items sections, research gets Methodology/Findings), and type-aware extraction pipelines.

---

## Features

- **Hybrid search** — BM25 + dense vectors + RRF fusion with MMR diversity reranking
- **Multilingual FTS** — CJK bigrams, emoji trigrams, 6+ language stemmers, script auto-detection
- **Search operators** — AND, OR, NOT, phrase search via `websearch_to_tsquery`
- **Knowledge graph** — Automatic linking, recursive CTE exploration, SNN scoring, PFNET sparsification, Louvain community detection
- **W3C SKOS vocabularies** — Hierarchical concept organization with semantic tagging
- **131 document types** — Auto-detection with content-type-aware chunking and revision
- **13 extraction adapters** — Image vision, audio transcription, speaker diarization, video scene analysis, 3D model rendering, email parsing, spreadsheet extraction, archive listing
- **Synchronous chat** — Direct LLM conversation with GPU concurrency gating and multi-turn history
- **Multi-memory archives** — Schema-isolated parallel memories with federated cross-archive search
- **Embedding sets** — Matryoshka Representation Learning for 12x storage savings, auto-embed rules, two-stage retrieval
- **Multi-provider inference** — Ollama, OpenAI, OpenRouter, llama.cpp with hot-swap runtime configuration
- **OAuth2 + API keys** — Opt-in authentication with client credentials and authorization code grants
- **Public-key encryption** — X25519/AES-256-GCM for secure note sharing
- **Real-time events** — SSE + WebSocket + webhook notifications
- **Spatial-temporal search** — PostGIS location + time range queries
- **TUS resumable uploads** — tus v1.0.0 protocol for reliable large-file uploads
- **HTTP Range requests** — Partial content download for large attachments
- **Thumbnail sprite sheets** — CSS sprite grids with WebVTT maps for video seek-bar previews
- **43 MCP agent tools** — Model Context Protocol integration for AI agent workflows
- **Edge hardware** — Runs on 8GB GPUs; scales with hardware profiles (`edge`, `gpu-12gb`, `gpu-24gb`)
- **Knowledge health dashboard** — Orphan tags, stale notes, unlinked notes, cold spots, access frequency

---

## How It Works

```
 ┌──────────────────────────────────────────────────────────────────────────┐
 │                              Fortémi                                     │
 │                                                                          │
 │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
 │  │  Ingest  │─▶│ Extract  │─▶│  Embed   │─▶│  Link    │─▶│  Store   │  │
 │  │          │  │          │  │          │  │          │  │          │  │
 │  │ Notes    │  │ Vision   │  │ Dense    │  │ Auto-    │  │ pgvector │  │
 │  │ Media    │  │ Audio    │  │ vectors  │  │ link     │  │ PostGIS  │  │
 │  │ Email    │  │ Video    │  │ BM25     │  │ Graph    │  │ FTS      │  │
 │  │ Archives │  │ 3D       │  │ index    │  │ build    │  │          │  │
 │  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │
 │                                                                          │
 │  ┌──────────────────────────────────────────────────────────────────┐    │
 │  │                         Search & Retrieve                        │    │
 │  │  BM25 full-text ─┐                                               │    │
 │  │  Dense vectors ──┼──▶ RRF Fusion ──▶ MMR Diversity ──▶ Results   │    │
 │  │  Graph traverse ─┘                                               │    │
 │  └──────────────────────────────────────────────────────────────────┘    │
 │                                                                          │
 │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
 │  │ REST API │  │ MCP Srvr │  │  Chat    │  │  Events  │  │  OAuth2  │  │
 │  │  :3000   │  │  :3001   │  │  (LLM)   │  │ SSE/WS   │  │ + Keys   │  │
 │  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │
 └──────────────────────────────────────────────────────────────────────────┘
```

1. **Ingest** — Notes, files, and media enter via REST API or MCP tools
2. **Extract** — 13 adapters pull text, metadata, scenes, transcripts, and descriptions from every content type
3. **Embed** — Content is vectorized for semantic search and indexed for full-text search
4. **Link** — Embedding similarity >70% creates automatic graph connections; SNN + PFNET refine topology
5. **Store** — PostgreSQL with pgvector (vectors), PostGIS (spatial), FTS (text), and per-memory schema isolation
6. **Search** — BM25 + dense + graph results fused via RRF and diversified via MMR

---

## Quick Start

> **Note**: this Quick Start covers the **Docker backend path**. If you want a native desktop app instead, see [the desktop callout above](#-looking-for-a-desktop-app-not-just-an-api) — install [HotM](https://git.integrolabs.net/Fortemi/HotM/releases/latest), then run:
> - **Linux:** [`bash scripts/setup-linux.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-linux.sh) — install guide: [desktop-linux.md](https://git.integrolabs.net/Fortemi/HotM/src/branch/main/docs/installation/desktop-linux.md)
> - **macOS:** [`bash scripts/setup-macos.sh`](https://git.integrolabs.net/Fortemi/HotM/raw/branch/main/scripts/setup-macos.sh) — install guide: [desktop-macos.md](https://git.integrolabs.net/Fortemi/HotM/src/branch/main/docs/installation/desktop-macos.md)
> - **Windows:** `scripts/prereq_once.ps1` (in HotM repo)
> - **Day-2 ops** (any platform): [`operator-guide.md`](https://git.integrolabs.net/Fortemi/HotM/src/branch/main/docs/operations/operator-guide.md)

### Docker Bundle (headless backend deployment)

All-in-one container with PostgreSQL, Redis, API server, MCP server, and Open3D renderer. Runs on any GPU with 6GB+ VRAM:

```bash
mkdir -p fortemi && cd fortemi
curl -fsSL -o docker-compose.bundle.yml \
  https://raw.githubusercontent.com/fortemi/fortemi/main/docker-compose.bundle.yml

# Create .env with your hardware profile
echo 'COMPOSE_PROFILES=edge' > .env          # 6-8GB VRAM (RTX 3060/4060/5060)
# echo 'COMPOSE_PROFILES=gpu-12gb' > .env    # 12-16GB VRAM (RTX 4070/5070)
# echo 'COMPOSE_PROFILES=gpu-24gb' > .env    # 24GB+ VRAM (RTX 4090/5090)

docker compose -f docker-compose.bundle.yml up -d
```

Wait ~30 seconds for first-time initialization, then verify:

```bash
curl http://localhost:3000/health
# → {"status":"healthy","database":"connected",...}
```

**Ports:** 3000 (API + Swagger UI at `/docs`), 3001 (MCP), 8080 (Open3D renderer)

The bundle automatically initializes PostgreSQL, runs all migrations, auto-registers MCP OAuth credentials, starts Redis, and launches all services. For AI features (semantic search, auto-linking, chat), install [Ollama](https://ollama.ai) and pull `nomic-embed-text` + `qwen3.5:9b`. The Fortémi documentation knowledge base (the "support archive") is **not loaded by default** — see [Support Archive](#support-archive-fortemi-docs) below to add it with one command.

**Guided installer:** `installer/scripts/` provides 8 shell scripts for step-by-step deployment, plus a `setup.manifest.yaml` for the AIWG installer framework.

Clean reset: `docker compose -f docker-compose.bundle.yml down -v && docker compose -f docker-compose.bundle.yml up -d`

### From Source

```bash
# Prerequisites: Rust 1.70+, PostgreSQL 18+ with pgvector + PostGIS, Ollama (optional)
psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql -c "CREATE EXTENSION IF NOT EXISTS postgis;"
for f in migrations/*.sql; do psql -d matric -f "$f"; done
DATABASE_URL="postgres://matric:matric@localhost/matric" cargo run --release -p matric-api
```

> **First build fails with "missing graph"?** Fortemi uses `sqlx::query!` compile-time checks. Either `export DATABASE_URL=...` against a Postgres with migrations applied, or generate offline metadata once with `cargo sqlx prepare --workspace` and build with `SQLX_OFFLINE=true`. See [CONTRIBUTING.md → sqlx compile-time query checks](CONTRIBUTING.md#sqlx-compile-time-query-checks).

### Try It

```bash
# Store a note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"title": "RAG Architecture", "content": "Retrieval-augmented generation combines..."}'

# Hybrid search (BM25 + semantic + RRF)
curl "http://localhost:3000/api/v1/search?q=using+AI+to+answer+questions+from+documents"

# Chat with your knowledge
curl -X POST http://localhost:3000/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What do I know about retrieval architectures?"}'

# Browse all endpoints
open http://localhost:3000/docs
```

See [Getting Started](docs/content/getting-started.md) for the full walkthrough.

---

## Architecture

```
┌──────────────────┬──────────────────────────────────────────────┐
│  matric-api      │ Axum HTTP REST API with OpenAPI/Swagger       │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-search   │ Hybrid retrieval (BM25 + dense + RRF)         │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-jobs     │ Async NLP pipeline (embed, revise, link,      │
│                  │ extract, diarize, chunk)                       │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-inference│ Multi-provider LLM abstraction                │
│                  │ (Ollama, OpenAI, OpenRouter, llama.cpp)        │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-db       │ PostgreSQL + pgvector + PostGIS repositories   │
│                  │ (sqlx, 106 migrations)                         │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-crypto   │ X25519/AES-256-GCM public-key encryption      │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-core     │ Core types, traits, and error handling         │
├──────────────────┼──────────────────────────────────────────────┤
│  mcp-server      │ MCP agent integration (Node.js, 43/205 tools) │
└──────────────────┴──────────────────────────────────────────────┘
```

### Directory Structure

```
fortemi/
├── crates/
│   ├── matric-api/          # Axum HTTP API server (routes, handlers, middleware)
│   ├── matric-core/         # Core types, traits, models
│   ├── matric-crypto/       # Public-key encryption (X25519/AES-256-GCM)
│   ├── matric-db/           # PostgreSQL repositories (sqlx)
│   ├── matric-inference/    # Multi-provider inference abstraction
│   ├── matric-jobs/         # Background job worker (NLP pipeline)
│   └── matric-search/       # Hybrid search (FTS + semantic + RRF)
├── mcp-server/              # MCP server (Node.js, 43 core tools)
├── migrations/              # 106 PostgreSQL migrations
├── docker/                  # Docker entrypoints and configs
├── build/                   # CI Dockerfiles (testdb, builder)
├── installer/               # Guided installer scripts
├── docs/                    # 65+ documentation files
│   ├── content/             # Feature and operations guides
│   ├── research/            # Research background
│   └── releases/            # Release announcements
└── docker-compose.bundle.yml  # All-in-one deployment
```

See [Architecture](docs/content/architecture.md) for detailed system design with research citations.

---

## API Endpoints

Full REST API with OpenAPI/Swagger documentation at `/docs`.

### Core Resources

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/notes` | GET, POST | List and create notes |
| `/api/v1/notes/{id}` | GET, PUT, DELETE | Read, update, delete notes |
| `/api/v1/search` | GET | Hybrid search (BM25 + semantic + RRF) |
| `/api/v1/search/federated` | POST | Cross-archive federated search |
| `/api/v1/chat` | POST | Synchronous LLM chat with knowledge context |
| `/api/v1/tags` | GET, POST | Tag management |
| `/api/v1/collections` | GET, POST | Collection/folder hierarchy |
| `/api/v1/graph` | GET | Knowledge graph exploration |
| `/api/v1/graph/maintenance` | POST | Graph quality pipeline (normalize, SNN, PFNET) |

### Media & Attachments

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/notes/{id}/attachments` | POST | Upload file attachment |
| `/api/v1/attachments/{id}/content` | GET | Download with HTTP Range support |
| `/api/v1/upload` | POST | TUS resumable upload (tus v1.0.0) |

### Inference & Configuration

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/inference/config` | GET, POST, DELETE | View/override/reset inference providers |
| `/api/v1/inference/test-connection` | POST | Test backend connectivity |
| `/api/v1/archives` | GET, POST | Multi-memory archive management |

### Metadata & Health

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/concepts` | GET, POST | W3C SKOS concept management |
| `/api/v1/concepts/schemes/{id}/export/turtle` | GET | SKOS Turtle export |
| `/api/v1/embeddings` | GET, POST | Embedding set management |
| `/health` | GET | System health with capability report |
| `/docs` | GET | Swagger UI |
| `/openapi.yaml` | GET | OpenAPI specification |

See [API Reference](docs/content/api.md) for all endpoints with request/response examples.

---

## MCP Server

43 core agent tools via Model Context Protocol. Docker bundle exposes MCP on port 3001 with automatic OAuth credential management.

### Connect

```json
{
  "mcpServers": {
    "fortemi": { "url": "https://your-domain.com/mcp" }
  }
}
```

### Core Tools (43)

Discriminated-union pattern for agent-friendly interaction:

| Tool | What It Does |
|------|-------------|
| `capture_knowledge` | Create, update, and manage notes |
| `search` | Hybrid search with tag filtering and federation |
| `record_provenance` | Track knowledge lineage and sourcing |
| `manage_tags` | Tag lifecycle and vocabulary management |
| `manage_collection` | Collection hierarchy operations |
| `manage_concepts` | W3C SKOS concept and scheme management |
| `manage_embeddings` | Embedding set configuration and lifecycle |
| `manage_archives` | Multi-memory archive operations |
| `manage_encryption` | Public-key encryption and key management |
| `manage_backups` | Backup and restore operations |
| `manage_jobs` | Background job monitoring |
| `manage_inference` | Provider configuration and model selection |
| `manage_attachments` | File upload, metadata, and retrieval |
| `trigger_graph_maintenance` | Graph quality pipeline |
| `explore_graph` | Knowledge graph traversal |
| `get_knowledge_health` | Dashboard for orphans, stale notes, cold spots |
| `select_memory` / `get_active_memory` | Multi-memory context switching |
| `purge_note` / `purge_notes` / `purge_all_notes` | Destructive cleanup |

Set `MCP_TOOL_MODE=full` for all 205 granular tools. See [MCP Guide](docs/content/mcp.md) · [MCP Deployment](docs/content/mcp-deployment.md).

---

## Search Capabilities

### Query Syntax

```
hello world        # Match all words (AND)
apple OR orange    # Match either word
apple -orange      # Exclude word
"hello world"      # Match exact phrase
```

### Multilingual Support

| Script | Strategy | Languages |
|--------|----------|-----------|
| Latin | Full stemming | English, German, French, Spanish, Portuguese, Russian |
| CJK | Bigram/trigram character matching | Chinese, Japanese, Korean |
| Emoji & symbols | Trigram substring matching | Universal |
| Arabic, Cyrillic, Greek, Hebrew | Basic tokenization | Various |

### Search Modes

| Mode | Description |
|------|-------------|
| **Hybrid** (default) | BM25 + dense vectors + RRF fusion |
| **Semantic** | Dense vector similarity only |
| **Full-text** | BM25 keyword matching only |
| **Graph** | Traverse knowledge graph connections |
| **Federated** | Search across multiple memory archives |

See [Search Guide](docs/content/search-guide.md) · [Multilingual FTS](docs/content/multilingual-fts.md) · [Search Operators](docs/content/search-operators.md).

---

## Media Processing

### Extraction Adapters (13)

| Adapter | Input | Output |
|---------|-------|--------|
| **Vision** | Images (PNG, JPEG, WebP, etc.) | Scene descriptions via Ollama vision LLM |
| **Audio Transcription** | Audio files (MP3, WAV, FLAC, etc.) | Timestamped transcripts via Whisper |
| **Speaker Diarization** | Audio with multiple speakers | Speaker-labeled captions via pyannote |
| **Video Multimodal** | Video files (MP4, MKV, WebM, etc.) | Keyframes + scene detection + transcript alignment |
| **3D Model** | GLB/glTF files | Multi-view rendered images + vision description |
| **Email** | EML/MSG files | RFC 2822/MIME parsing + embedded attachment extraction |
| **Spreadsheet** | XLSX/XLS/ODS files | Markdown tables per sheet |
| **Archive** | ZIP/tar/gz files | File listing + text content extraction |
| **PDF** | PDF documents | Text extraction with layout preservation |
| **Media Optimizer** | Video/audio | Faststart, web-compatible remux, 720p preview |
| **Thumbnail** | Video | CSS sprite grids + WebVTT maps for seek-bar previews |
| **GLiNER** | Text | Named entity extraction (concepts, topics) |
| **Fast/Standard NLP** | Text | Concept extraction cascade (granite4:3b → gpt-oss:20b) |

### Extraction Pipeline

```
 Upload ──▶ Type Detection ──▶ Adapter Selection ──▶ Extract ──▶ Embed ──▶ Link
                │                                       │
                ▼                                       ▼
         131 document types                    Derived attachments
         auto-detected from                    (thumbnails, transcripts,
         filename + content                     captions, sprite sheets)
```

### Hardware Profiles

| Profile | GPU VRAM | Audio/Diarization | Gen Model | Example GPUs |
|---------|----------|-------------------|-----------|--------------|
| `edge` (default) | 6-8GB | CPU | qwen3.5:9b | RTX 3060 8GB, 4060, 5060 |
| `gpu-12gb` | 12-16GB | GPU | qwen3.5:9b | RTX 3060 12GB, 4070, 5070 |
| `gpu-24gb` | 24GB+ | GPU | configurable | RTX 3090, 4090, 5090 |

### Resource Requirements

Idle footprint of the default Docker bundle:

| Component | Idle RAM | Notes |
|-----------|----------|-------|
| PostgreSQL 18 | ~500 MB | required |
| Redis | ~256 MB | required |
| `qwen3.5:9b` (fast gen + vision) | ~8 GB VRAM/RAM | set `MATRIC_FAST_GEN_MODEL=` to disable |
| `nomic-embed-text` (embeddings) | ~500 MB | required for indexing |
| Whisper (`gpu-12gb`+ profile) | ~2 GB | optional |
| GLiNER (`gpu-12gb`+ profile) | ~1 GB | optional |
| **Default bundle total** | **~10 GB** | with qwen3.5:9b loaded |
| **Minimal profile total** | **~2 GB** | qwen2.5:3b, no support archive |

The Docker bundle does **not** auto-load the bundled support archive — it mirrors the native build path. See [Support Archive](#support-archive-fortemi-docs) below to opt in (one command).

Operators on tight resources can stack the minimal overlay:

```bash
docker compose -f docker-compose.bundle.yml -f docker-compose.minimal.yml up -d
```

The minimal overlay swaps the fast-extraction model to `qwen2.5:3b`, caps `JOB_MAX_CONCURRENT=1`, and trims `MAX_MEMORIES=2`. Target idle ~2 GB. Trade-off: chat quality with `qwen2.5:3b` is materially lower than the default — this is for "make it run on my laptop", not production.

---

## Support Archive (fortemi-docs)

The bundle ships a pre-built `.shard` of the Fortémi documentation as an in-product knowledge base — same content as the docs site, but searchable through the same `/api/v1/search` endpoint as your own notes. Off by default (the Docker bundle mirrors the native build path; neither auto-seeds). Opt in when you want it.

### Add it with one command (running instance)

```bash
docker compose -f docker-compose.bundle.yml \
  exec fortemi /app/seed-support-archive.sh
```

Idempotent — re-running is a no-op once seeded (a flag file on the persistent `pgdata` volume tracks state). Takes ~10–30 seconds depending on disk speed.

### Auto-seed on first boot

If you know up front you want the docs available, set this in `.env` before running `docker compose ... up`:

```bash
LOAD_SUPPORT_MEMORY=true
```

The seed runs in the background after the API reports healthy.

### Querying the archive

The seeded data lives at memory `fortemi-docs`. Reach it with the `X-Fortemi-Memory` header:

```bash
# Full-text search (works immediately after seeding)
curl -H 'X-Fortemi-Memory: fortemi-docs' \
  'http://localhost:3000/api/v1/search?q=hybrid+search'

# List notes
curl -H 'X-Fortemi-Memory: fortemi-docs' \
  'http://localhost:3000/api/v1/notes?limit=10'
```

MCP tool clients can scope to the archive via the `memory` argument on most tools.

### Add semantic search over the archive (additional opt-in)

The seed populates Postgres `tsvector` (FTS) only — no embeddings, so the archive is queryable without an inference provider. To enable semantic search over the docs:

```bash
curl -X POST http://localhost:3000/api/v1/notes/reprocess \
  -H 'X-Fortemi-Memory: fortemi-docs' \
  -H 'Content-Type: application/json' \
  -d '{"steps":["embedding"],"revision_mode":"none"}'
```

Adds `"linking"` to `steps` for auto-linking; drops `revision_mode:"none"` to also AI-revise notes. Cost depends on your configured inference provider (see [Multi-Provider Inference](#multi-provider-inference) for routing).

### Refreshing on upgrade

The bundle ships a fresh `.shard` baked into each release image (auto-rebuilt in CI from the source tree at the tagged commit; see #652). On upgrade the seed flag persists with your data, so the docs archive stays at whatever version you originally seeded. To pick up the latest docs after an image upgrade:

```bash
# Drop the existing archive and re-seed from the upgraded image
docker compose -f docker-compose.bundle.yml exec fortemi \
  curl -fsS -X DELETE http://localhost:3000/api/v1/archives/fortemi-docs
docker compose -f docker-compose.bundle.yml exec fortemi \
  rm -f /var/lib/postgresql/data/.fortemi-docs-seeded
docker compose -f docker-compose.bundle.yml \
  exec fortemi /app/seed-support-archive.sh
```

Your own data in other archives is unaffected.

### Skipping or disabling

- **Don't enable it**: do nothing. Default is off.
- **Force-skip even if `LOAD_SUPPORT_MEMORY=true` is set**: `DISABLE_SUPPORT_MEMORY=true` (legacy override; useful if you're inheriting an `.env` from before the opt-in flip).

---

## Multi-Provider Inference

Fortemi treats every advertised provider as a first-class peer. The runtime is driven by a static **provider profile catalog** — the four v1 entries below cover hosted (OpenAI, OpenRouter), local-daemon (Ollama), and bring-your-own-server (llama.cpp) inference, all reachable via the same hot-swap API.

### Provider profiles (v1)

| Provider | Backend protocol | API key | Embeddings | Default model |
|----------|------------------|---------|------------|---------------|
| **Ollama** | Ollama-native (`/api/generate`) | none | yes | `qwen3.5:9b` / `nomic-embed-text` |
| **OpenAI** | OpenAI-compatible (`/v1/*`) | required | yes | `gpt-4o-mini` / `text-embedding-3-small` |
| **OpenRouter** | OpenAI-compatible | required | **no** | `anthropic/claude-sonnet-4` / *(none)* |
| **llama.cpp** | OpenAI-compatible | optional | depends on build | *(operator-set)* |

Adding new well-known providers (vLLM, LiteLLM, LocalAI, Groq, Together, …) is a 5-line addition to `crates/matric-inference/src/provider_profiles.rs` with no enum or parser changes.

### Provider-qualified slugs

```
qwen3:8b                                    → default provider
ollama:qwen3:8b                             → explicit Ollama
openai:gpt-4o                               → OpenAI
openrouter:anthropic/claude-sonnet-4        → OpenRouter
llamacpp:qwen2.5-7b-instruct                → llama.cpp
```

### Runtime reconfiguration

Hot-swap any provider's credentials, model, or routing via `POST /api/v1/inference/config` — no restart required. Configuration precedence: `db_override` → `env` → `default`. Two safety primitives:

- **`?dry_run=true`** — validate the merged config and return the effective state without persisting or hot-swapping. Useful for operator UIs running pre-flight checks.
- **`?atomic=true`** — probe every backend the request touches before committing. On any probe failure: 503 + structured `failures: [...]` array; the live registry and DB stay on the previous good config. Avoids the brief error window where a half-applied swap serves bad creds.

```bash
# Validate without applying
curl -X POST http://localhost:3000/api/v1/inference/config?dry_run=true \
  -H 'Content-Type: application/json' \
  -d '{"openrouter":{"api_key":"sk-or-v1-...","generation_model":"anthropic/claude-3.5-sonnet"}}'

# Atomic swap — abort if any probe fails
curl -X POST 'http://localhost:3000/api/v1/inference/config?atomic=true' \
  -H 'Content-Type: application/json' \
  -d '{"openrouter":{"api_key":"sk-or-v1-..."}}'
```

### Independent embedding/generation routing

OpenRouter doesn't expose embeddings; Groq is API-only with no local model; some operators want to keep embeddings on-device for privacy while paying for hosted chat. Set `MATRIC_EMBEDDING_PROVIDER` (or the `embedding_backend` field on `POST /api/v1/inference/config`) to route embedding calls through a different provider than the active default.

```bash
# .env: chat through OpenRouter, embed locally via Ollama
MATRIC_INFERENCE_DEFAULT=openrouter
MATRIC_EMBEDDING_PROVIDER=ollama
OPENROUTER_API_KEY=sk-or-v1-...
```

The runtime validates the override against the catalog at boot and on every `POST /config` call: pointing `embedding_backend` at OpenRouter (which has no embedding capability) returns 400 with a descriptive error before persisting.

### Bring Your Own LLM

Ollama is the default for the Docker bundle, but it is **one option among four** — Fortemi does not require Ollama. Pick a profile and set the matching `.env` block:

```bash
# Native llama.cpp profile (recommended for self-hosted local inference)
MATRIC_INFERENCE_DEFAULT=llamacpp
LLAMACPP_BASE_URL=http://host.docker.internal:8080/v1
LLAMACPP_GEN_MODEL=qwen2.5-7b-instruct
# LLAMACPP_API_KEY=...           # only if llama-server launched with --api-key

# Native OpenAI proper
MATRIC_INFERENCE_DEFAULT=openai
OPENAI_API_KEY=sk-...
OPENAI_GEN_MODEL=gpt-4o-mini

# Native OpenRouter (chat only — pair with MATRIC_EMBEDDING_PROVIDER for embed)
MATRIC_INFERENCE_DEFAULT=openrouter
OPENROUTER_API_KEY=sk-or-v1-...
OPENROUTER_GEN_MODEL=anthropic/claude-sonnet-4
MATRIC_EMBEDDING_PROVIDER=ollama
```

OpenRouter routing rules and analytics use `HTTP-Referer` and `X-Title` headers; Fortemi defaults them to `https://fortemi.io` / `Fortemi`. Override per-app for downstream tools that ship Fortemi as a sidecar:

```bash
OPENROUTER_HTTP_REFERER=https://your-app.example.com
OPENROUTER_APP_NAME=Your App
```

To run a llama.cpp sidecar alongside the bundle, place a GGUF model at `./models/model.gguf` and bring up both compose files:

```bash
docker compose -f docker-compose.bundle.yml -f docker-compose.llamacpp.yml up -d
```

`docker-compose.llamacpp.yml` ships the `ghcr.io/ggerganov/llama.cpp:server` image with the OpenAI-compatible protocol exposed at `:8080/v1`. See the file header for tunables (`LLAMACPP_MODEL_FILE`, `LLAMACPP_CTX_SIZE`, `LLAMACPP_GPU_LAYERS`).

**Custom OpenAI-compatible endpoints** (vLLM, LiteLLM, LocalAI, on-prem providers not yet in the catalog) keep working via the legacy escape hatch:

```bash
MATRIC_INFERENCE_DEFAULT=openai
OPENAI_BASE_URL=http://your-host:8000/v1
OPENAI_API_KEY=anything-or-real-key
```

**Disabling Ollama entirely**: set `MATRIC_INFERENCE_DEFAULT` to anything other than `ollama` and leave `OLLAMA_BASE` unset. The Ollama backend isn't constructed when it isn't the default or embedding override, so a dead `host.docker.internal:11434` won't be probed.

---

## Multi-Memory Archives

Parallel memory archives with schema-level isolation for tenant separation, project segmentation, or context switching.

- `X-Fortemi-Memory` header selects target memory per request
- Default memory maps to `public` schema (no header needed)
- 14 shared tables (auth, jobs, config) + 41 per-memory tables (notes, tags, embeddings, etc.)
- `POST /api/v1/archives` creates new archives with automatic schema cloning
- `POST /api/v1/search/federated` searches across multiple archives simultaneously

See [Multi-Memory Guide](docs/content/multi-memory.md) · [Agent Strategies](docs/content/multi-memory-agent-guide.md).

---

## Authentication

Opt-in via `REQUIRE_AUTH=true`. When disabled (default), all endpoints are publicly accessible.

| Method | How |
|--------|-----|
| **OAuth2** | Client credentials or authorization code via `/oauth/token` |
| **API Keys** | Create via `POST /api/v1/api-keys`, use as Bearer token |

Public endpoints (always accessible): `/health`, `/docs`, `/openapi.yaml`, `/oauth/*`, `/.well-known/*`

See [Authentication Guide](docs/content/authentication.md).

---

## Configuration

Key variables (see [full reference](docs/content/configuration.md) for all ~27 variables):

| Variable | Default | Description |
|----------|---------|-------------|
| `COMPOSE_PROFILES` | `edge` | Hardware profile: `edge`, `gpu-12gb`, `gpu-24gb` |
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection |
| `PORT` | `3000` | API server port |
| `REQUIRE_AUTH` | `false` | Enable OAuth2/API key auth |
| `ISSUER_URL` | `https://localhost:3000` | OAuth2 issuer URL (required for OAuth/MCP) |
| `OLLAMA_BASE` | `http://localhost:11434` | Ollama API endpoint |
| `OLLAMA_GEN_MODEL` | `qwen3.5:9b` | Generation + vision model |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model |
| `WHISPER_BASE_URL` | `http://whisper:8000` | Audio transcription endpoint |
| `MAX_MEMORIES` | `10` | Max archives (scale with RAM: 10→8GB, 50→16GB, 200→32GB, 500→64GB+) |
| `MCP_TOOL_MODE` | `core` | `core` (43 tools) or `full` (205 tools) |

---

## Security Model

| Feature | Description |
|---------|-------------|
| **Opt-in auth** | OAuth2 (client credentials + auth code) and API keys |
| **Schema isolation** | Per-memory PostgreSQL schemas for tenant separation |
| **PKE encryption** | X25519/AES-256-GCM public-key encryption for notes |
| **MCP credential auto-management** | Auto-registers OAuth client on startup; credentials persisted across restarts |
| **Input validation** | Request validation at API boundary |
| **TUS checksums** | Integrity verification on resumable uploads |
| **Edge deployment** | No cloud dependency; runs entirely self-hosted |

---

## Development

```bash
# Install pre-commit hooks (first time)
./scripts/install-hooks.sh

# Run tests
cargo test --workspace

# Format + lint
cargo fmt && cargo clippy -- -D warnings

# Run with debug logging
RUST_LOG=debug cargo run -p matric-api
```

### Database

- PostgreSQL 18 with pgvector + PostGIS extensions
- Connection: `postgres://matric:matric@localhost/matric`
- 106 migrations in `migrations/` directory
- Extensions created by entrypoint/CI as superuser

### Testing

```bash
cargo test                    # Unit tests
cargo test --workspace        # All crates
```

Tests run against real PostgreSQL (not mocks). CI provides dedicated test containers with pgvector + PostGIS. See [Testing Guide](docs/content/testing-guide.md).

### Versioning

**CalVer**: `YYYY.M.PATCH` (e.g., `2026.4.0`). Git tags use `v` prefix: `v2026.4.0`. See [Releasing](docs/content/releasing.md).

---

## Documentation

### Getting Started

- **[Getting Started](docs/content/getting-started.md)** — First steps and concepts
- **[Quickstart](docs/content/quickstart.md)** — Deploy and run in minutes
- **[Use Cases](docs/content/use-cases.md)** — Deployment patterns and scenarios
- **[Best Practices](docs/content/best-practices.md)** — Research-backed guidance
- **[Glossary](docs/content/glossary.md)** — Terminology

### Features

- **[Search Guide](docs/content/search-guide.md)** — Modes, RRF tuning, query patterns
- **[Multilingual Search](docs/content/multilingual-fts.md)** — CJK, emoji, language-specific FTS
- **[Search Operators](docs/content/search-operators.md)** — AND, OR, NOT, phrase search
- **[Knowledge Graph](docs/content/knowledge-graph-guide.md)** — Traversal, linking, community detection
- **[Embedding Sets](docs/content/embedding-sets.md)** — MRL, auto-embed, two-stage retrieval
- **[Document Types](docs/content/document-types-guide.md)** — 131 types with auto-detection
- **[File Attachments](docs/content/file-attachments.md)** — Media upload and extraction pipeline
- **[Real-Time Events](docs/content/real-time-events.md)** — SSE, WebSocket, webhooks
- **[Encryption](docs/content/encryption.md)** — PKE for secure sharing

### Operations

- **[Configuration](docs/content/configuration.md)** — All environment variables
- **[Authentication](docs/content/authentication.md)** — OAuth2, API keys, migration path
- **[Multi-Memory](docs/content/multi-memory.md)** — Archives, federated search, isolation
- **[MCP Server](docs/content/mcp.md)** · **[MCP Deployment](docs/content/mcp-deployment.md)** — Agent integration
- **[Inference Providers](docs/content/inference-providers.md)** — Multi-provider configuration
- **[Operators Guide](docs/content/operators-guide.md)** — Monitoring, maintenance
- **[Hardware Planning](docs/content/hardware-planning.md)** — Sizing and capacity
- **[Backup & Restore](docs/content/backup.md)** — Database recovery
- **[Troubleshooting](docs/content/troubleshooting.md)** — Diagnostics

### Technical

- **[Architecture](docs/content/architecture.md)** — System design with research citations
- **[API Reference](docs/content/api.md)** — All endpoints with examples
- **[Research Background](docs/content/research-background.md)** — Methodology and benchmarks
- **[Executive Summary](docs/content/executive-summary.md)** — Capabilities overview
- **[Feature & Hardware Matrix](docs/content/feature-hardware-matrix.md)** — Requirements by feature

---

## References

- Cormack, G.V., Clarke, C.L.A., & Büttcher, S. (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." *SIGIR '09*.
- Lewis, P. et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." *NeurIPS 2020*.
- Reimers, N. & Gurevych, I. (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." *EMNLP 2019*.
- Malkov, Y.A. & Yashunin, D.A. (2020). "Efficient and robust approximate nearest neighbor search using HNSW." *IEEE TPAMI*.
- Hogan, A. et al. (2021). "Knowledge graphs." *ACM Computing Surveys*.
- Kusupati, A. et al. (2022). "Matryoshka representation learning." *NeurIPS 2022*.
- Miles, A. & Bechhofer, S. (2009). "SKOS simple knowledge organization system reference." *W3C Recommendation*.

See [docs/research/](docs/research/) for detailed paper analyses.

---

## Related Projects

- **[AIWG](https://github.com/jmagly/aiwg)** — Multi-agent AI framework with 43 Fortémi MCP tools
- **[Agentic Sandbox](https://github.com/fortemi/agentic-sandbox)** — Runtime isolation for persistent AI agent processes
- **[HotM](https://git.integrolabs.net/Fortemi/HotM)** ([GitHub mirror](https://github.com/Fortemi/HotM)) — first-party desktop app for Fortemi (React 19 SPA + bundled `matric-api` sidecar; Linux `.deb` / Windows `.msi` / macOS `.dmg` / `.AppImage`)

---

## License

**BSL-1.1** (Business Source License 1.1). See [LICENSE](LICENSE).

---

<div align="center">

**[Back to Top](#fortémi)**

Made with determination by [Joseph Magly](https://github.com/jmagly)

</div>

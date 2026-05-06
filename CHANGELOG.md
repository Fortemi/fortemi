# Changelog

All notable changes to Fortémi are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [CalVer](https://calver.org/) versioning: `YYYY.M.PATCH`.

## [Unreleased]

## [2026.5.0] - 2026-05-03

### Fixed

- **Silent attachment data loss on filesystem backend** (#631) — Three-part hardening of the filesystem-backed attachment write/read path to close the failure mode where `attachment_blob` rows outlived their on-disk files. The atomic write path now performs a best-effort parent-directory fsync after rename and uses a `.bin.tmp` suffix that the startup sweeper can discriminate. `FilesystemBackend::sweep_temp_files()` runs at server boot and removes stale `.bin.tmp` orphans (default threshold 5 minutes) from prior crashed writes. Missing-blob reads now return a structured `404 {error: "blob_missing", attachment_id, expected_path, storage_backend}` distinct from the generic 500, so clients can surface a permanent-loss recovery UI instead of retrying a transient I/O fault.
- **CI Docker CLI / daemon API mismatch** (#632) — `build/Dockerfile.builder` now installs `docker-ce-cli` + `docker-buildx-plugin` from Docker's official apt repo instead of Debian bookworm's `docker.io` (CLI 20.10, API 1.41). Pins the in-builder CLI to track upstream Docker so it can talk to host daemons running Docker 25.0+ / API 1.44+. Unblocks `test.yml` and `ci-builder.yaml` after the runner host daemon was upgraded to Docker 29.x (API 1.52).

### Changed

- **Docs: surface HotM desktop app prominently** — Public-facing docs now lead with the [HotM (Hall of the Mind)](https://git.integrolabs.net/Fortemi/HotM) desktop app for end users and clarify that this Fortemi repo is the Docker-only backend service. Adds deep links to HotM prerequisite scripts and install guides.

## [2026.4.2] - 2026-04-22

### Added

- **Stateless inference endpoints with per-request BYOK** (#628) — Three new endpoints let downstream UIs and external integrations drive chat completions through Fortemi as a CORS-bypassing proxy without server-side key storage:
  - `POST /api/v1/inference/complete` — provider-agnostic chat completion with optional `{provider_id, api_key, base_url}` in the request body. Falls back to registered config then env vars. Supports `ollama`, `openai`, `openrouter`, `llamacpp`.
  - `POST /api/v1/inference/stream` — SSE streaming with real token-by-token output for Ollama; one-chunk fallback for other backends pending their streaming implementations.
  - `GET /api/v1/inference/providers` — lists known providers with `server_configured` + `requires_user_key` flags so BYOK UIs know which keys to prompt for.
  - `ProviderRegistry::resolve_generation_inline()` — factory that builds a fresh `Box<dyn GenerationBackend>` from transient credentials without mutating the registry. Never caches between calls.
  - `OllamaBackend::set_base_url()` — new setter for per-request base URL override.
- **Real token streaming for Ollama** (#629) — `POST /api/v1/inference/stream` now emits one SSE `delta` event per token for Ollama backends. `GenerationBackend` trait gains `stream_generate()` and `stream_generate_with_system()` with default one-chunk fallback for backends without a streaming implementation. Enables per-token visibility in HotM and other downstream UIs.
- **Periodic inference provider reprobe** (#630) — `capabilities.inference.available` no longer latches false permanently when a provider is unreachable at startup. A background probe re-checks every `INFERENCE_PROBE_INTERVAL_SECS` (default 30s) and updates `AppState.inference_available`. Chat handler returns 503 + `retry_after` when the provider is currently unreachable. Emits `InferenceAvailabilityChanged` SSE event on transitions so clients can clear/raise offline banners without polling `/health`.
- **Caller-defined extraction pipeline** — Optional `pipeline` field on `CreateNoteRequest` to opt-in to specific AI processing stages (`revision`, `title_generation`, `concept_tagging`, `reference_extraction`, `metadata_extraction`, `document_type_inference`). Empty array stores the note without any AI processing. Backwards compatible — omitting `pipeline` runs the full default pipeline.
- **Configurable archive extraction limits** — `ARCHIVE_MAX_EXTRACT_BYTES` (default 1 GB, was 100 MB) and `ARCHIVE_MAX_SINGLE_FILE_BYTES` (default 50 MB, was 10 MB) env vars replace hardcoded constants. `MAX_FILES` cap removed entirely.
- **Multi-provider installer manifest** — `setup.manifest.yaml` extended to support configuring multiple inference providers simultaneously during guided deployment. Complements the 8 installer scripts shipped in v2026.4.0.
- **README redesign** — Restructured with normalized design language: problem/solution framing, ASCII ingest-to-search pipeline diagram, API endpoint tables, full MCP tool table, search capabilities and media processing sections, multi-provider inference slug reference, and security model overview.
- **Feature and hardware requirements matrix** — New `docs/content/feature-hardware-matrix.md` maps every feature to its minimum hardware requirements, GPU VRAM tiers, and optional dependencies.

### Fixed

- **Revision mode "none" creates fake history** (#625) — Notes created or updated with `revision_mode=none` no longer write misleading `note_revision` records with "Original preserved (no AI revision)" rationale. New `sync_revised_to_original_tx` keeps FTS content synced without creating fake revision history.
- **Redis connection timeout** (#624) — 5-second timeout on `ConnectionManager::new()` prevents server startup from blocking indefinitely when Redis is unreachable. Bundle image defaults to `REDIS_ENABLED=false`.
- **arm64 optional components** (#623) — Build args `ENABLE_OPEN3D`, `ENABLE_POSTGIS`, `ENABLE_OCR`, `ENABLE_FFMPEG` allow disabling platform-specific components for cross-architecture builds.
- **Think tokens blocked streaming chunks** — Disabled Qwen3.5 `think` mode during streaming so content delta events flow immediately without waiting for the full `<think>…</think>` block to complete.
- **`:latest` Docker tag overwritten by dev builds** — CI now reserves `:latest` and `:bundle-latest` exclusively for tagged releases. Dev pushes produce `:main` and `:sha-<short>` only. Downstream consumers should pin to `:main` for rolling dev or `:<version>` for pinned releases. Fixes v2026.4.0 being masked on `ghcr.io` by later main builds.

## [2026.4.0] - 2026-04-04

### Added

- **Synchronous chat API** (#549) — `POST /api/v1/chat` provides direct LLM conversation with GPU concurrency semaphore, multi-turn history, and model selection. `GET /api/v1/chat/models` lists available models with metadata. Health endpoint exposes `capabilities.chat` for client availability detection.
- **Chunked audio transcription** (#540, #541, #542, #543) — Long audio files automatically split into chunks for parallel transcription via fan-out `AudioChunkTranscription` jobs with atomic dual fan-in for video assembly.
- **Content-type-aware AI revision** (#571) — Revision pipeline produces type-specific output (meetings get Decisions/Action Items, movies get Synopsis/Cast). Adaptive chunking, chunk-count-based revision budgets, and user-configurable `chunk_max_chars`/`chunk_overlap` per request.
- **Decomposed 3D model extraction** (#531, #533, #534, #535) — Atomic per-view vision jobs with `RENDER_GPU` tier, Open3D EGL GPU support, and AI revision after view assembly.
- **Video keyframe vision** (#550) — `KeyframeCharacterVision` and `KeyframeSettingVision` job types with improved scene-dialog interleaving and keyframe merging prompts.
- **MMR diversity search and access analytics** — Maximal Marginal Relevance for search diversity, access frequency tracking, cold-spot detection, and `agent-reflection` document type.
- **Inference runtime config API** (#568-570) — Runtime Ollama configuration with connection testing via `GET/PUT /api/v1/config/inference`. Extended with llama.cpp section; every PUT rebuilds the full provider registry for hot-swap without server restart.
- **llama.cpp provider** — Register llama.cpp as a first-class inference provider via `LLAMACPP_BASE_URL`. Routes through the existing `OpenAIBackend` (same HTTP protocol, zero new backend code). Use provider-qualified slugs (`llamacpp:model-name`) for per-request routing. `LLAMACPP_BASE_URL`, `LLAMACPP_API_KEY`, `OPENAI_API_KEY`, and `OPENROUTER_API_KEY` passed through in compose.
- **Inference resilience** (#545, #546, #547, #548) — Retry with exponential backoff, circuit breaker, fail-fast detection, and memory limits for sidecar services (Whisper, pyannote, GLiNER).
- **Edge-first hardware profiles** — `COMPOSE_PROFILES` selects deployment tier: `edge` (CPU sidecars, 6-8GB VRAM), `gpu-12gb`, `gpu-24gb`. Defaults target RTX 3060/4060/5060.
- **Qwen3.5 model family** — Default generation upgraded to `qwen3.5:9b` (262K context, natively multimodal). Single model serves generation, fast extraction, and vision with one ~6.5GB VRAM load.
- **HotM consumer contract tests** (#549) — Chat endpoint contract tests for HotM integration.
- **Installer scripts** — `setup.manifest.yaml` machine-readable install manifest for the AIWG installer framework. `installer/scripts/` contains 8 shell scripts for guided deployment: `clone.sh`, `configure.sh`, `deploy.sh`, `pull-models.sh`, `check-ports.sh`, `setup-nvidia.sh`, `verify.sh`, `reset.sh`.

### Changed

- **GPU sidecar defaults** — Whisper and pyannote now run on CPU by default to preserve GPU VRAM for inference. GPU variants available via `--profile gpu-12gb` or `--profile gpu-24gb`. **Breaking:** existing deployments using GPU sidecars must set `COMPOSE_PROFILES=gpu-12gb` or `gpu-24gb`.
- **GPU job scheduling** — GPU jobs serialize by default to prevent VRAM contention. Ollama models proactively unloaded between tier transitions.
- **Video chunk size** — `REVISION_VIDEO_CHUNK_SIZE_MAX` reduced from 60K to 20K characters to prevent job-level timeout on long videos.
- **Job timeout** — `JOB_TIMEOUT_SECS` default raised from 600s to 1800s; env-var clamp raised to 7200s.
- **Concept tagging pipeline** (#538, #539) — `ConceptTagging` chains from `AiRevision` (operates on enriched content). Enriches with existing DB concepts for better consistency.
- **Media-aware job ordering** (#578) — AI revision deferred for notes with pending media attachments; bypass on explicit reprocess.
- **MCP tools** — Purge tools exposed in core toolset (#530). Tool count updated to 43.
- **PG 18.3** — Reverted PG 18.2 TOAST workaround after upstream fix (#419). All `convert_from(convert_to(...))` instances reverted to `substring()`.
- **Workspace version**: `2026.2.13` → `2026.4.0`

### Fixed

- **3D rendering** (#538, #539) — Normalize extreme-scale models to prevent blank renders; validate render quality to prevent grey thumbnails; fix thumbnail MIME type.
- **Inference** — Disable thinking mode for Qwen3.5 generation (prevents empty responses from thinking models).
- **AI revision on non-default archives** — Single-note `reprocess_note` was missing the archive schema in the AI revision job payload, causing "Failed to fetch note" errors on non-default archives.
- **Video revision budget** — Total revision budget now computed as `Σ(per-chunk adaptive timeouts)`; deadline checked before each chunk so the loop exits cleanly rather than timing out mid-chunk.
- **TUS uploads** (#544) — Add GET handler for upload finalization and `DefaultBodyLimit` on TUS routes.
- **Migrations** — Use snake_case enum values for `job_type`; use correct singular `document_type` table name.
- **Archives** — Sync column drift in archive schemas on auto-migration; exclude identity columns from archive clone.
- **Docker** — Add NVIDIA EGL ICD for GPU rendering; add nvidia default runtime migration (#542).
- **Dependencies** — Patch security vulnerabilities in npm and Rust dependencies.
- **GLiNER OOM** — Increased default memory limit from 2GB to 4GB.

## [2026.2.13] - 2026-02-23

### Added

- **Independent sidecar CI/CD** — GLiNER and pyannote Docker images now have dedicated build-and-release workflows (`build-gliner.yaml`, `build-pyannote.yaml`) that publish to both internal and GHCR registries. Sidecar images are released independently from the main API/bundle via their own tags (`sidecar-gliner-v*`, `sidecar-pyannote-v*`), avoiding expensive ML image rebuilds on every core release.

### Changed

- **docker-compose.bundle.yml default tags** — Sidecar image tags changed from `*-main` (internal-only) to `*-latest` (available on both registries). Users setting `FORTEMI_REGISTRY=ghcr.io` no longer need to manually override `FORTEMI_GLINER_TAG` or `FORTEMI_PYANNOTE_TAG`.
- **Sidecar builds removed from main release pipeline** — `publish-release` and `publish-github` jobs in `ci-builder.yaml` no longer build GLiNER/pyannote, reducing release build time.
- **Workspace version**: `2026.2.12` → `2026.2.13`

## [2026.2.12] - 2026-02-22

### Added

- **TUS v1.0.0 Resumable Upload Protocol** (#528) — Standards-compliant [tus](https://tus.io/) resumable file uploads with Creation, Termination, and Checksum extensions. Enables reliable upload of large files over unreliable connections with automatic resume from the last successful byte. Endpoints at `/api/v1/attachments/{note_id}/tus`.
- **Atomic Per-Frame Keyframe Vision Pipeline** (#526) — Keyframe vision descriptions processed as individual `KeyframeVision` jobs instead of inline during extraction. Enables per-frame checkpointing, retry, parallel processing, and independent failure recovery. Keyframes persisted as derived attachments first, then described asynchronously.
- **Thumbnail Sprite Sheet Generation** (#525) — `ThumbnailSprite` handler generates CSS sprite sheets from video keyframes with WebVTT timestamp maps, enabling video preview scrubbing in UI clients.
- **Video Extraction Hardening** — Feature-length video support with adaptive keyframe budgets, timeout scaling, and memory-bounded frame processing.
- **3D Model Rendering as Derived Attachments** — Multi-view 3D renderings persisted as derived child attachments with individual AI descriptions per view and preview thumbnail. Ground-plane grid and off-white background for better visual clarity.
- **Speaker Diarization Pipeline** (#497) — pyannote-based speaker identification for audio and video transcripts. Runs as a GPU sidecar container (`DIARIZATION_BASE_URL`). Produces speaker-labeled VTT/SRT/TXT caption files and a speaker configuration block in note content. Speaker names editable via `SpeakerRelabel` job.
- **Speaker Diarization Foundation Types** (#497) — Core inference types and backend abstraction for diarization providers.
- **Media Optimize Handler** (#506) — Pre-generates streaming-friendly media variants during attachment upload using ffmpeg. Variant types: `faststart` (moov atom relocation), `web_compatible` (H.264+AAC remux), `audio_only` (extracted audio), `preview_720p` (downscaled preview), `web_audio` (AAC transcode), `audio_preview` (lossless→lossy). Variants stored as derived attachments and served via `?variant=` query parameter on the download endpoint.
- **Media Optimize Flag** (#506) — `media_optimize` parameter on attachment upload API and MCP `manage_attachments` tool. Defaults to true for video/audio content types.
- **Email Extraction Adapter** (#508–#512) — RFC 2822 / MIME email parsing (`.eml`, `.mbox`). Extracts message body, headers, and binary attachments as derived child attachments that trigger their own extraction jobs.
- **Spreadsheet Extraction Adapter** (#508–#512) — Excel (`.xlsx`, `.xls`) and ODS spreadsheet extraction. Converts each sheet to markdown tables.
- **Archive Extraction Adapter** (#514–#515) — ZIP, tar, and gzip archive extraction. Produces file listing with text content extraction (capped at 1000 files, 100 MB total).
- **Derived Attachment Storage** (#498, #502) — Thumbnails, transcripts (VTT, SRT, TXT), and media variants stored as child attachments linked to their source via `extracted_metadata` JSON.
- **Video Thumbnail & Audio Waveform** (#502, #503) — Auto-generated preview images persisted as derived attachments during extraction.
- **MP4 Faststart Optimization** (#503) — Automatic moov atom relocation during video extraction for progressive download.
- **Diagramming & Layout Document Types** (#516) — New document type category supporting SVG, Graphviz (DOT), Mermaid, D2, PlantUML, and layout formats.
- **HTTP Range Request Support** (#493) — Partial content download (`Range` header) for large attachment files.
- **Open3D GPU Renderer** (#492) — Replaces Three.js with Open3D for 3D model multi-view rendering. Supports EGL headless rendering on GPU.
- **Global Attachment Listing** — `GET /api/v1/attachments` endpoint for listing all attachments across notes.
- **Related Notes with LLM Summary** — `GET /api/v1/notes/{id}/related` endpoint returns related notes with AI-generated context summary.
- **Document Type Slug Validation** (#490, #491) — Accept `document_type` slug on note creation; validate `revision_mode` parameter.
- **Handler-Initiated SSE Events** — `job.queued` SSE events now emitted for downstream jobs queued by handlers (e.g., Extraction → Embedding), not just API-initiated jobs.

### Changed

- **axum 0.7→0.8 Framework Upgrade** (#524) — Major dependency upgrade: axum 0.7→0.8, tower 0.4→0.5, tower-http 0.5→0.6, tokio-tungstenite 0.24→0.28. Adapts all `Service` implementations to `call(&self)` signature change.
- **Workspace version**: `2026.2.11` → `2026.2.12` (56 commits)

### Fixed

- **KeyframeVision Jobs Silently Orphaned Without Vision Backend** (#529) — `KeyframeVisionHandler` is now always registered regardless of vision backend availability. When the vision backend is unavailable, jobs return `Retry` and stay in the queue until the backend is configured, rather than being silently orphaned with no handler to execute them. Added startup warning when vision backend is missing.
- **Keyframe DerivedFiles Lost to TempDir Drop** — Keyframe JPEG bytes now read inline before the temp directory is dropped, matching the audio extraction pattern. Previously all keyframe attachments were silently lost, breaking the entire downstream pipeline (no KeyframeVision jobs, no ThumbnailSprite content).
- **Keyframe Extraction Gated on Vision Backend** (#527) — Video keyframes now extract regardless of vision backend availability. Keyframes are valuable for thumbnails and sprite sheets even without AI descriptions.
- **Native uuidv7() for TUS Uploads** (#528) — Switched TUS upload tracking from application-generated UUIDs to PostgreSQL native `uuidv7()`; suppressed clippy `too_many_arguments` on upload handler.
- **Audio Transcript Key Normalization** (#523) — Unified `transcript_segments` key in API responses for UI consistency.
- **ETag Middleware Bypass for Downloads** (#522) — File download responses now skip ETag calculation, fixing slow responses for large attachments.
- **Video Audio Track and extracted_text** (#517–#521) — Populate `extracted_text` from transcription and persist video audio track as derived attachment.
- **GLB Adapter and Diagnostics Fixes** (#517–#521) — Multiple fixes for 3D model extraction, AI revision context, and diagnostic snapshot handling.
- **Derived Caption Deduplication** (#516) — Prevent duplicate VTT/SRT/TXT caption files; add speaker labels to diarized captions.
- **Pyannote 4.x Compatibility** — Handle `DiarizeOutput` from pyannote.audio 4.x, replace deprecated `use_auth_token` parameter, normalize audio to WAV before diarization.
- **Nginx Upload and Proxy Configuration** — Dedicated upload endpoint with `proxy_request_buffering off` for streaming; 1 GB upload limit; optimized proxy headers for large file operations.
- **AI Revision Cross-Contamination** (#494) — Prevent RAG revision from injecting unrelated note content.
- **Extraction Reliability** — Fix job deduplication, OGG format detection, and timeout handling.
- **Inline Disposition for Media** — Use inline content disposition for browser-playable media types; fix CORS headers for streaming.
- **Extraction Re-queue Logic** — Only re-queue downstream NLP jobs when extraction actually updates note content.
- **AI Description Propagation** (#492) — Persist `ai_description` from vision/3D extraction and propagate to note metadata.
- **SSE Progress Alignment** — Align progress events with documented checkpoint percentages across all handlers.
- **Clippy Lint Warnings** — Fix `nonminimal_bool`, `neg_cmp_op_on_partial_ord`, and `approx_constant` in test assertions.

### Documentation

- CPU-only deployment guide (`docs/content/cpu-only-deployment.md`)
- Job monitoring guide expanded with multi-chunk tracking, tier escalation, and SSE event emission completeness table
- Extraction pipeline design updated with all 13 extraction strategies and derived file documentation
- MediaOptimize handler documented in job monitoring guide (progress stages, variant types, download endpoint)
- KeyframeVision, KeyframeAssembly, and ThumbnailSprite handlers documented in job monitoring guide
- Media Integration Guide (`docs/content/media-integration-guide.md`) — frontend integration for streaming playback, subtitles, sprite sheets, TUS uploads, SSE events
- Full documentation sync with current code state

## [2026.2.11] - 2026-02-20

### Fixed

- **Stale Job Reaping** — Worker automatically reaps orphaned `running` jobs on startup. Jobs stuck longer than 2× the timeout threshold (600 s) are reset to `pending` (with retries remaining) or failed. Uses `FOR UPDATE SKIP LOCKED` CTE to avoid blocking concurrent workers. ([ADR-084](docs/architecture/adr/ADR-084-stale-job-reaping.md))
- **PDF Null Byte Sanitization** — `PdfTextAdapter` now strips null bytes (`\0`) from both `pdfinfo` metadata values and extracted text before database insertion, preventing PostgreSQL `22P05` encoding errors on legacy PDFs (Acrobat 3.0/4.0 era). ([ADR-085](docs/architecture/adr/ADR-085-null-byte-sanitization.md))
- **Ambiguous Column in Reap CTE** — Fully qualified `retry_count` reference in the stale-job reap query to prevent PostgreSQL ambiguity error.
- **Archive Note Counts** — `list_archive_schemas` now computes live note counts instead of returning stale cached values.

### Changed

- **Testdb `max_locks_per_transaction`** — Increased to 256 in the test database image to support parallel archive schema tests without lock exhaustion.

### Documentation

- ADR-084: Stale job reaping on worker startup
- ADR-085: Null byte sanitization in PDF extraction pipeline
- ADR index backfilled with all entries ADR-037 through ADR-085
- Operators guide: automatic stale job recovery section
- Troubleshooting: stale jobs, PDF null byte errors
- Retired stale UAT reports

## [2026.2.10] - 2026-02-19

### Highlights

This release delivers **86 commits**, **206 files changed**, **+19K/-41K lines** closing **64 issues** (#422–#485)
across three major themes: a complete **Graph Quality Overhaul** (Louvain community detection, SNN scoring,
PFNET sparsification, automated maintenance pipeline), a comprehensive **SSE Event System** (46 event types,
replay, filtering, backpressure), and a hardened **Extraction Pipeline** (GLiNER NER, tiered job architecture,
cascaded model routing, configurable document composition for embeddings).

| What Changed | Why You Care | Issues |
|--------------|--------------|--------|
| **Graph Quality Pipeline** | Automated normalize → SNN → PFNET → diagnostics in a single API call. Breaks the "seashell" pattern of noisy, unstructured graphs. | #470–#484 |
| **Louvain Community Detection** | Topically cohesive note clusters with SKOS-derived labels | #473 |
| **SNN + PFNET Graph Analysis** | Structural similarity scoring and topology-preserving edge pruning | #474, #476 |
| **SSE Event System Overhaul** | 46 event types, versioned envelope, replay, auth-scoped filtering, backpressure | #450–#465 |
| **Tiered Job Architecture** | Three-tier compute model (CPU_NER → FAST_GPU → STANDARD_GPU) with queue-based escalation | #436–#449 |
| **GLiNER NER Sidecar** | Zero-shot NER at <300ms/doc, CPU-only, enabled by default | #437 |
| **Configurable Embedding Composition** | Choose what goes into embeddings (title, content, concepts, tags) per embedding set | #485 |
| **Versioned Graph API** | v1 payload contract with community hints and server-side guardrails | #467–#469 |
| **Pause/Resume Jobs** | Global and per-archive job processing control, persisted across restarts | #466 |
| **Multi-Provider Inference** | Provider-qualified model slugs with discovery endpoint | #431 |
| **12 New ADRs** | Architecture decisions documented for graph, embeddings, jobs, inference, and branding | ADR-072–ADR-083 |

### Added

- **Graph Quality Maintenance Pipeline** (#482) — Automated `normalize → SNN → PFNET → diagnostics snapshot` pipeline triggered via `POST /api/v1/graph/maintenance`. Brings graph topology to a consistent, analytically useful state in a single operation.

- **Louvain Community Detection** (#473) — Partition the knowledge graph into topically cohesive communities using the Louvain algorithm. Community labels are derived from SKOS concept terms for human-readable groupings.

- **SNN Similarity Scoring** (#474) — Shared Nearest Neighbor (SNN) scoring for structural graph analysis. Identifies strongly connected note clusters based on neighborhood overlap rather than raw embedding distance.

- **PFNET Sparsification** (#476) — Pathfinder Network (PFNET) algorithm for topology-preserving edge pruning. Removes redundant edges while retaining the shortest-path skeleton of the knowledge graph.

- **MRL 64-dim Coarse Community Detection** (#477) — Fast community detection using 64-dimensional Matryoshka Representation Learning embeddings. Provides coarse-grained topic groupings at significantly lower compute cost than full-dimension clustering.

- **Edge Community Filter and Structural Collection Edges** (#480) — Filter graph edges by community membership (intra-community or inter-community). Structural edges from collection membership are included in graph payloads for richer topology.

- **Embedding Quality Diagnostics with Snapshot Comparison** (#483, #484) — Capture point-in-time embedding quality metrics (coverage, dimension statistics, cluster cohesion) and compare against prior snapshots to detect quality drift over time.

- **Graph Maintenance API Endpoint** (#482) — `POST /api/v1/graph/maintenance` triggers the full maintenance pipeline. Returns a structured report with per-step outcomes and timing.

- **7 New Graph API Endpoints** — New endpoints for graph maintenance, diagnostics, community inspection, SNN scoring, PFNET sparsification, coarse community detection, and edge community filtering.

- **2 New MCP Core Tools** — `trigger_graph_maintenance` and `coarse_community_detection` added to the core tool surface (37 core tools total).

- **20 Graph Algorithm Unit Tests** — Unit test coverage for normalization, SNN, Louvain, and PFNET implementations.

- **Pause/Resume Job Processing** (#466) — Global and per-archive pause/resume control for the job worker. State persisted in `system_config` table across container restarts. Endpoints: `GET /api/v1/jobs/status`, `POST /api/v1/jobs/pause`, `POST /api/v1/jobs/resume`, `POST /api/v1/jobs/pause/{archive}`, `POST /api/v1/jobs/resume/{archive}`.

- **Tiered Job Architecture** — Three-tier compute model: CPU_NER (tier 0, GLiNER), FAST_GPU (tier 1, qwen3:8b), STANDARD_GPU (tier 2, gpt-oss:20b). Queue-based tier escalation replaces inline model fallback — each job runs exactly one model, failures enqueue at the next tier.

- **GLiNER NER Sidecar** (#437) — Zero-shot named entity recognition via GLiNER (0.5B BERT, CPU-only, <300ms/doc). Runs as a Docker sidecar (`http://gliner:8090`). Enabled by default in Docker bundle; set `GLINER_BASE_URL=` to disable.

- **Fast-First Chunked Extraction** (#439) — Small model (qwen3:8b) handles concept tagging, reference extraction, and title generation with automatic document chunking. Large documents split into context-window-sized chunks and processed in parallel.

- **Related Concept Inference** (#435) — New pipeline step infers `skos:related` relationships between extracted concepts using LLM analysis.

- **Reference Extraction** — Bibliographic reference and entity extraction pipeline step with provenance metadata.

- **Metadata Extraction & Document Type Inference** (#430) — AI-extracted structured metadata (authors, year, venue, DOI) and automatic document type classification from filename/MIME/content.

- **SSE Event System Overhaul** (#451-#465) — Versioned envelope schema, memory-scoped auth routing, server-side type/entity filtering, `Last-Event-ID` replay, backpressure with event coalescing, health metrics, and expanded catalog to 46 event types covering notes, attachments, collections, archives, jobs, and system events.

- **Multi-Provider Inference Routing** (#431) — Provider-qualified model slugs (`ollama:qwen3:8b`, `openai:gpt-4o`) with model discovery endpoint and per-operation model selection.

- **SKOS Concept Scheme Management** — `manage_concepts` MCP tool extended with scheme CRUD operations.

- **Constrained JSON Decoding** — Structured JSON output for extraction jobs using Ollama's constrained generation.

- **SKOS Concepts in NoteFull** — API responses include SKOS concept tags on note detail endpoints.

- **Hierarchical SKOS Auto-Tagging** (#425) — Concept tagging uses hierarchical SKOS broader/narrower relationships for richer taxonomy.

- **AsyncAPI 3.0 Spec** — Runtime-generated AsyncAPI 3.0 specification for the SSE event catalog at `/api/v1/asyncapi`.

- **Configurable Embedding Document Composition** (#485) — Choose which fields compose embedding text (title, content, concepts, tags) per embedding set. Stored as `DocumentComposition` model with migration. MCP `manage_embeddings` tool extended with `document_composition` action.

- **Auto Re-embed on Composition Change** (#485) — Changing an embedding set's composition config automatically triggers re-embedding of all affected notes.

- **Versioned Graph API Payload Contract** (#467, #468, #469) — v1 versioned response format for all graph endpoints with community hints, neighborhood explainability, and server-side guardrails with tuning defaults.

- **Automatic Graph Maintenance After Embedding** — Graph maintenance pipeline auto-triggers after note ingest completes, keeping graph topology fresh without manual intervention.

- **RNG Linking Strategy** (#478) — Relative Neighborhood Graph for local edge sparsification, complementing PFNET for different graph density regimes.

- **Parallel Subtask Execution** (#441) — Extraction pipeline subtasks execute in parallel where dependencies allow, improving throughput on multi-core systems.

### Changed

- **Embedding enrichment instruction prefix** (#472) — Embeddings generated for graph use now include a `clustering:` instruction prefix, improving cluster cohesion in community detection.

- **Embedding content separated from record metadata** (#479) — Embedding payloads no longer bundle record metadata fields. Content and metadata are stored and retrieved separately, reducing payload size and eliminating cross-contamination during similarity scoring.

- **TF-IDF concept filtering excludes high-frequency concepts** (#475) — Concepts appearing in a large proportion of notes are treated as "stopword" concepts and excluded from graph edge weighting, reducing noise in dense graphs.

- **Edge weight normalization with configurable gamma** (#470) — Graph edge weights are normalized using a configurable gamma parameter, making weight distributions comparable across archives of different sizes.

- **GraphConfig extended with 10+ new tuning parameters** — New parameters cover SNN neighborhood size, PFNET `q` and `r` values, Louvain resolution, MRL dimension selection, gamma normalization, and community filter mode.

- **MCP core tools** expanded from 27 to 37: added `trigger_graph_maintenance`, `coarse_community_detection`, and 8 additional graph/observability tools.

- **Default concept target** lowered from 15 to 5 per note (`EXTRACTION_TARGET_CONCEPTS`).
- **Pipeline reorder** (#424) — Embedding now runs after concept tagging, using enriched content for better semantic search.
- **GLiNER extracted to sidecar** — Removed from bundle image, runs as independent container for simpler upgrades and resource isolation.
- **Inference endpoint** — Switched from `/api/generate` to `/api/chat` for Ollama generation.
- **Job handlers normalized** — MetadataExtraction, TitleGeneration, and RelatedConceptInference handlers now use queue-based tier escalation (matching ConceptTagging and ReferenceExtraction pattern).
- **Workspace version**: `2026.2.9` → `2026.2.10` (86 commits)

### Deprecated

- **Mutual k-NN filter** (#471) — Deferred as a no-op. The `create_reciprocal` step already enforces bidirectional edges, making a separate mutual k-NN filter redundant. The filter remains in the codebase but is not applied during graph construction.

### Fixed

- LLM returning object instead of array in JSON parsing for extraction jobs.
- SKOS breadth limit only counts promoted concepts toward the limit.
- SKOS breadth limit raised from 50 to 200 children per concept.
- WebSocket endpoint routing at `/api/v1/ws` (#423).
- Multi-memory schema context in extraction pipeline (#426).
- MCP StreamableHTTP transport JSON responses (#422).
- Default concept scheme seeding in new archives.
- Chunked extraction resilience to partial failures.
- Prevent 20B model escalation when GLiNER produces enough concepts.
- Three graph bugs causing stuck jobs and empty graph output (#485).
- Concept labels removed from default embedding text to reduce noise in similarity scoring (#485).
- Tiered job escalation: phase-2 and phase-3 jobs no longer queue prematurely on tier escalation (#444, #445).
- Escalation methods now use `queue_deduplicated()` instead of `queue()` (#446).
- Job `warmup()` timeout added to prevent stalling the drain loop (#447).
- Tier-0 NER skips DB round-trip when GLiNER backend is unavailable (#448).
- Clippy warnings resolved for Rust 1.92 compatibility.

### Documentation

- **Comprehensive documentation overhaul** — Rewrote and consolidated graph quality pipeline docs, extraction pipeline docs, and architecture guides.
- **12 new ADRs** (ADR-072 through ADR-083): inference provider abstraction, graph quality pipeline, Louvain community detection, PFNET sparsification, MRL coarse community detection, embedding content separation, SNN sparse graph guard, global job deduplication, auto graph maintenance, document composition, queue-based tier escalation, brand naming.
- **SSE event catalog and migration guide** rewritten (#461).
- **Tagging and inference docs** updated to clarify automatic vs manual tagging.
- **fortemi-docs shard** rebuilt from current sources.
- **CLAUDE.md accuracy pass** — Updated MCP core tool list to 37 tools, full tool count to 202, test isolation guidance to recommend UUIDs.
- **README.md accuracy pass** — Updated full tool count from 187 to 202.

### Issues Resolved

**64 issues closed** (#422–#485):

- **Epics**: #436 (RLM Extraction Pipeline), #450 (SSE Multi-Client Reactive State), #481 (Graph Quality Overhaul)
- **Graph Quality**: #467–#470, #471, #472, #473, #474, #475, #476, #477, #478, #479, #480, #482, #483, #484
- **SSE Events**: #451–#465
- **Extraction Pipeline**: #437, #438, #439, #440, #441, #442
- **Tiered Jobs**: #443–#449, #466
- **Embeddings**: #485
- **Multi-Provider Inference**: #431
- **SKOS/Tagging**: #425, #430, #435
- **Fixes**: #422, #423, #424, #426, #428, #429

## [2026.2.9] - 2026-02-16

### Highlights

This is the largest release since the project's inception — **290 commits**, **756 files changed**,
**+103,000 / -154,000 lines** across every layer of the stack. The headline feature is **Multi-Memory
Architecture**: fully isolated knowledge bases backed by PostgreSQL schema-per-memory isolation,
with zero-drift cloning, per-request routing, session-scoped MCP memory selection, federated
cross-memory search, and memory-scoped backup/restore.

Alongside multi-memory, this release resolves **100+ issues** discovered during comprehensive UAT
(530+ MCP test cases, 96.3% pass rate), upgrades PostgreSQL 16 → 18, enables SCRAM-SHA-256 auth,
ships native uuidv7() defaults, adds agent-friendly MCP tool surface (27 core tools with
discriminated-union pattern), hardens security with resource limits and SQL injection fixes,
rewrites the database restore pipeline, adds comprehensive content extraction framework,
ships multimodal capabilities (vision, audio, video, 3D models), and includes a built-in
243-note documentation archive loaded on first boot.

| What Changed | Why You Care | Learn More |
|--------------|--------------|------------|
| **Multi-Memory Architecture** | Create, switch, and isolate independent knowledge bases per PostgreSQL schema | [User Guide](docs/content/multi-memory.md) · [Design](docs/architecture/multi-memory-design.md) · [ADR-068](docs/adr/ADR-068-archive-isolation-routing.md) |
| **PostgreSQL 18 + SCRAM-SHA-256** | Major database upgrade with enhanced password authentication security | [ADR-096](docs/adr/ADR-096-scram-sha256-auth.md) |
| **MCP Agent-Friendly Tools** | 23-tool "core" mode with discriminated-union pattern (capture_knowledge, search, record_provenance, manage_tags, manage_collection, manage_concepts) | [MCP Guide](docs/content/mcp.md) · [ADR-095](docs/adr/ADR-095-mcp-tool-surface.md) |
| **X-Fortemi-Memory Header** | Per-request memory routing with 3-step fallback (header → default cache → public) | [Architecture](docs/content/architecture.md) |
| **MCP Session Memory** | `select_memory` / `get_active_memory` tools bind a memory to an AI agent session | [MCP Guide](docs/content/mcp.md) · [Agent Guide](docs/content/multi-memory-agent-guide.md) |
| **Federated Search** | Search across multiple memories in a single query | [Search Guide](docs/content/search-guide.md) |
| **Per-Archive Search** | Enable semantic and FTS search in non-default archives with schema-pinned connection pools | [Search Guide](docs/content/search-guide.md) |
| **Memory-Scoped Backup/Restore** | Per-memory `pg_dump --schema` and `DROP SCHEMA CASCADE` restore | [Backup Guide](docs/content/backup.md) · [Operations](docs/content/operations.md) |
| **Content Extraction Pipeline** | Document type registry (131 types), smart chunking, PDF/code/media adapters | [Document Types](docs/content/document-types-guide.md) · [Extraction Design](docs/content/extraction-pipeline-design.md) |
| **Video Multimodal Extraction** | Scene-detection keyframe extraction + audio-visual alignment + temporal context | [Video Guide](docs/content/video-guide.md) |
| **3D Model Understanding** | Multi-view rendering extraction via Three.js + vision model description | [3D Models Guide](docs/content/3d-models-guide.md) |
| **Auth Middleware & OAuth Scopes** | Centralized scope enforcement, configurable token lifetimes, API key support | [Authentication](docs/content/authentication.md) · [ADR-071](docs/architecture/ADR-071-auth-middleware.md) |
| **MCP File-Based I/O** | Replaced base64 binary tools with HTTP API upload/download for remote agents | [MCP Guide](docs/content/mcp.md) · [File Attachments](docs/content/file-attachments.md) |
| **Database Restore Rewrite** | Thread-safe psql pipe, extension-owned object exclusion, FTS index rebuild | [Backup Guide](docs/content/backup.md) |
| **Security Hardening** | SQL injection fixes, resource limits, input validation, wildcard injection prevention | [Security](docs/content/security.md) |
| **Multipart Shard Upload** | Upload knowledge shards via `multipart/form-data` — no base64 overhead, supports large shards | [Backup Guide](docs/content/backup.md) |
| **Built-in Documentation Archive** | 243-note `fortemi-docs` knowledge base automatically loaded on first boot | [Getting Started](docs/content/getting-started.md) |
| **Adaptive Tag-Boosted Linking** | Two-phase linking pipeline: tag-overlap candidates boosted before semantic scoring | [Knowledge Graph](docs/content/knowledge-graph-guide.md) |
| **Event-Driven Job Worker** | PostgreSQL NOTIFY/LISTEN wake pattern with concurrent job processing | [Configuration](docs/content/configuration.md) |
| **PG 18.2 TOAST Workaround** | Automatic workaround for PostgreSQL 18.2 substring/left() UTF-8 bug (#19406) | [Troubleshooting](docs/content/troubleshooting.md) |

### Added

- **Multipart Shard Upload** — `POST /api/v1/backup/knowledge-shard/upload` accepts `multipart/form-data` file uploads, eliminating base64 encoding overhead and ARG_MAX limits for large shards. JSON endpoint preserved for backward compatibility.

- **Built-in Documentation Archive** (#411) — On first boot, the Docker bundle automatically imports a 243-note `fortemi-docs` knowledge base containing all user guides, architecture docs, research papers, ADRs, and SDLC artifacts. Idempotent via flag file at `$PGDATA/.fortemi-docs-seeded`.

- **Adaptive Tag-Boosted Linking** (#420) — Two-phase auto-linking pipeline: Phase 1 discovers tag-overlap candidates and boosts their similarity scores; Phase 2 applies standard embedding-based linking. Produces denser, more meaningful knowledge graphs.

- **Event-Driven Job Worker** — PostgreSQL `NOTIFY`/`LISTEN` wake pattern replaces polling for immediate job pickup. Configurable concurrent processing via `JOB_MAX_CONCURRENT` (default: 4) with drain-loop shutdown.

- **MCP Core Tools Expanded** — Added `manage_archives`, `manage_encryption`, `manage_backups`, and `manage_embeddings` to the 23-tool core surface (now 27 core tools).

- **Vision and Audio Enabled by Default** — `OLLAMA_VISION_MODEL` and `WHISPER_BASE_URL` now configured by default in Docker bundle for out-of-box multimodal extraction.

- **Capacity Planning** — `MAX_MEMORIES` scales with hardware: 10 (8GB), 50 (16GB), 200 (32GB), 500 (64GB+). Documentation updated with sizing guidance.

- **Per-Archive Search** — Enable search in non-default archives
  - Per-schema connection pools with `search_path` pinned per archive
  - Cached `HybridSearchEngine` instances per schema
  - Removes the 400 guard for non-default archives
  - Enables semantic and FTS search in all memory archives

- **HNSW Algorithm 4 Graph Topology** (#386) — Graph topology statistics using HNSW Algorithm 4 for efficient neighbor traversal

- **Light Revision + Softer Licensing** — AI revision defaults to light mode with improved licensing messaging

- **Live Health Probe** — `/health/live` readiness probe with dependency checks for all critical services

- **Move Collection with Cycle Detection** — Move collections in hierarchy with circular reference prevention

- **PKE Keyset REST API** — Full REST API endpoints for PKE keyset management

- **Ad-hoc Image Description API** — `POST /api/v1/vision/describe` + MCP `describe_image` tool

- **Auto-Generated OpenAPI Spec** — utoipa replaces static OpenAPI spec with auto-generation from code annotations

- **EXIF Metadata Extraction** (#278) — Automatic EXIF metadata extraction on image upload

- **Note-Level Provenance** (#262) — Notes can have location + time provenance for spatial-temporal context

- **Provenance Creation MCP Tools** (#261) — MCP tools for recording W3C PROV provenance

#### Multi-Memory Architecture (#170 Epic, #171–#181)

The flagship feature of this release. Each "memory" is a fully isolated PostgreSQL schema containing
all per-memory tables (notes, tags, collections, links, embeddings, SKOS concepts, files, templates,
etc.) while sharing infrastructure tables (auth, jobs, migrations) in the public schema.

- **Zero-drift schema cloning** (#171) — `CREATE TABLE ... (LIKE public.table INCLUDING ALL)` with
  deny-list approach. New migrations automatically included without code changes. FK discovery from
  `information_schema` with proper schema-qualification.
- **Text search config cloning** (#172) — Custom FTS configurations (e.g., `matric_english`) cloned
  into each memory schema via `pg_ts_config` catalog queries.
- **Per-request memory selection** (#173) — `X-Fortemi-Memory` header on every API request. Middleware
  validates memory exists (404), resolves schema, injects `ArchiveContext`. 3-step fallback:
  header → `DefaultArchiveCache` (60s TTL) → public schema.
- **All 91 API handlers routed** — Every handler uses `SchemaContext` with `SET LOCAL search_path`
  per transaction. `_tx` method pattern on all repositories for transaction-scoped isolation.
- **MCP session memory selection** (#174) — `select_memory` and `get_active_memory` tools. Session
  state tracked per transport. All MCP API calls automatically include `X-Fortemi-Memory` header.
- **Memory-scoped backup** (#175) — `GET /api/v1/backup/memory/:name` using `pg_dump --schema`.
- **Memory-scoped restore** (#176) — `DROP SCHEMA IF EXISTS CASCADE` + `pg_restore`. Clean and atomic
  because memories are self-contained schemas.
- **Cross-memory federated search** (#177) — `POST /api/v1/search/federated` with dynamic UNION ALL
  across specified schemas. Results annotated with `memory_name`.
- **Memory clone endpoint** (#178) — `POST /api/v1/memories/:name/clone` with FK-ordered
  `INSERT...SELECT` via recursive CTE. Handles generated columns. No superuser required.
- **Memory API naming** (#179) — `/api/v1/memories/*` routes with `/api/v1/archives/*` backward
  compatibility. MCP tools use "memory" terminology.
- **Schema drift detection test** (#180) — CI-time integration test comparing archive table/column
  structure against public schema.
- **Default archive seed migration** (#158) — Fresh deployments now seed a default archive pointing
  to the public schema.
- **Archive schema version tracking** — `schema_version` column on `archive_registry` for
  auto-sync detection.

#### Video Multimodal Extraction

Enhanced video processing via attachment pipeline:
- Scene-detection keyframe extraction using ffmpeg (`select='gt(scene,0.3)'`)
- Frame-to-frame temporal context: sliding window of 3 previous descriptions in vision prompts
- Audio-visual alignment: transcript segments matched to frame timestamps (+/- 5s window)
- `KeyframeStrategy` enum: `Interval`, `SceneDetection`, `Hybrid` modes
- `VideoMultimodalAdapter` wired into extraction pipeline (requires ffmpeg + vision/whisper)
- MCP `process_video` guidance tool directs agents to attachment upload workflow
- MCP documentation topic (`get_documentation({ topic: "video" })`)
- `get_system_info` reports video extraction status (`extraction.video`)
- UAT Phase 2F with 10 test cases (4 always-execute, 6 conditional on ffmpeg)
- All video processing goes through attachment pipeline — no ad-hoc base64 API

#### 3D Model Understanding

Multi-view rendering extraction via attachment pipeline:
- `Glb3DModelAdapter` with Three.js headless multi-view rendering + vision model description
- `ExtractionStrategy::Glb3DModel` variant routes all `model/*` MIME types
- Lightweight Node.js renderer using Three.js + headless-gl (replaces heavyweight Blender)
- Configurable view count (default 6, min 3, max 15) from multiple camera angles
- Composite synthesis: individual view descriptions combined into holistic summary
- MCP `process_3d_model` guidance tool directs agents to attachment upload workflow
- MCP documentation topic (`get_documentation({ topic: "3d-models" })`)
- `get_system_info` reports 3D model extraction status (`extraction.3d_model`)
- Bundled Three.js renderer at `RENDERER_URL` (default: localhost:8080) + vision backend
- UAT Phase 2G with 10 test cases (5 always-execute, 5 conditional on renderer + vision)
- All 3D model processing goes through attachment pipeline — no ad-hoc base64 API

#### Audio Transcription

Ad-hoc audio transcription via Whisper-compatible backend:
- Wires existing `TranscriptionBackend` trait + `WhisperBackend` into API server
- `POST /api/v1/audio/transcribe` API endpoint (base64 audio, optional mime_type and language)
- MCP `transcribe_audio` tool for agent access
- `AudioTranscribeAdapter` registered in extraction pipeline for automatic attachment processing
- Configurable via `WHISPER_BASE_URL` and `WHISPER_MODEL` env vars
- Returns transcription text, timestamped segments, detected language, duration, model, and audio size
- 503 Service Unavailable when transcription backend not configured
- Health check integration via `get_system_info` (`extraction.audio.enabled`)
- Supports WAV, MP3, OGG, FLAC, AAC, WebM formats
- MCP documentation topic (`get_documentation({ topic: "audio" })`)
- Bundled with GPU Whisper by default in Docker bundle

#### Vision (Image Description)

Ad-hoc image description via Ollama vision LLM:
- `VisionBackend` trait + `OllamaVisionBackend` in `matric-inference` crate
- `POST /api/v1/vision/describe` API endpoint (base64 image, optional mime_type and prompt)
- MCP `describe_image` tool for agent access
- Configurable via `OLLAMA_VISION_MODEL` env var (e.g., `qwen3-vl:8b`, `llava`)
- Returns AI-generated description, model name, and decoded image size
- 503 Service Unavailable when vision model not configured
- Health check integration via `get_system_info` (`extraction.vision.available`)
- UAT Phase 2D with 8 test cases

#### Content Extraction Pipeline (#87–#99, #101, #102)

- Complete content extraction framework with pluggable adapters
- **Document Type Registry** — 131 pre-configured types across 19 categories
- Auto-detection from filename patterns, extensions, and content analysis
- Category-specific chunking strategies (semantic, syntactic, per_section, fixed)
- REST API and MCP tools for document type management
- See: [Document Types Guide](docs/content/document-types-guide.md), [Extraction Design](docs/content/extraction-pipeline-design.md)

#### Authentication & Security (#103, #111, #112, #114, #115, #118, #119)

- **Auth middleware** — Centralized Bearer token validation with `REQUIRE_AUTH` toggle
- **OAuth2 scope enforcement** — Centralized scope checks for all mutation endpoints
- **Configurable OAuth token lifetimes** — `ACCESS_TOKEN_TTL` and `REFRESH_TOKEN_TTL` env vars
- **API key system** — `POST /api/v1/api-keys` for programmatic access
- **PKE HTTP API** — Encryption tools accessible via REST (not just CLI binary)
- See: [Authentication Guide](docs/content/authentication.md), [ADR-071](docs/architecture/ADR-071-auth-middleware.md)

#### Archive Isolation Pipeline (#86, #107–#110, #113)

- Archive creation with full schema cloning
- Archive metadata (stats, version tracking)
- PKE keyset registry per archive
- Archive-scoped operations

#### MCP Server Improvements

- **Agent-friendly tool surface** (#365) — 23-tool "core" mode with discriminated-union pattern
  (capture_knowledge, search, record_provenance, manage_tags, manage_collection, manage_concepts).
  Set `MCP_TOOL_MODE=full` for all 187 tools.
- **File-based I/O pattern** — Replaced base64 binary tools with HTTP API upload/download.
  MCP tools now guide agents to use `POST /api/v1/attachments` multipart upload.
- **Tool definition extraction** — `tools.js` extracted from `index.js` for maintainability
- **Automated JSON Schema validation** — All 100+ MCP tool schemas validated against draft 2020-12
  on startup. One broken schema no longer blocks all tools.
- **MCP OAuth auto-registration** — Bundle entrypoint auto-registers OAuth client credentials
  on first startup. Credentials persisted at `$PGDATA/.fortemi-mcp-credentials`.
- **10+ tool descriptions updated** with memory scoping context and search limitation warnings
- **Session memory sync** — MCP session state sync when setting default archive (Issue #316)
- See: [MCP Guide](docs/content/mcp.md), [MCP Deployment](docs/content/mcp-deployment.md)

#### Eventing & Streaming Infrastructure

- Server-Sent Events (SSE) for real-time note change notifications
- WebSocket support for bidirectional streaming
- Webhook system for external integrations
- See: [Real-time Events](docs/content/real-time-events.md)

#### Documentation (10 files, +851/-216 lines)

- **NEW: Multi-Memory Agent Guide** — 15KB purpose-built guide for AI agents with decision matrix,
  5 segmentation strategies, tradeoffs table, and common mistakes.
  See: [Agent Guide](docs/content/multi-memory-agent-guide.md)
- **ADR-068 rewritten** — Full implementation status with all 91 handlers documented
- **Architecture docs updated** — Routing flow, transaction patterns, SchemaContext
- **Operations guide expanded** — Multi-memory monitoring, per-memory backup, troubleshooting
- **Backup guide expanded** — Per-memory backup procedures and restore caveats
- **MCP tool table expanded** — 8 → 12 memory management tools documented
- **CLAUDE.md updated** — Multi-memory section, MAX_MEMORIES config
- **Consolidated MCP docs** — Fixed public URLs, added OpenAPI CI export
- **UAT suite rewrite** — Rewrote UAT suite for 23-tool core surface

### Fixed

- **PG 18.2 TOAST UTF-8 bug** (#418) — Workaround for PostgreSQL bug #19406 where `substring()`/`left()` fail on TOAST-compressed text with multi-byte UTF-8 characters. Applied `left(convert_from(content::bytea, 'UTF8'), N)` pattern across 18 instances in 5 files. Revert tracked in #419 (after PG 18.3+).

- **Backup multi-memory headers** (#421) — All backup and restore handlers now properly respect the `X-Fortemi-Memory` header for memory-scoped operations, including knowledge-shard export and full backup endpoints.

- **Schema-qualified FTS configs** (#412) — Text search configurations now use schema-qualified names in all queries, fixing FTS failures in non-default memory archives.

- **Archive AI pipeline** — AI pipeline jobs (embedding, linking, title generation, concept tagging) now execute correctly for notes in non-default memory archives.

- **Orphaned job cleanup** — Archive deletion now cleans up orphaned jobs and FTS configurations, preventing stale job references.

- **MCP parameter validation** (#398) — Validate required params before search URLSearchParams serialization (prevents MCP crash on missing required search params)
- **Template tag merge** — Template instantiation now merges tags instead of override, preserving existing tags
- **Whisper transcription bundled** — Enable Whisper transcription by default with GPU in Docker bundle
- **MPEG-2/2.5 MP3 detection** — Detect MPEG-2/2.5 MP3 files for audio transcription (broader MP3 format support)
- **Attachment blob refcount** — Preserve shared blobs on sibling deletion (blob refcount safety)
- **Link similarity calibration** — Calibrate similarity thresholds by content type for better auto-linking accuracy
- **EXIF extraction gaps** — Resolve GPS, camera, and datetime extraction gaps (improved EXIF field coverage)
- **Temporal search null safety** — Resolve temporal search inconsistencies with null provenance
- **Content type validation** (#253) — Validate actual content type via magic bytes (security: prevent fake content type uploads)
- **Configurable upload size** (#257) — Make upload size limit configurable via `MATRIC_MAX_BODY_SIZE_BYTES` env var
- **Search cache invalidation** (#247) — Invalidate search cache on note delete/purge/restore (cache consistency)
- **Empty content support** — Accept empty content in create_note and bulk_create_notes (allow content-free notes for attachment-only)
- **Job deduplication** — Deduplicate against running jobs, not just pending (prevent duplicate job execution)
- **MCP numeric arguments** — Coerce numeric tool arguments to numbers before API calls (MCP parameter type safety)
- **MCP non-JSON responses** — Handle non-JSON responses in apiRequest (graceful error handling)
- **Binary media validation** — Enforce magic byte detection for binary media types (security: binary file validation)
- **Vision Ollama URL in Docker** — `OllamaVisionBackend` now reads `OLLAMA_BASE` env var first (matching embedding backend), fixing 500 errors in Docker containers where only `OLLAMA_BASE` is set

#### Database Restore Pipeline (Complete Rewrite)

The restore system was rewritten for correctness and robustness:

- **Thread-safe psql pipe** (#166) — `tokio::task::spawn_blocking` to prevent pipe deadlocks
  with large dumps
- **Extension-owned object exclusion** — DROP script queries `pg_depend` with `deptype = 'e'`
  to skip PostGIS-owned objects like `spatial_ref_sys`
- **Comprehensive object cleanup** — DROP tables, enum types, functions, text search configs,
  dictionaries, views, and sequences before restore
- **FTS index rebuild** — REINDEX + ANALYZE after restore to rebuild search indexes (#166)
- See: [Backup Guide](docs/content/backup.md)

#### UAT Wave 1: 17 Issues (#132–#151)

- **SKOS search fixes** (#132, #133, #134, #149) — `autocomplete_concepts` and `search_concepts`
  now work with custom schemes; `get_concept_full` returns complete data
- **Auth scope fixes** (#135, #138, #139, #140) — MCP tools for backup, archive, location search,
  and embedding config no longer return 403
- **Attachment upload** (#137, #150, #153, #154, #155) — File storage diagnostics, volume mount
  validation, HTTP API upload guidance for remote agents
- **MCP response fixes** (#141, #142) — `add_skos_collection_member` JSON parsing,
  `update_concept` null return
- **PKE address format** (#143) — PEM-stored keys now correctly converted to raw 32-byte binary
- **Time search validation** (#144–#148) — ISO 8601 timestamps with colons accepted; invalid
  coordinates rejected with 400
- **Job worker isolation** (#151) — Workers only claim jobs they have handlers for

#### UAT Wave 2: 18 Issues (#152–#169)

- **Error message sanitization** (#152, #163) — Raw SQL constraint names replaced with
  user-friendly messages
- **Attachment file I/O** (#153, #154, #155, #157) — Complete HTTP API workflow documented
  for remote MCP agents
- **MCP test tool names** (#156) — Attachment test references corrected
- **Default archive on fresh deploy** (#158) — `list_archives()` no longer returns empty
- **Archive note routing** (#159) — Notes now land in the active archive, not always public
- **SKOS relation cleanup** (#160) — `remove_related` cleans up inverse relations
- **SKOS export all schemes** (#161) — Export without scheme_id now exports all schemes
- **PKE remote access** (#162) — PKE tools work via API, not just local filesystem
- **Job reprocessing** (#164) — `reprocess_note` respects `steps` parameter
- **SKOS cascade delete** (#165) — `delete_concept_scheme(force=true)` cascade-deletes concepts
- **Archive schema completeness** (#169) — `note_original` table included in schema cloning

#### CI/CD & Build Fixes

- **Migration timestamp deduplication** — Duplicate prefixes cause `_sqlx_migrations_pkey` violations
- **CONCURRENTLY removed from migrations** — sqlx wraps migrations in transactions
- **COMMENT ON EXTENSION removed** — Requires superuser; non-owner can't comment
- **Env var race conditions eliminated** — Constructor injection replaces `std::env::set_var` in tests
- **Auto-migrate on startup** — `sqlx::migrate!()` runs on API startup from `main.rs`
- **Stale container cleanup** — CI kills containers by port before starting new ones

#### Refactoring

- **Centralized constants** (#60) — Magic numbers moved to `defaults.rs`
- **Algorithm config** (#61, #62) — Runtime-overridable algorithm parameters
- **Hardcoded chunking eliminated** — Chunking config now driven by document type registry

### Security

- **SQL injection prevention** (#215, #216, #217) — Critical SQL injection fixes across multiple endpoints
- **Public schema protection** (#244) — Prevent DROP SCHEMA public CASCADE in archive delete and restore (critical: prevent accidental public schema deletion)
- **Resource limits** (#218, #189) — Rate limiting, input size validation, connection pool limits
- **Input validation** (#218) — Comprehensive input validation across all endpoints
- **Wildcard injection prevention** (#216) — Prevent wildcard injection in pattern matching

### Changed

- **Workspace version**: `2026.2.7` → `2026.2.9` (290 commits)
- **PostgreSQL**: 16 → 18 (#396)
- **Authentication**: SCRAM-SHA-256 password authentication enabled (#397)
- **UUID generation**: Native uuidv7() function for UUID generation (#397)
- **MCP tool surface**: ~95 → 27 core tools (discriminated-union) / 187 full tools (#365)
- **Migration count**: 57 → 59+ migration files
- **API handler count**: 91 handlers, all schema-routed
- **Test infrastructure**: Two UAT passes (530+ MCP test cases, 96.3% pass rate)
- **UAT test cases**: 530+ MCP tests across multiple passes
- **Issues resolved**: 100+ issues since v2026.2.7

### Database Migrations

| Migration | Purpose |
|-----------|---------|
| `20260208000002_seed_default_archive.sql` | Seed default archive for fresh deployments |
| `20260208100000_archive_schema_version.sql` | Add schema_version tracking to archive_registry |
| PostgreSQL 18 upgrade | Major database engine upgrade |
| SCRAM-SHA-256 auth | Enhanced password authentication security |
| Native uuidv7() | Time-ordered UUID generation |

Plus significant refactoring of 28 existing migrations (removed CONCURRENTLY, fixed timestamps,
separated schema DDL from seed data).

### Breaking Changes

None. Full backward compatibility maintained:
- `/api/v1/archives/*` routes continue to work alongside new `/api/v1/memories/*`
- Default behavior (no `X-Fortemi-Memory` header) routes to public schema as before
- Existing MCP tool names preserved; new tools added with "memory" terminology

### Upgrade Notes

1. **Database migrations run automatically** on startup via `sqlx::migrate!()`
2. **PostgreSQL 18 upgrade** — Review upgrade notes in [ADR-096](docs/adr/ADR-096-postgresql-18.md)
3. **SCRAM-SHA-256 auth** — Existing passwords automatically upgraded on next login
4. **Fresh deployments** now seed a default archive — `list_archives()` returns the public schema
5. **MCP clients** should update tool descriptions — memory-scoping context added to 10+ tools
6. **Backup scripts** — If using custom backup scripts, consider switching to per-memory backup
   (`GET /api/v1/backup/memory/:name`) for targeted exports
7. **Docker bundle** — MCP OAuth credentials now auto-registered on first startup; manual
   registration no longer required
8. **Whisper transcription** — Now bundled by default with GPU in Docker bundle
9. **Shard import** — New `POST /api/v1/backup/knowledge-shard/upload` multipart endpoint available;
   existing JSON base64 endpoint remains for backward compatibility
10. **First-boot documentation** — Fresh Docker bundle deployments automatically load the
    `fortemi-docs` archive (243 notes of product documentation)

### Issues Resolved

**100+ issues closed** in this release:

- **Epic**: #170 (Multi-Memory Schema Isolation)
- **Multi-Memory**: #158, #159, #169, #171–#181
- **Auth & Security**: #135, #138, #139, #140, #152, #163, #215, #216, #217, #218, #244
- **MCP Server**: #134, #137, #141, #142, #149, #153, #174, #316, #365, #398
- **Backup & Restore**: #136, #166, #167, #168, #175, #176
- **Search**: #132, #133, #144–#148, #177
- **SKOS**: #160, #161, #165
- **Attachments**: #150, #154, #155, #157, #247, #253, #257
- **CI/Testing**: #151, #156, #319
- **PKE**: #143, #162
- **Jobs**: #164
- **Documentation**: #181
- **Database**: #396, #397
- **Extraction**: #278, #386
- **Provenance**: #261, #262
- **Linking**: #420
- **Seed**: #411
- **PG Compat**: #418

## [2026.2.7] - 2026-02-05

### Fixed
- **SKOS Collections MCP endpoints** - Fixed 404 errors on all SKOS collection tools (`list_skos_collections`, `create_skos_collection`, etc.) by correcting API paths from `/api/v1/skos/collections` to `/api/v1/concepts/collections` (#36)
- **SKOS Turtle export** - Fixed `export_skos_turtle` to use scheme_id as path parameter instead of query parameter (#36)
- **SKOS Collection create/update** - Fixed `ordered` → `is_ordered` field name mapping (#36)

## [2026.2.6] - 2026-02-05

### Changed
- **Repository reset** - Squashed history for clean baseline
- **License date updated** - BSL change date set to February 16th, 2030

## [2026.2.5] - 2026-02-05

### Fixed
- **Emoji search for ⭐ and arrow symbols** - Added missing Unicode range U+2B00-U+2BFF (Miscellaneous Symbols and Arrows) to emoji detection, fixing search for ⭐, ⬆️, ⬇️, etc.

### Documentation
- Updated search guide with comprehensive emoji Unicode range reference

## [2026.2.4] - 2026-02-05

### Fixed
- **MCP limit=0 parameter** - Fixed JavaScript falsy check that skipped `limit=0` parameter (#29)
  - Changed from `if (args.limit)` to `if (args.limit !== undefined && args.limit !== null)`
  - API now correctly returns 400 "limit must be >= 1"

### Verified
- **CJK 2+ character search** - Works correctly; single-char limitation is industry standard (#30)
- **Emoji search** - All patterns work: single, repeated, adjacent different emojis (#31)

## [2026.2.3] - 2026-02-05

### Fixed
- **UAT issues #29-#31 resolved**
  - `limit=0` now returns 400 "limit must be >= 1" instead of all notes (#29)
  - CJK single-character search now works (FTS flags enabled by default) (#30)
  - Emoji search now works (trigram fallback enabled) (#31)
- **OAuth endpoints routing** - Fixed nginx returning 405 HTML instead of proxying to API
- **CI/CD pipeline** - Fixed host runner PATH, duplicate docker socket mount, clippy compliance

### Added
- **Nginx proxy documentation** - `deploy/nginx/README.md` with SPA+API routing guidance

## [2026.2.2] - 2026-02-04

### Fixed
- **UAT findings resolved** (#13-#26) - All user acceptance testing issues addressed
- **MCP authorization_servers metadata** - Now correctly uses ISSUER_URL for OAuth discovery

### Changed
- README enhanced with plain-language vision statement
- Removed build status badge from README (unreliable external service)

## [2026.2.0] - 2026-02-02

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **CI/CD Pipeline Stabilization** | All tests pass reliably - no more `#[ignore]` workarounds |
| **Redis Container Integration** | Test container now includes Redis for full integration testing |
| **Worker Test Infrastructure** | Background job tests run serially with proper isolation |

### Fixed
- **CI worker tests** - Converted from `#[sqlx::test]` to `#[tokio::test]` to avoid `CREATE INDEX CONCURRENTLY` transaction conflicts
- **CI slow tests** - Fixed table name (`note_revised_current`), unique identifiers, check_source constraint, tstzrange bounds
- **CI Test Container** - Added Redis container for search cache integration testing
- **Checksum test flakiness** - Fixed Base58 non-uniformity causing intermittent test failures
- **Hierarchical tag filtering** (#283) - Tags now match with hierarchical prefix (e.g., `project` matches `project/alpha`)
- **limit=0 parameter handling** (#284) - MCP server now correctly returns empty array when limit=0
- **Case-insensitive tag matching** (#290) - Tag queries now use `LOWER()` for case-insensitive comparison
- **Ollama connectivity in Docker** (#287, #320) - Added `extra_hosts` configuration for Linux Docker containers
  - `host.docker.internal:host-gateway` enables container-to-host Ollama communication
  - OLLAMA_BASE environment variable now properly configured

### Added
- **File Attachment System** (#430-#440) - Intelligent file processing with provenance tracking
  - Content-addressable storage with BLAKE3 deduplication
  - EXIF metadata extraction (GPS, camera info, timestamps)
  - Multi-layer file safety validation (magic bytes, blocklist, sanitization)
  - Support for images, documents, audio, video, 3D models, and code files
  - Automatic processing via extraction strategies (Vision, AudioTranscribe, CodeAst, etc.)
  - UUIDv7 filesystem paths for large files
- **Temporal-Spatial Memory Search** (#437) - PostGIS-powered memory queries
  - Search by geographic location (radius queries)
  - Search by time range (capture date filtering)
  - Combined location + time intersection queries
  - Full provenance chain retrieval (location, device, temporal context)
- **W3C PROV Integration** (#434) - Standards-based provenance tracking
  - prov:atLocation with PostGIS geography type
  - prov:wasGeneratedBy for device attribution
  - Temporal ranges with tstzrange for capture time uncertainty
- **3D File Analysis** (#438) - Support for GLB, STL, OBJ formats
  - Geometric metadata extraction (vertices, faces, bounds)
  - Thumbnail generation via trimesh
- **Structured Media Formats** (#439) - SVG, MIDI, tracker module support
- **Embedding config MCP tools** (#298)
  - `list_embedding_configs` - List all embedding configurations
  - `get_default_embedding_config` - Get the default embedding configuration
- **Document Type Registry** - 131 pre-configured document types across 19 categories (#391-#411)
  - Automatic detection from filename, extension, and content patterns
  - Category-specific chunking strategies (semantic, syntactic, per_section, etc.)
  - REST API and MCP tools for document type management
  - Extensible with custom document types

### Changed
- Tag filtering in `list_notes`, `search_notes`, `strict_filter`, and `embedding_sets` now supports:
  - Case-insensitive matching: `LOWER(tag_name) = LOWER($1)`
  - Hierarchical matching: `LOWER(tag_name) LIKE LOWER($1) || '/%'`

## [2026.1.12] - 2026-02-01

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **FTS Unicode Normalization** | Search now matches accented/unaccented text (café ↔ cafe) |
| **MCP Security Hardening** | Error messages no longer leak implementation details |
| **Metadata API** | Notes can now store custom JSON metadata |
| **Tag Filtering in Search** | Search results can be filtered by tags |

### Added
- `metadata` field exposed in create/update note API endpoints (#359)
- `tags` parameter for `search_notes` MCP tool with strict filtering (#315)
- `validateUUID()` helper for clear parameter validation errors (#348)
- `sanitizeError()` helper to prevent information leakage (#346)
- FTS test suite for text search configuration verification

### Fixed
- **FTS accent/diacritic folding** - "café" now matches "cafe" search (#328)
  - Added `unaccent` PostgreSQL extension
  - Created `matric_english` text search configuration
  - All FTS queries updated to use new configuration
- **Embedding set ID assignment** - Embeddings now properly assigned to sets (#353)
  - `store()` method now sets `embedding_set_id` from default set
  - Migration backfills orphaned embeddings
- **MCP parameter validation** - Clear error messages for missing/invalid UUIDs (#348)
- **MCP error sanitization** - Internal errors no longer exposed to clients (#346)

### Changed
- All Rust FTS queries use `matric_english` config instead of `english`
- MCP error responses now return safe, user-friendly messages

### Database Migrations
- `20260131000000_fts_unicode_normalization.sql` - Unicode search support
- `20260131000001_fix_embedding_set_id.sql` - Embedding set backfill

## [2026.1.11] - 2026-01-31

### Fixed
- CI race condition when main branch and tag pushes run simultaneously
  - Container names now include `GITHUB_RUN_ID` for uniqueness
  - Database ports dynamically assigned to avoid conflicts
  - Affects both build and test-container jobs

## [2026.1.10] - 2026-01-31

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **CI Consolidation** | Single builder-based CI workflow for consistent, reproducible builds |
| **Test Infrastructure** | PostgreSQL test database properly integrated in CI pipeline |

### Changed
- Consolidated CI to single builder-only workflow (`ci-builder.yaml`)
  - Removed redundant `ci.yaml` (bare runner)
  - All builds now use pre-built builder container for consistency
- CI workflow renamed from "CI (Builder)" to "CI"

### Fixed
- PostgreSQL test database now properly spun up in CI for database-dependent tests
- 16 tag_resolver tests no longer fail due to missing database connection

### Removed
- `ci.yaml` - redundant bare-runner workflow (superseded by builder-based CI)

## [2026.1.9] - 2026-01-30

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **License Migration** | Moved from MIT/Apache-2.0 to BSL 1.1 with AGPL-3.0 change license |
| **Dependency Audit** | All 400+ dependencies verified BSL-compatible (no GPL conflicts) |
| **Licensing Documentation** | Plain-English licensing guide for users and enterprises |

### Changed
- **License**: Migrated from MIT/Apache-2.0 to Business Source License 1.1
  - Current: BSL 1.1 (production use requires commercial license)
  - After February 16, 2030: Converts to AGPL-3.0 (open source)
  - Personal, educational, and evaluation use remains free
  - See `docs/content/licensing.md` for plain-English explanation

### Added
- `LICENSE` - BSL 1.1 license terms with parameters
- `LICENSE.txt` - AGPL-3.0 full text (change license, effective 2030)
- `NOTICE` - Copyright and third-party attribution
- `docs/content/licensing.md` - Comprehensive licensing FAQ and guide

### Fixed
- Missing license metadata in `matric-search` crate Cargo.toml

### Security
- Completed dependency license audit: 400+ packages verified
- No GPL-only dependencies found (all permissive: MIT, Apache, BSD, ISC)
- All dependencies compatible with BSL 1.1 during proprietary period

## [2026.1.8] - 2026-01-30

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **CI/CD Pipeline Hardened** | Both ci.yaml and ci-builder.yaml now pass reliably with proper isolation |
| **GPU Tests Fixed Properly** | NVML driver mismatch resolved - no tests skipped or bypassed |
| **Build Container Docs** | Clear rationale for why we use containerized builds at Integro Labs |

### Fixed
- **Issue #207**: NVML driver/library version mismatch causing GPU integration test failures
  - Root cause: Kernel module out of sync with userspace libraries after update
  - Resolution: System reboot to load updated NVIDIA kernel module
- ci-builder PostgreSQL connectivity issues in Docker-based runners
  - Changed from `services:` directive to manual container management
  - Used isolated port 15432 to avoid conflicts with host PostgreSQL
- Workflow execution order for builder image updates
  - Added `paths-ignore` to prevent CI race conditions with builder updates
  - Added `trigger-ci` job in build-builder.yaml to dispatch CI after builder publishes

### Added
- Build container architecture documentation in `build/RUNNER_SETUP.md`
  - Runner label strategy (matric-builder, titan, gpu)
  - Rationale: Isolation, reproducibility, no version conflicts on shared dev servers
- Comprehensive environment variables in `.env.example` and Dockerfile

### Changed
- ci-builder.yaml now uses port 15432 for PostgreSQL (isolated from host)
- build-builder.yaml triggers CI workflows after successful builder image push

## [2026.1.7] - 2026-01-30

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **All-in-one Docker bundle** | Single container with PostgreSQL + API + MCP server for easy deployment |
| **matric-pke bundled** | PKE encryption binary included in container for MCP keyset operations |
| **Comprehensive env var docs** | All environment variables documented with comments in Dockerfile |

### Added
- All-in-one Docker bundle (`Dockerfile.bundle`, `docker-compose.bundle.yml`)
  - Embedded PostgreSQL 16 with pgvector extension
  - matric-api server on port 3000
  - MCP server on port 3001
  - `matric-pke` binary at `/usr/local/bin/matric-pke`
- Comprehensive environment variable documentation in Dockerfile
  - PostgreSQL, API, Ollama, OpenAI, and MCP configuration sections
  - Rate limiting controls (disabled by default in bundle)
  - OAuth/MCP client credential configuration

### Fixed
- MCP OAuth metadata now uses external `ISSUER_URL` instead of internal address
- MCP protected resource URL configurable via `MCP_BASE_URL`

## [2026.1.6] - 2026-01-30

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **update_note returns entity** | API now returns full note object after update (REST best practice) |
| **Backup auto-provisioning** | Backup directory created automatically on first use |
| **PKE keyset management** | 7 new MCP tools for managing encryption identities |

### Fixed
- **Issue #203**: `update_note` now returns full `NoteFull` object instead of HTTP 204
- **Issue #204**: `backup_status` auto-creates backup directory with graceful permission handling
- **Issue #205**: Backup tools now work out of the box (resolved by #204)

### Added
- PKE keyset management MCP tools:
  - `pke_list_keysets` - List all keysets in ~/.matric/keys/
  - `pke_create_keyset` - Create new named keyset with passphrase
  - `pke_get_active_keyset` - Get currently active keyset info
  - `pke_set_active_keyset` - Set active keyset by name
  - `pke_export_keyset` - Export keyset to directory for backup/transfer
  - `pke_import_keyset` - Import keyset from files or export directory
  - `pke_delete_keyset` - Delete a keyset permanently
- Backup status now returns "cannot_create_directory: {error}" on permission failure

### Changed
- `update_note` MCP handler returns `{ success: true, note }` instead of just `{ success: true }`
- Backup directory defaults to `/var/backups/matric-memory` (auto-created)

## [2026.1.5] - 2026-01-29

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **SQL Parameter Fix** | `update_note` with single field (archived/starred only) now works correctly |
| **String Tag Search** | `search_notes_strict` now accepts simple string tags, not just SKOS URIs |
| **MCP Content-Type Handling** | `diff_note_versions` returns plain text correctly |
| **PKE Deployment** | `matric-pke` encryption binary now deployed to production |

### Fixed
- **Issue #198**: SQL parameter mismatch in `update_note` when updating only `archived` or `starred`
- **Issue #199**: `search_notes_strict` with `required_tags` now supports simple string tags via fallback
- **Issue #201**: MCP server now handles `text/plain` responses (e.g., version diffs) correctly
- **Issue #202**: `matric-pke` binary built and deployed to `/usr/local/bin`

### Added
- `StrictTagFilter` now supports `required_string_tags`, `any_string_tags`, `excluded_string_tags`
- `simple_tag_exists()` method for simple tag lookup fallback
- Content-Type aware response parsing in MCP server

### Changed
- Dynamic SQL parameter indexing in note update operations
- Tag resolver tries SKOS concept first, falls back to simple tag if not found

## [2026.1.4] - 2026-01-29

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **Semantic Search Isolation Fix** | Critical fix: strict_filter now applies to vector search, preventing data leakage |
| **SKOS ENUM Fixes** | All SKOS APIs now correctly handle PostgreSQL ENUM types |
| **MCP strict_filter Fix** | MCP server correctly passes strict_filter parameter |

### Fixed
- **Critical: Semantic search data isolation** - strict_filter was only applied to FTS, not vector search
- **SKOS ENUM type casting** (Issue #197) - All SELECT/INSERT queries now properly cast ENUMs
- **MCP server strict_filter parameter** - Changed from "filters" to "strict_filter"
- **API strict_filter JSON parsing** - Query string now correctly deserializes nested JSON

### Added
- `find_similar_with_strict_filter()` for isolated semantic search
- `test-skos-regression.sh` - 17 regression tests for SKOS ENUM fixes
- `test-strict-search.sh` - 7 data isolation tests for strict_filter

## [2026.1.0] - 2026-01-24 (previous)

### Added

#### Research-Backed Modules (#162-165, #167-170, #172, #174, #176-177)
- **W3C PROV provenance tracking** (#162) - Activity/entity/relation models with full CRUD and chain queries
- **Self-Refine iterative revision** (#163) - Multi-pass AI revision pipeline with quality scoring
- **ReAct agent pattern** (#164) - Thought/action/observation traces for structured reasoning
- **Reflexion self-improvement** (#165) - Episodic memory for learning from past revisions
- **E5 embedding model support** (#167) - Asymmetric prefix support, ReEmbedAll job type
- **Miller's Law context limits** (#168) - 7±2 chunk limits for cognitive load management
- **BM25F field-weighted scoring** (#169) - Weighted scoring across title/body/tags fields
- **FAIR metadata export** (#170) - Dublin Core (ISO 15836), JSON-LD, compliance scoring
- **Few-shot prompt builder** (#172) - Curated in-context learning examples
- **Semantic link classification** (#174) - Typed links: supports/contradicts/extends
- **Adaptive RRF k-parameter** (#176) - Query-dependent k tuning (default k=20)
- **Dynamic HNSW ef_search** (#177) - Recall/latency trade-off tuning
- **SKOS Collections** (#175) - W3C SKOS labeled/ordered concept groups with full CRUD
- **RRF parameter tuning** (#187) - K=60→K=20, adaptive weights, Relative Score Fusion (RSF)

#### Infrastructure
- **UUIDv7 identifiers** (#178) - Time-ordered UUIDs with timestamp extraction
- **Unified strict filter system** (#179-184) - Multi-dimensional pre-search filtering (tags, temporal, collections, security)
- **Docker builder pattern for CI/CD** - Multi-stage builds with isolated container testing
- **Container API test suite** - 64 assertions covering all major API endpoints
- **Pre-commit hooks** - Automated formatting and lint checks

#### Documentation
- Comprehensive operators guide (`docs/guides/operators-guide.md`)
- Research foundation analysis with paper-level citations
- Architecture Decision Records (ADR) and test strategy documentation
- Professionalized multi-audience documentation structure

### Fixed
- Note versioning: populate `note_revised_current` on creation, fix provenance column name
- Note revision INSERT statements: correct column names
- Note original table: add missing `id` column
- Database constraints: use UNIQUE instead of duplicate PRIMARY KEY
- Database indexes: remove non-IMMUTABLE `NOW()` from index predicates
- CI pipeline: restructured build-before-test, GPU runner for integration tests

### Changed
- RRF default k parameter from 60 to 20 (better discrimination for small result sets)
- Test count: 933 → 1,056 tests passing (6 ignored)

## [2026.1.0] - 2026-01-24

### Highlights

| What Changed | Why You Care |
|--------------|--------------|
| **Strict Tag Filtering** | Guaranteed data segregation by SKOS tags/schemes - enables multi-tenancy |
| **W3C SKOS Tagging** | Hierarchical semantic tagging with broader/narrower/related relations |
| **Hybrid Search** | FTS + semantic + RRF fusion for best-of-both-worlds search |
| **MCP Server** | 65+ tools for AI agent integration (Claude, etc.) |
| **PKE Encryption** | X25519 public-key encryption for secure note sharing |
| **OpenAI Backend** | Support for OpenAI-compatible APIs (OpenAI, vLLM, OpenRouter) |

### Added

#### Core Features
- **Hybrid search engine** with Reciprocal Rank Fusion (RRF)
  - Full-text search via PostgreSQL tsvector/GIN
  - Semantic search via pgvector cosine similarity
  - Configurable weights and modes (hybrid/fts/semantic)
- **AI enhancement pipeline** for notes
  - Automatic revision with context from related notes
  - Embedding generation for semantic search
  - Title generation from content
  - Bidirectional semantic link creation (>70% similarity)
- **W3C SKOS-compliant tagging system**
  - Hierarchical concepts with broader/narrower/related relations
  - Concept schemes for vocabulary organization
  - Faceted classification (PMEST facets)
  - Tag governance with candidate/controlled/deprecated status
- **Strict tag filtering** (Epic #145)
  - Pre-search WHERE clause filtering for guaranteed isolation
  - Filter types: required_tags (AND), any_tags (OR), excluded_tags (NOT)
  - Scheme isolation: required_schemes, excluded_schemes
  - Foundation for multi-tenancy without separate databases
- **Collections** - Hierarchical folder organization for notes
- **Templates** - Reusable note structures with {{variable}} substitution
- **Note versioning** - Dual-track versioning preserving original and revised content

#### Infrastructure
- **MCP Server** with 65+ tools for AI agent integration
  - Note management (CRUD, search, export)
  - Collection and template management
  - SKOS concept management
  - Backup and knowledge shard operations
  - PKE encryption tools
- **PKE encryption** (matric-crypto crate)
  - X25519 ECDH key exchange
  - AES-256-GCM symmetric encryption
  - Multi-recipient envelope encryption
  - Wallet-style addresses with checksums
  - Argon2id-protected private key storage
- **Pluggable inference backends**
  - Ollama (default) - local inference
  - OpenAI-compatible APIs (feature-gated)
  - Model capability registry for recommendations
- **Background job processing**
  - Async NLP pipelines (embedding, revision, linking, title generation)
  - Priority-based job queue
  - Status tracking and monitoring
- **CI/CD pipeline** via GitHub Actions
  - Format checking, linting, testing
  - Integration tests with GPU + Ollama
  - Docker image builds

#### API & Documentation
- RESTful API with OpenAPI 3.1 specification
- Swagger UI at `/docs`
- Comprehensive documentation
  - Architecture guide
  - Integration guide
  - API reference
  - MCP server documentation
  - Encryption guide

### Database
- PostgreSQL 14+ with pgvector extension
- HNSW indexes for vector similarity search
- GIN indexes for full-text search
- Optimized indexes for strict tag filtering

### Security
- Input validation on all endpoints
- CORS support for browser access
- TLS termination at reverse proxy
- No stored credentials in codebase

---

## Version Format

This project uses **CalVer** (Calendar Versioning):

- Format: `YYYY.M.PATCH` (e.g., `2026.1.0`, `2026.12.3`)
- Year: 4 digits
- Month: 1-2 digits, no leading zeros
- Patch: Resets each month, starts at 0

Tags use `v` prefix: `v2026.1.0`

[Unreleased]: https://github.com/fortemi/fortemi/compare/v2026.2.12...HEAD
[2026.2.12]: https://github.com/fortemi/fortemi/compare/v2026.2.11...v2026.2.12
[2026.2.11]: https://github.com/fortemi/fortemi/compare/v2026.2.10...v2026.2.11
[2026.2.10]: https://github.com/fortemi/fortemi/compare/v2026.2.9...v2026.2.10
[2026.2.9]: https://github.com/fortemi/fortemi/compare/v2026.2.7...v2026.2.9
[2026.2.7]: https://github.com/fortemi/fortemi/compare/v2026.2.6...v2026.2.7
[2026.2.6]: https://github.com/fortemi/fortemi/compare/v2026.2.5...v2026.2.6
[2026.2.5]: https://github.com/fortemi/fortemi/compare/v2026.2.4...v2026.2.5
[2026.2.4]: https://github.com/fortemi/fortemi/compare/v2026.2.3...v2026.2.4
[2026.2.3]: https://github.com/fortemi/fortemi/compare/v2026.2.2...v2026.2.3
[2026.2.2]: https://github.com/fortemi/fortemi/compare/v2026.2.0...v2026.2.2
[2026.2.0]: https://github.com/fortemi/fortemi/compare/v2026.1.12...v2026.2.0
[2026.1.12]: https://github.com/fortemi/fortemi/compare/v2026.1.11...v2026.1.12
[2026.1.0]: https://github.com/fortemi/fortemi/releases/tag/v2026.1.0

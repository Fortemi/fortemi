# Changelog

All notable changes to matric-memory are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [CalVer](https://calver.org/) versioning: `YYYY.M.PATCH`.

## [Unreleased]

### Added

- **Video Multimodal Extraction** — Enhanced video processing via attachment pipeline
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

- **3D Model Understanding** — Multi-view rendering extraction via attachment pipeline
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

- **Audio Transcription** — Ad-hoc audio transcription via Whisper-compatible backend
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

- **Vision (Image Description)** — Ad-hoc image description via Ollama vision LLM
  - `VisionBackend` trait + `OllamaVisionBackend` in `matric-inference` crate
  - `POST /api/v1/vision/describe` API endpoint (base64 image, optional mime_type and prompt)
  - MCP `describe_image` tool for agent access
  - Configurable via `OLLAMA_VISION_MODEL` env var (e.g., `qwen3-vl:8b`, `llava`)
  - Returns AI-generated description, model name, and decoded image size
  - 503 Service Unavailable when vision model not configured
  - Health check integration via `get_system_info` (`extraction.vision.available`)
  - UAT Phase 2D with 8 test cases

### Fixed

- **Vision Ollama URL in Docker** — `OllamaVisionBackend` now reads `OLLAMA_BASE` env var first (matching embedding backend), fixing 500 errors in Docker containers where only `OLLAMA_BASE` is set

## [2026.2.8] - 2026-02-08

### Highlights

This is the largest release since the project's inception — **90 commits**, **262 files changed**,
**+67,000 / -29,000 lines** across every layer of the stack. The headline feature is **Multi-Memory
Architecture**: fully isolated knowledge bases backed by PostgreSQL schema-per-memory isolation,
with zero-drift cloning, per-request routing, session-scoped MCP memory selection, federated
cross-memory search, and memory-scoped backup/restore.

Alongside multi-memory, this release resolves **50 issues** discovered during two comprehensive UAT
passes (530+ MCP test cases, 96.3% pass rate), hardens authentication and OAuth scopes, rewrites
the database restore pipeline, adds the content extraction framework, and ships the MCP file-based
I/O pattern.

| What Changed | Why You Care | Learn More |
|--------------|--------------|------------|
| **Multi-Memory Architecture** | Create, switch, and isolate independent knowledge bases per PostgreSQL schema | [User Guide](docs/content/multi-memory.md) · [Design](docs/architecture/multi-memory-design.md) · [ADR-068](docs/adr/ADR-068-archive-isolation-routing.md) |
| **X-Fortemi-Memory Header** | Per-request memory routing with 3-step fallback (header → default cache → public) | [Architecture](docs/content/architecture.md) |
| **MCP Session Memory** | `select_memory` / `get_active_memory` tools bind a memory to an AI agent session | [MCP Guide](docs/content/mcp.md) · [Agent Guide](docs/content/multi-memory-agent-guide.md) |
| **Federated Search** | Search across multiple memories in a single query | [Search Guide](docs/content/search-guide.md) |
| **Memory-Scoped Backup/Restore** | Per-memory `pg_dump --schema` and `DROP SCHEMA CASCADE` restore | [Backup Guide](docs/content/backup.md) · [Operations](docs/content/operations.md) |
| **Content Extraction Pipeline** | Document type registry (131 types), smart chunking, PDF/code/media adapters | [Document Types](docs/content/document-types-guide.md) · [Extraction Design](docs/content/extraction-pipeline-design.md) |
| **Auth Middleware & OAuth Scopes** | Centralized scope enforcement, configurable token lifetimes, API key support | [Authentication](docs/content/authentication.md) · [ADR-071](docs/architecture/ADR-071-auth-middleware.md) |
| **MCP File-Based I/O** | Replaced base64 binary tools with HTTP API upload/download for remote agents | [MCP Guide](docs/content/mcp.md) · [File Attachments](docs/content/file-attachments.md) |
| **Database Restore Rewrite** | Thread-safe psql pipe, extension-owned object exclusion, FTS index rebuild | [Backup Guide](docs/content/backup.md) |
| **50 UAT Issues Resolved** | Two comprehensive UAT passes with 530+ MCP test cases | [Troubleshooting](docs/content/troubleshooting.md) |

### Added

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

- **File-based I/O pattern** — Replaced base64 binary tools with HTTP API upload/download.
  MCP tools now guide agents to use `POST /api/v1/attachments` multipart upload.
- **Tool definition extraction** — `tools.js` extracted from `index.js` for maintainability
- **Automated JSON Schema validation** — All 100+ MCP tool schemas validated against draft 2020-12
  on startup. One broken schema no longer blocks all tools.
- **MCP OAuth auto-registration** — Bundle entrypoint auto-registers OAuth client credentials
  on first startup. Credentials persisted at `$PGDATA/.fortemi-mcp-credentials`.
- **10+ tool descriptions updated** with memory scoping context and search limitation warnings
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

### Fixed

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

### Changed

- **Workspace version**: `2026.2.7` → `2026.2.8`
- **Migration count**: 57 → 59 migration files
- **MCP tool count**: ~95 → 100+ tools (memory management additions)
- **API handler count**: 91 handlers, all schema-routed
- **Test infrastructure**: Two UAT passes (530+ MCP test cases, 96.3% pass rate)

### Database Migrations

| Migration | Purpose |
|-----------|---------|
| `20260208000002_seed_default_archive.sql` | Seed default archive for fresh deployments |
| `20260208100000_archive_schema_version.sql` | Add schema_version tracking to archive_registry |

Plus significant refactoring of 28 existing migrations (removed CONCURRENTLY, fixed timestamps,
separated schema DDL from seed data).

### Breaking Changes

None. Full backward compatibility maintained:
- `/api/v1/archives/*` routes continue to work alongside new `/api/v1/memories/*`
- Default behavior (no `X-Fortemi-Memory` header) routes to public schema as before
- Existing MCP tool names preserved; new tools added with "memory" terminology

### Upgrade Notes

1. **Database migrations run automatically** on startup via `sqlx::migrate!()`
2. **Fresh deployments** now seed a default archive — `list_archives()` returns the public schema
3. **MCP clients** should update tool descriptions — memory-scoping context added to 10+ tools
4. **Backup scripts** — If using custom backup scripts, consider switching to per-memory backup
   (`GET /api/v1/backup/memory/:name`) for targeted exports
5. **Docker bundle** — MCP OAuth credentials now auto-registered on first startup; manual
   registration no longer required

### Issues Resolved

**50 issues closed** in this release:

- **Epic**: #170 (Multi-Memory Schema Isolation)
- **Multi-Memory**: #158, #159, #169, #171–#181
- **Auth & Security**: #135, #138, #139, #140, #152, #163
- **MCP Server**: #134, #137, #141, #142, #149, #153, #174
- **Backup & Restore**: #136, #166, #167, #168, #175, #176
- **Search**: #132, #133, #144–#148, #177
- **SKOS**: #160, #161, #165
- **Attachments**: #150, #154, #155, #157
- **CI/Testing**: #151, #156
- **PKE**: #143, #162
- **Jobs**: #164
- **Documentation**: #181

## [2026.2.7] - 2026-02-05

### Fixed
- **SKOS Collections MCP endpoints** - Fixed 404 errors on all SKOS collection tools (`list_skos_collections`, `create_skos_collection`, etc.) by correcting API paths from `/api/v1/skos/collections` to `/api/v1/concepts/collections` (#36)
- **SKOS Turtle export** - Fixed `export_skos_turtle` to use scheme_id as path parameter instead of query parameter (#36)
- **SKOS Collection create/update** - Fixed `ordered` → `is_ordered` field name mapping (#36)

## [2026.2.6] - 2026-02-05

### Changed
- **Repository reset** - Squashed history for clean baseline
- **License date updated** - BSL change date set to February 5th, 2030

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
  - After January 30, 2030: Converts to AGPL-3.0 (open source)
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

[Unreleased]: https://github.com/fortemi/fortemi/compare/v2026.2.8...HEAD
[2026.2.8]: https://github.com/fortemi/fortemi/compare/v2026.2.7...v2026.2.8
[2026.2.7]: https://github.com/fortemi/fortemi/compare/v2026.2.6...v2026.2.7
[2026.2.6]: https://github.com/fortemi/fortemi/compare/v2026.2.5...v2026.2.6
[2026.2.5]: https://github.com/fortemi/fortemi/compare/v2026.2.4...v2026.2.5
[2026.2.4]: https://github.com/fortemi/fortemi/compare/v2026.2.3...v2026.2.4
[2026.2.3]: https://github.com/fortemi/fortemi/compare/v2026.2.2...v2026.2.3
[2026.2.2]: https://github.com/fortemi/fortemi/compare/v2026.2.0...v2026.2.2
[2026.2.0]: https://github.com/fortemi/fortemi/compare/v2026.1.12...v2026.2.0
[2026.1.12]: https://github.com/fortemi/fortemi/compare/v2026.1.11...v2026.1.12
[2026.1.0]: https://github.com/fortemi/fortemi/releases/tag/v2026.1.0

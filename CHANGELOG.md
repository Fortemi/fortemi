# Changelog

All notable changes to matric-memory are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [CalVer](https://calver.org/) versioning: `YYYY.M.PATCH`.

## [Unreleased]

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
- **CI/CD pipeline** via Gitea Actions
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

[Unreleased]: https://git.integrolabs.net/roctinam/matric-memory/compare/v2026.1.0...HEAD
[2026.1.0]: https://git.integrolabs.net/roctinam/matric-memory/releases/tag/v2026.1.0

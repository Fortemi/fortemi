# Changelog

All notable changes to matric-memory are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [CalVer](https://calver.org/) versioning: `YYYY.M.PATCH`.

## [Unreleased]

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

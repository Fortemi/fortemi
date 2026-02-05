# Project Intake Form

**Document Type**: Greenfield Library/Crate Extraction
**Generated**: 2026-01-02
**Source**: Gitea issues #1 (matric-memory/issues/1) + related HotM issues #6, #7

## Metadata

- **Project name**: matric-memory
- **Requestor/owner**: roctinam
- **Date**: 2026-01-02
- **Stakeholders**: Engineering (roctinam), Consumers (HotM application, future agents/applications)

## System Overview

**Purpose**: Extract and establish matric-memory as an independent foundational library providing vector-enhanced PostgreSQL storage, hybrid search capabilities, and NLP pipeline management. This library enables multiple consumer applications (starting with HotM) to leverage persistent embeddings and semantic search functionality.

**Current Status**: Planning/Inception (new repository, migrating from HotM)

**Users**: Internal library consumers (developers integrating matric-memory into applications)
- Primary: HotM application (beta note-taking app)
- Future: Additional agents and applications requiring embedding storage and semantic search

**Tech Stack** (from HotM migration + enhancement):
- **Language**: Rust (2021 edition)
- **Workspace Structure**: Cargo workspace with multiple crates
- **Database**: PostgreSQL 14+ with pgvector extension
- **Inference Backends**:
  - Ollama (local models)
  - OpenAI-compatible API (future)
- **Background Processing**: Job queue system for async NLP pipelines
- **Search**: Full-text search (FTS) + semantic search + hybrid
- **Packaging**: Custom .matric dataset format
- **Deployment**: Rust crate published to crates.io (future), Docker for development

## Problem and Outcomes

**Problem Statement**:

HotM currently contains tightly-coupled backend API and embedding management code that should be extracted into a reusable library. This tight coupling prevents:
1. Reuse of embedding storage and search capabilities across multiple applications
2. Independent versioning and evolution of core memory management vs application features
3. Clear API boundaries between foundational services and application logic
4. Testing and validation of core functionality in isolation

**Target Personas**:
- **Primary**: Application developers (like HotM team) integrating persistent embeddings and semantic search
- **Secondary**: AI agent developers requiring structured memory and retrieval capabilities
- **Tertiary**: Future matric-memory contributors extending backends or search algorithms

**Success Metrics (KPIs)**:
- **Extraction completeness**: All 6 core components migrated from HotM (database, models, search, semantic, job queue, inference)
- **API stability**: Public API surface defined and documented for v0.1.0
- **Consumer integration**: HotM successfully consumes matric-memory as a crate dependency
- **Performance baseline**: Hybrid search p95 latency <200ms for 10k document corpus
- **Documentation coverage**: README, API docs, integration guide complete

## Current Scope and Features

**Core Features** (in-scope for v0.1.0 - see milestone issues #1-#20):

**Database Layer (matric-db)** (#3):
- PostgreSQL connection pool management
- pgvector extension integration
- Core data models (documents, embeddings, collections)
- Schema migrations

**Search Engine (matric-search)** (#4):
- Full-text search (FTS) using PostgreSQL tsvector
- Semantic search using pgvector similarity
- Hybrid search combining FTS + semantic with configurable weighting
- Query API abstraction

**Inference Backend Abstraction** (#7-11):
- InferenceBackend trait definition (#8)
- Ollama backend implementation (#9)
- OpenAI-compatible backend implementation (#10)
- Inference configuration and routing (#11)

**Job Queue System** (#5):
- Background job processing for async NLP pipelines
- Task scheduling and retry logic
- Job status tracking

**Dataset Packaging** (#12-16):
- .matric package format design (#13)
- Dataset export functionality (#14)
- Dataset encryption for sensitive data (#15)
- Dataset import/load (#16)

**Documentation** (#17-19):
- README and Getting Started Guide (#17)
- API Reference Documentation (#18)
- Integration Guide for Consumers (#19)

**CI/CD** (#20):
- GitHub Actions or Gitea CI pipeline
- Automated testing (unit + integration)
- Crate build and validation

**Authentication & Authorization** (added post-initial intake - Issue #41):

**OAuth2 Authorization Server**:
- Dynamic Client Registration (RFC 7591)
- Authorization Code with PKCE (RFC 7636)
- Client Credentials Grant (RFC 6749)
- Token Introspection (RFC 7662)
- Token Revocation (RFC 7009)
- Discovery endpoint (RFC 8414)

**API Authentication**:
- Bearer token authentication (OAuth2 access tokens)
- API key authentication (simple integrations)
- Scope-based authorization (read, write, admin, mcp)
- Authentication middleware with optional bypass for public endpoints

**MCP Server Authentication**:
- HTTP/SSE transport mode for remote OAuth access
- Token validation via introspection endpoint
- Preserves stdio mode for local use

**Out-of-Scope** (explicitly excluded for v0.1.0, may revisit in v0.2.0+):
- Publishing to crates.io (deferred until API stabilizes)
- Multi-tenant isolation (HotM and initial consumers are single-user)
- Advanced embedding models beyond Ollama/OpenAI (e.g., Hugging Face integration)
- Distributed deployment (single PostgreSQL instance sufficient for v0.1.0)
- GraphQL or alternative query interfaces (REST/function calls only)
- Real-time sync/subscriptions (polling sufficient initially)
- Embedding fine-tuning or training infrastructure

**Future Considerations** (post-v0.1.0):
- Additional inference backends (Hugging Face, Cohere, Anthropic)
- Vector index optimization for >1M documents
- Distributed search across sharded databases
- Streaming embeddings for large documents
- Integration with LangChain or other orchestration frameworks
- Web UI for dataset visualization and management
- Prometheus metrics and observability integrations

## Architecture (Proposed)

**Architecture Style**: Modular Library (Rust Cargo Workspace)
- **Chosen**: Multi-crate workspace with clear separation of concerns
- **Rationale**: Enables independent compilation, testing, and evolution of components. Consumers can selectively depend on subsets (e.g., only matric-db without search).

**Components** (proposed crate structure):

**matric-core**:
- Core traits and abstractions (InferenceBackend, SearchProvider, etc.)
- Shared data models and types
- Error handling and result types
- **Technology**: Pure Rust, minimal dependencies
- **Rationale**: Stable foundation, minimal breaking changes

**matric-db**:
- PostgreSQL + pgvector integration
- Connection pooling (sqlx or diesel)
- Schema migrations
- Core CRUD operations for documents/embeddings/collections
- **Technology**: Rust + sqlx (async) or diesel (sync)
- **Rationale**: Industry-standard PostgreSQL, pgvector proven for vector similarity

**matric-search**:
- FTS, semantic, and hybrid search implementations
- Query DSL and builder pattern
- Result ranking and relevance tuning
- **Technology**: Rust + PostgreSQL full-text + pgvector
- **Rationale**: Leverages existing PostgreSQL investments, no separate search engine needed

**matric-inference**:
- Inference backend abstraction (InferenceBackend trait)
- Ollama client implementation
- OpenAI-compatible client implementation
- Configuration and routing logic
- **Technology**: Rust + async HTTP clients (reqwest)
- **Rationale**: Pluggable backends enable consumer choice (local Ollama vs cloud APIs)

**matric-jobs**:
- Job queue system for async processing
- Task scheduling, retries, error handling
- Postgres-backed queue or Redis integration
- **Technology**: Rust + PostgreSQL (simple) or Redis (scalable)
- **Rationale**: Decouple slow NLP operations from request path

**matric-datasets**:
- .matric package format (design, serialize, deserialize)
- Import/export utilities
- Encryption/decryption for sensitive datasets
- **Technology**: Rust + bincode/serde for serialization, AES for encryption
- **Rationale**: Portable datasets enable sharing and backup

**Data Models** (estimated from HotM migration):

**Document**:
- `id` (UUID)
- `content` (text)
- `embedding` (vector, dimension configurable)
- `metadata` (JSONB - tags, source, timestamps)
- `collection_id` (foreign key)

**Collection**:
- `id` (UUID)
- `name` (text)
- `description` (text)
- `embedding_model` (text - tracks which model generated embeddings)
- `created_at`, `updated_at`

**Job**:
- `id` (UUID)
- `job_type` (enum: embed_document, reindex_collection, etc.)
- `payload` (JSONB)
- `status` (pending, processing, completed, failed)
- `retries` (int)
- `created_at`, `updated_at`

**Integration Points** (from HotM architecture):
- **HotM Backend**: Consumes matric-memory as crate, calls APIs for document storage/search
- **PostgreSQL**: External database service (existing HotM database migrates to matric-memory schema)
- **Ollama**: External inference service (local or networked)
- **OpenAI API** (future): External cloud inference service

## Scale and Performance (Target)

**Target Capacity**:
- **Initial users**: 1 consumer (HotM)
- **6-month projection**: 3-5 consumers (HotM + experimental agents)
- **2-year vision**: 10+ consumers, potential public crate release

**Document Corpus Scale**:
- **v0.1.0**: 10k-100k documents (HotM note corpus)
- **v0.2.0**: 100k-1M documents (multiple consumers, larger datasets)
- **Future**: >1M documents (vector index optimization required)

**Performance Targets**:
- **Latency**:
  - Hybrid search p95 <200ms for 10k corpus
  - Hybrid search p95 <500ms for 100k corpus
  - Embedding generation <2s per document (depends on model, async via job queue)
- **Throughput**:
  - 10 searches/sec sustained (initially single consumer)
  - 100 documents/min ingest with background embedding (job queue)
- **Availability**:
  - 99% uptime (internal library, tolerates brief PostgreSQL restarts)
  - Graceful degradation if Ollama unavailable (queue embedding jobs)

**Performance Strategy**:
- **Database Indexing**: GIN index for FTS, HNSW index for pgvector similarity
- **Connection Pooling**: Maintain 10-20 connection pool to PostgreSQL
- **Async I/O**: Use async Rust (tokio) for concurrent embedding and search
- **Caching**: In-memory LRU cache for frequently accessed documents (optional)
- **Background Jobs**: Decouple slow embedding operations from user-facing paths

## Security and Compliance (Requirements)

**Security Posture**: Baseline+ (internal library, no external users, but handling potentially sensitive documents)

**Chosen**: Baseline+
**Rationale**: Library used internally, but documents may contain sensitive notes, research, or personal information. Must protect data at rest and support encryption for dataset exports.

**Data Classification**: Internal-Confidential
- **Public**: None (no data intended for public exposure)
- **Internal**: Document content, embeddings, metadata (general notes, research)
- **Confidential**: Potentially sensitive notes, personal information, proprietary research
- **Restricted**: None initially (no PHI/PCI-DSS, but design supports future encryption)

**Identified**: Internal-Confidential
**Evidence**: HotM note-taking application may contain personal information, sensitive research, or proprietary content. Datasets may be exported and shared.

**Security Controls** (required):

**Authentication** (updated with Issue #41 implementation):
- OAuth2 Authorization Server with Dynamic Client Registration (RFC 7591)
- Bearer token authentication for API access
- API key authentication for simple integrations
- Optional authentication (public endpoints remain accessible)

**Authorization**:
- Scope-based access control (read, write, admin, mcp)
- Collection-level access control (future, API supports passing user context)
- Consumer applications (like HotM) can add additional user-level auth

**Data Protection**:
- **Encryption at rest**: PostgreSQL database encryption (via LUKS or cloud provider)
- **Encryption in transit**: PostgreSQL TLS connections (configurable)
- **Dataset encryption**: AES-256 encryption for .matric exports (optional, user-provided key)

**Secrets Management**:
- Database credentials: Environment variables or config files (consumer responsibility)
- Inference API keys (OpenAI): Environment variables (consumer responsibility)
- Dataset encryption keys: User-provided, not stored by library

**Dependency Security**:
- **SBOM**: Generate Software Bill of Materials (cargo audit)
- **Vulnerability scanning**: cargo audit in CI/CD pipeline
- **Dependency review**: Minimize dependencies, prefer well-maintained crates

**Compliance Requirements**: None
- **GDPR**: Not applicable (internal tooling, no EU user data processing)
- **HIPAA**: Not applicable (no PHI)
- **PCI-DSS**: Not applicable (no payment data)
- **SOC2**: Not applicable (no enterprise compliance requirements)
- **Future**: Design supports GDPR "right to be forgotten" (document deletion), encryption (dataset exports)

## Team and Operations (Planned)

**Team Size**: Solo developer (roctinam) + automation (roctibot)

**Team Skills**:
- **Rust**: Advanced (migrating existing HotM codebase)
- **PostgreSQL**: Intermediate (existing HotM experience, pgvector new)
- **Vector Databases**: Beginner-Intermediate (learning pgvector, familiar with embeddings)
- **NLP/Embeddings**: Intermediate (Ollama integration experience from HotM)
- **DevOps**: Intermediate (Docker, CI/CD setup)

**Development Velocity** (target):
- **Sprint length**: 1 week iterations (agile solo development)
- **Release frequency**:
  - v0.1.0: 6-8 weeks (initial extraction and stabilization)
  - Post-v0.1.0: 2-4 week feature increments

**Process Maturity** (planned - MVP profile):

**Version Control**:
- Git with feature branches
- Main branch protected (require passing CI)
- Conventional commits (feat/fix/docs/refactor)

**Code Review**:
- Self-review (solo developer)
- Automated linting (clippy, rustfmt) in CI
- Future: Peer review if contributors join

**Testing**:
- **Target coverage**: 60% for v0.1.0 (core paths tested, edge cases deferred)
- **Unit tests**: All public API functions
- **Integration tests**: Database layer, search engine, inference backends
- **End-to-end tests**: HotM integration (consumer perspective)
- **Test Strategy**: Fast unit tests, slower integration tests with real PostgreSQL (Docker)

**CI/CD**:
- **Platform**: Gitea Actions or GitHub Actions (TBD)
- **Pipeline**:
  1. Lint (clippy, rustfmt)
  2. Build (all crates, all feature flags)
  3. Test (unit + integration with PostgreSQL + Ollama containers)
  4. Security scan (cargo audit)
  5. Documentation build (cargo doc)
- **Triggers**: Push to any branch, pull request to main

**Documentation**:
- **README**: Project overview, quick start, architecture diagram
- **API docs**: Inline rustdoc comments, published to docs.rs (future)
- **Integration guide**: Step-by-step for consumers (HotM as example)
- **ADRs** (Architecture Decision Records): Key design choices documented in repo
- **Wiki**: Internal design notes, research, non-public documentation (Gitea wiki)

**Operational Support** (planned - internal library):

**Monitoring**:
- **Logs**: Structured logging with tracing crate (consumer applications capture logs)
- **Metrics**: None initially (consumers can instrument via tracing)
- **Observability**: Debug logging for development, info/warn for production

**Logging**:
- **Format**: Structured JSON logs (machine-parseable)
- **Levels**: trace (dev), debug (troubleshooting), info (runtime), warn/error (issues)
- **Outputs**: stdout (consumer applications route to CloudWatch, files, etc.)

**Alerting**:
- Not applicable (library crate, consuming applications handle alerting)

**On-call**:
- Not applicable (solo developer, no production SLA)

## Dependencies and Infrastructure

**Third-Party Services** (proposed):
- **PostgreSQL 14+**: Core database (pgvector extension required)
- **Ollama**: Local inference backend (optional, can use OpenAI-compatible)
- **OpenAI API** (future): Cloud inference backend (optional)
- **Gitea**: Issue tracking, CI/CD, wiki
- **crates.io** (future): Rust crate publishing
- **docs.rs** (future): Rust documentation hosting

**Infrastructure** (proposed for development):

**Hosting**:
- Local development (developer workstation)
- Docker Compose for integration testing (PostgreSQL + Ollama containers)

**Database**:
- **PostgreSQL 14+** with pgvector extension
- Docker container for CI/CD (official postgres:14 + pgvector install)
- Local PostgreSQL for development

**Testing Infrastructure**:
- GitHub Actions or Gitea Actions runners
- Docker-in-Docker for integration tests
- Ollama container for inference tests (optional, can mock)

**Storage**:
- Git repository on Gitea (https://git.integrolabs.net/roctinam/matric-memory)
- Database files (PostgreSQL data directory, developer-managed)

**No External Cloud Dependencies** (v0.1.0):
- All infrastructure local or in containers
- No AWS/Azure/GCP required
- Future: Docker Hub for published images, crates.io for library

## Known Risks and Uncertainties

**Technical Risks**:

**Risk 1: pgvector Performance at Scale**
- **Description**: pgvector performance with HNSW index for >100k documents unknown
- **Likelihood**: Medium
- **Impact**: High (core search functionality)
- **Mitigation**:
  - Benchmark early with synthetic datasets (10k, 100k, 1M docs)
  - Explore index tuning parameters (ef_construction, ef_search)
  - Document performance characteristics and limits
  - Fallback: Investigate Qdrant or Weaviate if pgvector insufficient

**Risk 2: API Boundary Stability**
- **Description**: Unclear which APIs should be public vs internal, risk of breaking changes
- **Likelihood**: Medium
- **Impact**: Medium (affects HotM integration, future consumers)
- **Mitigation**:
  - Start with minimal public API (issue #2 - Define Public API Surface)
  - Use semantic versioning strictly (0.1.x patch, 0.2.0 minor breaking)
  - Extensive rustdoc and examples for public APIs
  - Feature flags for experimental APIs

**Risk 3: Inference Backend Reliability**
- **Description**: Ollama service may be unavailable, causing embedding failures
- **Likelihood**: Low (local service, controlled environment)
- **Impact**: Medium (blocks document ingest if embedding required synchronously)
- **Mitigation**:
  - Job queue with retries (issue #5)
  - Graceful degradation (allow document storage without embedding, embed later)
  - Health checks and circuit breaker pattern
  - Support multiple backends (fallback from Ollama to OpenAI if needed)

**Risk 4: Migration from HotM**
- **Description**: Extracting code from HotM may introduce regressions or missing dependencies
- **Likelihood**: Medium
- **Impact**: High (blocks HotM integration, v0.1.0 milestone)
- **Mitigation**:
  - Comprehensive integration tests (HotM as consumer, test real workflows)
  - Gradual migration (keep HotM functional throughout)
  - Reference HotM issue #6 & #7 for detailed migration plan
  - Maintain backward compatibility layer if needed

**Risk 5: Solo Developer Capacity**
- **Description**: 20 issues in v0.1.0 milestone may exceed single developer bandwidth
- **Likelihood**: Medium
- **Impact**: Medium (delays v0.1.0 release)
- **Mitigation**:
  - Prioritize ruthlessly (defer non-critical features to v0.2.0)
  - Use automation (roctibot for issue management, CI/CD for testing)
  - Break work into small increments (1-week iterations)
  - Accept MVP quality (60% test coverage, not 90%)

**Integration Risks**:

**Risk: HotM Regression**
- **Description**: HotM may break during matric-memory extraction
- **Likelihood**: Medium
- **Impact**: High (HotM is active project, can't afford downtime)
- **Mitigation**:
  - Feature branches for migration work
  - Keep HotM main branch functional
  - Integrate matric-memory incrementally (one crate at a time)
  - Rollback plan (maintain HotM copy of code until migration validated)

**Timeline Risks**:

**Risk: Scope Creep**
- **Description**: 20 issues may expand with discovered requirements
- **Likelihood**: High (common in extraction projects)
- **Impact**: Medium (delays release, but not critical path)
- **Mitigation**:
  - Strict scope control (defer nice-to-have features)
  - Issue triage weekly (reassign to v0.2.0 if non-essential)
  - Use option-matrix to evaluate new features against priorities

**Team Risks**:

**Risk: Knowledge Loss**
- **Description**: Solo developer, no redundancy if unavailable
- **Likelihood**: Low
- **Impact**: High (project stalls)
- **Mitigation**:
  - Comprehensive documentation (README, API docs, integration guide)
  - Clear issue tracking (future contributors can pick up work)
  - Code comments and ADRs for context
  - Consider open-sourcing (community backup)

## Why This Intake Now?

**Context**:
Matric-memory is being extracted from HotM to enable reuse across multiple applications and agents. This intake establishes the foundational requirements, scope, and architecture for the new independent repository.

**Goals**:
- Define clear API boundaries between matric-memory (library) and HotM (consumer)
- Establish scope for v0.1.0 milestone (core functionality migration)
- Document architectural decisions (ADRs) for crate structure and component design
- Align on performance targets and security posture
- Create structured SDLC process for ongoing development

**Triggers**:
- HotM issue #6: [EPIC] Extract matric-memory as Independent Repository
- HotM issue #7: Define matric-memory API Boundaries
- matric-memory issue #1: [EPIC] matric-memory Core Foundation
- v0.1.0 milestone created with 20 sub-tasks

**Decision Context**:
This intake supports the Concept → Inception phase transition. Key decisions include:
1. Crate structure (monolithic vs multi-crate workspace) → Multi-crate chosen for modularity
2. Database choice (PostgreSQL+pgvector vs dedicated vector DB) → PostgreSQL chosen for simplicity
3. Inference abstraction (tight Ollama coupling vs pluggable backends) → Pluggable chosen for flexibility
4. Profile selection (Prototype vs MVP vs Production) → MVP chosen (initial release, prove viability)

## Attachments

- Solution profile: `.aiwg/intake/solution-profile.md`
- Option matrix: `.aiwg/intake/option-matrix.md`
- Related Gitea issues:
  - matric-memory #1: https://git.integrolabs.net/roctinam/matric-memory/issues/1
  - HotM #6: https://git.integrolabs.net/roctinam/hotm/issues/6
  - HotM #7: https://git.integrolabs.net/roctinam/hotm/issues/7
- v0.1.0 milestone: https://git.integrolabs.net/roctinam/matric-memory/milestones/v0.1.0

## Next Steps

**Your intake documents are now complete and ready for the next phase!**

1. **Review** generated intake files for accuracy (cross-reference with Gitea issues #1, #6, #7)
2. **Validate** solution-profile.md and option-matrix.md align with project priorities
3. **Update Gitea issues** as needed based on intake insights (roctibot can automate)
4. **Proceed directly to Inception** using natural language or explicit commands:
   - Natural language: "Start Inception" or "Let's transition to Inception"
   - Explicit command: `/flow-concept-to-inception .`

**Note**: You do NOT need to run `/intake-start` - that command is only for teams who manually created their own intake documents. The `intake-wizard` and `intake-from-codebase` commands produce validated intake ready for immediate use.

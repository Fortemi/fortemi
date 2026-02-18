# Fortemi Architecture

## Overview

Fortemi is an AI-enhanced knowledge management system implementing Retrieval-Augmented Generation (RAG)[^1] with hybrid search combining full-text retrieval (BM25)[^2] and dense passage retrieval[^3] via Reciprocal Rank Fusion (RRF)[^4]. The system provides automatic knowledge graph construction[^5] through semantic similarity analysis and W3C SKOS-compliant[^6] controlled vocabulary management.

The Rust workspace consists of 7 crates that together provide vector-enhanced note storage, hybrid retrieval, NLP pipeline management, and cryptographic data protection.

## System Context

```
                    ┌─────────────────┐
                    │   HotM Frontend │
                    └────────┬────────┘
                             │ HTTPS
                    ┌────────▼────────┐
                    │   matric-api    │
                    │  (REST Server)  │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
┌───────▼───────┐   ┌───────▼───────┐   ┌───────▼───────┐
│ matric-search │   │  matric-jobs  │   │matric-inference│
│(Hybrid Search)│   │ (Job Queue)   │   │  (Ollama LLM)  │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                   │                   │
        └─────────┬─────────┴─────────┬─────────┘
                  │                   │
          ┌───────▼───────┐   ┌───────▼───────┐
          │   matric-db   │   │    Ollama     │
          │ (PostgreSQL)  │   │   (Local)     │
          └───────┬───────┘   └───────────────┘
                  │
          ┌───────▼───────┐
          │   PostgreSQL  │
          │pgvector+PostGIS│
          └───────────────┘
```

## Crate Dependencies

```
matric-core (traits, types, errors)
     │
     ├── matric-db (database layer)
     │        │
     │        ├── matric-search (hybrid retrieval)
     │        │
     │        └── matric-jobs (job processing)
     │
     ├── matric-inference (LLM abstraction)
     │        │
     │        └── ollama, openai backends
     │
     ├── matric-crypto (PKE encryption)
     │
     └── matric-api (HTTP server)
              │
              └── uses all other crates
```

## Multi-Memory Architecture

Fortemi supports parallel memory archives using PostgreSQL schema isolation. Each memory operates as an independent namespace with complete data isolation. Implementation is complete as of 2026-02-08, with all 91 API handlers routing through `SchemaContext`.

### Schema-Based Isolation

Each memory is a PostgreSQL schema containing a full set of per-user data tables. Shared infrastructure (authentication, job queue, document types, migration tracking) lives in the `public` schema and is accessible to all memories via `search_path`.

```
Database Structure:
├── public schema (shared tables)
│   ├── archive_registry - Memory metadata
│   ├── oauth_clients - Authentication
│   ├── api_keys - API keys
│   ├── job_queue - Background jobs
│   ├── embedding_config - Model configs
│   └── ... (14 shared tables total)
├── default schema (default memory)
│   ├── note - Notes and content
│   ├── embedding - Vector embeddings
│   ├── link - Semantic relationships
│   ├── tag, skos_* - Taxonomy
│   ├── file_storage, attachment - Files
│   ├── template - Note templates
│   └── ... (41 per-memory tables total)
└── custom schemas (user-created memories)
    └── Same 41-table structure per memory
```

### Deny-List Approach

The system uses a **deny-list** model: all tables are per-memory unless explicitly listed in `SHARED_TABLES` (14 tables). This ensures:

- Zero drift when new tables are added (automatically per-memory)
- Clear separation between shared infrastructure and memory-specific data
- Automatic migration when memories access new table structures

### Shared vs Per-Memory Tables

**Shared Tables (14):**
- Authentication: `oauth_clients`, `oauth_authorization_codes`, `oauth_access_tokens`, `oauth_refresh_tokens`, `api_keys`
- Job Queue: `job_queue` (jobs reference memory context in payload)
- Events: `event_subscription`, `webhook`, `webhook_delivery`
- System: `embedding_config`, `archive_registry`, `backup_metadata`, `_sqlx_migrations`

**Per-Memory Tables (41):**
- Notes: `note`, `note_original`, `note_revision`, `note_version`
- Embeddings: `embedding`, `embedding_set`, `embedding_set_member`, `embedding_set_stats`
- Links: `note_links`
- Tags: `tag`, `tag_note`, `skos_concept_schemes`, `skos_concepts`, `skos_labels`, `skos_relations`, `skos_collections`, `skos_collection_members`
- Collections: `collection`, `collection_note`
- Templates: `template`
- Attachments: `file_attachment`, `file_provenance`, `file_metadata`
- Document Types: `document_type`, `document_type_pattern`
- Provenance: `provenance_activity`, `provenance_edge`
- Search: `search_cache` (if Redis not enabled)
- Versioning: Various version tracking tables

### Per-Request Routing

Requests are routed to specific memories via the `X-Fortemi-Memory` header. The `archive_routing_middleware` in `crates/matric-api/src/middleware/archive_routing.rs` resolves the target schema:

1. **Header present**: Looks up the memory by name in `archive_registry`, returns 404 if not found, auto-migrates schema if outdated
2. **Header absent**: Falls back to `DefaultArchiveCache` (60-second TTL) to resolve the default archive
3. **No default set**: Falls back to `public` schema

The middleware injects an `ArchiveContext { schema, is_default }` into request extensions. All 91 handlers extract this context and create a `SchemaContext` via `state.db.for_schema(&archive_ctx.schema)?`, which sets `SET LOCAL search_path TO {schema}, public` per transaction.

### Handler Transaction Patterns

Handlers use one of two patterns depending on complexity:

**`execute()` for simple operations** (most handlers): Pass a closure to `ctx.execute(move |tx| ...)` which handles the full transaction lifecycle (begin, SET LOCAL, commit/rollback).

**`begin_tx()` for complex operations** (file_storage, analytics, loops): Call `ctx.begin_tx().await?` to get a pre-configured transaction, then call multiple `_tx` methods directly. Used when repositories cannot be moved into closures.

Repository methods have parallel `_tx` variants accepting `&mut Transaction<'_, Postgres>` (Option A from ADR-068), using `&mut **tx` instead of `&self.pool`.

### Auto-Migration

Memories are automatically migrated when accessed:

1. System checks `schema_version` in `archive_registry` (table count)
2. If `schema_version < expected_version`, missing tables are created
3. Uses same `CREATE TABLE` statements that initialized default memory
4. `schema_version` is updated to reflect current table count
5. Operation is idempotent and non-destructive

### Current Limitations

The standard hybrid search endpoint (`GET /api/v1/search`) currently rejects non-public archives, returning a 400 error. This is a temporary limitation because the `HybridSearchEngine` operates directly on the connection pool without `SchemaContext` support. Federated search (`POST /api/v1/search/federated`) works across all memories using dynamically-built schema-qualified queries.

See [Multi-Memory Design](../architecture/multi-memory-design.md) for the comprehensive design document and [Multi-Memory Guide](./multi-memory.md) for usage documentation.

## Crate Details

### matric-core

Core types and traits shared across all crates.

**Key Components:**
- `Error` - Unified error type with domain-specific variants
- `Note`, `NoteSummary`, `NoteFull` - Note data models
- `Job`, `JobType`, `JobStatus` - Job queue models
- `Tag`, `Link` - Relationship models
- `SearchHit` - Search result model
- `ServerEvent`, `EventBus` - Real-time event broadcasting (SSE, WebSocket, webhooks)
- `DocumentComposition` - Controls what note properties are assembled into embedding text (title, content, tags, concepts). Stored per-embedding-set with inheritance from defaults. See the Embedding Pipeline section for details.
- Repository traits: `NoteRepository`, `TagRepository`, `LinkRepository`, `JobRepository`

**Advanced Features:**
- `events.rs` - Unified event bus (`EventBus`) and `ServerEvent` enum for real-time notifications (SSE, WebSocket, webhooks, telemetry). See ADR-037.
- `embedding_provider.rs` - `EmbeddingSetConfig` with per-set `DocumentComposition` and MRL dimension support
- `fair.rs` - FAIR metadata export (Findable, Accessible, Interoperable, Reusable) with Dublin Core and JSON-LD support
- `temporal.rs` - Temporal filtering for time-based queries
- `uuid_utils.rs` - UUIDv7 generation for time-ordered identifiers
- `strict_filter.rs` - Type-safe strict filtering predicates
- `defaults.rs` - Centralized default constants for system-wide configuration, including `GraphConfig` for graph quality pipeline tuning

### matric-db

PostgreSQL database layer with pgvector (vector similarity) and PostGIS (spatial queries) extensions.

**Key Components:**
- `Database` - Connection pool manager
- `PgNoteRepository` - Note CRUD operations
- `PgTagRepository` - SKOS concept management
- `PgLinkRepository` - Knowledge graph edge management
- `PgJobRepository` - Job queue operations
- `PgDocumentTypeRepository` - Document type registry and auto-detection

**Advanced Features:**
- `provenance.rs` - W3C PROV tracking for AI revision operations (entities, activities, relations)
- `memory_search.rs` - Temporal-spatial search on file provenance (PostGIS location, time range, combined queries)
- `versioning.rs` - Dual-track note history (original content versions + revision versions)
- `unified_filter.rs` - Multi-dimensional filtering combining tags, dates, collections, metadata
- `strict_filter.rs` - Pre-search WHERE clause implementation for guaranteed data isolation
- `oauth.rs` - OAuth provider integration for authentication
- `skos_tags.rs` - W3C SKOS semantic tagging with Collections support
- `document_types.rs` - Document type detection pipeline with confidence scoring
- `schema_context.rs` - Schema-scoped database operations for multi-memory isolation

**Tables:**
- `note` - Note metadata
- `note_original` - Immutable original content
- `note_revision` - RAG-enhanced versions
- `embedding` - Sentence embeddings[^7] stored as pgvector
- `skos_concepts`, `skos_labels`, `skos_relations` - W3C SKOS vocabulary[^6]
- `note_links` - Knowledge graph edges with similarity scores
- `job_queue` - Background NLP jobs
- `provenance_edge`, `provenance_activity` - W3C PROV tracking tables
- `document_type` - Document type registry with 131 pre-configured types

### matric-search

Hybrid retrieval engine implementing Reciprocal Rank Fusion (RRF)[^4] to combine lexical and semantic search with strict tag filtering for guaranteed data isolation.

**Key Components:**
- `HybridSearchEngine` - Main retrieval coordinator
- `HybridSearchConfig` - Search mode configuration with optional strict filter
- `SearchRequest` - Query builder pattern
- `StrictTagFilter` - Pre-search WHERE clause for guaranteed SKOS-based isolation
- `rrf_fusion()` - RRF algorithm implementation (k=60)

**Advanced Features:**
- `adaptive_rrf.rs` - Query-dependent RRF k parameter selection (default k=20, adapts to query length and type)
- `adaptive_weights.rs` - FTS/semantic weight selection based on query characteristics (keyword vs conceptual queries)
- `rsf.rs` - Relative Score Fusion as alternative to RRF using min-max normalization
- `deduplication.rs` - Result deduplication for chunked documents
- `hnsw_tuning.rs` - Dynamic HNSW ef_search parameter tuning based on recall targets and corpus size

**Search Modes:**
1. **FTS Only** - BM25-based ranking[^2] via PostgreSQL tsvector/GIN
2. **Semantic Only** - Dense retrieval[^3] via pgvector cosine similarity
3. **Hybrid** (default) - RRF fusion of lexical and semantic rankings[^4]

**Strict Filtering:**
- Pre-search WHERE clause applied before fuzzy matching
- Guarantees 100% isolation by SKOS concepts/schemes
- Supports AND/OR/NOT logic on controlled vocabulary terms
- Foundation for multi-tenancy and access control

### matric-inference

LLM inference abstraction for text generation and sentence embedding computation.

**Key Components:**
- `InferenceBackend` trait - Pluggable backend interface
- `OllamaBackend` - Local inference via Ollama (default)
- `OpenAIBackend` - OpenAI-compatible API inference (feature-gated)
- `EmbeddingRequest/Response` - Sentence embedding generation
- `GenerateRequest/Response` - Text generation for RAG
- `ModelRegistry` - Model profiles and capability recommendations

**Advanced Features:**
- `capabilities.rs` - Model capability detection and classification
- `discovery.rs` - Automatic discovery of available models from inference backends
- `eval.rs` - Model evaluation and performance tracking
- `few_shot.rs` - In-context learning prompt construction with curated examples (3-5 optimal)
- `selector.rs` - Intelligent model selection based on task requirements
- `thinking.rs` - Thinking model detection and response parsing (explicit tags, verbose reasoning, pattern-based)

**Embedding Approach:**
Uses contrastive learning-based models[^8] (nomic-embed-text) producing 768-dimensional sentence embeddings with mean pooling aggregation[^7]. What text is sent to the embedding model is controlled by `DocumentComposition` — see the Embedding Pipeline section below.

### matric-crypto

Public-key encryption (PKE) for secure multi-recipient data sharing.

**Key Components:**
- `encrypt_pke` / `decrypt_pke` - Multi-recipient public-key encryption
- `Keypair` / `Address` - X25519 keypairs and wallet-style addresses
- `save_private_key` / `load_private_key` - Argon2id-protected key storage
- `detect_format` - Auto-detect encrypted file formats
- `DerivedKey` - Secure key wrapper with zeroize

**Cryptographic Primitives:**
- X25519 (Curve25519 ECDH key exchange)
- AES-256-GCM (authenticated symmetric encryption)
- HKDF-SHA256 (key derivation from shared secrets)
- BLAKE3 (address hashing)
- Argon2id (memory-hard KDF for key storage)
- ChaCha20-based CSPRNG (random generation)

**File Format:**
- MMPKE01 - Public-key multi-recipient envelope encryption

### matric-jobs

Background job processing for asynchronous NLP operations and document processing.

**Key Components:**
- `JobWorker` - Background worker process. Event-driven (wakes instantly on job enqueue via `Notify`); safety-net poll interval (default 60s) handles crash recovery and race conditions
- `JobHandler` trait - Job type handlers
- `ExtractionRegistry` - Adapter registry for file processing strategies
- `ExtractionAdapter` trait - Pluggable extraction strategy interface
- `GraphMaintenanceHandler` - Runs the graph quality pipeline (SNN, PFNET, Louvain) after embedding changes; registered in main.rs and triggered automatically by the Linking job
- Job types: 24 total (see Job Processing Architecture section)

**Extraction Adapters:**
- `TextNativeAdapter` - Plain text files with UTF-8 conversion
- `StructuredExtractAdapter` - JSON, YAML, TOML, CSV, XML with format validation and schema extraction
- `PdfTextAdapter` - PDF text extraction
- `PdfOcrAdapter` - PDF OCR processing
- `VisionAdapter` - Image captioning via Ollama vision LLM
- `AudioTranscribeAdapter` - Audio transcription via Whisper-compatible backend
- `VideoMultimodalAdapter` - Video multimodal extraction (keyframes, scene detection, transcription alignment)
- `CodeAstAdapter` - Code AST analysis via tree-sitter
- `Glb3DModelAdapter` - 3D model understanding via Three.js multi-view rendering + vision (requires Blender)
- `OfficeConvertAdapter` - Office document conversion

**RAG Pipeline Jobs:**
1. **Extraction** - File content extraction via adapter registry (priority 7, gates downstream work)
2. **Embedding** - Generate sentence embeddings[^7] for semantic search (uses `DocumentComposition` to build text)
3. **AiRevision** - RAG-based content enhancement[^1] with retrieved context
4. **Linking** - Knowledge graph construction[^5] via embedding similarity (>70% threshold); auto-queues `GraphMaintenance` on completion
5. **GraphMaintenance** - Graph quality pipeline: SNN scoring, PFNET sparsification, Louvain community detection
6. **TitleGeneration** - LLM-generated descriptive titles
7. **ContextUpdate** - Inject related note context into revisions
8. **ConceptTagging** - Auto-generate SKOS concept tags using AI analysis (GLiNER tier-0)
9. **EntityExtraction** - Extract named entities for tri-modal search
10. **ExifExtraction** - Extract EXIF metadata from image attachments
11. **DocumentTypeInference** - Auto-detect document type from filename, MIME, content

### matric-api

HTTP REST API server using Axum framework.

**Key Features:**
- RESTful endpoints for CRUD operations
- Real-time event streaming via SSE (`/api/v1/events`) and WebSocket (`/api/v1/ws`)
- Webhook dispatcher with HMAC-SHA256 signing (`/api/v1/webhooks`)
- Worker event bridge: translates job worker events into ServerEvents on the EventBus
- OpenAPI 3.1 specification
- Swagger UI at `/docs`
- CORS support
- Request tracing

## Centralized Configuration

### defaults.rs Module

**Purpose:** Single source of truth for all default constants across the system. Prevents configuration drift and magic numbers scattered across crates.

**Organization:**
- **Chunking** - Chunk sizes, overlaps, minimums
- **Embedding** - Model names, dimensions
- **Pagination** - Limits for different endpoint types
- **Server** - Ports, rate limits, timeouts, body sizes
- **Inference** - Ollama URLs, timeouts
- **Job Processing** - Max retries, poll intervals, concurrency limits
- **Search** - Stale thresholds, trend periods
- **Two-Stage Retrieval** - Coarse dimensions, top-k, ef_search
- **Tri-Modal Fusion** - Weight distributions
- **Fine-Tuning** - Query generation, quality thresholds, splits
- **Similarity Thresholds** - Link creation, context filtering, AI confidence
- **Content Previews** - Preview sizes for different contexts
- **Health Scoring** - Weight distributions
- **Document Detection** - Confidence scores per detection method

**Runtime Override Pattern:**
```rust
use matric_core::defaults;

let chunk_size = std::env::var("CHUNK_SIZE")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(defaults::CHUNK_SIZE);
```

**Benefits:**
- All crates reference constants via `matric_core::defaults::`
- Changes propagate system-wide
- Documentation auto-generated from code
- Environment variables override at runtime
- Type-safe integer conversions where needed (e.g., `CHUNK_SIZE_I32`)

## Document Type Detection Pipeline

**Purpose:** Automatically identify document types and assign appropriate chunking strategies without user input. Handles 131 document categories from code to prose to multimedia.

### Detection Cascade

The system uses a confidence-scored cascade with early-exit on high-confidence matches:

```
┌─────────────────────────────────────────────────────┐
│  1. Filename Pattern Match                          │
│     Input: "package.json", "Cargo.toml", ".env"     │
│     Confidence: 1.0 (highest)                       │
│     Examples: Exact matches like "requirements.txt" │
└────────────────┬────────────────────────────────────┘
                 │ No match
                 ▼
┌─────────────────────────────────────────────────────┐
│  2. MIME Type Match                                 │
│     Input: "application/json", "text/x-python"      │
│     Confidence: 0.95                                │
│     Examples: Well-defined binary formats (PDF, PNG)│
└────────────────┬────────────────────────────────────┘
                 │ No match
                 ▼
┌─────────────────────────────────────────────────────┐
│  3. File Extension Match                            │
│     Input: ".py", ".md", ".cpp", ".rs"              │
│     Confidence: 0.9                                 │
│     Examples: Common source file extensions         │
└────────────────┬────────────────────────────────────┘
                 │ No match
                 ▼
┌─────────────────────────────────────────────────────┐
│  4. Content Pattern Match (Magic)                   │
│     Input: "#!/usr/bin/env python", "<?xml"         │
│     Confidence: 0.7                                 │
│     Examples: Shebang lines, XML prolog             │
└────────────────┬────────────────────────────────────┘
                 │ No match
                 ▼
┌─────────────────────────────────────────────────────┐
│  5. Default Fallback                                │
│     Type: "plaintext"                               │
│     Confidence: 0.1 (lowest)                        │
│     Strategy: Semantic chunking                     │
└─────────────────────────────────────────────────────┘
```

### Confidence Scoring

Confidence scores guide downstream decisions:

| Score | Method | Use Case |
|-------|--------|----------|
| 1.0 | Filename pattern | Exact name matches (e.g., "Dockerfile") |
| 0.95 | MIME type | Binary formats with well-defined types |
| 0.9 | File extension | Common text file extensions |
| 0.7 | Content magic | Shebang, XML prolog, JSON structure |
| 0.1 | Default fallback | Unknown files default to plaintext |

**Decision Logic:**
- High confidence (>=0.9): Apply specialized chunking without confirmation
- Medium confidence (0.7-0.89): Apply strategy but log for review
- Low confidence (<0.7): Use semantic chunking (safest fallback)

### Document Categories

**131 Pre-Configured Types** organized by category:

1. **Code** (33 types) - Syntactic chunking via tree-sitter
   - Languages: Python, Rust, JavaScript, Go, Java, C++, etc.
   - Configs: Cargo.toml, package.json, requirements.txt
   - Scripts: Bash, PowerShell, SQL

2. **Prose** (18 types) - Semantic chunking with paragraph boundaries
   - Formats: Markdown, AsciiDoc, LaTeX, ReStructuredText
   - Office: DOCX, ODT, RTF
   - Email: EML, MSG

3. **Data** (25 types) - Schema extraction then semantic chunking
   - Structured: JSON, YAML, TOML, XML, CSV, Parquet
   - Binary: Protocol Buffers, MessagePack, BSON

4. **Tabular** (12 types) - Row-based chunking with header preservation
   - Spreadsheets: XLSX, ODS, Numbers
   - Databases: SQL dumps, SQLite, Parquet

5. **Multimedia** (24 types) - Metadata extraction only (no chunking)
   - Images: PNG, JPG, SVG, GIF, TIFF
   - Video: MP4, WebM, MOV, AVI
   - Audio: MP3, FLAC, WAV, OGG

6. **Documents** (19 types) - Mixed strategies based on structure
   - Binary office: DOCX, PPTX, XLSX (extract then semantic)
   - PDFs: Text extraction then semantic chunking
   - Archives: TAR, ZIP, 7Z (manifest extraction)

### Chunking Strategy Assignment

Each document type specifies:

```rust
DocumentType {
    name: "python",
    chunking_strategy: ChunkingStrategy::Syntactic,
    tree_sitter_language: Some("python"),
    chunk_size_default: 1000,
    chunk_overlap_default: 100,
    preserve_boundaries: true, // Don't split functions/classes
    extraction_strategy: ExtractionStrategy::TextNative,
}
```

**Strategy Mapping:**

| Strategy | Document Types | Chunking Approach |
|----------|---------------|-------------------|
| Syntactic | Code files (33 types) | Tree-sitter AST traversal, respects function/class boundaries |
| Semantic | Prose (18 types), Data (25 types) | Paragraph/section-based, preserves semantic units |
| FixedSize | Binary data, logs | Fixed 1000-char chunks with 100-char overlap |
| Adaptive | Unknown types | Dynamic chunk sizing based on content density |

### Detection Pipeline Implementation

**Location:** `crates/matric-db/src/document_types.rs`

**Key Methods:**
```rust
impl DocumentTypeRepository for PgDocumentTypeRepository {
    async fn detect(
        &self,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>>
}
```

**Returned Result:**
```rust
DetectDocumentTypeResult {
    document_type: DocumentTypeSummary {
        id, name, display_name,
        chunking_strategy,
        tree_sitter_language,
        extraction_strategy,
    },
    confidence: f32,  // 0.0 to 1.0
    detection_method: String,  // "filename_pattern", "mime_type", etc.
}
```

**Usage in Job Processing:**
When processing a file attachment, the system:
1. Calls `detect()` with available metadata
2. Receives detected type with confidence score
3. Selects extraction adapter based on `extraction_strategy`
4. Applies chunking using `chunking_strategy` and parameters
5. Stores detection metadata for audit trail

## Extraction Pipeline Architecture

**Purpose:** Pluggable adapter pattern for file processing with strategy-based dispatching. Separates extraction logic from job processing infrastructure.

### Adapter Registry Pattern

**Core Abstraction:**
```rust
#[async_trait]
pub trait ExtractionAdapter {
    fn strategy(&self) -> ExtractionStrategy;
    fn name(&self) -> &str;
    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult>;
    async fn health_check(&self) -> Result<bool>;
}
```

**Registry Implementation:**
```
┌─────────────────────────────────────────────────────┐
│           ExtractionRegistry                        │
│  HashMap<ExtractionStrategy, Arc<dyn Adapter>>      │
└────────────────┬────────────────────────────────────┘
                 │
         ┌───────┴───────┬───────────────┬─────────┐
         ▼               ▼               ▼         ▼
┌─────────────┐  ┌──────────────┐  ┌────────┐  ┌────────┐
│TextNative   │  │Structured    │  │PdfText │  │Future  │
│Adapter      │  │Extract       │  │(TODO)  │  │Adapters│
│             │  │Adapter       │  │        │  │        │
│.txt, .md    │  │.json, .yaml  │  │.pdf    │  │.docx   │
└─────────────┘  └──────────────┘  └────────┘  └────────┘
```

### NER Backend: GLiNER

**GLiNER** is the tier-0 NER backend used for concept extraction (`ConceptTagging`) and reference extraction (`ReferenceExtraction`). It runs entirely on CPU via a sidecar HTTP service (`GLINER_BASE_URL`, default `http://gliner:8090` in Docker bundle), requiring no GPU. GLiNER performs zero-shot named-entity recognition using generative listwise ranking and returns typed entity spans directly from raw text.

When GLiNER is unavailable (health check fails or `GLINER_BASE_URL` is unset), the job escalates to the FAST_GPU tier (qwen3:8b) via the queue-based escalation mechanism described in Tiered Job Architecture above.

### Available Adapters

**1. TextNativeAdapter** (ExtractionStrategy::TextNative)

**Handles:** Plain text files (.txt, .md, .log, .csv, .ini, etc.)

**Process:**
- UTF-8 decode with lossy conversion for invalid sequences
- Count characters and lines
- Return full text with basic metadata

**Metadata Output:**
```json
{
  "char_count": 1024,
  "line_count": 42
}
```

**Health Check:** Always healthy (no external dependencies)

**2. StructuredExtractAdapter** (ExtractionStrategy::StructuredExtract)

**Handles:** Structured data formats (JSON, YAML, TOML, CSV, XML)

**Process:**
1. Auto-detect format from MIME type or file extension
2. Parse structure and validate
3. Extract schema metadata (keys, types, counts)
4. Return text with format info

**Format Detection Priority:**
1. MIME type (`application/json`, `text/yaml`)
2. File extension (`.json`, `.yaml`, `.toml`, `.csv`, `.xml`)
3. Default to `text` if unknown

**Metadata Output (JSON example):**
```json
{
  "format": "json",
  "format_metadata": {
    "valid": true,
    "type": "object",
    "top_level_keys": ["name", "version", "dependencies"],
    "key_count": 12
  }
}
```

**Metadata Output (CSV example):**
```json
{
  "format": "csv",
  "format_metadata": {
    "row_count": 100,
    "headers": ["name", "age", "city"],
    "column_count": 3
  }
}
```

**Health Check:** Always healthy (no external dependencies)

### Strategy Assignment

**Extraction Strategy Types:**

```rust
pub enum ExtractionStrategy {
    TextNative,        // Plain text UTF-8 decode
    StructuredExtract, // JSON/YAML/TOML/CSV/XML parsing
    PdfText,           // PDF text extraction (future)
    PdfOcr,            // PDF OCR processing (future)
    ImageCaption,      // Image captioning via LLM (future)
    VideoTranscript,   // Video transcription (future)
    AudioTranscript,   // Audio transcription (future)
}
```

**Assignment Logic:**

Document type detection pipeline outputs an `extraction_strategy` field that maps to the appropriate adapter:

```rust
// Python source code
DocumentType {
    name: "python",
    extraction_strategy: ExtractionStrategy::TextNative,
    // ... tree-sitter used for CHUNKING, not extraction
}

// JSON data file
DocumentType {
    name: "json",
    extraction_strategy: ExtractionStrategy::StructuredExtract,
}

// PDF document (future)
DocumentType {
    name: "pdf",
    extraction_strategy: ExtractionStrategy::PdfText,
}
```

### Extraction Flow

```
File Upload -> Document Type Detection -> Strategy Selection -> Adapter Dispatch
                     |                          |                  |
              (filename, MIME)          ExtractionStrategy   Registry.extract()
                     |                          |                  |
              confidence: 0.95              TextNative         UTF-8 decode
                     |                          |                  |
              "python" type              StructuredExtract   Parse + validate
                                                 |                  |
                                            Return Result    ExtractionResult
```

**ExtractionResult Structure:**
```rust
pub struct ExtractionResult {
    pub extracted_text: Option<String>,      // Main text content
    pub metadata: JsonValue,                 // Format-specific metadata
    pub ai_description: Option<String>,      // Optional AI caption
    pub preview_data: Option<JsonValue>,     // Optional preview payload
}
```

### Registry Lifecycle

**Initialization:**
```rust
let mut registry = ExtractionRegistry::new();
registry.register(Arc::new(TextNativeAdapter));
registry.register(Arc::new(StructuredExtractAdapter));
```

**Usage in Job Worker:**
```rust
let worker = JobWorker::new(db, config, Some(registry));

// Worker delegates to registry during job execution
let result = worker.extraction_registry()
    .unwrap()
    .extract(strategy, data, filename, mime_type, &config)
    .await?;
```

**Health Monitoring:**
```rust
let health = registry.health_check_all().await;
// HashMap<ExtractionStrategy, bool>
// { TextNative: true, StructuredExtract: true }
```

### Adapter Development Guide

**Adding a New Adapter:**

1. Implement `ExtractionAdapter` trait
2. Define strategy variant in `ExtractionStrategy` enum
3. Register in worker initialization
4. Add test coverage

**Example (PdfTextAdapter skeleton):**
```rust
pub struct PdfTextAdapter {
    // External tool client (e.g., poppler, pdfium)
}

#[async_trait]
impl ExtractionAdapter for PdfTextAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::PdfText
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        // Call external tool, parse output
        todo!()
    }

    async fn health_check(&self) -> Result<bool> {
        // Verify tool availability
        todo!()
    }

    fn name(&self) -> &str {
        "pdf_text"
    }
}
```

## Job Processing Architecture

**Purpose:** Priority-based async job queue for long-running NLP operations, document processing, and maintenance tasks.

### Job Types (24 Total)

**Core Priority Mapping** (1=lowest, 10=highest):

| Job Type | Priority | Cost Tier | Purpose |
|----------|----------|-----------|---------|
| **PurgeNote** | 9 | agnostic | Permanently delete note and all related data |
| **AiRevision** | 8 | agnostic | RAG-based content enhancement with retrieved context |
| **Extraction** | 7 | agnostic | File content extraction via adapter registry (gates downstream work) |
| **Embedding** | 5 | agnostic | Generate sentence embeddings for semantic search (respects `DocumentComposition`) |
| **EmbedForSet** | 5 | agnostic | Embed specific notes into a specific embedding set |
| **ExifExtraction** | 5 | agnostic | Extract EXIF metadata from image attachments |
| **ConceptTagging** | 4 | CPU_NER (0) | Auto-generate SKOS concept tags using GLiNER |
| **EntityExtraction** | 4 | agnostic | Extract named entities for tri-modal search |
| **ReferenceExtraction** | 4 | CPU_NER (0) | Extract named entity references (companies, people, tools) |
| **MetadataExtraction** | 4 | FAST_GPU (1) | AI-extracted structured metadata (authors, year, venue) |
| **RelatedConceptInference** | 4 | agnostic | Infer skos:related relationships between concepts |
| **ThreeDAnalysis** | 4 | agnostic | Analyze 3D model geometry/materials via vision LLM |
| **Linking** | 3 | agnostic | Auto-detect and create knowledge graph edges; queues GraphMaintenance on completion |
| **BuildSetIndex** | 3 | agnostic | Build or rebuild vector index for embedding set |
| **GenerateGraphEmbedding** | 3 | agnostic | Generate graph embedding from extracted entities |
| **DocumentTypeInference** | 2 | agnostic | Auto-detect document type from filename/MIME/content |
| **TitleGeneration** | 2 | FAST_GPU (1) | LLM-generated descriptive titles from content |
| **CreateEmbeddingSet** | 2 | agnostic | Create new embedding set (evaluate criteria, add members) |
| **RefreshEmbeddingSet** | 2 | agnostic | Refresh embedding set (re-evaluate criteria, update membership) |
| **GenerateCoarseEmbedding** | 2 | agnostic | Generate coarse MRL embedding for two-stage retrieval |
| **GraphMaintenance** | 2 | agnostic | Graph quality pipeline: SNN scoring, PFNET sparsification, Louvain community detection |
| **ContextUpdate** | 1 | agnostic | Update context/metadata for related notes |
| **ReEmbedAll** | 1 | agnostic | Re-embed all notes (embedding model migration) |
| **GenerateFineTuningData** | 1 | agnostic | Generate synthetic query-document pairs for fine-tuning |

### Tiered Job Architecture

Jobs are classified into three compute tiers based on their model requirements. `JobType::default_cost_tier()` is the single source of truth for tier assignment. Most jobs are tier-agnostic (no GPU required).

| Tier | Name | Model | Assigned Job Types |
|------|------|-------|--------------------|
| NULL | agnostic | any / CPU | Most jobs (Embedding, Linking, AiRevision, Extraction, etc.) |
| 0 | CPU_NER | GLiNER | `ConceptTagging`, `ReferenceExtraction` (CPU-only, no GPU required) |
| 1 | FAST_GPU | qwen3:8b | `TitleGeneration`, `MetadataExtraction` |
| 2 | STANDARD_GPU | gpt-oss:20b | Full RAG generation, complex reasoning (via escalation) |

**Tiered drain order** — the worker processes tiers sequentially to avoid VRAM contention: agnostic/CPU jobs first, then FAST_GPU (with model warmup), then STANDARD_GPU (with model warmup). The worker loops until all tiers are drained before sleeping.

**Escalation via queue** — when a tier-N job produces insufficient results (e.g., GLiNER unavailable or concept count below threshold), the handler enqueues a new job of the same logical type at tier N+1. There is no inline model fallback: each job runs exactly one model, and the queue handles progression. This design keeps handlers simple and makes escalation observable via normal job events.

```
ConceptTagging (tier 0: GLiNER)
   └─ insufficient results → enqueue ConceptTagging (tier 1: qwen3:8b)
                                 └─ insufficient results → enqueue ConceptTagging (tier 2: gpt-oss:20b)
```

### Worker Architecture

**Configuration:**
```rust
WorkerConfig {
    poll_interval_ms: 60_000,     // Safety-net poll interval (not the primary wake mechanism)
    max_concurrent_jobs: 4,       // Parallel job limit
    enabled: true,                // Master switch
}
```

**Worker Lifecycle:**
```
┌─────────────────────────────────────────────────────┐
│  1. Initialization                                  │
│     - Register job handlers                         │
│     - Configure extraction registry                 │
│     - Set up event broadcast channel                │
│     - Configure model backends for tier warmup      │
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  2. Event-Driven Sleep                              │
│     - Wakes instantly when job is enqueued (Notify) │
│     - Safety-net timer (60s) catches edge cases     │
│     - Shutdown signal terminates loop               │
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  3. Tiered Drain Loop                               │
│     Phase 1: Drain agnostic/CPU jobs (tier NULL+0)  │
│     Phase 2: Warmup fast model → drain tier 1       │
│     Phase 3: Warmup standard model → drain tier 2   │
│     Loop until all tiers empty (handles escalations)│
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  4. Event Broadcasting (per job)                    │
│     - JobStarted                                    │
│     - JobProgress (percent, message)                │
│     - JobCompleted                                  │
│     - JobFailed (error message)                     │
└─────────────────────────────────────────────────────┘
```

**Handler Registration:**
```rust
let worker = WorkerBuilder::new(db)
    .with_config(WorkerConfig::from_env())
    .with_handler(EmbeddingHandler::new(inference.clone(), db.clone()))
    .with_handler(LinkingHandler::new(db.clone()))
    .with_handler(AiRevisionHandler::new(inference.clone(), db.clone()))
    .with_handler(GraphMaintenanceHandler::new(db.clone()))
    .with_handler(ExtractionHandler::new(db.clone(), registry.clone()))
    // ... additional handlers registered in main.rs
    .with_extraction_registry(registry)
    .build()
    .await;

let handle = worker.start();
```

### Retry Logic

**Configuration:**
```rust
pub const DEFAULT_MAX_RETRIES: i32 = 3;
```

**Retry Behavior:**

1. Handler returns `JobResult::Retry(error)`
2. Worker increments `retry_count`
3. If `retry_count < MAX_RETRIES`, job status set to `Pending` with backoff
4. If `retry_count >= MAX_RETRIES`, job status set to `Failed`

**Backoff Strategy:**
- Linear backoff: `retry_count * poll_interval_ms`
- Example with default 60s safety-net: jobs retry on the next wake cycle (event-driven wakeup or 60s timeout)

**Use Cases:**
- Transient network failures (Ollama connection)
- Rate limiting from external APIs
- Database deadlock resolution

### Event Broadcasting

**WorkerEvent Types:**
```rust
pub enum WorkerEvent {
    JobStarted { job_id, job_type },
    JobProgress { job_id, percent, message },
    JobCompleted { job_id, job_type },
    JobFailed { job_id, job_type, error },
    WorkerStarted,
    WorkerStopped,
}
```

**Event Flow:**
```
Worker -> tokio::sync::broadcast -> EventBus -> API Subscribers
   |                                   |              |
JobProgress                      ServerEvent      SSE/WebSocket
   |                                   |              |
{percent: 50}              JobProgress payload   Client UI update
```

**API Integration:**
The matric-api crate bridges worker events to `ServerEvent::JobProgress`:
```rust
let mut worker_events = worker.events();
tokio::spawn(async move {
    while let Ok(event) = worker_events.recv().await {
        match event {
            WorkerEvent::JobProgress { job_id, percent, message } => {
                event_bus.broadcast(ServerEvent::JobProgress {
                    job_id,
                    percent,
                    message,
                }).await;
            }
            // ... other event mappings
        }
    }
});
```

### Handler Interface

**JobHandler Trait:**
```rust
#[async_trait]
pub trait JobHandler: Send + Sync {
    fn job_type(&self) -> JobType;
    async fn execute(&self, ctx: JobContext) -> JobResult;
}

pub enum JobResult {
    Success(JsonValue),        // Job completed
    Failed(String),            // Job failed permanently
    Retry(String),             // Job failed, retry later
}
```

**JobContext Structure:**
```rust
pub struct JobContext {
    pub job: Job,
    progress_callback: Option<Box<dyn Fn(i32, Option<&str>) + Send + Sync>>,
}

impl JobContext {
    pub fn report_progress(&self, percent: i32, message: Option<&str>) {
        // Triggers WorkerEvent::JobProgress
    }
}
```

**Example Handler:**
```rust
pub struct EmbeddingHandler {
    inference: Arc<dyn InferenceBackend>,
}

#[async_trait]
impl JobHandler for EmbeddingHandler {
    fn job_type(&self) -> JobType {
        JobType::Embedding
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        ctx.report_progress(0, Some("Loading note content"));

        // Extract note_id from job.payload
        let note_id = ctx.job.payload["note_id"].as_str().unwrap();

        ctx.report_progress(50, Some("Generating embedding"));

        // Call inference backend
        let embedding = self.inference.embed(content).await
            .map_err(|e| JobResult::Retry(e.to_string()))?;

        ctx.report_progress(100, Some("Saving to database"));

        // Store embedding
        db.embeddings.create(note_id, embedding).await?;

        JobResult::Success(serde_json::json!({"embedding_id": id}))
    }
}
```

### Queue Management

**Priority Processing:**
Jobs are claimed in priority order using PostgreSQL `ORDER BY`:
```sql
SELECT * FROM job_queue
WHERE status = 'pending'
ORDER BY priority DESC, created_at ASC
LIMIT 1
FOR UPDATE SKIP LOCKED;
```

**Concurrency Control:**
- `FOR UPDATE SKIP LOCKED` prevents multiple workers claiming same job
- Worker marks job as `running` immediately after claim
- `max_concurrent_jobs` limits parallelism within single worker

**Status Transitions:**
```
Pending -> Running -> Completed
    |         |
    └─────> Failed (retry_count < MAX_RETRIES)
              |
           Pending (retry with backoff)
              |
           Failed (retry_count >= MAX_RETRIES)
```

## Database Schema

### Core Tables

```sql
-- Note metadata
CREATE TABLE note (
    id UUID PRIMARY KEY,
    collection_id UUID,
    format TEXT DEFAULT 'markdown',
    source TEXT DEFAULT 'api',
    created_at_utc TIMESTAMPTZ,
    updated_at_utc TIMESTAMPTZ,
    starred BOOLEAN DEFAULT FALSE,
    archived BOOLEAN DEFAULT FALSE,
    deleted BOOLEAN DEFAULT FALSE,
    title TEXT,
    metadata JSONB,
    document_type_id UUID REFERENCES document_type(id)
);

-- Immutable original content
CREATE TABLE note_original (
    note_id UUID PRIMARY KEY REFERENCES note(id),
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    user_created_at TIMESTAMPTZ,
    user_last_edited_at TIMESTAMPTZ
);

-- RAG-enhanced versions
CREATE TABLE note_revision (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    content TEXT NOT NULL,
    model TEXT,  -- LLM model used for generation
    ai_metadata JSONB,  -- RAG context, tokens, etc.
    created_at TIMESTAMPTZ
);

-- Sentence embeddings for dense retrieval
CREATE TABLE embedding (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    source TEXT,  -- 'original' or 'revision'
    model TEXT,   -- embedding model identifier
    embedding vector(768),  -- contrastive learning embeddings
    created_at TIMESTAMPTZ
);

-- Document type registry
CREATE TABLE document_type (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    category document_category NOT NULL,
    description TEXT,
    file_extensions TEXT[] DEFAULT '{}',
    mime_types TEXT[] DEFAULT '{}',
    magic_patterns TEXT[] DEFAULT '{}',
    filename_patterns TEXT[] DEFAULT '{}',
    chunking_strategy chunking_strategy NOT NULL,
    chunk_size_default INTEGER DEFAULT 1000,
    chunk_overlap_default INTEGER DEFAULT 100,
    preserve_boundaries BOOLEAN DEFAULT TRUE,
    chunking_config JSONB DEFAULT '{}',
    tree_sitter_language TEXT,
    extraction_strategy extraction_strategy,
    extraction_config JSONB DEFAULT '{}',
    requires_attachment BOOLEAN DEFAULT FALSE,
    is_system BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Search Indexes

```sql
-- Full-text search using GIN index (BM25-based ranking)
CREATE INDEX idx_note_original_fts ON note_original
    USING GIN (to_tsvector('english', content));

-- Vector similarity using HNSW index (O(log N) query complexity)
CREATE INDEX idx_embedding_vector ON embedding
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
```

The HNSW (Hierarchical Navigable Small World)[^9] index provides approximate nearest neighbor search with logarithmic query complexity, enabling sub-second semantic search over large document collections.

## Search Algorithm

### Reciprocal Rank Fusion (RRF)

Fortemi implements adaptive RRF[^4] for combining lexical and semantic retrieval results:

```rust
// RRF score calculation (Cormack et al., 2009)
score(doc) = Sigma 1/(k + rank_i(doc))

// Adaptive k parameter (default k=20)
// Short queries (<=2 tokens): k *= 0.7 (tighter fusion)
// Long queries (>=6 tokens): k *= 1.3 (looser fusion)
// Quoted queries: k *= 0.6 (precision focus)
```

RRF was chosen over alternatives like Condorcet fusion or CombMNZ because it:
- Requires no training data (unsupervised)
- Outperforms individual rankers by 4-5% on TREC benchmarks
- Achieves better results than supervised learning-to-rank methods[^4]

The k parameter was optimized from the original k=60 recommendation to k=20 based on Elasticsearch BEIR benchmark analysis (2024), which showed improved performance for knowledge base retrieval.

### Relative Score Fusion (RSF)

Alternative to RRF using min-max normalization:

```rust
// Normalize scores to [0,1] via min-max scaling
normalized_score = (score - min) / (max - min)

// Combine with weighted sum
final_score = w_fts * norm_fts + w_sem * norm_sem
```

RSF preserves score magnitude differences, unlike RRF which only uses rank position. Weaviate made RSF their default fusion in v1.24 (2024) after measuring +6% recall on the FIQA benchmark.

### Adaptive Weights

Query-dependent FTS/semantic weight selection:

| Query Type | FTS | Semantic | Rationale |
|------------|-----|----------|-----------|
| Quoted phrases | 0.7 | 0.3 | Lexical precision matters |
| Keywords (1-2 tokens) | 0.6 | 0.4 | FTS handles keywords well |
| Natural language (3-5) | 0.5 | 0.5 | Balanced |
| Conceptual (6+ tokens) | 0.35 | 0.65 | Semantic captures intent |

### Hybrid Retrieval Pipeline

1. Parse query string and extract filters
2. **Apply strict tag filter** - Pre-filters via SQL WHERE on SKOS concepts
3. Execute **lexical retrieval** (BM25 via tsvector) within filtered set
4. Execute **dense retrieval** (embedding similarity) within filtered set
5. **Fuse results with RRF or RSF** - Combine rankings from both systems
6. Apply additional soft filters (dates, metadata)
7. **Deduplicate chunked documents** - Keep best-scoring chunk per document
8. Return top-k results with combined scores

**Strict vs Soft Filtering:**
- **Strict filter**: Guaranteed isolation via pre-search WHERE clause (100% precision)
- **Soft filter**: Combined with fuzzy search, may have relevance-based ordering

## Embedding Pipeline

### DocumentComposition

`DocumentComposition` (defined in `matric_core::models`) is the single most important characteristic of an embedding set — it entirely determines the semantic geometry of the vector space. Different compositions produce fundamentally different clustering behaviors.

**Fields:**

| Field | Default | Description |
|-------|---------|-------------|
| `include_title` | `true` | Include note title in embedding text |
| `include_content` | `true` | Include note body in embedding text |
| `tag_strategy` | `None` | Which SKOS tags to inject: `None`, `All`, `Schemes([...])`, `Specific([...])` |
| `include_concepts` | `false` | Include SKOS concept labels with TF-IDF filtering |
| `concept_max_doc_freq` | 0.8 | Exclude concepts appearing in more than this fraction of notes |
| `instruction_prefix` | `"clustering: "` | Instruction prefix for the model (e.g., `"search_document: "`, `"clustering: "`) |

**Storage:** Each embedding set stores its own `DocumentComposition` in the `embedding_set` table as a JSONB column. Notes inherit the composition from their embedding set's config. The empty object `{}` inherits defaults (title + content only).

**Text Construction:**
```rust
// DocumentComposition::build_text assembles the final embedding input
let text = composition.build_text(
    &note.title,
    &note.content,
    &concept_labels,  // pre-fetched SKOS labels, filtered by tag_strategy
);
// Result: "{instruction_prefix}{title}\n\n{tags}\n\n{content}"
```

**Example compositions:**
- Default: title + content (best for general semantic similarity)
- `tag_strategy: All`: title + tags + content (topic-aware clustering)
- `include_concepts: true`: adds SKOS concept labels as metadata signal

## Knowledge Graph Construction

Fortemi automatically constructs a knowledge graph[^5] by discovering semantic relationships between notes:

1. **Embedding Generation** - Each note is encoded as a sentence embedding[^7] using its configured `DocumentComposition`
2. **Similarity Computation** - Cosine similarity calculated between all note pairs
3. **Link Creation** - Notes with >70% similarity are bidirectionally linked
4. **Property Storage** - Similarity scores stored as edge weights in property graph
5. **Graph Maintenance** - After linking completes, a `GraphMaintenance` job runs automatically to improve graph quality

The 70% threshold balances precision (avoiding false connections) with recall (discovering meaningful relationships), validated empirically against semantic textual similarity benchmarks.

## Graph Quality Pipeline

After raw similarity edges are created by the `Linking` job, the `GraphMaintenance` job runs an automatic pipeline to improve graph quality. The pipeline is implemented in `PgLinkRepository` (`crates/matric-db/src/links.rs`) and orchestrated by `GraphMaintenanceHandler` (`crates/matric-api/src/handlers/jobs.rs`).

The pipeline executes up to four configurable steps:

```
Raw similarity edges (from Linking job)
         │
         ▼
┌─────────────────────────────────────────────────────┐
│  Step 1: Edge Normalization                         │
│  Rescale raw cosine scores to [0.0, 1.0] with       │
│  optional gamma exponent for contrast enhancement.  │
│  Formula: normalized = ((score - min) / range)^γ   │
│  Applied at graph traversal time (query-time).      │
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  Step 2: SNN Scoring (Shared Nearest Neighbor)      │
│  SNN(A, B) = |kNN(A) ∩ kNN(B)| / k                 │
│  k = log2(N) clamped to [k_min, k_max]             │
│  Edges below threshold are pruned.                  │
│  Result: edges annotated with snn_score in metadata.│
│  Skipped when graph is too sparse (mean degree < k).│
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  Step 3: PFNET Sparsification                       │
│  Pathfinder Network pruning removes geometrically   │
│  redundant edges. Uses graph-PFNET approximation    │
│  (witnesses from shared neighbors only).            │
│  Equivalent to Relative Neighborhood Graph          │
│  (Toussaint 1980) with PFNET(∞, q=2).              │
│  Result: retained edges tagged pfnet_retained=true. │
└────────────────┬────────────────────────────────────┘
                 ▼
┌─────────────────────────────────────────────────────┐
│  Step 4: Graph Quality Snapshot                     │
│  Records before/after diagnostics: edge counts,     │
│  SNN coverage fraction, PFNET retention ratio.      │
└─────────────────────────────────────────────────────┘
         │
         ▼
Quality graph (used for graph traversal + visualization)
```

**Louvain Community Detection** runs separately, at graph traversal time (not during maintenance). When `GET /api/v1/notes/:id/graph` is called, the traversal result includes community assignments computed inline via a pure-Rust Louvain implementation:

- Each traversal result has `community_id`, `community_label`, and `community_confidence` per node
- Community labels are derived from the most frequent SKOS concept among notes in each community
- Configurable resolution parameter controls community granularity

**Configuration** (via `GraphConfig` from environment variables or defaults):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `snn_threshold` | 0.10 | Minimum SNN score to retain an edge |
| `k_min` / `k_max` | 5 / 15 | Bounds for adaptive k in SNN computation (k = log2(N) clamped) |
| `pfnet_q` | 2 | PFNET Minkowski parameter (2 = Euclidean / Relative Neighborhood Graph) |
| `normalization_gamma` | 1.0 | Gamma exponent for edge weight normalization (1.0 = linear) |
| `community_resolution` | 1.0 | Louvain resolution (higher = more, smaller communities) |

**Triggering:**

- **Automatic**: Queued by `Linking` job on completion (deduplicated: only one pending `GraphMaintenance` at a time)
- **Manual**: `POST /api/v1/graph/maintenance` with optional `steps` array to run specific pipeline stages
- **Selective**: Payload can specify `{"steps": ["snn", "pfnet"]}` to skip normalization or snapshot steps

## API Design

### RESTful Conventions

- `GET /api/v1/resources` - List resources
- `POST /api/v1/resources` - Create resource
- `GET /api/v1/resources/:id` - Get single resource
- `PATCH /api/v1/resources/:id` - Update resource
- `DELETE /api/v1/resources/:id` - Delete resource

### Response Formats

```json
// Success (list)
{
  "notes": [...],
  "total": 42
}

// Success (single)
{
  "id": "uuid",
  "title": "...",
  ...
}

// Error
{
  "error": "Resource not found"
}
```

### Status Codes

- `200 OK` - Success
- `201 Created` - Resource created
- `204 No Content` - Update/delete success
- `400 Bad Request` - Invalid input
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

## Security Considerations

- No authentication at API level (consumer responsibility)
- Database credentials via environment variables
- TLS termination at reverse proxy (nginx)
- CORS headers for browser access
- Input validation on all endpoints
- PKE encryption for sensitive data sharing

## Performance Targets

| Metric | Target | Research Basis |
|--------|--------|----------------|
| Hybrid search p95 | <200ms (10k docs) | RRF adds minimal overhead[^4] |
| Hybrid search p95 | <500ms (100k docs) | HNSW O(log N) scaling[^9] |
| API response time | <100ms (CRUD) | Standard REST latency |
| Embedding generation | <2s per note | Model-dependent |

## Deployment

### Production

```
┌─────────────────────────────────────────┐
│  nginx (TLS termination, /etc/nginx)   │
│  your-domain.com:443                   │
└───────────────┬─────────────────────────┘
                │ :3000
┌───────────────▼─────────────────────────┐
│  matric-api (systemd service)           │
│  /path/to/fortemi       │
└───────────────┬─────────────────────────┘
                │
┌───────────────▼─────────────────────────┐
│  PostgreSQL + pgvector                  │
│  localhost:5432                         │
└─────────────────────────────────────────┘
```

### Development

```bash
# Start with docker-compose
docker-compose up -d

# Run API server
cargo run -p matric-api
```

## ADR Summary

| ADR | Decision | Rationale | Research Basis |
|-----|----------|-----------|----------------|
| ADR-001 | Strict tag filtering | Pre-search WHERE for guaranteed isolation | See ADR-001 |
| ADR-002 | PostgreSQL + pgvector | Simplicity, proven at 100k docs | HNSW[^9] |
| ADR-003 | InferenceBackend trait | Pluggable backends (Ollama, OpenAI) | - |
| ADR-004 | RRF fusion | Outperforms alternatives unsupervised | Cormack et al.[^4] |
| ADR-005 | X25519 + AES-256-GCM | Forward secrecy with ephemeral keys | - |
| ADR-006 | Envelope encryption | Efficient multi-recipient | - |
| ADR-007 | Argon2id key storage | Memory-hard KDF protection | - |
| ADR-008 | SKOS vocabulary | W3C standard for controlled terms | W3C[^6] |
| ADR-037 | Unified event bus | Single broadcast channel for SSE, WebSocket, webhooks, telemetry | - |
| ADR-068 | Archive isolation routing | Schema-per-memory with SchemaContext + middleware | See ADR-068 |
| Issue #467 | Versioned graph payload contract | v1 graph response with nodes, edges, meta + guardrails | See links.rs |
| Issue #470 | Edge normalization | Gamma-adjusted min-max normalization at query time | See links.rs |
| Issue #473 | Louvain community detection | Pure-Rust inline community assignment at traversal time | See links.rs |
| Issue #474 | SNN scoring | Shared Nearest Neighbor pruning in GraphMaintenance pipeline | See links.rs |
| Issue #476 | PFNET sparsification | Pathfinder Network pruning in GraphMaintenance pipeline | See links.rs |

See `.aiwg/intake/option-matrix.md` for detailed analysis.
See `docs/adr/ADR-001-strict-tag-filtering.md` for strict filtering details.
See `docs/adr/ADR-068-archive-isolation-routing.md` for multi-memory implementation details.

---

## References

[^1]: Lewis, P., et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." NeurIPS 2020. [REF-008]

[^2]: Robertson, S., & Zaragoza, H. (2009). "The probabilistic relevance framework: BM25 and beyond." Foundations and Trends in Information Retrieval. [REF-028]

[^3]: Karpukhin, V., et al. (2020). "Dense passage retrieval for open-domain question answering." EMNLP 2020. [REF-029]

[^4]: Cormack, G. V., Clarke, C. L. A., & Buttcher, S. (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." SIGIR '09. [REF-027]

[^5]: Hogan, A., et al. (2021). "Knowledge graphs." ACM Computing Surveys. [REF-032]

[^6]: Miles, A., & Bechhofer, S. (2009). "SKOS simple knowledge organization system reference." W3C Recommendation. [REF-033]

[^7]: Reimers, N., & Gurevych, I. (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." EMNLP 2019. [REF-030]

[^8]: Gao, T., Yao, X., & Chen, D. (2021). "SimCSE: Simple contrastive learning of sentence embeddings." EMNLP 2021.

[^9]: Malkov, Y. A., & Yashunin, D. A. (2020). "Efficient and robust approximate nearest neighbor search using hierarchical navigable small world graphs." IEEE TPAMI. [REF-031]

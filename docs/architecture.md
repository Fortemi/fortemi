# matric-memory Architecture

## Overview

matric-memory is a Rust workspace consisting of 7 crates that together provide vector-enhanced note storage, hybrid search, NLP pipeline management, and secure data encryption.

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
          │  + pgvector   │
          └───────────────┘
```

## Crate Dependencies

```
matric-core (traits, types, errors)
     │
     ├── matric-db (database layer)
     │        │
     │        ├── matric-search (hybrid search)
     │        │
     │        └── matric-jobs (job processing)
     │
     ├── matric-inference (LLM abstraction)
     │        │
     │        └── ollama, openai backends
     │
     ├── matric-crypto (encryption)
     │
     └── matric-api (HTTP server)
              │
              └── uses all other crates
```

## Crate Details

### matric-core

Core types and traits shared across all crates.

**Key Components:**
- `Error` - Unified error type with domain-specific variants
- `Note`, `NoteSummary`, `NoteFull` - Note data models
- `Job`, `JobType`, `JobStatus` - Job queue models
- `Tag`, `Link` - Relationship models
- `SearchHit` - Search result model
- Repository traits: `NoteRepository`, `TagRepository`, `LinkRepository`, `JobRepository`

### matric-db

PostgreSQL database layer with pgvector support.

**Key Components:**
- `Database` - Connection pool manager
- `PgNoteRepository` - Note CRUD operations
- `PgTagRepository` - Tag management
- `PgLinkRepository` - Link management
- `PgJobRepository` - Job queue operations

**Tables:**
- `note` - Note metadata
- `note_original` - Immutable original content
- `note_revision` - AI-revised versions
- `embedding` - Vector embeddings (pgvector)
- `tag`, `note_tag` - Tag system
- `link` - Note relationships
- `job_queue` - Background jobs

### matric-search

Hybrid search engine combining FTS and semantic search with strict tag filtering.

**Key Components:**
- `HybridSearchEngine` - Main search coordinator
- `HybridSearchConfig` - Search mode configuration with optional strict filter
- `SearchRequest` - Query builder pattern
- `StrictTagFilter` - Guaranteed tag-based isolation
- `rrf_fusion()` - Reciprocal Rank Fusion algorithm

**Search Modes:**
1. **FTS Only** - PostgreSQL tsvector/GIN full-text search
2. **Semantic Only** - pgvector cosine similarity
3. **Hybrid** (default) - Combined with RRF fusion

**Strict Filtering:**
- Pre-search WHERE clause applied before fuzzy matching
- Guarantees 100% isolation by SKOS concepts/schemes
- Supports AND/OR/NOT logic on tags
- Foundation for multi-tenancy

### matric-inference

LLM inference abstraction for text generation and embeddings.

**Key Components:**
- `InferenceBackend` trait - Pluggable backend interface
- `OllamaBackend` - Ollama local inference (default)
- `OpenAIBackend` - OpenAI-compatible API inference (feature-gated)
- `EmbeddingRequest/Response` - Embedding generation
- `GenerateRequest/Response` - Text generation
- `ModelRegistry` - Model profiles and recommendations

**Backends:**
- **Ollama** - Local inference via Ollama API
- **OpenAI** - Cloud or local OpenAI-compatible APIs (OpenAI, vLLM, LocalAI, etc.)

### matric-crypto

Public-key encryption (PKE) for secure data sharing.

**Key Components:**
- `encrypt_pke` / `decrypt_pke` - Multi-recipient public-key encryption
- `Keypair` / `Address` - X25519 keypairs and wallet-style addresses
- `save_private_key` / `load_private_key` - Encrypted key storage
- `detect_format` - Auto-detect encrypted file formats
- `DerivedKey` - Secure key wrapper with zeroize

**Cryptographic Primitives:**
- X25519 (Curve25519 ECDH key exchange)
- AES-256-GCM (symmetric encryption)
- HKDF-SHA256 (key derivation from shared secrets)
- BLAKE3 (address hashing)
- Argon2id (private key storage encryption)
- ChaCha20-based CSPRNG (random generation)

**File Format:**
- MMPKE01 - Public-key multi-recipient encryption (wallet-style)

### matric-jobs

Background job processing for async NLP operations.

**Key Components:**
- `JobWorker` - Background worker process
- `JobHandler` trait - Job type handlers
- Job types: `Embedding`, `AiRevision`, `Linking`, `TitleGeneration`, `ContextUpdate`

**Job Flow:**
1. API creates job via `POST /api/v1/jobs`
2. Job inserted into `job_queue` table
3. Worker polls for pending jobs
4. Handler processes job (calls inference, updates DB)
5. Job marked complete/failed

### matric-api

HTTP REST API server using Axum.

**Key Features:**
- RESTful endpoints for CRUD operations
- OpenAPI 3.1 specification
- Swagger UI at `/docs`
- CORS support
- Request tracing

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
    metadata JSONB
);

-- Immutable original content
CREATE TABLE note_original (
    note_id UUID PRIMARY KEY REFERENCES note(id),
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    user_created_at TIMESTAMPTZ,
    user_last_edited_at TIMESTAMPTZ
);

-- AI-revised versions
CREATE TABLE note_revision (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    content TEXT NOT NULL,
    model TEXT,
    ai_metadata JSONB,
    created_at TIMESTAMPTZ
);

-- Vector embeddings
CREATE TABLE embedding (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    source TEXT,
    model TEXT,
    embedding vector(768),
    created_at TIMESTAMPTZ
);
```

### Search Indexes

```sql
-- Full-text search (GIN)
CREATE INDEX idx_note_original_fts ON note_original
    USING GIN (to_tsvector('english', content));

-- Vector similarity (HNSW)
CREATE INDEX idx_embedding_vector ON embedding
    USING hnsw (embedding vector_cosine_ops);
```

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

## Search Algorithm

### Reciprocal Rank Fusion (RRF)

```rust
// Combine FTS and semantic results
score(doc) = Σ 1/(k + rank_i(doc))

// Parameters
k = 60 (default smoothing factor)
fts_weight = 0.5
semantic_weight = 0.5
```

### Search Pipeline

1. Parse query string
2. **Apply strict tag filter (if provided)** - pre-filters note IDs via SQL WHERE
3. Execute FTS query (tsvector match) within filtered set
4. Execute semantic query (embedding similarity) within filtered set
5. Merge results with RRF
6. Apply soft filters (tags, dates)
7. Return top-k results

**Strict vs Soft Filtering:**
- **Strict filter**: Guaranteed isolation, applied before search (via `strict_filter` parameter)
- **Soft filter**: Combined with fuzzy search, may have false positives (query string syntax)

## Security Considerations

- No authentication at API level (consumer responsibility)
- Database credentials via environment variables
- TLS termination at reverse proxy (nginx)
- CORS headers for browser access
- Input validation on all endpoints

## Performance Targets

| Metric | Target |
|--------|--------|
| Search p95 latency | <200ms (10k docs) |
| Search p95 latency | <500ms (100k docs) |
| API response time | <100ms (CRUD) |
| Embedding generation | <2s per note |

## Deployment

### Production

```
┌─────────────────────────────────────────┐
│  nginx (TLS termination, /etc/nginx)   │
│  memory.integrolabs.net:443            │
└───────────────┬─────────────────────────┘
                │ :3000
┌───────────────▼─────────────────────────┐
│  matric-api (systemd service)           │
│  /home/roctinam/dev/matric-memory       │
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

| ADR | Decision | Rationale |
|-----|----------|-----------|
| ADR-001 | Multi-crate workspace | Modularity, independent compilation |
| ADR-002 | PostgreSQL + pgvector | Simplicity, proven at 100k docs |
| ADR-003 | InferenceBackend trait | Pluggable backends (Ollama, OpenAI) |
| ADR-004 | RRF fusion | Industry standard for hybrid search |
| ADR-005 | X25519 + AES-256-GCM | Wallet-style PKE with ephemeral keys for forward secrecy |
| ADR-006 | Envelope encryption | Efficient multi-recipient without re-encryption |
| ADR-007 | Argon2id for key storage | Memory-hard KDF protects private keys at rest |
| ADR-008 | Strict tag filtering | Pre-search WHERE clause for guaranteed SKOS tag isolation |

See `.aiwg/intake/option-matrix.md` for detailed analysis.
See `docs/adr/ADR-001-strict-tag-filtering.md` for strict filtering details.

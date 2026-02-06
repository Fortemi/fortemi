# Fortémi Architecture

## Overview

Fortémi is an AI-enhanced knowledge management system implementing Retrieval-Augmented Generation (RAG)[^1] with hybrid search combining full-text retrieval (BM25)[^2] and dense passage retrieval[^3] via Reciprocal Rank Fusion (RRF)[^4]. The system provides automatic knowledge graph construction[^5] through semantic similarity analysis and W3C SKOS-compliant[^6] controlled vocabulary management.

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
- Repository traits: `NoteRepository`, `TagRepository`, `LinkRepository`, `JobRepository`

**Advanced Features:**
- `events.rs` - Unified event bus (`EventBus`) and `ServerEvent` enum for real-time notifications (SSE, WebSocket, webhooks, telemetry). See ADR-037.
- `fair.rs` - FAIR metadata export (Findable, Accessible, Interoperable, Reusable) with Dublin Core and JSON-LD support
- `temporal.rs` - Temporal filtering for time-based queries
- `uuid_utils.rs` - UUIDv7 generation for time-ordered identifiers
- `strict_filter.rs` - Type-safe strict filtering predicates

### matric-db

PostgreSQL database layer with pgvector (vector similarity) and PostGIS (spatial queries) extensions.

**Key Components:**
- `Database` - Connection pool manager
- `PgNoteRepository` - Note CRUD operations
- `PgTagRepository` - SKOS concept management
- `PgLinkRepository` - Knowledge graph edge management
- `PgJobRepository` - Job queue operations

**Advanced Features:**
- `provenance.rs` - W3C PROV tracking for AI revision operations (entities, activities, relations)
- `memory_search.rs` - Temporal-spatial search on file provenance (PostGIS location, time range, combined queries)
- `versioning.rs` - Dual-track note history (original content versions + revision versions)
- `unified_filter.rs` - Multi-dimensional filtering combining tags, dates, collections, metadata
- `strict_filter.rs` - Pre-search WHERE clause implementation for guaranteed data isolation
- `oauth.rs` - OAuth provider integration for authentication
- `skos_tags.rs` - W3C SKOS semantic tagging with Collections support

**Tables:**
- `note` - Note metadata
- `note_original` - Immutable original content
- `note_revision` - RAG-enhanced versions
- `embedding` - Sentence embeddings[^7] stored as pgvector
- `skos_concepts`, `skos_labels`, `skos_relations` - W3C SKOS vocabulary[^6]
- `note_links` - Knowledge graph edges with similarity scores
- `job_queue` - Background NLP jobs
- `provenance_edge`, `provenance_activity` - W3C PROV tracking tables

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
Uses contrastive learning-based models[^8] (nomic-embed-text) producing 768-dimensional sentence embeddings with mean pooling aggregation[^7].

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

Background job processing for asynchronous NLP operations.

**Key Components:**
- `JobWorker` - Background worker process
- `JobHandler` trait - Job type handlers
- Job types: `Embedding`, `AiRevision`, `Linking`, `TitleGeneration`, `ContextUpdate`

**RAG Pipeline Jobs:**
1. **Embedding** - Generate sentence embeddings[^7] for semantic search
2. **AiRevision** - RAG-based content enhancement[^1] with retrieved context
3. **Linking** - Knowledge graph construction[^5] via embedding similarity (>70% threshold)
4. **TitleGeneration** - LLM-generated descriptive titles
5. **ContextUpdate** - Inject related note context into revisions

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

Fortémi implements adaptive RRF[^4] for combining lexical and semantic retrieval results:

```rust
// RRF score calculation (Cormack et al., 2009)
score(doc) = Σ 1/(k + rank_i(doc))

// Adaptive k parameter (default k=20)
// Short queries (≤2 tokens): k *= 0.7 (tighter fusion)
// Long queries (≥6 tokens): k *= 1.3 (looser fusion)
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

## Knowledge Graph Construction

Fortémi automatically constructs a knowledge graph[^5] by discovering semantic relationships between notes:

1. **Embedding Generation** - Each note is encoded as a 768-dim sentence embedding[^7]
2. **Similarity Computation** - Cosine similarity calculated between all note pairs
3. **Link Creation** - Notes with >70% similarity are bidirectionally linked
4. **Property Storage** - Similarity scores stored as edge weights in property graph

The 70% threshold balances precision (avoiding false connections) with recall (discovering meaningful relationships), validated empirically against semantic textual similarity benchmarks.

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

See `.aiwg/intake/option-matrix.md` for detailed analysis.
See `docs/adr/ADR-001-strict-tag-filtering.md` for strict filtering details.

---

## References

[^1]: Lewis, P., et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." NeurIPS 2020. [REF-008]

[^2]: Robertson, S., & Zaragoza, H. (2009). "The probabilistic relevance framework: BM25 and beyond." Foundations and Trends in Information Retrieval. [REF-028]

[^3]: Karpukhin, V., et al. (2020). "Dense passage retrieval for open-domain question answering." EMNLP 2020. [REF-029]

[^4]: Cormack, G. V., Clarke, C. L. A., & Büttcher, S. (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." SIGIR '09. [REF-027]

[^5]: Hogan, A., et al. (2021). "Knowledge graphs." ACM Computing Surveys. [REF-032]

[^6]: Miles, A., & Bechhofer, S. (2009). "SKOS simple knowledge organization system reference." W3C Recommendation. [REF-033]

[^7]: Reimers, N., & Gurevych, I. (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." EMNLP 2019. [REF-030]

[^8]: Gao, T., Yao, X., & Chen, D. (2021). "SimCSE: Simple contrastive learning of sentence embeddings." EMNLP 2021.

[^9]: Malkov, Y. A., & Yashunin, D. A. (2020). "Efficient and robust approximate nearest neighbor search using hierarchical navigable small world graphs." IEEE TPAMI. [REF-031]

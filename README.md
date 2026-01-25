# matric-memory

**AI-Enhanced Knowledge Management System with Hybrid Retrieval**

A Rust implementation of Retrieval-Augmented Generation (RAG) combining full-text search (BM25) with dense passage retrieval via Reciprocal Rank Fusion (RRF), automatic knowledge graph construction, and W3C SKOS-compliant controlled vocabulary management.

[![Build Status](https://git.integrolabs.net/roctinam/matric-memory/actions/workflows/ci.yaml/badge.svg)](https://git.integrolabs.net/roctinam/matric-memory/actions)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)]()

---

## Quick Navigation

| Audience | Start Here |
|----------|------------|
| **Executives** | [What It Does](#what-it-does) · [Key Capabilities](#key-capabilities) |
| **Developers** | [Quick Start](#quick-start) · [API Reference](#api-endpoints) |
| **Researchers** | [Technical Foundation](#technical-foundation) · [Architecture](docs/architecture.md) |

---

## What It Does

matric-memory transforms unstructured notes into a navigable knowledge graph with intelligent retrieval. Unlike simple note storage, it:

- **Understands meaning** - Semantic search finds conceptually related content even without keyword matches
- **Discovers connections** - Automatically links related notes via embedding similarity analysis
- **Enhances content** - RAG pipeline enriches notes with context from related knowledge
- **Guarantees isolation** - Strict SKOS-based filtering ensures 100% data segregation for multi-tenancy

## Key Capabilities

| Capability | Implementation | Research Basis |
|------------|----------------|----------------|
| **Hybrid Search** | RRF fusion of BM25 + dense retrieval | Cormack et al. (2009) |
| **Semantic Understanding** | 768-dim sentence embeddings | Reimers & Gurevych (2019) |
| **Knowledge Graph** | Automatic linking at >70% similarity | Hogan et al. (2021) |
| **Controlled Vocabulary** | W3C SKOS concepts and relations | Miles & Bechhofer (2009) |
| **Content Enhancement** | Retrieval-augmented generation | Lewis et al. (2020) |
| **Vector Indexing** | HNSW approximate nearest neighbor | Malkov & Yashunin (2020) |

---

## Technical Foundation

matric-memory implements state-of-the-art information retrieval techniques:

### Hybrid Retrieval with RRF

Combines **lexical retrieval** (BM25 via PostgreSQL tsvector) with **dense retrieval** (sentence embeddings via pgvector) using Reciprocal Rank Fusion:

```
RRFscore(d) = Σ 1/(k + rank_i(d))    where k=60
```

RRF consistently outperforms individual rankers and supervised learning-to-rank methods on TREC benchmarks (Cormack et al., 2009).

### Sentence Embeddings

Uses contrastive learning-based models (nomic-embed-text) producing 768-dimensional embeddings. Documents are encoded using bi-encoder architecture for efficient similarity computation (Reimers & Gurevych, 2019).

### Knowledge Graph Construction

Automatically constructs a property graph by:
1. Encoding notes as sentence embeddings
2. Computing pairwise cosine similarity
3. Creating bidirectional links above 70% threshold
4. Storing similarity scores as edge weights

### HNSW Vector Index

Approximate nearest neighbor search via Hierarchical Navigable Small World graphs provides O(log N) query complexity, enabling sub-second semantic search over large collections (Malkov & Yashunin, 2020).

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        matric-memory                            │
├─────────────────────────────────────────────────────────────────┤
│  matric-api      │ HTTP REST API with OpenAPI/Swagger           │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-search   │ Hybrid retrieval (BM25 + dense + RRF)        │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-jobs     │ Async NLP pipeline (embedding, RAG, linking) │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-inference│ LLM abstraction (Ollama, OpenAI backends)    │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-db       │ PostgreSQL + pgvector + SKOS vocabulary      │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-crypto   │ X25519/AES-256-GCM public-key encryption     │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-core     │ Core types, traits, and error handling       │
└─────────────────────────────────────────────────────────────────┘
```

See [docs/architecture.md](docs/architecture.md) for detailed system design with research citations.

---

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 14+ with pgvector extension
- Ollama (for local embedding generation)

### Database Setup

```bash
# Install pgvector extension
psql -c "CREATE EXTENSION IF NOT EXISTS vector;"

# Run migrations
sqlx migrate run
```

### Running the API Server

```bash
# Set environment variables
export DATABASE_URL="postgres://user:pass@localhost/matric"
export OLLAMA_URL="http://localhost:11434"

# Build and run
cargo build --release
./target/release/matric-api
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/v1/notes` | GET, POST | List/create notes |
| `/api/v1/notes/:id` | GET, PATCH, DELETE | CRUD operations |
| `/api/v1/search` | GET | Hybrid search with mode selection |
| `/api/v1/tags` | GET | SKOS concept listing |
| `/api/v1/collections` | GET, POST | Hierarchical organization |
| `/api/v1/graph/:id/explore` | GET | Knowledge graph traversal |
| `/api/v1/jobs` | GET, POST | NLP pipeline job management |
| `/docs` | GET | Swagger UI |

### Search Modes

```bash
# Hybrid search (RRF fusion) - default
curl "https://memory.integrolabs.net/api/v1/search?q=retrieval+augmented+generation"

# Lexical only (BM25)
curl "https://memory.integrolabs.net/api/v1/search?q=RAG&mode=fts"

# Semantic only (dense retrieval)
curl "https://memory.integrolabs.net/api/v1/search?q=machine+learning&mode=semantic"
```

---

## MCP Server

Model Context Protocol server for AI agent integration:

```bash
cd mcp-server
npm install
```

Claude Desktop configuration (`~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "matric-memory": {
      "command": "node",
      "args": ["/path/to/matric-memory/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "https://memory.integrolabs.net"
      }
    }
  }
}
```

---

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection URL |
| `HOST` | `0.0.0.0` | API server bind address |
| `PORT` | `3000` | API server port |
| `OLLAMA_URL` | `http://localhost:11434` | Inference backend endpoint |
| `RUST_LOG` | `matric_api=debug` | Log level configuration |

---

## Development

```bash
# Run tests
cargo test --workspace

# Run with logging
RUST_LOG=debug cargo run -p matric-api

# Format and lint
cargo fmt && cargo clippy -- -D warnings
```

### CI/CD Pipeline

Gitea Actions runs:
- Code formatting and linting checks
- Unit and integration tests
- GPU-enabled tests with Ollama
- Docker image builds

See [docs/ci-cd.md](docs/ci-cd.md) for details.

---

## Documentation

| Document | Audience | Description |
|----------|----------|-------------|
| [Architecture](docs/architecture.md) | Technical | System design with research citations |
| [Glossary](docs/glossary.md) | All | Professional terminology definitions |
| [Terminology Mapping](docs/terminology-mapping.md) | Contributors | Informal → professional term mapping |
| [ADR-001: Strict Filtering](docs/adr/ADR-001-strict-tag-filtering.md) | Technical | Tag-based isolation decision |

---

## References

The implementation draws from peer-reviewed research:

- Cormack, G. V., Clarke, C. L. A., & Büttcher, S. (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." *SIGIR '09*.
- Lewis, P., et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." *NeurIPS 2020*.
- Reimers, N., & Gurevych, I. (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." *EMNLP 2019*.
- Malkov, Y. A., & Yashunin, D. A. (2020). "Efficient and robust approximate nearest neighbor search using HNSW." *IEEE TPAMI*.
- Hogan, A., et al. (2021). "Knowledge graphs." *ACM Computing Surveys*.
- Miles, A., & Bechhofer, S. (2009). "SKOS simple knowledge organization system reference." *W3C Recommendation*.

See [docs/research/](docs/research/) for detailed paper analyses.

---

## Related Projects

- [HotM](https://git.integrolabs.net/roctinam/hotm) - Knowledge management frontend

## License

MIT OR Apache-2.0

# Fortémi

*Pronounced: for-TAY-mee*

**Memory that understands.**

Most storage systems are passive—they hold your data and wait for exact queries. Fortémi is different. It's an intelligent memory system that actually comprehends what you store: the meaning behind your notes, the relationships between ideas, and the context that connects them.

Ask it a question, and it doesn't just search for matching words. It finds answers that are *conceptually relevant*, even when you can't remember the right terminology. Store a document, and it automatically discovers how that knowledge connects to everything else you know. Over time, your knowledge base becomes a living network that grows smarter with every piece of information you add.

Built in Rust. Backed by PostgreSQL. Powered by embeddings. No cloud dependency required.

> **Under the hood:** Hybrid retrieval (BM25 + dense vectors) with Reciprocal Rank Fusion, automatic knowledge graph construction, 131 document types, and W3C SKOS vocabulary management. ~85k lines of Rust.

[![License](https://img.shields.io/badge/license-BSL--1.1-blue.svg)](LICENSE)

---

## Quick Navigation

| Audience | Start Here |
|----------|------------|
| **New Users** | [Getting Started](docs/content/getting-started.md) · [Use Cases](docs/content/use-cases.md) |
| **Executives** | [What It Does](#what-it-does) · [Key Capabilities](#key-capabilities) |
| **Developers** | [Quick Start](#quick-start) · [API Reference](#api-endpoints) |
| **Operators** | [Configuration](#configuration) · [Operators Guide](docs/content/operators-guide.md) |
| **Researchers** | [Technical Foundation](#technical-foundation) · [Architecture](docs/content/architecture.md) |
| **Best Practices** | [Best Practices](docs/content/best-practices.md) · [Configuration](docs/content/configuration.md) |

---

## What It Does

Fortemi transforms unstructured notes into a navigable knowledge graph with intelligent retrieval. Unlike simple note storage, it:

- **Understands meaning** - Semantic search finds conceptually related content even without keyword matches
- **Discovers connections** - Automatically links related notes via embedding similarity analysis
- **Enhances content** - RAG pipeline enriches notes with context from related knowledge
- **Guarantees isolation** - Strict SKOS-based filtering ensures 100% data segregation for multi-tenancy

## Use Cases

Fortemi adapts to how you actually work:
- **Personal knowledge management** - Organize research notes, articles, and ideas with automatic linking and semantic search
- **Team documentation hub** - Centralize team knowledge with controlled vocabularies and guaranteed data isolation
- **AI research assistant / RAG pipeline** - Power chatbots and AI agents with hybrid retrieval and context-aware generation
- **Enterprise document management** - Process and search across 131 document types with automatic classification
- **Hybrid cloud/edge deployment** - Run on-premises for data sovereignty or in cloud for scalability

See [Use Cases](docs/content/use-cases.md) for detailed deployment patterns.

## Key Capabilities

| Capability | Implementation | Research Basis |
|------------|----------------|----------------|
| **Hybrid Search** | RRF fusion of BM25 + dense retrieval | Cormack et al. (2009) |
| **Multilingual FTS** | websearch_to_tsquery + pg_trgm/pg_bigm | PostgreSQL contrib |
| **Semantic Understanding** | 768-dim sentence embeddings | Reimers & Gurevych (2019) |
| **Knowledge Graph** | Automatic linking at >70% similarity | Hogan et al. (2021) |
| **Controlled Vocabulary** | W3C SKOS concepts and relations | Miles & Bechhofer (2009) |
| **Content Enhancement** | Retrieval-augmented generation | Lewis et al. (2020) |
| **Vector Indexing** | HNSW approximate nearest neighbor | Malkov & Yashunin (2020) |
| **Document Type Registry** | 131 pre-configured types across 19 categories | Industry standards |

---

## Technical Foundation

Fortemi implements state-of-the-art information retrieval techniques:

### Hybrid Retrieval with RRF

Combines **lexical retrieval** (BM25 via PostgreSQL tsvector) with **dense retrieval** (sentence embeddings via pgvector) using Reciprocal Rank Fusion:

```
RRFscore(d) = Σ 1/(k + rank_i(d))    where k=20
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

### Document Type Registry

Intelligent content processing through automatic document type detection:
- **131 pre-configured types** across 19 categories (code, prose, config, markup, data, API specs, IaC, etc.)
- **Auto-detection** from filename patterns, extensions, and content magic
- **Optimized chunking** strategies per document type (semantic for prose, syntactic for code, per-section for docs)
- **Extensible** with custom document types

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          Fortemi                                │
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

See [docs/content/architecture.md](docs/content/architecture.md) for detailed system design with research citations.

---

## Quick Start

### Docker Bundle (Recommended)

The fastest way to get started. Includes PostgreSQL, API server, and MCP server in one container:

```bash
# Start Fortemi
docker compose -f docker-compose.bundle.yml up -d

# Verify it's running
curl http://localhost:3000/health

# View logs
docker compose -f docker-compose.bundle.yml logs -f
```

**Ports:** 3000 (API), 3001 (MCP)

To reset with a clean database:
```bash
docker compose -f docker-compose.bundle.yml down -v
docker compose -f docker-compose.bundle.yml up -d
```

### Manual Setup (Development)

For local development without Docker:

**Prerequisites:**
- Rust 1.70+
- PostgreSQL 16+ with pgvector extension
- Ollama (optional, for AI features)

```bash
# Database setup
psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
for f in migrations/*.sql; do psql -d matric -f "$f"; done

# Run the API
export DATABASE_URL="postgres://matric:matric@localhost/matric"
cargo run --release --package matric-api
```

### API Endpoints

Full API documentation available at `/docs` (Swagger UI) and `/openapi.yaml`.

Key endpoints: `/api/v1/notes` (CRUD), `/api/v1/search` (hybrid search), `/api/v1/tags` (SKOS), `/api/v1/graph/:id/explore` (knowledge graph).

See [OpenAPI specification](docs/openapi.yaml) for all API endpoints.

### Search Modes

```bash
# Hybrid search (RRF fusion) - default
curl "http://localhost:3000/api/v1/search?q=retrieval+augmented+generation"

# Lexical only (BM25)
curl "http://localhost:3000/api/v1/search?q=RAG&mode=fts"

# Semantic only (dense retrieval)
curl "http://localhost:3000/api/v1/search?q=machine+learning&mode=semantic"
```

Full-text search supports multiple languages and scripts with query operators (OR, NOT, phrase search). See [Search Guide](docs/content/search-guide.md) for multilingual documentation and advanced query patterns.

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
    "fortemi": {
      "command": "node",
      "args": ["/path/to/fortemi/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "http://localhost:3000"
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
| `OLLAMA_BASE` | `http://localhost:11434` | Ollama API endpoint |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model |
| `OLLAMA_GEN_MODEL` | (none) | Generation model for AI revision |
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

See [docs/content/ci-cd.md](docs/content/ci-cd.md) for details.

---

## Documentation

| Document | Audience | Description |
|----------|----------|-------------|
| [Getting Started](docs/content/getting-started.md) | New users | 5-minute quickstart guide |
| [Use Cases](docs/content/use-cases.md) | All | Deployment patterns by scenario |
| [Best Practices](docs/content/best-practices.md) | Developers | Research-backed usage guidance |
| [Configuration](docs/content/configuration.md) | Operators | Complete configuration reference |
| [Troubleshooting](docs/content/troubleshooting.md) | Operators | Diagnostic and fix guide |
| [Executive Summary](docs/content/executive-summary.md) | Executives | Capabilities, performance targets, use cases |
| [Search Guide](docs/content/search-guide.md) | Developers | Search modes, RRF tuning, query patterns |
| [Knowledge Graph Guide](docs/content/knowledge-graph-guide.md) | Developers | Graph traversal, linking, exploration |
| [Operators Guide](docs/content/operators-guide.md) | Operators | Deployment, monitoring, troubleshooting |
| [Architecture](docs/content/architecture.md) | Technical | System design with research citations |
| [Research Background](docs/content/research-background.md) | Researchers | Methodology and benchmark analysis |
| [Glossary](docs/content/glossary.md) | All | Professional terminology definitions |
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

- [HotM](https://github.com/jmagly/hotm) - Knowledge management frontend

## License

Fortemi is licensed under the [Business Source License 1.1](BSL-LICENSE) (BSL-1.1).

**Free forever for personal use.** Use Fortemi for yourself — your notes, your research, your side projects, your learning. No license key, no time limit, no strings attached. Students, hobbyists, solo researchers, open-source contributors: this is for you.

**Small team or startup?** We want to work with you. If you're a team of 10 or fewer, [reach out](https://github.com/fortemi/fortemi/issues) — we'll find something that works for your budget.

**Larger deployments** (multi-user servers, SaaS, enterprise) need a commercial license. [Let's talk](https://github.com/fortemi/fortemi/issues) — we respond within 2–3 days.

**Fully open source in 2030.** On February 8, 2030 the license automatically converts to AGPL v3. Every version, past and future.

See [docs/content/licensing.md](docs/content/licensing.md) for the full FAQ and plain-English explanation.

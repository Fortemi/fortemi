# Fortémi

*Pronounced: for-TAY-mee*

**Memory that understands.**

Most storage systems are passive—they hold your data and wait for exact queries. Fortémi is different. It's an intelligent memory system that actually comprehends what you store: the meaning behind your notes, the relationships between ideas, and the context that connects them.

Ask it a question, and it doesn't just search for matching words. It finds answers that are *conceptually relevant*, even when you can't remember the right terminology. Store a document, and it automatically discovers how that knowledge connects to everything else you know. Over time, your knowledge base becomes a living network that grows smarter with every piece of information you add.

Built in Rust. Backed by PostgreSQL. Powered by embeddings. No cloud dependency required.

> **Under the hood:** Hybrid retrieval (BM25 + dense vectors), automatic knowledge graph, 131 document types, W3C SKOS vocabularies, multi-memory isolation, OAuth2 auth, 23 MCP agent tools, and multimodal media processing. ~85k lines of Rust.

[![License](https://img.shields.io/badge/license-BSL--1.1-blue.svg)](LICENSE)

---

## Quick Navigation

| Audience | Start Here |
|----------|------------|
| **New Users** | [Getting Started](docs/content/getting-started.md) · [Use Cases](docs/content/use-cases.md) |
| **Developers** | [Quick Start](#quick-start) · [API Docs](docs/content/api.md) · [Search Guide](docs/content/search-guide.md) |
| **AI Agents** | [MCP Server](#mcp-server) · [MCP Deployment](docs/content/mcp-deployment.md) |
| **Operators** | [Configuration](docs/content/configuration.md) · [Operators Guide](docs/content/operators-guide.md) |
| **Security** | [Authentication](docs/content/authentication.md) · [Encryption](docs/content/encryption.md) |

---

## What It Does

- **Understands meaning** — Semantic search finds related content even without keyword matches
- **Discovers connections** — Automatically links related notes via embedding similarity
- **Enhances content** — RAG pipeline enriches notes with context from related knowledge
- **Processes media** — Extracts knowledge from images, audio, video, and 3D models
- **Isolates tenants** — Parallel memory archives with schema-level isolation and federated search
- **Streams events** — Real-time SSE, WebSocket, and webhook notifications

See [Use Cases](docs/content/use-cases.md) for deployment patterns and [Executive Summary](docs/content/executive-summary.md) for a capabilities overview.

## Key Capabilities

| Capability | What It Does |
|------------|-------------|
| **Hybrid Search** | RRF fusion of BM25 + dense retrieval ([details](docs/content/search-guide.md)) |
| **Multilingual FTS** | CJK bigrams, emoji trigrams, 6+ language stemmers ([details](docs/content/multilingual-fts.md)) |
| **Knowledge Graph** | Automatic linking at >70% similarity with graph exploration ([details](docs/content/knowledge-graph-guide.md)) |
| **SKOS Vocabularies** | W3C controlled vocabulary with hierarchical concepts ([details](docs/content/tags.md)) |
| **Multi-Memory** | Schema-isolated archives with federated cross-archive search ([details](docs/content/multi-memory.md)) |
| **Authentication** | OAuth2 + API keys, opt-in enforcement ([details](docs/content/authentication.md)) |
| **Media Processing** | Vision, audio, video, 3D model extraction ([details](docs/content/file-attachments.md)) |
| **Embedding Sets** | MRL dimensionality reduction, auto-embed, two-stage retrieval ([details](docs/content/embedding-sets.md)) |
| **Real-Time Events** | SSE + WebSocket + webhook notifications ([details](docs/content/real-time-events.md)) |
| **Spatial-Temporal** | PostGIS location + time range queries |
| **Encryption** | X25519/AES-256-GCM public-key encryption ([details](docs/content/encryption.md)) |
| **131 Document Types** | Auto-detection with optimized chunking per type ([details](docs/content/document-types-guide.md)) |

---

## Quick Start

### Docker Bundle (Recommended)

Includes PostgreSQL, API server, and MCP server in one container:

```bash
docker compose -f docker-compose.bundle.yml up -d
curl http://localhost:3000/health
```

**Ports:** 3000 (API + Swagger UI at `/docs`), 3001 (MCP)

Clean reset: `docker compose -f docker-compose.bundle.yml down -v && docker compose -f docker-compose.bundle.yml up -d`

### From Source

```bash
# Prerequisites: Rust 1.70+, PostgreSQL 18+ with pgvector, Ollama (optional)
psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
for f in migrations/*.sql; do psql -d matric -f "$f"; done
DATABASE_URL="postgres://matric:matric@localhost/matric" cargo run --release -p matric-api
```

### Try It

```bash
# Hybrid search (default: BM25 + semantic + RRF)
curl "http://localhost:3000/api/v1/search?q=retrieval+augmented+generation"

# Browse all endpoints
open http://localhost:3000/docs
```

See [Getting Started](docs/content/getting-started.md) for the full walkthrough and [API docs](docs/content/api.md) for all endpoints.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          Fortemi                                │
├──────────────────┬──────────────────────────────────────────────┤
│  matric-api      │ HTTP REST API with OpenAPI/Swagger           │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-search   │ Hybrid retrieval (BM25 + dense + RRF)        │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-jobs     │ Async NLP pipeline (embedding, RAG, linking) │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-inference│ LLM abstraction (Ollama, OpenAI backends)    │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-db       │ PostgreSQL + pgvector + PostGIS repositories  │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-crypto   │ X25519/AES-256-GCM public-key encryption     │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-core     │ Core types, traits, and error handling       │
├──────────────────┼──────────────────────────────────────────────┤
│  mcp-server      │ MCP agent integration (Node.js, 23 tools)    │
└──────────────────┴──────────────────────────────────────────────┘
```

See [Architecture](docs/content/architecture.md) for detailed system design with research citations.

---

## MCP Server

23 core agent tools via Model Context Protocol. Docker bundle exposes MCP on port 3001.

**Connect** (`.mcp.json` or Claude Desktop):

```json
{
  "mcpServers": {
    "fortemi": { "url": "https://your-domain.com/mcp" }
  }
}
```

**Local stdio** (development): `node mcp-server/index.js` with `MATRIC_MEMORY_URL=http://localhost:3000`

Set `MCP_TOOL_MODE=full` for all 187 granular tools. See [MCP Guide](docs/content/mcp.md) · [MCP Deployment](docs/content/mcp-deployment.md).

---

## Multi-Memory

Parallel memory archives with schema-level isolation. Select per request via `X-Fortemi-Memory` header. Search across all archives with federated search.

See [Multi-Memory Guide](docs/content/multi-memory.md) · [Agent Strategies](docs/content/multi-memory-agent-guide.md).

---

## Authentication

Opt-in via `REQUIRE_AUTH=true`. Supports OAuth2 (client credentials + authorization code) and API keys. Public endpoints (`/health`, `/docs`, `/oauth/*`) always accessible.

See [Authentication Guide](docs/content/authentication.md).

---

## Configuration

Key variables (see [full reference](docs/content/configuration.md) for all ~27 variables):

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection |
| `PORT` | `3000` | API server port |
| `REQUIRE_AUTH` | `false` | Enable OAuth2/API key auth |
| `ISSUER_URL` | `https://localhost:3000` | OAuth2 issuer URL |
| `OLLAMA_BASE` | `http://localhost:11434` | Ollama API endpoint |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model |
| `OLLAMA_VISION_MODEL` | (none) | Vision model for image description |
| `WHISPER_BASE_URL` | (none) | Audio transcription endpoint |
| `MAX_MEMORIES` | `100` | Maximum memory archives |
| `MCP_TOOL_MODE` | `core` | `core` (23 tools) or `full` (187) |

---

## Development

```bash
cargo test --workspace                        # Run tests
cargo fmt && cargo clippy -- -D warnings      # Format + lint
RUST_LOG=debug cargo run -p matric-api        # Run with logging
```

See [CI/CD](docs/content/ci-cd.md) for pipeline details.

---

## Documentation

### Getting Started
- [Getting Started](docs/content/getting-started.md) — 5-minute quickstart
- [Use Cases](docs/content/use-cases.md) — Deployment patterns
- [Best Practices](docs/content/best-practices.md) — Research-backed guidance
- [Glossary](docs/content/glossary.md) — Terminology

### Features
- [Search Guide](docs/content/search-guide.md) — Modes, RRF tuning, query patterns
- [Multilingual Search](docs/content/multilingual-fts.md) — CJK, emoji, language-specific FTS
- [Knowledge Graph](docs/content/knowledge-graph-guide.md) — Traversal, linking, exploration
- [Embedding Sets](docs/content/embedding-sets.md) — MRL, auto-embed, two-stage retrieval
- [Real-Time Events](docs/content/real-time-events.md) — SSE, WebSocket, webhooks
- [File Attachments](docs/content/file-attachments.md) — Media upload and extraction
- [Encryption](docs/content/encryption.md) — PKE for secure sharing

### Operations
- [Configuration](docs/content/configuration.md) — All environment variables
- [Authentication](docs/content/authentication.md) — OAuth2, API keys, migration
- [Multi-Memory](docs/content/multi-memory.md) — Archives, federated search
- [MCP Server](docs/content/mcp.md) · [MCP Deployment](docs/content/mcp-deployment.md) — Agent integration
- [Operators Guide](docs/content/operators-guide.md) — Monitoring, troubleshooting
- [Backup & Restore](docs/content/backup.md) — Database recovery
- [Hardware Planning](docs/content/hardware-planning.md) — Sizing and resources
- [Troubleshooting](docs/content/troubleshooting.md) — Diagnostics

### Technical
- [Architecture](docs/content/architecture.md) — System design with citations
- [Research Background](docs/content/research-background.md) — Methodology and benchmarks
- [Executive Summary](docs/content/executive-summary.md) — Capabilities overview

---

## References

- Cormack, G. V., Clarke, C. L. A., & Büttcher, S. (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." *SIGIR '09*.
- Lewis, P., et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." *NeurIPS 2020*.
- Reimers, N., & Gurevych, I. (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." *EMNLP 2019*.
- Malkov, Y. A., & Yashunin, D. A. (2020). "Efficient and robust approximate nearest neighbor search using HNSW." *IEEE TPAMI*.
- Hogan, A., et al. (2021). "Knowledge graphs." *ACM Computing Surveys*.
- Kusupati, A., et al. (2022). "Matryoshka representation learning." *NeurIPS 2022*.
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

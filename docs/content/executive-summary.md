# Executive Summary

## What is Fortemi?

Fortemi is an **AI-enhanced knowledge management system** that transforms unstructured notes into a navigable, searchable knowledge base with automatic relationship discovery. It combines traditional full-text search, semantic understanding, and NLP pipelines to deliver intelligent retrieval and content enhancement.

## The Challenge

Organizations and individuals face four fundamental problems with knowledge management:

**Unstructured knowledge grows chaotic.** What starts as a few dozen notes quickly becomes thousands of disconnected documents. Without automatic organization, valuable insights remain buried in a growing pile of information.

**Search fails when you don't know exact keywords.** Traditional search requires knowing precisely what terms the author used. Looking for "machine learning" won't find notes that only mention "neural networks" or "deep learning," even though they're conceptually related.

**Manual organization doesn't scale.** Tagging and linking notes by hand works for hundreds of items but becomes impractical at thousands or tens of thousands. Maintenance overhead grows faster than the knowledge base itself.

**Data isolation is an afterthought.** Most knowledge systems bolt on multi-tenancy at the application level, where bugs or misconfigurations can expose sensitive data across organizational boundaries. True isolation requires database-level guarantees.

Fortemi addresses these challenges through hybrid search (keyword + semantic), automatic relationship discovery, and strict database-level isolation.

## Key Differentiators

| Feature | Traditional Note Storage | Fortemi |
|---------|-------------------------|---------------|
| **Search** | Keyword matching only | Hybrid search combining full-text, semantic similarity, and rank fusion |
| **Organization** | Manual folders/tags | Automatic knowledge graph with semantic linking at 70%+ similarity |
| **Content** | Static storage | AI revision with context from related notes, template expansion |
| **Isolation** | Application-level filtering | Database-level guaranteed segregation via strict tag filtering |
| **Multilingual** | English-only stemming | 8 languages with full stemming, CJK bigram support, emoji search |
| **Retrieval** | One-stage search | Two-stage coarse-to-fine retrieval with 128× compute reduction |

## Core Capabilities

### 1. Intelligent Hybrid Search

Combines three retrieval strategies using Reciprocal Rank Fusion (RRF) to find relevant content even when exact keywords don't match. The system understands that "machine learning" relates to "neural networks" and "AI" without explicit tagging.

**Full-text search** handles complex queries with operators (OR, NOT, phrase search) and multilingual stemming for 8 languages. **Semantic search** uses vector embeddings to find conceptually similar notes, discovering relationships beyond keyword overlap. **Rank fusion** intelligently combines both approaches, elevating results that score well in both systems.

Example: Searching for "authentication security" returns notes mentioning "OAuth implementation," "credential management," and "access control" because they share semantic meaning, even if they never use the word "authentication."

The system includes automatic script detection to route queries through appropriate search strategies (Latin stemming, CJK bigram, emoji trigram) and supports two-stage retrieval with Matryoshka embeddings for 12× storage savings and 128× compute reduction at scale.

### 2. Automatic Knowledge Graph Construction

Notes are automatically connected based on semantic similarity without manual linking. The system continuously analyzes content, creating bidirectional links when similarity exceeds 70%.

**Semantic linking** runs in the background, comparing new notes against the existing knowledge base. When a strong relationship is detected, both notes gain navigation links. The knowledge graph grows organically as content is added.

**Graph exploration** uses recursive SQL (Common Table Expressions) to discover multi-hop relationships. Finding notes related to a starting point traverses the graph to specified depths, revealing indirect connections.

Example: Adding a note about "Kubernetes deployment strategies" automatically links to existing notes on "container orchestration," "microservices architecture," and "infrastructure as code," even if those notes were written months earlier.

### 3. AI-Powered Content Enhancement

New notes are enriched with context from related existing knowledge, creating a cohesive knowledge base rather than isolated documents. This transforms note-taking from storage to synthesis.

**AI revision** takes a draft note and related content, then uses language models to expand, clarify, or restructure the text while preserving the author's intent. The system provides relevant context without requiring manual searches.

**Template expansion** supports variable substitution for common document types (meeting notes, project specs, incident reports). Templates can reference existing notes by ID or tag, pulling live content into new documents.

**Document type registry** with 131 pre-configured types automatically selects chunking strategies based on content. Code files use syntactic splitting (by function/class), while prose uses semantic boundaries (by topic coherence). Auto-detection works from filename patterns and magic content analysis.

Example: Writing a project retrospective note triggers AI revision that incorporates relevant excerpts from meeting notes, bug reports, and architectural decisions, presenting a unified narrative instead of scattered references.

### 4. Multi-Tenant Security Architecture

Strict isolation guarantees data segregation at the database level, enabling secure multi-tenant deployments without relying on application-layer filtering.

**Tag-based filtering** is enforced at every database query. All operations (search, retrieval, linking) require an explicit tag context. Queries cannot accidentally cross tenant boundaries because the isolation is structural, not procedural.

**Schema isolation** provides additional separation for enterprise deployments with hundreds or thousands of tenants, partitioning data at the PostgreSQL schema level. Each memory operates as an independent PostgreSQL schema with full search, embedding, and linking capabilities.

**Public Key Encryption (PKE)** enables secure note sharing across tenant boundaries. Encrypted exports can be shared externally, decryptable only by recipients with the corresponding private key.

Example: A SaaS deployment hosts knowledge bases for 50 companies. Each company's data is tagged with a unique tenant ID. Even if application code has bugs, a query for Company A's notes cannot return Company B's data because the database enforces tag matching.

## Technical Foundation

Built on peer-reviewed research and production-grade infrastructure:

- **Hybrid Retrieval** using Reciprocal Rank Fusion (SIGIR 2009) combines full-text and semantic search scores
- **Sentence Embeddings** via contrastive learning (EMNLP 2019) for semantic similarity
- **Vector Indexing** with HNSW graphs (IEEE TPAMI 2020) for efficient nearest-neighbor search
- **Controlled Vocabulary** following W3C SKOS standard for semantic taxonomy
- **PostgreSQL 18 + pgvector** for unified storage and vector operations
- **Matryoshka embeddings** for multi-resolution retrieval (12× storage savings)

The system is implemented in Rust (API server) and Node.js (MCP server), with comprehensive OpenAPI documentation covering all 107 REST endpoints.

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Hybrid search (10K docs) | <200ms p95 | Full-text + semantic + RRF |
| Hybrid search (100K docs) | <500ms p95 | With HNSW index and two-stage retrieval |
| API response (CRUD) | <100ms p95 | Create, read, update, delete operations |
| Embedding generation | 50-100 docs/sec | Local Ollama on mid-range GPU |
| Automatic linking | Background | Async job queue, no user-facing latency |

## Use Cases

| Scale | Description | Configuration | Details |
|-------|-------------|---------------|---------|
| **Personal Knowledge Base** (1-10K notes) | Individual note-taking with semantic search and automatic linking. Ideal for researchers, writers, and knowledge workers building a second brain. | Local Ollama with nomic-embed-text (137M params), PostgreSQL on same machine. No GPU required for entry tier, but recommended for faster embedding generation. | [Use Cases Guide](./use-cases.md#personal-knowledge-base) |
| **Team Documentation Hub** (10K-100K notes) | Collaborative knowledge base for engineering teams, product documentation, or project wikis. OAuth authentication, SKOS taxonomy for controlled vocabularies, strict tag filtering for project isolation. | Mid-range GPU (RTX 4060 or better) for team-shared Ollama instance. API keys for service accounts, OAuth for human users. Multi-tenant with tag-based isolation. | [Use Cases Guide](./use-cases.md#team-documentation) |
| **AI Research Assistant / RAG** (50K-500K notes) | Retrieval-Augmented Generation for AI agents and chatbots. Hybrid search with adaptive RRF provides high-quality context for LLM prompts. Embedding sets enable context isolation across multiple projects. | High-end GPU (RTX 4090) or cloud inference (OpenAI-compatible). Filter embedding sets for shared model instances, full embedding sets for isolated contexts. MCP integration for Claude/AI agent connectivity. | [Use Cases Guide](./use-cases.md#ai-assistant) |
| **Enterprise Document Management** (500K+) | Large-scale knowledge management with compliance requirements. Multi-tenancy with scheme isolation (roadmap), PKE encryption for cross-boundary sharing, audit logs for regulatory adherence. | Multi-GPU or cloud-hosted embedding services. PostgreSQL with read replicas for search scaling. Separate inference tier for generation vs. embedding workloads. Consider Matryoshka embeddings for storage efficiency. | [Use Cases Guide](./use-cases.md#enterprise-scale) |
| **Hybrid Cloud/Edge** | Privacy-sensitive embeddings on-premises, generation in the cloud. Local Ollama for semantic search (data never leaves network), cloud LLMs for AI revision (only selected context sent). | Split deployment: Local PostgreSQL + Ollama for embeddings, cloud API for generation. Configure inference endpoints per operation type. Use PKE for secure exports to cloud processing. | [Use Cases Guide](./use-cases.md#hybrid-deployment) |

## Total Cost of Ownership

Infrastructure requirements scale with note count and usage patterns. Four representative tiers:

**Tier 1 (Entry - 1K notes)**: Consumer hardware with no GPU. 8GB RAM, 4 CPU cores, 10GB storage. Ollama CPU inference at 5-10 docs/sec. Suitable for personal use with infrequent embedding updates. Estimated cost: Existing hardware or $10-20/month cloud VPS.

**Tier 3 (Standard - 10-100K notes)**: Mid-range GPU (RTX 4060 or similar). 32GB RAM, 8 CPU cores, 100GB NVMe, RTX 4060 (8GB VRAM). Ollama GPU inference at 50-100 docs/sec. Handles team documentation with daily embedding jobs. Estimated cost: $1500 hardware or $100-150/month cloud GPU instance.

**Tier 5 (Scale - 500K+ notes)**: Multi-GPU or cloud inference. 128GB RAM, 32 CPU cores, 1TB NVMe, 2× RTX 4090 (24GB VRAM each) or cloud service. Parallel embedding generation at 200-500 docs/sec. Supports enterprise workloads with continuous updates. Estimated cost: $5000+ hardware or $500-1000/month cloud infrastructure.

**Hybrid**: On-premises Tier 3 for embeddings, cloud inference for generation. Balance cost (local GPU amortized over time) with capability (cloud models for advanced generation). Privacy-sensitive data stays local, only curated context sent to cloud.

See [Hardware Planning Guide](./hardware-planning.md) for detailed capacity planning, GPU comparisons, and cost optimization strategies.

## Deployment Options

### Docker Bundle (Recommended)

All-in-one container with PostgreSQL, API server, and MCP server. Best for quick starts and production deployments.

```bash
# Clone and start
git clone https://github.com/fortemi/fortemi
cd Fortémi
docker compose -f docker-compose.bundle.yml up -d

# Access API at http://localhost:3000
# MCP server at http://localhost:3001
```

The bundle automatically initializes the database, runs migrations, and starts all services. Configure OAuth and inference endpoints via `.env` file.

See [Operator's Guide](./operators-guide.md) for production deployment, monitoring, and backup procedures.

### Manual Installation

Build from source for development or custom deployments. Requires Rust toolchain, PostgreSQL 18+ with pgvector, and optional Ollama for local inference.

```bash
# Install dependencies
cargo build --release --workspace

# Run migrations
export DATABASE_URL=postgres://matric:matric@localhost/matric
sqlx migrate run

# Start API server
./target/release/matric-api

# Start MCP server (optional)
cd mcp-server && npm install && npm start
```

See [Getting Started Guide](./getting-started.md) for detailed installation steps, dependency setup, and configuration options.

### MCP Integration

The Model Context Protocol (MCP) server enables AI agents (Claude, ChatGPT with plugins, custom assistants) to interact with Fortemi as a knowledge base. Agents can search notes, create new content, and traverse the knowledge graph through standardized MCP tools.

Configure Claude Code or other MCP clients to connect:

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "http://localhost:3001"
    }
  }
}
```

The MCP server authenticates via OAuth client credentials, providing secure API access without exposing personal tokens. It supports all core operations: search, CRUD, graph traversal, and AI revision.

See [MCP Documentation](./mcp.md) for available tools, authentication setup, and integration examples.

## Production Readiness

**CI/CD Pipeline**: Gitea Actions workflows for build, test, and deployment. Unit tests with coverage (cargo-llvm-cov), integration tests with PostgreSQL containers, automated Docker image builds.

**Monitoring**: Health endpoints for API server, database connection pool metrics, job queue status. Structured logging (JSON) for centralized log aggregation. Prometheus-compatible metrics (roadmap).

**Backup and Recovery**: PostgreSQL dump/restore for data backups, migration rollback support, documented disaster recovery procedures in [Operator's Guide](./operators-guide.md).

**Security**: OAuth 2.0 authentication, API key management, rate limiting (roadmap), PKE encryption for note sharing, strict tag-based isolation for multi-tenancy.

**Documentation**: OpenAPI spec covering all API endpoints, comprehensive guides for operators, developers, and users, inline code documentation (rustdoc), MCP tool descriptions.

## Getting Started

1. **Quick Evaluation**: Run Docker bundle locally, add sample notes, try hybrid search
2. **Development Setup**: Clone repo, install Rust toolchain, run tests, read architecture docs
3. **Production Deployment**: Configure OAuth, choose inference backend (local/cloud), set up monitoring
4. **Integration**: Connect MCP client, configure embedding models, customize document types

See the [Getting Started Guide](./getting-started.md) for step-by-step instructions.

## Learn More

- **[Getting Started](./getting-started.md)** - Installation, configuration, first steps
- **[Architecture](./architecture.md)** - System design, component interactions, data flows
- **[Use Cases](./use-cases.md)** - Detailed scenarios with configuration examples
- **[Hardware Planning](./hardware-planning.md)** - Capacity planning, GPU selection, cost analysis
- **[Best Practices](./best-practices.md)** - Search strategies, tagging conventions, performance tuning
- **[Configuration Reference](./configuration.md)** - Environment variables, feature flags, tuning parameters
- **[Operator's Guide](./operators-guide.md)** - Deployment, monitoring, backup, troubleshooting
- **[Research Background](./research-background.md)** - Academic foundations, algorithm details

---

*Fortémi is licensed under the Business Source License 1.1. See [licensing details](./licensing.md). Contributions welcome at [github.com/fortemi/fortemi](https://github.com/fortemi/fortemi).*

# Executive Summary

## What is matric-memory?

matric-memory is an **AI-enhanced knowledge management system** that transforms unstructured notes into a navigable, searchable knowledge base with automatic relationship discovery.

## Key Differentiators

| Feature | Traditional Note Storage | matric-memory |
|---------|-------------------------|---------------|
| **Search** | Keyword matching only | Hybrid search finds meaning, not just words |
| **Organization** | Manual folders/tags | Automatic knowledge graph construction |
| **Content** | Static storage | AI enhancement with context enrichment |
| **Isolation** | Application-level | Database-level guaranteed segregation |

## Core Capabilities

### 1. Intelligent Search
Find relevant content even when exact keywords don't match. The system understands that "machine learning" relates to "neural networks" and "AI" without explicit tagging.

### 2. Automatic Linking
Notes are automatically connected based on semantic similarity. No manual linking required—the system discovers relationships.

### 3. Content Enhancement
New notes are enriched with context from related existing knowledge, creating a cohesive knowledge base rather than isolated documents.

### 4. Multi-Tenant Ready
Strict isolation guarantees data segregation at the database level, enabling secure multi-tenant deployments.

## Technical Foundation

Built on peer-reviewed research:

- **Hybrid Retrieval** using Reciprocal Rank Fusion (SIGIR 2009)
- **Sentence Embeddings** via contrastive learning (EMNLP 2019)
- **Vector Indexing** with HNSW graphs (IEEE TPAMI 2020)
- **Controlled Vocabulary** following W3C SKOS standard

## Performance Targets

| Metric | Target |
|--------|--------|
| Hybrid search (10k docs) | <200ms p95 |
| Hybrid search (100k docs) | <500ms p95 |
| API response (CRUD) | <100ms |

## Deployment

- **API Server**: Rust/Axum with OpenAPI documentation
- **Database**: PostgreSQL with pgvector extension
- **Inference**: Local (Ollama) or cloud (OpenAI-compatible)
- **Integration**: MCP server for AI agent integration

## Use Cases

1. **Personal Knowledge Management** - Connect ideas across notes automatically
2. **Team Documentation** - Searchable, linked knowledge base
3. **AI Agent Memory** - Persistent context for AI assistants
4. **Research Databases** - Semantic search over academic content

---

*For technical details, see [Architecture](./architecture.md) and [Research Background](./research-background.md).*

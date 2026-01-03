# matric-memory

AI-enhanced note storage and retrieval system with semantic search and NLP pipelines.

## Overview

matric-memory is a Rust library providing:

- **Vector-enhanced PostgreSQL storage** - Store notes with embeddings using pgvector
- **Hybrid search** - Full-text search (FTS) + semantic vector search with RRF fusion
- **Background job processing** - Async NLP pipelines for embedding generation, AI revision, and linking
- **HTTP API** - RESTful API with OpenAPI documentation

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        matric-memory                            │
├─────────────────────────────────────────────────────────────────┤
│  matric-api      │ HTTP REST API with OpenAPI/Swagger           │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-search   │ Hybrid search (FTS + semantic + RRF)         │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-jobs     │ Background job processing                    │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-inference│ LLM inference abstraction (Ollama)           │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-db       │ PostgreSQL + pgvector database layer         │
├──────────────────┼──────────────────────────────────────────────┤
│  matric-core     │ Core types, traits, and error handling       │
└─────────────────────────────────────────────────────────────────┘
```

## Crate Structure

| Crate | Description |
|-------|-------------|
| `matric-core` | Core traits, types, and error handling |
| `matric-db` | PostgreSQL + pgvector database layer |
| `matric-search` | Hybrid search engine with RRF fusion |
| `matric-inference` | LLM inference abstraction |
| `matric-jobs` | Background job queue and processing |
| `matric-api` | HTTP REST API server |

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 14+ with pgvector extension
- Ollama (optional, for local inference)

### Database Setup

```bash
# Install pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;

# Run migrations
sqlx migrate run
```

### Running the API Server

```bash
# Set environment variables
export DATABASE_URL="postgres://user:pass@localhost/matric"
export OLLAMA_URL="http://localhost:11434"  # optional

# Build and run
cargo build --release
./target/release/matric-api
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/v1/notes` | GET, POST | List/create notes |
| `/api/v1/notes/:id` | GET, PATCH, DELETE | Get/update/delete note |
| `/api/v1/search` | GET | Hybrid search |
| `/api/v1/tags` | GET | List all tags |
| `/api/v1/jobs` | GET, POST | List/create jobs |
| `/docs` | GET | Swagger UI |
| `/openapi.yaml` | GET | OpenAPI spec |

### Search Modes

```bash
# Hybrid search (default) - combines FTS + semantic
curl "https://memory.integrolabs.net/api/v1/search?q=API+design"

# FTS only
curl "https://memory.integrolabs.net/api/v1/search?q=API+design&mode=fts"

# Semantic only
curl "https://memory.integrolabs.net/api/v1/search?q=API+design&mode=semantic"
```

## MCP Server

An MCP (Model Context Protocol) server is included for AI agent integration:

```bash
cd mcp-server
npm install
```

Add to Claude Desktop config (`~/.config/claude/claude_desktop_config.json`):

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

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection URL |
| `HOST` | `0.0.0.0` | API server host |
| `PORT` | `3000` | API server port |
| `OLLAMA_URL` | `http://localhost:11434` | Ollama inference endpoint |
| `RUST_LOG` | `matric_api=debug` | Log level |

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -p matric-api

# Format code
cargo fmt

# Run linter
cargo clippy
```

## API Documentation

- **OpenAPI Spec**: `/openapi.yaml`
- **Swagger UI**: `/docs`
- **Production**: https://memory.integrolabs.net/docs

## Related Projects

- [HotM](https://git.integrolabs.net/roctinam/hotm) - Note-taking frontend that consumes this API

## License

MIT OR Apache-2.0

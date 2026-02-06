# Getting Started

> **Note:** Internal code and configurations use the name `Fortémi` — this is Fortémi.

This quickstart guide takes you from zero to your first search result in 5 minutes. You'll deploy Fortemi, create notes, and explore hybrid search combining traditional keyword matching with AI-powered semantic understanding.

## Prerequisites

- Docker and Docker Compose (version 20.10 or later)
- Optional: Ollama for AI features (embeddings, semantic search, auto-linking)
- Optional: curl or any HTTP client for testing

## Step 1: Start Fortemi

The Docker bundle runs PostgreSQL, the API server, and the MCP server in a single container. Start it with one command:

```bash
docker compose -f docker-compose.bundle.yml up -d
```

This automatically:
- Initializes PostgreSQL 16 with pgvector and PostGIS extensions
- Runs all database migrations
- Starts the API on port 3000
- Starts the MCP server on port 3001

Verify the deployment:

```bash
curl http://localhost:3000/health
```

You should see:

```json
{
  "status": "healthy",
  "version": "2026.2.0",
  "database": "connected"
}
```

The interactive API documentation is available at http://localhost:3000/docs. This Swagger UI lets you explore all API endpoints and test them directly in your browser.

## Step 2: Create Your First Notes

Let's create three notes covering different topics. These will demonstrate Fortemi's search and auto-linking capabilities.

### Note 1: Rust Ownership

```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Rust Ownership Model",
    "content": "Rust uses a unique ownership system to manage memory without garbage collection. Each value has a single owner, and when the owner goes out of scope, the value is dropped. This prevents memory leaks and data races at compile time.",
    "tags": ["programming/rust", "memory-management"]
  }'
```

Save the `id` from the response - you'll need it later.

### Note 2: C++ Memory Management

```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "title": "C++ Manual Memory Management",
    "content": "C++ requires explicit memory management using new and delete operators. Smart pointers like unique_ptr and shared_ptr provide RAII-based automatic cleanup. Memory leaks occur when allocated memory is not freed, while dangling pointers reference freed memory.",
    "tags": ["programming/cpp", "memory-management"]
  }'
```

### Note 3: Knowledge Management Systems

```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Building a Personal Knowledge Graph",
    "content": "Effective knowledge management systems connect ideas through semantic relationships. Tools like Obsidian, Roam Research, and Fortemi enable networked thought by automatically linking related concepts. The key is balancing structure (tags, folders) with organic discovery (search, graph exploration).",
    "tags": ["research", "knowledge-management"]
  }'
```

## Step 3: Search Your Notes

Full-text search works immediately without any AI setup. Let's search for "memory management":

```bash
curl "http://localhost:3000/api/v1/search?q=memory+management"
```

You'll see results ranked by BM25 relevance:

```json
{
  "results": [
    {
      "note_id": "...",
      "title": "Rust Ownership Model",
      "score": 0.876,
      "snippet": "...ownership system to manage memory without garbage collection..."
    },
    {
      "note_id": "...",
      "title": "C++ Manual Memory Management",
      "score": 0.823,
      "snippet": "...explicit memory management using new and delete..."
    }
  ],
  "total": 2,
  "mode": "hybrid"
}
```

### Search Modes

Fortemi supports three search strategies:

**Hybrid (default)**: Combines full-text search and semantic similarity using Reciprocal Rank Fusion.

```bash
curl "http://localhost:3000/api/v1/search?q=memory+management&mode=hybrid"
```

**Full-text search (FTS)**: Pure keyword matching with BM25 ranking. Best for exact phrases or code snippets.

```bash
curl "http://localhost:3000/api/v1/search?q=memory+management&mode=fts"
```

**Semantic search**: Pure vector similarity. Best for conceptual queries. Requires embeddings (see Step 4).

```bash
curl "http://localhost:3000/api/v1/search?q=memory+management&mode=semantic"
```

Semantic search returns an error until you generate embeddings in Step 4.

### Query Syntax

Fortemi supports advanced query operators:

```bash
# Match all words (AND)
curl "http://localhost:3000/api/v1/search?q=rust+ownership"

# Match either word (OR)
curl "http://localhost:3000/api/v1/search?q=rust+OR+cpp"

# Exclude words (NOT)
curl "http://localhost:3000/api/v1/search?q=memory+-garbage"

# Exact phrase match
curl "http://localhost:3000/api/v1/search?q=%22garbage+collection%22"
```

See the [Search Operators Guide](./search-operators.md) for advanced filtering and ranking techniques.

## Step 4: Add Intelligence with Ollama

AI features (semantic search, auto-linking, revision suggestions) require embeddings generated by a local or remote LLM. We'll use Ollama for local inference.

### Install Ollama

```bash
# Linux or macOS
curl -fsSL https://ollama.ai/install.sh | sh

# Verify installation
ollama --version
```

For Windows, download from https://ollama.ai/download.

### Pull the Embedding Model

Fortemi defaults to `nomic-embed-text`, a high-quality 768-dimension model optimized for retrieval:

```bash
ollama pull nomic-embed-text
```

This downloads approximately 274MB and runs inference on your CPU or GPU.

### Optional: Pull a Generation Model

For AI-powered note revision and summarization:

```bash
ollama pull llama3.2:3b
```

See the [Embedding Model Selection Guide](./embedding-model-selection.md) for alternative models and trade-offs.

### Generate Embeddings

Fortemi automatically generates embeddings when notes are created if Ollama is running. For existing notes, trigger embedding jobs manually:

```bash
# Get all note IDs
curl http://localhost:3000/api/v1/notes | jq -r '.notes[].id'

# Generate embeddings for a specific note
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "YOUR_NOTE_ID",
    "job_type": "embedding"
  }'
```

Or generate embeddings for all notes in a tag:

```bash
# Embed all notes tagged "programming"
curl -X POST http://localhost:3000/api/v1/embedding-sets/default/embed \
  -H "Content-Type: application/json" \
  -d '{
    "tag_filter": "programming"
  }'
```

Check job status:

```bash
curl http://localhost:3000/api/v1/jobs
```

### Test Semantic Search

Once embeddings exist, semantic search finds conceptually related notes even with different terminology:

```bash
# Finds both Rust and C++ notes despite different wording
curl "http://localhost:3000/api/v1/search?q=preventing+memory+leaks&mode=semantic"
```

Hybrid mode (default) now combines both keyword matching and semantic similarity for optimal results.

### Automatic Semantic Linking

Fortemi automatically creates bidirectional links between notes with >70% semantic similarity. Check links for your Rust ownership note:

```bash
# Replace {id} with the note ID from Step 2
curl http://localhost:3000/api/v1/graph/{id}/explore
```

You should see a link to the C++ memory management note because both discuss memory safety, even though they use different terminology (ownership vs. pointers, dropped vs. freed).

The knowledge graph response shows:

```json
{
  "node": {
    "id": "...",
    "title": "Rust Ownership Model",
    "tags": ["programming/rust", "memory-management"]
  },
  "links": [
    {
      "target_id": "...",
      "target_title": "C++ Manual Memory Management",
      "similarity": 0.78,
      "link_type": "semantic"
    }
  ]
}
```

## Step 5: Organize with Tags and Collections

### Hierarchical Tags

Tags use `/` for hierarchy, enabling filtered searches:

```bash
# Find all programming notes
curl "http://localhost:3000/api/v1/search?q=*&tags=programming"

# Find only Rust notes
curl "http://localhost:3000/api/v1/search?q=*&tags=programming/rust"
```

Fortemi enforces strict tag filtering. When you specify `tags=programming/rust`, you'll never see results tagged with `programming/cpp` or `research`, ensuring data isolation.

See the [Tags Guide](./tags.md) for advanced taxonomy management.

### Collections

Group related notes into collections (folders):

```bash
# Create a collection
curl -X POST http://localhost:3000/api/v1/collections \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Programming Languages",
    "description": "Comparative notes on language memory models"
  }'

# Add notes to the collection
curl -X POST http://localhost:3000/api/v1/collections/{collection_id}/notes \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "YOUR_RUST_NOTE_ID"
  }'
```

Collections support nested hierarchies and can have their own tags and metadata.

### SKOS Semantic Taxonomy

For advanced knowledge organization, Fortemi implements W3C SKOS (Simple Knowledge Organization System):

```bash
# Create a concept for "memory safety"
curl -X POST http://localhost:3000/api/v1/skos/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "pref_label": "Memory Safety",
    "alt_labels": ["memory security", "safe memory access"],
    "definition": "Techniques preventing unauthorized memory access and corruption"
  }'
```

SKOS concepts support broader/narrower relationships and multilingual labels. See the [Tags Guide](./tags.md) for SKOS integration details.

## Step 6: Explore Version History

Fortemi maintains complete version history for all notes. Every update creates a new immutable version.

### Update a Note

```bash
# Update the Rust note to add borrowing details
curl -X PATCH http://localhost:3000/api/v1/notes/{id} \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Rust uses a unique ownership system to manage memory without garbage collection. Each value has a single owner, and when the owner goes out of scope, the value is dropped. Borrowing allows temporary references without transferring ownership, enforced by the borrow checker at compile time. This prevents memory leaks and data races."
  }'
```

### View Version History

```bash
curl http://localhost:3000/api/v1/notes/{id}/versions
```

Response shows all versions with timestamps and content diffs:

```json
{
  "versions": [
    {
      "version_number": 2,
      "created_at": "2026-02-03T14:32:18Z",
      "content": "Rust uses a unique ownership system...[updated]",
      "changed_fields": ["content"]
    },
    {
      "version_number": 1,
      "created_at": "2026-02-03T14:15:42Z",
      "content": "Rust uses a unique ownership system...[original]",
      "changed_fields": []
    }
  ]
}
```

### Restore a Previous Version

```bash
# Restore version 1
curl -X POST http://localhost:3000/api/v1/notes/{id}/versions/1/restore
```

This creates version 3 with version 1's content, preserving the complete audit trail.

## What's Next?

| Goal | Guide |
|------|-------|
| Understand deployment architectures | [Architecture](./architecture.md) |
| Configure search and AI features | [Search Guide](./search-guide.md), [Inference Backends](./inference-backends.md) |
| Deploy to production | [Deployment and Migrations](./deployment-and-migrations.md) |
| Connect AI assistants (Claude Code) | [MCP Server](./mcp.md) |
| Set up OAuth authentication | [Authentication](./authentication.md) |
| Troubleshoot MCP issues | [MCP Troubleshooting](./mcp-troubleshooting.md) |
| Optimize performance | [Operations Guide](./operations.md) |
| Configure multilingual search | [Multilingual FTS](./multilingual-fts.md) |
| Plan hardware requirements | [Hardware Planning](./hardware-planning.md) |
| Monitor in real-time | [Real-Time Events](./real-time-events.md) |
| Explore advanced features | [File Attachments](./file-attachments.md), [PKE Encryption](./pke-encryption.md) |

### Explore the API

The full API reference is available at http://localhost:3000/docs. Key endpoints to explore:

- `POST /api/v1/notes` - Create notes with optional file attachments
- `GET /api/v1/search` - Hybrid search with tag filtering
- `GET /api/v1/graph/{id}/explore` - Traverse the knowledge graph
- `POST /api/v1/ai/revise` - AI-powered note revision
- `POST /api/v1/export/markdown` - Export to Markdown with YAML frontmatter
- `GET /api/v1/document-types` - View 131 pre-configured document types

See the [API Documentation](./api.md) for detailed endpoint specifications and examples.

### Example Workflows

**Research Workflow**:
1. Import papers as file attachments with auto-chunking
2. Generate embeddings for semantic search
3. Tag with hierarchical taxonomy (e.g., `research/ml/nlp`)
4. Explore auto-generated links between related papers
5. Create synthesis notes linking to chunks

See [Workflows Guide](./workflows.md) for detailed scenarios.

**Software Documentation**:
1. Create notes from code comments or README files
2. Use document types for automatic chunking (e.g., `code/rust`, `code/python`)
3. Tag by project and component
4. Search with operators: `error handling -logging`
5. Export knowledge graph for architecture diagrams

See [Document Types Guide](./document-types-guide.md) for optimization strategies.

## Cleaning Up

### Stop Fortemi (Keep Data)

```bash
docker compose -f docker-compose.bundle.yml down
```

This stops the container but preserves all data in Docker volumes. Start again with `up -d`.

### Wipe All Data

```bash
# WARNING: Deletes all notes, tags, embeddings, and configurations
docker compose -f docker-compose.bundle.yml down -v
```

The `-v` flag removes volumes, giving you a clean slate. Useful for testing or starting fresh.

### View Logs

```bash
# Follow logs in real-time
docker compose -f docker-compose.bundle.yml logs -f

# View last 100 lines
docker compose -f docker-compose.bundle.yml logs --tail=100
```

## Troubleshooting

### Health Check Fails

If `curl http://localhost:3000/health` returns an error:

1. Check container status: `docker compose -f docker-compose.bundle.yml ps`
2. View logs: `docker compose -f docker-compose.bundle.yml logs matric`
3. Verify ports are not in use: `lsof -i :3000` (Linux/macOS)

### Semantic Search Returns "No embeddings found"

You need to generate embeddings first (Step 4). Verify Ollama is running:

```bash
# Check Ollama status
curl http://localhost:11434/api/tags

# Verify model is available
ollama list | grep nomic-embed-text
```

If Ollama is running inside Docker, update `OLLAMA_BASE` in `docker-compose.bundle.yml`:

```yaml
environment:
  - OLLAMA_BASE=http://host.docker.internal:11434  # Default
  # Or for Linux without host.docker.internal:
  # - OLLAMA_BASE=http://172.17.0.1:11434
```

### Automatic Linking Not Working

Auto-linking requires:
1. Embeddings generated for both notes
2. Semantic similarity >70% (configurable)
3. Notes in the same embedding set (default: "default")

Check embedding status:

```bash
# View embedding sets
curl http://localhost:3000/api/v1/embedding-sets

# Check if note has embeddings
curl http://localhost:3000/api/v1/notes/{id}
```

See the [Embedding Sets Guide](./embedding-sets.md) for advanced configuration.

### Docker Container Exits Immediately

Common causes:

1. **Port conflict**: Another service using 3000 or 3001
2. **Volume permission issues**: Delete volumes and recreate
   ```bash
   docker compose -f docker-compose.bundle.yml down -v
   docker compose -f docker-compose.bundle.yml up -d
   ```
3. **Invalid environment variables**: Check `.env` file syntax

View detailed error logs:

```bash
docker compose -f docker-compose.bundle.yml logs matric
```

For more troubleshooting guidance, see [MCP Troubleshooting](./mcp-troubleshooting.md) and [Operations Guide](./operations.md).

## Next Steps

Now that you have Fortemi running and understand the basics:

1. **Customize for your use case**: Adjust embedding models, search parameters, and document types in the configuration
2. **Integrate with your workflow**: Connect Fortemi to your note-taking tools, IDEs, or AI assistants via the API
3. **Scale up**: Review [Hardware Planning](./hardware-planning.md) for production deployments
4. **Secure your instance**: Configure OAuth authentication following the [Authentication Guide](./authentication.md)

Welcome to AI-enhanced knowledge management with Fortemi.

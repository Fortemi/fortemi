# Troubleshooting

This guide provides symptom-based troubleshooting for Fortemi. Run the Quick Diagnostics first to identify the problem area, then jump to the relevant section.

## Quick Diagnostics

Run these commands to identify the problem area:

```bash
# 1. Check container status
docker compose -f docker-compose.bundle.yml ps

# 2. Check API health
curl http://localhost:3000/health

# 3. Check MCP health
curl http://localhost:3001/health

# 4. Check logs for errors
docker compose -f docker-compose.bundle.yml logs --tail=50

# 5. Check database connectivity
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT 1"

# 6. Check Ollama connectivity (from host)
curl http://localhost:11434/api/tags

# 7. Check environment variables
docker exec Fortémi-matric-1 printenv | grep -E 'DATABASE|OLLAMA|MCP|ISSUER'
```

## Installation and Startup

### Container Won't Start

**Symptom:** `docker compose up` fails or container exits immediately.

**Diagnosis:**
```bash
# Check container status
docker compose -f docker-compose.bundle.yml ps

# Check for port conflicts
netstat -tulpn | grep -E ':(3000|3001|5432)'

# Check Docker daemon
docker info

# Check for image
docker images | grep Fortémi
```

**Fix:**

**Port conflict:**
```bash
# Identify process using port
lsof -i :3000

# Kill process or change Fortémi port in docker-compose.bundle.yml
# Edit ports section: "3100:3000" instead of "3000:3000"
```

**Missing image:**
```bash
# Build the image
docker compose -f docker-compose.bundle.yml build

# Or pull if available
docker compose -f docker-compose.bundle.yml pull
```

**Docker not running:**
```bash
# Start Docker daemon (systemd)
sudo systemctl start docker

# Or on macOS/Windows, start Docker Desktop
```

### Database Connection Failed

**Symptom:** API logs show "connection refused" or "could not connect to database".

**Diagnosis:**
```bash
# Check DATABASE_URL environment variable
docker exec Fortémi-matric-1 printenv DATABASE_URL

# Check if PostgreSQL is running
docker exec Fortémi-matric-1 pg_isready -U matric -d matric

# Test connection manually
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT version()"

# Check logs for PostgreSQL errors
docker compose -f docker-compose.bundle.yml logs | grep -i postgres
```

**Fix:**

**Wrong DATABASE_URL:**
```bash
# Correct format should be:
# postgres://matric:matric@localhost/matric

# Stop container
docker compose -f docker-compose.bundle.yml down

# Verify DATABASE_URL in docker-compose.bundle.yml or .env
# Restart
docker compose -f docker-compose.bundle.yml up -d
```

**PostgreSQL not ready (transient on first start):**
```bash
# Wait 10-15 seconds for PostgreSQL initialization
# Then restart API container
docker compose -f docker-compose.bundle.yml restart matric
```

**pgvector extension missing:**
```bash
# Connect to database
docker exec -it Fortémi-matric-1 psql -U matric -d matric

# Check for extension
SELECT * FROM pg_extension WHERE extname = 'vector';

# If missing, install (should be automatic on first run)
CREATE EXTENSION IF NOT EXISTS vector;
```

### Migration Errors

**Symptom:** Container logs show migration failures during startup.

**Diagnosis:**
```bash
# Check migration logs
docker compose -f docker-compose.bundle.yml logs | grep -i migration

# Check current migration version
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1"

# List all migrations
ls migrations/
```

**Fix:**

**Transient errors (network, timing):**
```bash
# Simply restart the container
docker compose -f docker-compose.bundle.yml restart matric

# Migrations run automatically on startup
```

**CREATE INDEX CONCURRENTLY in transaction:**
```bash
# This is a known PostgreSQL limitation
# The migration must be rewritten to avoid using CONCURRENTLY in tests
# Or tests must use manual pool setup instead of #[sqlx::test]
# No operator fix needed - report to developers
```

**Enum type conflicts:**
```bash
# Check for error like "type already exists"
docker compose -f docker-compose.bundle.yml logs | grep "already exists"

# This usually means partial migration
# Connect to database and check
docker exec -it Fortémi-matric-1 psql -U matric -d matric

# Check custom types
\dT+

# If migration is stuck, may need to roll back manually
# Contact developers with specific error message
```

**Old Docker image:**
```bash
# Rebuild from latest code
docker compose -f docker-compose.bundle.yml build --no-cache
docker compose -f docker-compose.bundle.yml up -d
```

### Port Conflicts

**Symptom:** "port is already allocated" error during container start.

**Diagnosis:**
```bash
# Check what's using the ports
netstat -tulpn | grep -E ':(3000|3001|5432)'

# Or on macOS
lsof -i :3000
lsof -i :3001
lsof -i :5432

# Check if old Fortémi container still running
docker ps -a | grep matric
```

**Fix:**

**Port 3000 (API):**
```bash
# Option 1: Kill the conflicting process
sudo kill -9 <PID>

# Option 2: Change Fortémi port in docker-compose.bundle.yml
# Edit the ports section:
ports:
  - "3100:3000"  # API now on 3100
```

**Port 3001 (MCP):**
```bash
# Change MCP port in docker-compose.bundle.yml
ports:
  - "3101:3001"  # MCP now on 3101

# Update nginx proxy config if using reverse proxy
```

**Port 5432 (PostgreSQL):**
```bash
# Check if another PostgreSQL instance is running
ps aux | grep postgres

# The bundle uses internal PostgreSQL, so port 5432 should not be exposed
# If docker-compose.bundle.yml exposes 5432, remove that line
```

## Search Issues

### No Search Results

**Symptom:** All searches return empty results or "no results found".

**Diagnosis:**
```bash
# 1. Check if notes exist
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM notes"

# 2. Check if FTS is populated
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM notes WHERE fts IS NOT NULL"

# 3. Check if embeddings exist (for semantic search)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM note_embeddings"

# 4. Test simple query via API
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","mode":"fts"}'
```

**Fix:**

**No notes created:**
```bash
# Create a test note via API
curl -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{"title":"Test Note","body":"This is a test note for search"}'
```

**FTS index not populated:**
```bash
# FTS should populate automatically on note creation
# Check logs for errors
docker compose -f docker-compose.bundle.yml logs | grep -i fts

# Manually trigger FTS update (requires database access)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE notes SET fts = to_tsvector('english', title || ' ' || body)"
```

**Embeddings not generated (semantic search):**
```bash
# Check job queue status
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT status, COUNT(*) FROM job_queue WHERE job_type = 'embedding' GROUP BY status"

# If jobs are stuck, check Ollama connectivity (see "Embedding and AI Issues")
# Or reset failed jobs
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE job_queue SET status = 'pending' WHERE status = 'failed' AND job_type = 'embedding'"
```

**Wrong search mode:**
```bash
# Try different modes:
# - "fts" for exact keyword matching
# - "semantic" for conceptual similarity
# - "hybrid" for combined approach

curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"your search term","mode":"hybrid"}'
```

### Irrelevant Search Results

**Symptom:** Search returns unrelated notes or poor ranking.

**Diagnosis:**
```bash
# Check which search mode is being used
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"your term","mode":"hybrid","explain":true}'

# Check if multilingual features are enabled
docker exec Fortémi-matric-1 printenv | grep FTS_

# Test with specific mode
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"your term","mode":"fts"}'
```

**Fix:**

**Wrong search mode for query type:**

For exact keyword matching (technical terms, names, IDs):
```bash
# Use FTS mode
{"query":"REQ-001","mode":"fts"}
```

For conceptual/semantic queries (questions, topics):
```bash
# Use semantic mode
{"query":"how to configure authentication","mode":"semantic"}
```

For general queries (most cases):
```bash
# Use hybrid mode with adaptive weighting
{"query":"authentication setup","mode":"hybrid"}
```

**Multilingual search not working:**
```bash
# Enable multilingual feature flags
# Edit docker-compose.bundle.yml or .env:
environment:
  - FTS_SCRIPT_DETECTION=true
  - FTS_MULTILINGUAL_CONFIGS=true
  - FTS_TRIGRAM_FALLBACK=true
  - FTS_BIGRAM_CJK=true

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**Wrong language configuration:**
```bash
# Provide language hint in search request
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"你好","mode":"hybrid","language":"zh"}'
```

### Slow Search Performance

**Symptom:** Search takes more than 1-2 seconds to return results.

**Diagnosis:**
```bash
# 1. Check database query performance
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "EXPLAIN ANALYZE SELECT id FROM notes WHERE fts @@ websearch_to_tsquery('english', 'test')"

# 2. Check HNSW index status
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'note_embeddings'"

# 3. Check result set size
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","mode":"hybrid","limit":1000}'

# 4. Monitor query time in logs
docker compose -f docker-compose.bundle.yml logs -f | grep -i search
```

**Fix:**

**HNSW index missing or misconfigured:**
```bash
# Check if index exists
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT indexname FROM pg_indexes WHERE indexname LIKE '%hnsw%'"

# If missing, create HNSW index (adjust dimensions to match your model)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "CREATE INDEX IF NOT EXISTS note_embeddings_hnsw_idx ON note_embeddings
   USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64)"
```

**ef_search too high:**
```bash
# Reduce ef_search for faster queries (trades recall for speed)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SET hnsw.ef_search = 40"  # Default is 100

# Or set globally in PostgreSQL config
# Add to postgresql.conf: hnsw.ef_search = 40
```

**Large result sets:**
```bash
# Limit results in search request
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","mode":"hybrid","limit":20}'

# Enable pagination
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","mode":"hybrid","limit":20,"offset":0}'
```

**Strict filter optimization needed:**
```bash
# Use tag filtering for better performance on large datasets
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","mode":"hybrid","tags":["project:alpha"]}'
```

### Multilingual Search Not Working

**Symptom:** Non-English searches return no results or poor results.

**Diagnosis:**
```bash
# Check feature flags
docker exec Fortémi-matric-1 printenv | grep FTS_

# Check available text search configurations
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT cfgname FROM pg_ts_config"

# Test with specific language
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":"Hallo Welt","mode":"fts","language":"de"}'
```

**Fix:**

**Feature flags not enabled:**
```bash
# Edit docker-compose.bundle.yml or .env
environment:
  - FTS_SCRIPT_DETECTION=true       # Auto-detect query language
  - FTS_MULTILINGUAL_CONFIGS=true   # Use language-specific stemming
  - FTS_TRIGRAM_FALLBACK=true       # Enable emoji/symbol search
  - FTS_BIGRAM_CJK=true             # Optimize CJK search

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**Language hint not provided:**
```bash
# Provide explicit language hint
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{
    "query":"你好世界",
    "mode":"hybrid",
    "language":"zh"
  }'
```

**CJK-specific setup:**
```bash
# Ensure pg_bigm or pg_trgm is installed
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT * FROM pg_extension WHERE extname IN ('pg_bigm', 'pg_trgm')"

# If missing, install pg_trgm (pg_bigm optional)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "CREATE EXTENSION IF NOT EXISTS pg_trgm"
```

## Embedding and AI Issues

### Ollama Connection Failed

**Symptom:** Job logs show "connection refused" to Ollama, or embedding jobs fail immediately.

**Diagnosis:**
```bash
# 1. Check if Ollama is running on host
curl http://localhost:11434/api/tags

# 2. Check OLLAMA_URL from container
docker exec Fortémi-matric-1 printenv OLLAMA_URL

# 3. Test connectivity from container
docker exec Fortémi-matric-1 curl http://host.docker.internal:11434/api/tags

# 4. Check job failures
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT id, error FROM job_queue WHERE status = 'failed' AND job_type = 'embedding' LIMIT 5"
```

**Fix:**

**Ollama not installed:**
```bash
# Install Ollama on host
curl -fsSL https://ollama.com/install.sh | sh

# Start Ollama service
ollama serve
```

**Ollama not running:**
```bash
# Start Ollama
ollama serve

# Or as systemd service
sudo systemctl start ollama
```

**Docker cannot reach host (missing extra_hosts):**
```bash
# Edit docker-compose.bundle.yml
# Add under the matric service:
extra_hosts:
  - "host.docker.internal:host-gateway"

# Update OLLAMA_URL to use host.docker.internal
environment:
  - OLLAMA_URL=http://host.docker.internal:11434

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

**Wrong OLLAMA_URL:**
```bash
# Check current URL
docker exec Fortémi-matric-1 printenv OLLAMA_URL

# Should be http://host.docker.internal:11434 from Docker
# Or http://localhost:11434 if running API outside Docker

# Update in docker-compose.bundle.yml or .env
# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

### Embedding Jobs Failing

**Symptom:** Job queue shows many failed embedding jobs.

**Diagnosis:**
```bash
# Check job status
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT status, COUNT(*) FROM job_queue WHERE job_type = 'embedding' GROUP BY status"

# Check recent failures
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT id, error FROM job_queue
   WHERE status = 'failed' AND job_type = 'embedding'
   ORDER BY updated_at DESC LIMIT 5"

# Check logs
docker compose -f docker-compose.bundle.yml logs | grep -i "embed"

# Check Ollama model availability
curl http://localhost:11434/api/tags | grep -i embedding
```

**Fix:**

**Model not pulled:**
```bash
# Pull required embedding model
ollama pull nomic-embed-text

# Or your configured model
ollama pull <model-name>

# Verify model is available
ollama list
```

**OOM (Out of Memory) errors:**
```bash
# Use smaller or quantized model
# Edit docker-compose.bundle.yml or .env:
environment:
  - EMBEDDING_MODEL=nomic-embed-text:q4_0  # Quantized version

# Or reduce parallel requests
environment:
  - OLLAMA_NUM_PARALLEL=1

# Restart and reset failed jobs
docker compose -f docker-compose.bundle.yml restart matric
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE job_queue SET status = 'pending' WHERE status = 'failed' AND job_type = 'embedding'"
```

**Connection timeout:**
```bash
# Increase timeout in configuration
# Edit docker-compose.bundle.yml or .env:
environment:
  - OLLAMA_TIMEOUT_SECONDS=300

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**Old URL cached in failed jobs:**
```bash
# Reset all failed embedding jobs
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE job_queue
   SET status = 'pending', error = NULL, attempts = 0
   WHERE status = 'failed' AND job_type = 'embedding'"

# Worker will retry with current configuration
```

### Semantic Search Returns No Results

**Symptom:** Semantic or hybrid search returns empty results, but FTS works.

**Diagnosis:**
```bash
# Check if embeddings exist
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM note_embeddings"

# Check embedding dimension
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT note_id, vector_dims(embedding) FROM note_embeddings LIMIT 1"

# Check for pending embedding jobs
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM job_queue WHERE job_type = 'embedding' AND status = 'pending'"

# Test embedding generation manually
curl -X POST http://localhost:11434/api/embeddings \
  -d '{"model":"nomic-embed-text","prompt":"test query"}'
```

**Fix:**

**Embeddings not generated:**
```bash
# Trigger embedding for all notes
curl -X POST http://localhost:3000/admin/reindex

# Or wait for background jobs to complete
# Check progress
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT status, COUNT(*) FROM job_queue WHERE job_type = 'embedding' GROUP BY status"
```

**Wrong embedding dimension:**
```bash
# This indicates model mismatch
# Check current model configuration
docker exec Fortémi-matric-1 printenv EMBEDDING_MODEL

# Verify model dimension
curl http://localhost:11434/api/show -d '{"name":"nomic-embed-text"}' | grep embedding

# If dimension changed, must re-embed all notes
# WARNING: This deletes existing embeddings
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "TRUNCATE note_embeddings"

# Trigger re-embedding
curl -X POST http://localhost:3000/admin/reindex
```

**Insufficient notes for semantic search:**
```bash
# Semantic search requires multiple embedded notes
# Create more test notes
for i in {1..10}; do
  curl -X POST http://localhost:3000/notes \
    -H "Content-Type: application/json" \
    -d "{\"title\":\"Test Note $i\",\"body\":\"Content about topic $i\"}"
done

# Wait for embedding jobs to complete
```

### Auto-links Not Created

**Symptom:** Related notes are not automatically linked.

**Diagnosis:**
```bash
# Check if embeddings exist
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM note_embeddings"

# Check linking job status
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT status, COUNT(*) FROM job_queue WHERE job_type = 'link' GROUP BY status"

# Check existing links
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM note_links WHERE link_type = 'semantic'"

# Check similarity threshold
docker exec Fortémi-matric-1 printenv SEMANTIC_LINK_THRESHOLD
```

**Fix:**

**Embedding jobs not complete:**
```bash
# Wait for embedding jobs to finish first
# Check progress
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT status, COUNT(*) FROM job_queue WHERE job_type = 'embedding' GROUP BY status"

# Once embeddings are complete, linking jobs will run automatically
```

**Similarity threshold too high:**
```bash
# Default threshold is 0.7 (70% similarity)
# Lower threshold to find more links (0.6 = 60%)
# Edit docker-compose.bundle.yml or .env:
environment:
  - SEMANTIC_LINK_THRESHOLD=0.6

# Restart container
docker compose -f docker-compose.bundle.yml restart matric

# Trigger re-linking
curl -X POST http://localhost:3000/admin/relink
```

**Insufficient notes:**
```bash
# Auto-linking requires at least 2 similar notes
# Create more related notes
curl -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{"title":"Topic A","body":"Detailed content about topic A"}'

curl -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{"title":"Related to Topic A","body":"More content about topic A"}'

# Wait for embedding and linking jobs to complete
```

### OOM Errors During Embedding

**Symptom:** Container crashes or Ollama shows out-of-memory errors during embedding.

**Diagnosis:**
```bash
# Check container memory limit
docker stats Fortémi-matric-1

# Check Ollama logs
journalctl -u ollama -n 50

# Check current embedding model
docker exec Fortémi-matric-1 printenv EMBEDDING_MODEL

# Check model size
ollama list
```

**Fix:**

**Model too large for VRAM:**
```bash
# Use smaller embedding model
# Edit docker-compose.bundle.yml or .env:
environment:
  - EMBEDDING_MODEL=all-minilm:l6-v2  # Small model (384 dims)

# Or use quantized version
environment:
  - EMBEDDING_MODEL=nomic-embed-text:q4_0

# Restart container
docker compose -f docker-compose.bundle.yml restart matric

# Re-embed notes (WARNING: deletes existing embeddings)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "TRUNCATE note_embeddings"
curl -X POST http://localhost:3000/admin/reindex
```

**Reduce batch size:**
```bash
# Reduce parallel embedding requests
# Edit docker-compose.bundle.yml or .env:
environment:
  - OLLAMA_NUM_PARALLEL=1
  - WORKER_THREADS=2

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**Increase container memory:**
```bash
# Edit docker-compose.bundle.yml
# Add under matric service:
deploy:
  resources:
    limits:
      memory: 4G

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

## MCP Server Issues

### OAuth "Protected Resource URL Mismatch"

**Symptom:** MCP authentication fails with "Protected resource URL mismatch" error.

**Diagnosis:**
```bash
# Check ISSUER_URL environment variable
docker exec Fortémi-matric-1 printenv ISSUER_URL

# Verify OAuth well-known endpoint
curl http://localhost:3000/.well-known/oauth-authorization-server

# Check if ISSUER_URL matches the "issuer" field in response
```

**Fix:**
```bash
# Set ISSUER_URL in .env file
# Must match your deployment domain
echo "ISSUER_URL=http://localhost:3000" >> .env

# Or for local testing
echo "ISSUER_URL=http://localhost:3000" >> .env

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Verify setting
docker exec Fortémi-matric-1 printenv ISSUER_URL
```

### "Unauthorized" with Valid Token

**Symptom:** MCP server returns 401 Unauthorized even with valid OAuth token.

**Diagnosis:**
```bash
# Check MCP_CLIENT_ID and MCP_CLIENT_SECRET
docker exec Fortémi-matric-1 printenv | grep MCP_CLIENT

# Test token introspection manually
curl -X POST http://localhost:3000/oauth/introspect \
  -u "$MCP_CLIENT_ID:$MCP_CLIENT_SECRET" \
  -d "token=<your-token>"

# Check MCP server logs
docker compose -f docker-compose.bundle.yml logs | grep -i mcp
```

**Fix:**

**MCP credentials not configured:**
```bash
# Register OAuth client first
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "MCP Server",
    "grant_types": ["client_credentials"],
    "scope": "mcp read"
  }'

# Save the returned client_id and client_secret

# Add to .env file
echo "MCP_CLIENT_ID=mm_xxxxx" >> .env
echo "MCP_CLIENT_SECRET=xxxxx" >> .env

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

**Wrong scope:**
```bash
# Ensure client has "mcp" and "read" scopes
# Re-register with correct scopes (see above)
```

### Authentication Succeeds but Connection Drops

**Symptom:** Claude Code shows "Authentication successful" but then reconnection fails.

**Diagnosis:**
```bash
# Check for stale credentials in Claude credentials file
cat ~/.claude/.credentials.json | grep Fortémi

# Check MCP server logs for auth errors
docker compose -f docker-compose.bundle.yml logs | grep -i "auth"

# Test MCP endpoint directly
curl http://localhost:3001/health
```

**Fix:**
```bash
# Remove stale OAuth credentials from Claude cache
# Edit ~/.claude/.credentials.json
# Remove or update the "Fortémi" entry

# Or delete entire credentials file (will require re-auth)
rm ~/.claude/.credentials.json

# Restart Claude Code
# Re-authenticate when prompted
```

### MCP Server Not Responding

**Symptom:** MCP endpoint returns connection refused or times out.

**Diagnosis:**
```bash
# Check if container is running
docker compose -f docker-compose.bundle.yml ps

# Check MCP server logs
docker compose -f docker-compose.bundle.yml logs | grep -i mcp

# Test MCP port directly
curl http://localhost:3001/health

# Check if Node.js process is running
docker exec Fortémi-matric-1 ps aux | grep node
```

**Fix:**

**Container crashed:**
```bash
# Restart container
docker compose -f docker-compose.bundle.yml restart matric

# Check logs for crash reason
docker compose -f docker-compose.bundle.yml logs --tail=100
```

**MCP server process died:**
```bash
# Check if supervisor is running
docker exec Fortémi-matric-1 ps aux | grep supervisor

# If not, restart container
docker compose -f docker-compose.bundle.yml restart matric

# Check logs
docker compose -f docker-compose.bundle.yml logs -f
```

**Port not exposed:**
```bash
# Verify docker-compose.bundle.yml exposes port 3001
# Should have:
ports:
  - "3001:3001"

# If missing, add and restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### Nginx Proxy Returns 502

**Symptom:** Accessing MCP through Nginx reverse proxy returns 502 Bad Gateway.

**Diagnosis:**
```bash
# Test direct container access (without proxy)
curl http://localhost:3001/health

# Check nginx error logs
sudo tail -f /var/log/nginx/error.log

# Check nginx configuration
sudo nginx -t

# Test if nginx can reach container
docker inspect Fortémi-matric-1 | grep IPAddress
curl http://<container-ip>:3001/health
```

**Fix:**

**Nginx cannot reach container:**
```bash
# Ensure nginx is configured to proxy to correct host
# nginx.conf should have:

upstream mcp_backend {
    server localhost:3001;
}

server {
    listen 443 ssl;
    server_name your-domain.com;

    location /mcp {
        proxy_pass http://mcp_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 600s;
        proxy_connect_timeout 600s;
    }
}

# Reload nginx
sudo nginx -s reload
```

**WebSocket upgrade headers missing:**
```bash
# Add WebSocket support to nginx config (see above)
# The critical headers are:
#   proxy_http_version 1.1;
#   proxy_set_header Upgrade $http_upgrade;
#   proxy_set_header Connection "upgrade";

# Reload nginx
sudo nginx -s reload
```

## Events / WebSocket Issues

### WebSocket Connection Drops

**Symptom:** WebSocket connections to `/api/v1/ws` disconnect frequently.

**Diagnosis:**
```bash
# Test WebSocket directly (bypass nginx)
wscat -c ws://localhost:3000/api/v1/ws

# Check nginx configuration
nginx -t
```

**Fix:**

**Missing nginx WebSocket headers:**
```nginx
location /api/v1/ws {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_read_timeout 86400;  # 24h timeout for long-lived connections
}
```

**Proxy timeout too short:**
```bash
# Increase proxy_read_timeout in nginx config
# Default 60s is too short for WebSocket connections
# Use 86400 (24 hours) for persistent connections
```

### SSE Not Receiving Events

**Symptom:** Connected to `/api/v1/events` but no events arrive.

**Diagnosis:**
```bash
# Test SSE directly
curl -N http://localhost:3000/api/v1/events

# Check if events are being emitted (should see keepalive every 15s)
# If no keepalive, the connection may be buffered by a proxy
```

**Fix:**

**Nginx buffering SSE responses:**
```nginx
location /api/v1/events {
    proxy_pass http://localhost:3000;
    proxy_set_header Connection '';
    proxy_http_version 1.1;
    chunked_transfer_encoding off;
    proxy_buffering off;
    proxy_cache off;
    proxy_read_timeout 86400;
}
```

**No active jobs or notes being processed:**
```bash
# QueueStatus events only emit when subscribers are connected
# Create a note to trigger events
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content": "test note"}'
```

### Webhook Delivery Failures

**Symptom:** Webhooks created but target URL not receiving events.

**Diagnosis:**
```bash
# Check webhook configuration
curl http://localhost:3000/api/v1/webhooks

# Check delivery logs for a webhook
curl http://localhost:3000/api/v1/webhooks/<webhook-id>/deliveries?limit=10

# Test webhook delivery manually
curl -X POST http://localhost:3000/api/v1/webhooks/<webhook-id>/test
```

**Fix:**

**Target URL unreachable:**
```bash
# Verify the webhook URL is accessible from the container
docker exec Fortémi-matric-1 curl -v https://your-webhook-url.com
```

**Webhook not subscribed to correct events:**
```bash
# Update webhook to subscribe to specific events
curl -X PATCH http://localhost:3000/api/v1/webhooks/<webhook-id> \
  -H "Content-Type: application/json" \
  -d '{"events": ["NoteUpdated", "JobCompleted", "JobFailed"]}'
```

**HMAC signature validation failing:**
```bash
# Verify the secret matches what the receiver expects
# Signature is HMAC-SHA256 of the JSON payload
# Header: X-Fortemi-Signature: sha256=<hex-digest>
```

## Performance Issues

### High Memory Usage

**Symptom:** Container uses excessive memory (multiple GB) or gets OOM-killed.

**Diagnosis:**
```bash
# Check container memory usage
docker stats Fortémi-matric-1

# Check process memory inside container
docker exec Fortémi-matric-1 ps aux --sort=-%mem | head -10

# Check PostgreSQL memory settings
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SHOW shared_buffers; SHOW work_mem; SHOW maintenance_work_mem;"

# Check Ollama model size
ollama list
```

**Fix:**

**Embedding model too large:**
```bash
# See "OOM Errors During Embedding" section above
# Use smaller or quantized model
```

**Too many parallel requests:**
```bash
# Reduce worker threads and parallel Ollama requests
# Edit docker-compose.bundle.yml or .env:
environment:
  - WORKER_THREADS=2
  - OLLAMA_NUM_PARALLEL=1

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**PostgreSQL work_mem too high:**
```bash
# Reduce PostgreSQL memory allocation
# Edit postgresql.conf or set in docker-compose.bundle.yml:
command: >
  postgres
  -c shared_buffers=256MB
  -c work_mem=16MB
  -c maintenance_work_mem=64MB

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

**Set container memory limit:**
```bash
# Edit docker-compose.bundle.yml
# Add under matric service:
deploy:
  resources:
    limits:
      memory: 2G

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### Slow Queries

**Symptom:** API requests take several seconds to return.

**Diagnosis:**
```bash
# Enable slow query logging
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "ALTER DATABASE matric SET log_min_duration_statement = 1000"

# Check slow queries in logs
docker compose -f docker-compose.bundle.yml logs | grep "duration:"

# Analyze specific query
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "EXPLAIN ANALYZE SELECT * FROM notes WHERE fts @@ websearch_to_tsquery('test')"

# Check index usage
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT schemaname, tablename, indexname, idx_scan
   FROM pg_stat_user_indexes
   WHERE schemaname = 'public'
   ORDER BY idx_scan"
```

**Fix:**

**Missing indexes:**
```bash
# Check what indexes exist
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT tablename, indexname FROM pg_indexes WHERE schemaname = 'public'"

# Common indexes should include:
# - notes_fts_idx (GIN index on fts column)
# - note_embeddings_hnsw_idx (HNSW index on embedding column)
# - notes_created_at_idx (for sorting by date)

# If missing, migrations should create them
# Rebuild container to run migrations
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml up -d
```

**HNSW tuning:**
```bash
# Increase ef_search for better recall (but slower)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SET hnsw.ef_search = 100"

# Or decrease for faster queries (but lower recall)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SET hnsw.ef_search = 40"

# Make permanent by adding to postgresql.conf
```

**Vacuum and analyze:**
```bash
# Update table statistics
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "VACUUM ANALYZE notes"

docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "VACUUM ANALYZE note_embeddings"
```

### Database Growth

**Symptom:** Database size grows unexpectedly large.

**Diagnosis:**
```bash
# Check total database size
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT pg_size_pretty(pg_database_size('matric'))"

# Check table sizes
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT tablename, pg_size_pretty(pg_total_relation_size(tablename::regclass))
   FROM pg_tables
   WHERE schemaname = 'public'
   ORDER BY pg_total_relation_size(tablename::regclass) DESC"

# Check for orphaned blobs
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT COUNT(*) FROM blobs WHERE id NOT IN (SELECT blob_id FROM note_blobs)"
```

**Fix:**

**Clean orphaned blobs:**
```bash
# Delete orphaned blobs (blobs not referenced by any note)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "DELETE FROM blobs WHERE id NOT IN (SELECT blob_id FROM note_blobs)"

# Reclaim space
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "VACUUM FULL blobs"
```

**Manage note versions:**
```bash
# Check version count
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT note_id, COUNT(*) as version_count
   FROM note_versions
   GROUP BY note_id
   ORDER BY version_count DESC
   LIMIT 10"

# Delete old versions (keep last 10 per note)
# WARNING: This is destructive
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "DELETE FROM note_versions
   WHERE id NOT IN (
     SELECT id FROM (
       SELECT id, ROW_NUMBER() OVER (PARTITION BY note_id ORDER BY created_at DESC) as rn
       FROM note_versions
     ) sub WHERE rn <= 10
   )"

# Reclaim space
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "VACUUM FULL note_versions"
```

**Vacuum full database:**
```bash
# WARNING: This locks tables and can take time
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "VACUUM FULL ANALYZE"
```

### Job Backlog Growing

**Symptom:** Job queue has hundreds of pending jobs that never complete.

**Diagnosis:**
```bash
# Check job queue status
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT job_type, status, COUNT(*)
   FROM job_queue
   GROUP BY job_type, status
   ORDER BY job_type, status"

# Check Ollama availability
curl http://localhost:11434/api/tags

# Check worker thread count
docker exec Fortémi-matric-1 printenv WORKER_THREADS

# Check for stuck jobs
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT id, job_type, status, attempts, error, updated_at
   FROM job_queue
   WHERE status = 'running'
   AND updated_at < NOW() - INTERVAL '5 minutes'"
```

**Fix:**

**Ollama unavailable:**
```bash
# See "Ollama Connection Failed" section above
# Ensure Ollama is running and reachable from container
```

**Increase worker threads:**
```bash
# Edit docker-compose.bundle.yml or .env:
environment:
  - WORKER_THREADS=4

# Restart container
docker compose -f docker-compose.bundle.yml restart matric
```

**Reset stuck jobs:**
```bash
# Reset jobs stuck in "running" state
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE job_queue
   SET status = 'pending', attempts = 0
   WHERE status = 'running'
   AND updated_at < NOW() - INTERVAL '5 minutes'"
```

**Clear failed jobs:**
```bash
# Reset failed jobs to retry
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "UPDATE job_queue
   SET status = 'pending', error = NULL, attempts = 0
   WHERE status = 'failed'"
```

## Events / WebSocket Issues

### WebSocket Connection Drops

**Symptom:** WebSocket connections to `/api/v1/ws` disconnect after a few seconds.

**Diagnosis:**
```bash
# Test direct WebSocket connection (bypassing nginx)
websocat ws://localhost:3000/api/v1/ws

# Check nginx error logs
sudo tail -f /var/log/nginx/error.log
```

**Fix:**

**Nginx not configured for WebSocket upgrade:**
```nginx
# Add these headers to your nginx location block for the API:
location / {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 86400s;  # Keep alive for 24h
    # ... other headers
}
```

**Proxy timeout too short:**
```nginx
# Increase timeouts for long-lived connections
proxy_read_timeout 86400s;
proxy_send_timeout 86400s;
```

### SSE Not Receiving Events

**Symptom:** Connected to `/api/v1/events` but no events arrive (only keepalive).

**Diagnosis:**
```bash
# Check if events are being emitted
curl -N http://localhost:3000/api/v1/events

# You should see keepalive every 15 seconds:
# : keepalive

# Create a note to trigger events:
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content": "test"}'
```

**Fix:**

**No activity in the system:**
- QueueStatus events only emit when there are subscribers AND the job queue has activity
- Create or update a note to trigger NoteUpdated and job events

**Buffering by reverse proxy:**
```nginx
# Disable buffering for SSE endpoints
location /api/v1/events {
    proxy_pass http://localhost:3000;
    proxy_buffering off;
    proxy_cache off;
    proxy_set_header Connection '';
    proxy_http_version 1.1;
    chunked_transfer_encoding off;
}
```

### Webhook Delivery Failures

**Symptom:** Webhook deliveries failing (check via `GET /api/v1/webhooks/:id/deliveries`).

**Diagnosis:**
```bash
# List webhook deliveries
curl http://localhost:3000/api/v1/webhooks/<id>/deliveries?limit=10

# Test webhook manually
curl -X POST http://localhost:3000/api/v1/webhooks/<id>/test
```

**Fix:**

**Target URL unreachable:**
- Verify the webhook URL is accessible from the Fortémi container
- Check firewall rules and DNS resolution
- Webhook delivery timeout is 10 seconds

**HMAC signature mismatch:**
- Ensure your webhook receiver validates the `X-Fortemi-Signature` header using HMAC-SHA256 with the configured secret
- The signature is computed over the raw JSON body

**Webhook not receiving expected events:**
```bash
# Check which events the webhook is subscribed to
curl http://localhost:3000/api/v1/webhooks/<id>

# Update event filter
curl -X PATCH http://localhost:3000/api/v1/webhooks/<id> \
  -H "Content-Type: application/json" \
  -d '{"events": ["JobFailed", "JobCompleted", "NoteUpdated"]}'
```

## Getting Help

If you cannot resolve the issue with this guide:

**Check logs for detailed error messages:**
```bash
docker compose -f docker-compose.bundle.yml logs -f
```

**Interactive API documentation:**
- OpenAPI UI: `http://localhost:3000/docs`
- OpenAPI spec: `http://localhost:3000/openapi.yaml`

**Report issues:**
- Repository issues: `https://github.com/fortemi/fortemi/issues`
- Include: error message, relevant logs, Docker/system info, steps to reproduce

**Additional documentation:**
- [Operators Guide](./operators-guide.md) - Deployment and maintenance procedures
- [Configuration Reference](./configuration.md) - All environment variables and settings
- [Embedding Model Selection](./embedding-model-selection.md) - Choosing embedding models
- [Real-Time Events](./real-time-events.md) - SSE, WebSocket, and webhook event streaming

---

*For routine maintenance and monitoring, see the [Operators Guide](./operators-guide.md). For advanced configuration, see the [Configuration Reference](./configuration.md).*

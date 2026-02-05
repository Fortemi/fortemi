# Use Cases

Matric-memory adapts to different scales and requirements. This guide maps common scenarios to recommended configurations, helping you choose the right setup for your needs.

## Personal Knowledge Base

**Scale:** 1-10K notes | **Users:** 1 | **Hardware:** Tier 1-2

### Who This Is For

Researchers, students, writers, and developers building a personal second brain. You need a reliable system to capture ideas, organize research, and discover connections across your notes.

### Recommended Configuration

**Deployment:**
- Docker bundle with local Ollama
- Single container deployment (PostgreSQL + API + MCP)
- No authentication required for single-user setups
- 8GB RAM minimum, 16GB recommended

**Models:**
- Embedding: nomic-embed-text (768 dimensions)
- Generation: llama3.2:3b for note revision and summarization
- Hardware: Tier 1 GPU (RTX 3060, 8GB VRAM) sufficient

**Storage Optimization:**
- Enable MRL at 256 dimensions for storage efficiency on large collections
- Semantic chunking for mixed markdown content
- Auto-embed rules: embed on creation, skip on minor edits

**Environment Variables:**
```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric
OLLAMA_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_DIMENSION=768
MRL_DIMENSION=256

# Disable auth for single-user
DISABLE_AUTH=true

# Enable multilingual search if needed
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
```

**Docker Deployment:**
```bash
# Start the bundle
docker compose -f docker-compose.bundle.yml up -d

# Verify health
curl http://localhost:3000/health
```

### Example Workflow

1. **Daily note capture:** Create notes via API or MCP integration
2. **Automatic tagging:** Apply SKOS concepts for organization
3. **Semantic search:** Find connections across your knowledge base
4. **Knowledge graph exploration:** Discover related notes through auto-linking
5. **AI revision:** Use llama3.2:3b to improve clarity and structure

### Key Features

- **Auto-linking:** Notes with >70% semantic similarity automatically connected
- **Version history:** Track changes over time, restore previous versions
- **Hybrid search:** Combine full-text and semantic search for best results
- **MCP integration:** Use with Claude Code for AI-assisted note management
- **Export:** Generate markdown with YAML frontmatter for portability

### Related Documentation

- [Getting Started](getting-started.md)
- [Hardware Planning](hardware-planning.md)
- [MCP Integration](mcp-server.md)

---

## Team Documentation Hub

**Scale:** 10K-100K notes | **Users:** 5-50 | **Hardware:** Tier 2-3

### Who This Is For

Engineering teams, product teams, and knowledge workers sharing documentation. You need consistent organization, access control, and the ability to isolate project documentation.

### Recommended Configuration

**Deployment:**
- Docker bundle with reverse proxy (nginx)
- OAuth2 authentication for user access
- API keys for automation and scripts
- 16GB RAM minimum, 32GB recommended

**Models:**
- Embedding: nomic-embed-text (768 dimensions)
- Generation: qwen2.5:7b for documentation generation
- Hardware: Tier 2 GPU (RTX 4060 Ti 16GB, 16GB VRAM)

**Access Control:**
- OAuth2 for interactive users
- API keys with scoped permissions for automation
- Rate limiting enabled (100 requests/minute per user)

**Organization:**
- SKOS taxonomy for consistent categorization
- Strict tag filtering with `required_schemes` for project isolation
- Per-project embedding sets (filter sets to share base embeddings)

**Environment Variables:**
```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric
OLLAMA_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_DIMENSION=768

# OAuth2 configuration
ISSUER_URL=https://docs.example.com
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx

# Rate limiting
RATE_LIMIT_ENABLED=true
RATE_LIMIT_PER_MINUTE=100

# Strict tag filtering
STRICT_TAG_FILTER=true
```

**Nginx Reverse Proxy:**
```nginx
server {
    listen 443 ssl http2;
    server_name docs.example.com;

    ssl_certificate /etc/letsencrypt/live/docs.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/docs.example.com/privkey.pem;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /mcp {
        proxy_pass http://localhost:3001;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### SKOS Scheme Setup

**Create schemes per project:**
```bash
# Create project scheme
curl -X POST https://docs.example.com/api/v1/schemes \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "https://docs.example.com/schemes/project-alpha",
    "label": "Project Alpha",
    "description": "Documentation for Project Alpha"
  }'

# Create concepts within scheme
curl -X POST https://docs.example.com/api/v1/concepts \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "https://docs.example.com/schemes/project-alpha/architecture",
    "pref_label": "Architecture",
    "in_scheme": "https://docs.example.com/schemes/project-alpha"
  }'
```

### Example Workflow

1. **Create scheme per project:** Establish consistent taxonomy
2. **Tag notes consistently:** Apply SKOS concepts to all documentation
3. **Search within project context:** Use `required_schemes` parameter to filter
4. **Share via knowledge shards:** Export project documentation for backup or migration
5. **AI-assisted documentation:** Use qwen2.5:7b to improve technical writing

### Key Features

- **Strict tag filtering:** Guarantee data isolation between projects with `required_schemes`
- **OAuth2 authentication:** Secure access with single sign-on
- **SKOS hierarchy:** Consistent organization across teams
- **Embedding sets:** Create filter sets per project for shared base embeddings
- **Knowledge shards:** Backup and restore project documentation independently

### Related Documentation

- [Authentication Guide](authentication.md)
- [Tags and SKOS](tags.md)
- [Strict Tag Filtering Design](strict-tag-filtering-design.md)
- [Embedding Sets](embedding-sets.md)

---

## AI Research Assistant / RAG

**Scale:** 50K-500K notes | **Users:** 1-10 | **Hardware:** Tier 3+

### Who This Is For

AI engineers building RAG (Retrieval-Augmented Generation) pipelines and researchers processing large document collections. You need high-quality retrieval, domain-specific embeddings, and efficient search at scale.

### Recommended Configuration

**Deployment:**
- Docker bundle with dedicated GPU server
- API-only access (MCP optional)
- High-performance PostgreSQL tuning
- 32GB RAM minimum, 64GB recommended

**Models:**
- Embedding: mxbai-embed-large (1024 dimensions) or domain fine-tuned models
- Re-ranking: Use smaller model (MiniLM-v6) with LLM re-ranking
- Generation: External LLM (GPT-4, Claude) for final output
- Hardware: Tier 3 GPU (RTX 4090, 24GB VRAM)

**Search Configuration:**
- Hybrid search with adaptive RRF (k parameter auto-adjusts by query type)
- Two-stage MRL retrieval for 128x compute reduction
- Per-corpus embedding sets for domain isolation

**Environment Variables:**
```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric
OLLAMA_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=mxbai-embed-large
EMBEDDING_DIMENSION=1024

# MRL configuration
MRL_DIMENSION=64  # Coarse stage
MRL_FINE_DIMENSION=1024  # Fine stage

# Two-stage retrieval
TWO_STAGE_ENABLED=true
TWO_STAGE_COARSE_LIMIT=1000
TWO_STAGE_FINE_LIMIT=100

# Adaptive RRF
RRF_ADAPTIVE=true
RRF_K_MIN=30
RRF_K_MAX=90

# PostgreSQL tuning
POSTGRES_MAX_CONNECTIONS=200
POSTGRES_SHARED_BUFFERS=8GB
POSTGRES_EFFECTIVE_CACHE_SIZE=24GB
POSTGRES_WORK_MEM=128MB
```

### Embedding Set Configuration

**Create corpus-specific embedding sets:**

```bash
# Create full embedding set for specialized domain
curl -X POST https://api.example.com/api/v1/embedding-sets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "medical-research",
    "description": "Medical research papers with domain-tuned embeddings",
    "model": "medical-bert-512",
    "dimension": 512,
    "mrl_dimension": 64,
    "is_filter_set": false
  }'

# Create filter set sharing base embeddings
curl -X POST https://api.example.com/api/v1/embedding-sets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "general-docs",
    "description": "General documentation using default embeddings",
    "parent_set_id": 1,
    "is_filter_set": true
  }'
```

### Two-Stage Retrieval Pipeline

**Stage 1: Coarse search (64-dimensional MRL vectors)**
- Search 1000 candidates quickly
- 128x faster than full-dimensional search
- Prune obviously irrelevant results

**Stage 2: Fine search (1024-dimensional full vectors)**
- Re-rank top 1000 with full embeddings
- Select top 100 for LLM re-ranking
- Highest precision for final results

**Stage 3: LLM re-ranking (optional)**
- Use GPT-4o-mini or Claude Haiku
- Consider query-document relevance with semantic understanding
- Return top 10 for RAG context

### Example Workflow

1. **Ingest documents:** Upload research papers, code, documentation
2. **Create domain-specific embedding sets:** Fine-tune or use specialized models
3. **Two-stage search:** Coarse retrieval (MRL) -> fine retrieval (full) -> LLM re-rank
4. **RAG generation:** Pass top results to GPT-4 for answer generation
5. **Feedback loop:** Track which results produce useful answers, refine embeddings

### Research-Backed Optimizations

**Per REF-068: MiniLM-v6 with LLM re-ranking outperforms larger models**
- MiniLM-v6 (384d) for initial retrieval
- GPT-4o-mini for re-ranking top 100 results
- 15% better nDCG@10 than all-MiniLM-L12-v2 (768d) alone
- Lower compute cost than using large embedding model

**Per REF-069: Fine-tuning embedding models yields 88% improvement**
- Domain-specific fine-tuning on medical corpus: 88% improvement in recall
- Legal corpus fine-tuning: 76% improvement
- Consider fine-tuning for specialized domains with >50K documents

**Per REF-070: Adaptive RRF k parameter improves multi-query fusion**
- k=30 for high semantic query overlap
- k=60 for mixed queries
- k=90 for diverse keyword queries
- Auto-adjustment based on query analysis

### Key Features

- **Two-stage retrieval:** 128x compute reduction with MRL coarse-to-fine search
- **Embedding sets:** Isolate corpora with independent embeddings
- **Adaptive RRF:** Automatic k parameter tuning for query fusion
- **Document type registry:** 131 pre-configured types with smart chunking
- **Fine-tuned models:** Support for domain-specific embedding models

### Related Documentation

- [Embedding Model Selection](embedding-model-selection.md)
- [Search Guide](search-guide.md)
- [Embedding Sets](embedding-sets.md)
- [Document Type Registry](document-types.md)

---

## Enterprise Document Management

**Scale:** 500K+ notes | **Users:** 50+ | **Hardware:** Tier 3-4 or Cloud Hybrid

### Who This Is For

Organizations managing large document collections with compliance requirements. You need multi-tenancy, encryption, audit trails, and guaranteed data isolation between departments or customers.

### Recommended Configuration

**Deployment:**
- High-availability Docker deployment with load balancing
- Multi-region backup with knowledge shards
- OAuth2 with scoped access control
- Compliance logging enabled
- 64GB RAM minimum, 128GB recommended for large deployments

**Models:**
- Hybrid deployment: local Ollama for embeddings (privacy), cloud for generation (quality)
- Embedding: nomic-embed-text (privacy-preserving, no data leaves network)
- Generation: GPT-4 or Claude via OpenAI-compatible API
- Hardware: Tier 3-4 GPU (A5000 or A6000, 24-48GB VRAM) or cloud inference

**Security:**
- PKE (Public Key Encryption) for sensitive documents
- Scheme-based multi-tenancy with strict tag filtering
- OAuth2 scopes: read, write, delete, admin
- API key rotation policies
- Rate limiting per tenant

**Compliance:**
- Content-addressable dedup (BLAKE3 hashing)
- Audit logs for all operations
- Knowledge shards for department-level backup/restore
- Retention policies via auto-embed rules

**Environment Variables:**
```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric
OLLAMA_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_DIMENSION=768

# OAuth2 configuration
ISSUER_URL=https://memory.enterprise.com
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx
OAUTH_SCOPES=read,write,delete,admin

# PKE encryption
PKE_ENABLED=true
PKE_KEY_ROTATION_DAYS=90

# Strict tag filtering (guaranteed isolation)
STRICT_TAG_FILTER=true
REQUIRED_SCHEMES_ENFORCEMENT=true

# Multilingual FTS for global teams
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_BIGRAM_CJK=true
FTS_MULTILINGUAL_CONFIGS=true

# Content deduplication
ATTACHMENT_DEDUP=true
ATTACHMENT_HASH_ALGORITHM=blake3

# Audit logging
AUDIT_LOG_ENABLED=true
AUDIT_LOG_LEVEL=info

# Rate limiting per tenant
RATE_LIMIT_ENABLED=true
RATE_LIMIT_PER_MINUTE=1000
RATE_LIMIT_PER_TENANT=true
```

### Multi-Tenancy with SKOS Schemes

**Create scheme per department/customer:**

```bash
# Department A
curl -X POST https://memory.enterprise.com/api/v1/schemes \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "https://memory.enterprise.com/schemes/dept-a",
    "label": "Department A",
    "description": "Department A documents",
    "metadata": {
      "tenant_id": "dept-a",
      "retention_days": 2555
    }
  }'

# Department B
curl -X POST https://memory.enterprise.com/api/v1/schemes \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "https://memory.enterprise.com/schemes/dept-b",
    "label": "Department B",
    "description": "Department B documents",
    "metadata": {
      "tenant_id": "dept-b",
      "retention_days": 2555
    }
  }'
```

**Enforce scheme isolation in searches:**
```bash
# Search only returns notes tagged with dept-a scheme
curl "https://memory.enterprise.com/api/v1/notes/search?q=contract&required_schemes=https://memory.enterprise.com/schemes/dept-a" \
  -H "Authorization: Bearer $DEPT_A_TOKEN"
```

### PKE Encryption for Sensitive Documents

**Generate key pair:**
```bash
curl -X POST https://memory.enterprise.com/api/v1/pke/keypair \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Sensitive Documents Key"
  }'
```

**Response:**
```json
{
  "public_key": "...",
  "private_key": "...",
  "key_id": "key_abc123"
}
```

**Create encrypted note:**
```bash
curl -X POST https://memory.enterprise.com/api/v1/notes \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Confidential Contract",
    "content": "...",
    "encryption": {
      "enabled": true,
      "key_id": "key_abc123"
    },
    "tags": ["https://memory.enterprise.com/schemes/dept-a/contracts"]
  }'
```

**Decrypt on retrieval:**
```bash
curl "https://memory.enterprise.com/api/v1/notes/123?decrypt=true" \
  -H "Authorization: Bearer $TOKEN" \
  -H "X-PKE-Private-Key: ..."
```

### Knowledge Shards for Backup/Restore

**Export department documentation:**
```bash
curl -X POST https://memory.enterprise.com/api/v1/shards/export \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "filter": {
      "required_schemes": ["https://memory.enterprise.com/schemes/dept-a"]
    },
    "include_embeddings": true,
    "include_attachments": true
  }' \
  -o dept-a-backup.shard
```

**Restore to new deployment:**
```bash
curl -X POST https://memory.enterprise.com/api/v1/shards/import \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @dept-a-backup.shard
```

### Example Workflow

1. **Onboard department:** Create SKOS scheme with tenant metadata
2. **Assign API keys:** Scoped permissions (department can only access their scheme)
3. **Upload documents:** Automatic tagging with department scheme
4. **Enable encryption:** PKE for sensitive documents (HR, legal, financial)
5. **Compliance audit:** Query audit logs for data access patterns
6. **Backup schedule:** Export knowledge shards daily per department
7. **Retention enforcement:** Auto-delete notes older than retention policy

### Key Features

- **PKE encryption:** X25519/AES-256-GCM for sensitive documents
- **Strict tag filtering:** `required_schemes` guarantees no cross-tenant data leakage
- **Multilingual FTS:** Support global teams (English, German, French, Spanish, Portuguese, Russian, CJK)
- **OAuth2 scopes:** Granular permissions (read, write, delete, admin)
- **File attachments:** Content-addressable storage with BLAKE3 deduplication
- **Knowledge shards:** Department-level backup/restore with embeddings and attachments
- **Audit logs:** Track all operations for compliance

### Related Documentation

- [Encryption Guide](encryption.md)
- [Authentication Guide](authentication.md)
- [Operators Guide](operators-guide.md)
- [Strict Tag Filtering Design](strict-tag-filtering-design.md)
- [Knowledge Shards](knowledge-shards.md)

---

## Hybrid Cloud/Edge Deployment

**Scale:** Variable | **Users:** Variable | **Hardware:** Mixed (Local + Cloud)

### Who This Is For

Organizations wanting privacy-sensitive local processing (embeddings stay on-premise) with cloud quality for generation. You need data sovereignty for embeddings while leveraging cloud LLMs for high-quality generation.

### Recommended Configuration

**Deployment:**
- Local: Docker bundle with Ollama for embeddings
- Cloud: OpenAI-compatible API for generation (GPT-4, Claude)
- Reverse proxy with OAuth2 for secure external access
- MCP server on private network only

**Models:**
- Embedding: Local Ollama (nomic-embed-text, mxbai-embed-large)
- Generation: Cloud API (GPT-4, Claude Opus, GPT-4o-mini)
- Fallback: Local generation model (qwen2.5:7b) if cloud unavailable

**Security:**
- Embeddings never leave local network
- Note content sent to cloud only for generation requests (user-initiated)
- OAuth2 for external API access
- MCP restricted to private network

**Environment Variables:**
```bash
# .env
DATABASE_URL=postgres://matric:matric@localhost/matric

# Local Ollama for embeddings
OLLAMA_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_DIMENSION=768

# Cloud API for generation
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_API_KEY=sk-...
GENERATION_MODEL=gpt-4o-mini

# Fallback to local generation
OLLAMA_GENERATION_MODEL=qwen2.5:7b
GENERATION_FALLBACK=ollama

# OAuth2 for external access
ISSUER_URL=https://memory.example.com
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx

# MCP restricted to private network
MCP_BIND_ADDRESS=10.0.0.100
MCP_PORT=3001
```

### Inference Configuration

**Create `inference.toml` for routing:**

```toml
[inference]
# Default backend for embeddings
backend = "ollama"

[inference.ollama]
base_url = "http://localhost:11434"
embedding_model = "nomic-embed-text"
generation_model = "qwen2.5:7b"
timeout_seconds = 300

[inference.openai]
# Cloud generation for high quality
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
generation_model = "gpt-4o-mini"
timeout_seconds = 60

[inference.routing]
# Route by operation type
embed = "ollama"        # Keep embeddings local
generate = "openai"     # Use cloud for generation
summarize = "openai"
revise = "openai"

[inference.fallback]
# Fallback chain: try cloud, fall back to local
enabled = true
chains = [
  ["openai", "ollama"]
]
```

### Nginx Reverse Proxy Configuration

**Expose API externally, keep MCP internal:**

```nginx
# External API with OAuth2
server {
    listen 443 ssl http2;
    server_name memory.example.com;

    ssl_certificate /etc/letsencrypt/live/memory.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/memory.example.com/privkey.pem;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=100r/m;
    limit_req zone=api_limit burst=20 nodelay;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # OAuth2 enforcement (via nginx-auth-request or similar)
        auth_request /oauth2/auth;
    }
}

# Internal MCP server (no external access)
# Only accessible from private network (10.0.0.0/8)
server {
    listen 3001;
    server_name 10.0.0.100;

    allow 10.0.0.0/8;
    deny all;

    location / {
        proxy_pass http://localhost:3001;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### Example Workflow

1. **Local embedding generation:** Documents ingested -> embeddings created with local Ollama
2. **Privacy-preserving search:** Hybrid search uses local embeddings (vectors never leave network)
3. **Cloud generation:** User requests note revision -> content sent to GPT-4 -> response returned
4. **Fallback handling:** Cloud API down -> automatic fallback to local qwen2.5:7b
5. **MCP integration:** Claude Code on developer machine (private network) -> MCP queries local embeddings

### Privacy Considerations

**What stays local:**
- All embeddings (vectors derived from your documents)
- Full-text search indexes
- PostgreSQL database with note content
- SKOS taxonomy and tags
- File attachments

**What goes to cloud (only when requested):**
- Note content for generation/revision requests
- Summary generation requests
- User-initiated AI operations

**Best practices:**
- Use PKE encryption for sensitive documents (never send encrypted content to cloud)
- Configure `generation_allowed_schemes` to restrict cloud access to non-sensitive schemes
- Monitor audit logs for cloud API calls
- Set up fallback to local generation for high-sensitivity operations

### Cost Optimization

**Embeddings (local):**
- One-time cost: GPU hardware (Tier 2-3)
- Ongoing cost: Electricity (~$20-50/month)
- No per-request charges

**Generation (cloud):**
- GPT-4o-mini: ~$0.15 per 1M input tokens, $0.60 per 1M output tokens
- Estimated cost: $50-200/month for 10K generation requests
- Use local fallback for non-critical requests to reduce costs

**Total cost:**
- Initial: $600-1500 (GPU hardware)
- Monthly: $70-250 (electricity + cloud API)
- Compare to Tier 5 (full cloud): $200-500/month with no hardware investment

### Key Features

- **Data sovereignty:** Embeddings stay on-premise for privacy
- **Cloud quality:** Leverage GPT-4, Claude for generation without storing vectors in cloud
- **Fallback chains:** Automatic failover from cloud to local
- **Flexible routing:** Route operations by type (embed local, generate cloud)
- **MCP privacy:** Restrict MCP server to private network

### Related Documentation

- [Inference Backends](inference-backends.md)
- [Operators Guide](operators-guide.md)
- [Configuration Reference](configuration.md)
- [Hardware Planning](hardware-planning.md)

---

## Comparison Table

| Scenario | Notes | Users | Auth | Embedding | Search Mode | Hardware | Key Feature |
|----------|-------|-------|------|-----------|-------------|----------|-------------|
| Personal KB | 1-10K | 1 | None | nomic-embed (local) | Hybrid | Tier 1-2 | Auto-linking |
| Team Docs | 10-100K | 5-50 | OAuth2 | nomic-embed (local) | Hybrid + strict | Tier 2-3 | Tag isolation |
| AI/RAG | 50-500K | 1-10 | API key | mxbai + re-rank | Two-stage MRL | Tier 3+ | Embedding sets |
| Enterprise | 500K+ | 50+ | OAuth2 + scopes | nomic-embed (local) | Multilingual hybrid | Tier 3-4 | PKE + schemes |
| Hybrid | Variable | Variable | OAuth2 | Ollama (local) | Hybrid | Tier 2-3 + cloud | Privacy + quality |

### Hardware Quick Reference

| Tier | VRAM | Quality | Cost | Use Case |
|------|------|---------|------|----------|
| 1 | 4-8GB | 75-80% | $300 | Personal KB |
| 2 | 12-16GB | 85-90% | $600 | Team Docs |
| 3 | 24GB | 93-95% | $1500 | AI/RAG, Enterprise |
| 4 | 48GB+ | 95-97% | $4000+ | Large Enterprise |
| 5 | Cloud | 97-99% | $50-200/mo | Hybrid (generation only) |

### Cost Considerations

**Initial Investment:**
- **Personal/Team:** $300-600 (GPU) + minimal server
- **AI/Enterprise:** $1500-4000 (GPU) + $500-2000 (server)
- **Hybrid:** $600-1500 (GPU for embeddings) + $0 (cloud generation)

**Monthly Operating Cost:**
- **Local only:** $20-100 (electricity, assuming 24/7 operation)
- **Cloud generation:** $50-200 (API usage, ~10K requests)
- **Full cloud:** $200-500 (embeddings + generation, no hardware)

**Break-even analysis:**
- Local investment pays off after 6-12 months vs full cloud
- Hybrid optimal for <50K generation requests/month
- Full cloud better for unpredictable usage patterns

### Feature Availability Matrix

| Feature | Personal | Team | AI/RAG | Enterprise | Hybrid |
|---------|----------|------|--------|------------|--------|
| Auto-linking | Yes | Yes | Yes | Yes | Yes |
| Hybrid search | Yes | Yes | Yes | Yes | Yes |
| OAuth2 | Optional | Yes | Optional | Yes | Yes |
| Strict tag filtering | No | Yes | Yes | Yes | Yes |
| PKE encryption | No | Optional | Optional | Yes | Optional |
| Knowledge shards | Optional | Yes | Yes | Yes | Yes |
| Two-stage retrieval | No | No | Yes | Yes | Yes |
| Embedding sets | No | Yes | Yes | Yes | Yes |
| Multilingual FTS | Optional | Optional | Optional | Yes | Yes |
| MRL optimization | Optional | Optional | Yes | Yes | Yes |

---

## Next Steps

1. **Identify your scenario:** Match your requirements to one of the use cases above
2. **Review hardware requirements:** Consult [hardware-planning.md](hardware-planning.md) for detailed specifications
3. **Follow deployment guide:** See [getting-started.md](getting-started.md) for step-by-step instructions
4. **Configure features:** Enable relevant features based on your scenario
5. **Test at scale:** Validate performance with realistic data volumes

For questions or custom deployment scenarios, refer to the [Operators Guide](operators-guide.md) or consult the documentation index.

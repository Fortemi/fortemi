# Embedding Pipeline Architecture and Tuning

This document explains Fortémi's embedding generation pipeline, job queue architecture, configuration options, and performance tuning strategies.

## Overview

The embedding pipeline transforms text content into dense vector representations (embeddings) for semantic search. The pipeline is designed for:

- **Asynchronous processing** - Non-blocking job queue
- **Intelligent chunking** - Document-type aware splitting
- **Multi-stage workflow** - Coordinated job types
- **Configurable models** - Support for multiple embedding backends
- **MRL optimization** - Matryoshka Representation Learning for storage efficiency
- **Auto-embed rules** - Automatic embedding lifecycle management

## Pipeline Architecture

### High-Level Flow

```
┌──────────────┐
│ User creates │
│ or updates   │──────┐
│ note         │      │
└──────────────┘      │
                      ▼
              ┌─────────────────┐
              │ Trigger AI      │
              │ revision job    │──────┐
              └─────────────────┘      │
                                       ▼
                               ┌──────────────────┐
                               │ AI Revision      │
                               │ - Enhance content│
                               │ - Fix formatting │──────┐
                               │ - Add structure  │      │
                               └──────────────────┘      │
                                                          ▼
                                                  ┌──────────────────┐
                                                  │ Embedding Job    │
                                                  │ - Chunk document │
                                                  │ - Generate       │──────┐
                                                  │   embeddings     │      │
                                                  │ - Store vectors  │      │
                                                  └──────────────────┘      │
                                                                             ▼
                                                                     ┌──────────────────┐
                                                                     │ Title Generation │
                                                                     │ - Create         │──────┐
                                                                     │   descriptive    │      │
                                                                     │   title          │      │
                                                                     └──────────────────┘      │
                                                                                                ▼
                                                                                        ┌──────────────────┐
                                                                                        │ Linking Job      │
                                                                                        │ - Find similar   │──────┐
                                                                                        │   notes (>70%)   │      │
                                                                                        │ - Create links   │      │
                                                                                        └──────────────────┘      │
                                                                                                                   ▼
                                                                                                          ┌──────────────────┐
                                                                                                          │ Concept Tagging  │
                                                                                                          │ - Extract topics │
                                                                                                          │ - Apply SKOS     │
                                                                                                          │   concepts       │
                                                                                                          └──────────────────┘
                                                                                                                   │
                                                                                                                   ▼
                                                                                                          ┌──────────────────┐
                                                                                                          │ Ready for search │
                                                                                                          └──────────────────┘
```

### Job Queue System

Fortémi uses a priority-based job queue built on PostgreSQL:

```sql
CREATE TYPE job_type AS ENUM (
    'ai_revision',
    'embedding',
    'title_generation',
    'linking',
    'concept_tagging',
    'context_update',
    'create_embedding_set',
    'refresh_embedding_set',
    'build_set_index',
    'embed_for_set',
    'generate_coarse_embedding',
    'exif_extraction',
    '3d_analysis',
    're_embed_all'
);

CREATE TYPE job_status AS ENUM (
    'pending',
    'running',
    'completed',
    'failed',
    'cancelled'
);

CREATE TABLE job_queue (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    job_type job_type NOT NULL,
    status job_status DEFAULT 'pending',
    priority INTEGER DEFAULT 0,
    payload JSONB,
    result JSONB,
    error_message TEXT,
    progress_percent INTEGER DEFAULT 0,
    progress_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    estimated_duration_ms INTEGER,
    actual_duration_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);
```

### Job Priority System

Jobs are processed in priority order:

| Job Type | Default Priority | Reasoning |
|----------|-----------------|-----------|
| `ai_revision` | 100 | First step, blocks subsequent jobs |
| `embedding` | 80 | Core functionality, needed for search |
| `title_generation` | 70 | Enhances UX, less critical |
| `linking` | 60 | Background processing |
| `concept_tagging` | 50 | Optional enrichment |
| `build_set_index` | 40 | Periodic maintenance |
| `re_embed_all` | 10 | Batch operations, low priority |

Custom priorities can override defaults:

```rust
// Queue high-priority embedding job
jobs.queue(
    Some(note_id),
    JobType::Embedding,
    150,  // High priority
    Some(json!({ "embedding_set_id": set_id }))
).await?;
```

### Job Deduplication

The queue prevents duplicate pending jobs:

```rust
// Only creates job if none exists for this note+type
let job_id = jobs.queue_deduplicated(
    Some(note_id),
    JobType::Embedding,
    80,
    None
).await?;

if job_id.is_none() {
    // Job already queued
}
```

This prevents redundant work when a note is updated multiple times before processing.

## Embedding Job Deep Dive

### Step 1: Document Type Detection

The embedding job begins by detecting document type:

```rust
// Simplified example
let doc_type = detect_document_type(&note).await?;

// Detection hierarchy:
// 1. Explicit note.document_type_id
// 2. Filename pattern matching
// 3. MIME type detection
// 4. Magic byte sniffing
// 5. Content analysis
// 6. Fallback to 'plain_text'
```

**Example detection:**

```
note.title = "neural_networks.md"
  → Pattern: *.md
  → Document type: markdown
  → Chunking strategy: semantic
  → Chunk size: 1000
  → Chunk overlap: 100
```

### Step 2: Chunking

The document is split using the configured strategy:

```rust
use matric_db::chunking::{Chunker, SemanticChunker, ChunkerConfig};

let config = ChunkerConfig {
    max_chunk_size: doc_type.chunk_size_default.unwrap_or(1000),
    min_chunk_size: doc_type.chunk_size_default.unwrap_or(1000) / 10,
    overlap: doc_type.chunk_overlap_default.unwrap_or(100),
};

let chunker: Box<dyn Chunker> = match doc_type.chunking_strategy {
    ChunkingStrategy::Semantic => Box::new(SemanticChunker::new(config)),
    ChunkingStrategy::Syntactic => {
        Box::new(TreeSitterChunker::new(config, doc_type.tree_sitter_language))
    },
    ChunkingStrategy::Paragraph => Box::new(ParagraphChunker::new(config)),
    ChunkingStrategy::Sentence => Box::new(SentenceChunker::new(config)),
    ChunkingStrategy::SlidingWindow => Box::new(SlidingWindowChunker::new(config)),
    ChunkingStrategy::Recursive => Box::new(RecursiveChunker::new(config)),
};

let chunks = chunker.chunk(&note.content);
```

**Example output:**

```rust
// Document: 5000 char markdown with 3 sections
chunks = vec![
    Chunk {
        text: "# Introduction\n\nNeural networks are...",
        start_offset: 0,
        end_offset: 1200,
        metadata: { "type": "heading" }
    },
    Chunk {
        text: "## Architecture\n\nLayers consist of...",
        start_offset: 1100,  // 100 char overlap
        end_offset: 2400,
        metadata: { "type": "heading" }
    },
    Chunk {
        text: "## Training\n\nBackpropagation...",
        start_offset: 2300,
        end_offset: 3600,
        metadata: { "type": "heading" }
    }
];
```

### Step 3: Embedding Generation

Each chunk is embedded using the configured model:

```rust
use matric_inference::EmbeddingService;

// Get embedding model configuration
let embedding_set = get_embedding_set_config(note_id).await?;
let model = embedding_set.embedding_model; // e.g., "nomic-embed-text"

// Get model profile for prefix handling
let registry = EmbeddingModelRegistry::new();
let profile = registry.get_or_default(&model);

// Generate embeddings for all chunks
for chunk in chunks {
    // Apply model-specific prefix (for E5 models: "passage: ")
    let prefixed_text = profile.prefix_passage(&chunk.text);

    // Call Ollama embedding API
    let embedding = embedding_service
        .embed(&model, &prefixed_text)
        .await?;

    // Store chunk + embedding
    store_chunk_embedding(note_id, chunk, embedding).await?;
}
```

**E5 Model Prefix Example:**

```rust
// Input chunk
text = "Neural networks are computing systems..."

// E5 model requires "passage:" prefix
profile.passage_prefix = Some("passage: ");
prefixed_text = "passage: Neural networks are computing systems..."

// Embedding generation
embedding = embed_model.encode(prefixed_text)
  → vector(768) = [0.023, -0.145, 0.087, ...]
```

See [Embedding Model Selection](./embedding-model-selection.md) for model comparison.

### Step 4: Chunk Chain Creation

Metadata linking all chunks is stored:

```rust
// Create chain record
let chain = NoteChunkChain {
    chain_id: note_id,
    note_id,
    original_title: note.title.clone(),
    total_chunks: chunks.len() as i32,
    chunking_strategy: doc_type.chunking_strategy.to_string(),
    created_at: Utc::now(),
};

chunk_chain_repo.create(chain).await?;
```

This enables:
- Search deduplication (group chunks by chain)
- Full document reconstruction
- Navigation between chunks

### Step 5: Coarse Embedding (MRL)

For embedding sets with MRL enabled, generate coarse embeddings:

```rust
if embedding_set.mrl_dimensions.is_some() {
    let coarse_dim = embedding_set.mrl_dimensions.unwrap();

    // Truncate full embedding to coarse dimensions
    // E.g., 768 dims → 64 dims
    let coarse_embedding = full_embedding[0..coarse_dim].to_vec();

    queue_job(JobType::GenerateCoarseEmbedding, payload).await?;
}
```

**MRL Two-Stage Retrieval:**

1. **Coarse search** - Fast scan using 64-dim vectors
2. **Rerank** - Precise scoring using full 768-dim vectors

This provides 12× storage savings and 128× compute reduction. See [Embedding Sets](./embedding-sets.md).

## Job Coordination and Dependencies

### Job Chaining

Jobs create follow-up jobs automatically:

```rust
// In AI revision job handler
async fn handle_ai_revision(job: Job) -> Result<()> {
    // 1. Perform AI revision
    let revised_content = ai_service.revise(&note.content).await?;
    note_repo.update_content(note_id, revised_content).await?;

    // 2. Queue embedding job (priority 80)
    job_repo.queue(
        Some(note_id),
        JobType::Embedding,
        80,
        None
    ).await?;

    // 3. Complete this job
    job_repo.complete(job.id, None).await?;

    Ok(())
}

// In embedding job handler
async fn handle_embedding(job: Job) -> Result<()> {
    // 1. Generate embeddings (as described above)
    generate_embeddings(note_id).await?;

    // 2. Queue title generation (priority 70)
    job_repo.queue(
        Some(note_id),
        JobType::TitleGeneration,
        70,
        None
    ).await?;

    // 3. Queue linking job (priority 60)
    job_repo.queue(
        Some(note_id),
        JobType::Linking,
        60,
        None
    ).await?;

    // 4. Complete this job
    job_repo.complete(job.id, None).await?;

    Ok(())
}
```

### Retry Logic

Failed jobs are automatically retried:

```rust
// Job execution wrapper
async fn execute_job(job: Job) -> Result<()> {
    match run_job_handler(job.job_type, &job).await {
        Ok(_) => {
            job_repo.complete(job.id, None).await?;
        }
        Err(e) => {
            // Retry up to max_retries (default: 3)
            if job.retry_count < job.max_retries {
                job_repo.retry(job.id, &e.to_string()).await?;
                // Job status → pending, retry_count += 1
            } else {
                job_repo.fail(job.id, &e.to_string()).await?;
                // Job status → failed
            }
        }
    }
    Ok(())
}
```

**Retry delays:**

```
Attempt 1: Immediate
Attempt 2: 30 seconds delay
Attempt 3: 2 minutes delay
Attempt 4 (final): Marked as failed
```

### Progress Tracking

Long-running jobs report progress:

```rust
// In embedding job
let total_chunks = chunks.len();
for (i, chunk) in chunks.iter().enumerate() {
    let embedding = embed(chunk).await?;
    store_chunk_embedding(chunk, embedding).await?;

    // Update progress
    let percent = ((i + 1) * 100 / total_chunks) as i32;
    job_repo.update_progress(
        job.id,
        percent,
        Some(&format!("Embedded chunk {}/{}", i + 1, total_chunks))
    ).await?;
}
```

Progress visible via API:

```bash
GET /api/v1/jobs/{job_id}

{
  "id": "...",
  "status": "running",
  "progress_percent": 65,
  "progress_message": "Embedded chunk 13/20"
}
```

## Embedding Model Configuration

### Model Selection

Fortémi supports multiple embedding backends:

```rust
pub enum EmbeddingProvider {
    Ollama,      // Local Ollama instance
    OpenAI,      // OpenAI embedding API
    Custom,      // Custom embedding endpoint
}
```

**Configuration via environment:**

```bash
# Use Ollama (default)
export EMBEDDING_PROVIDER=ollama
export OLLAMA_ENDPOINT=http://localhost:11434
export EMBEDDING_MODEL=nomic-embed-text

# Use OpenAI
export EMBEDDING_PROVIDER=openai
export OPENAI_API_KEY=sk-...
export EMBEDDING_MODEL=text-embedding-3-small
```

### Supported Models

See [embedding_models.rs](../crates/matric-inference/src/embedding_models.rs) for full registry.

**Popular choices:**

| Model | Dimensions | Type | Best For |
|-------|------------|------|----------|
| `nomic-embed-text` | 768 | Symmetric | General purpose, long context |
| `e5-base-v2` | 768 | Asymmetric | Balanced quality/speed |
| `e5-large-v2` | 1024 | Asymmetric | High quality retrieval |
| `all-minilm` | 384 | Symmetric | Fast, lightweight |
| `mxbai-embed-large` | 1024 | Symmetric | High quality symmetric |
| `multilingual-e5-base` | 768 | Asymmetric | Multilingual support |

### Asymmetric vs Symmetric Models

**Asymmetric models** (E5, BGE) use different prefixes for queries vs passages:

```rust
// E5 model
let query = "query: What is backpropagation?";
let passage = "passage: Backpropagation is an algorithm...";

// Query and passage embedded separately
query_embedding = embed(query);
passage_embedding = embed(passage);

// Cosine similarity used for matching
similarity = cosine_similarity(query_embedding, passage_embedding);
```

**Symmetric models** (Nomic, MxBAI) use same encoding:

```rust
// Nomic model
let query = "What is backpropagation?";  // No prefix
let passage = "Backpropagation is an algorithm...";  // No prefix

query_embedding = embed(query);
passage_embedding = embed(passage);
```

**When to use each:**

- **Asymmetric**: Search use cases (user query vs document corpus)
- **Symmetric**: Clustering, deduplication, similarity (document vs document)

Fortémi handles prefixes automatically based on model configuration.

### Model Dimension Truncation

For storage efficiency, embeddings can be truncated:

```rust
// Generate full 768-dim embedding
let full_embedding = embed(text).await?;

// Truncate to 384 dims for storage
let truncated = full_embedding[0..384].to_vec();

// Store truncated version
store_embedding(note_id, truncated).await?;
```

**Quality vs size tradeoff:**

| Dimensions | Storage | Recall | Use Case |
|------------|---------|--------|----------|
| 768 (full) | 100% | 100% | Maximum quality |
| 384 (half) | 50% | ~95% | Balanced |
| 192 (quarter) | 25% | ~85% | Fast search, lower recall |
| 64 (MRL coarse) | 8% | N/A (rerank stage) | First-pass filtering |

## Embedding Sets and Isolation

### Embedding Set Architecture

Embedding sets provide isolated search contexts:

```sql
CREATE TABLE embedding_set (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    chunking_config JSONB,
    auto_embed_rules JSONB,
    mrl_dimensions INTEGER,  -- Matryoshka coarse dims
    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ
);
```

**Two types of sets:**

1. **Filter Sets** - Share embeddings, filter by metadata
2. **Full Sets** - Independent embeddings, separate indexes

### Filter Sets (Default)

Most efficient for tag-based isolation:

```rust
let set = EmbeddingSet {
    name: "research_papers",
    embedding_model: "nomic-embed-text",  // Same as default
    embedding_dimensions: 768,
    auto_embed_rules: json!({
        "tag_filter": {
            "required_tags": ["research", "paper"]
        }
    }),
    mrl_dimensions: Some(64),
};
```

**Search query:**

```sql
-- Filter set applies tag constraints to default embeddings
SELECT n.id, n.title, 1 - (e.embedding <=> $1) as score
FROM note_embedding e
JOIN note n ON e.note_id = n.id
JOIN note_tag nt ON n.id = nt.note_id
WHERE nt.tag_name IN ('research', 'paper')
  AND 1 - (e.embedding <=> $1) > 0.5
ORDER BY score DESC
LIMIT 20;
```

No duplicate embeddings stored.

### Full Sets

Independent embeddings for different models or configurations:

```rust
let set = EmbeddingSet {
    name: "code_search",
    embedding_model: "e5-base-v2",  // Different from default
    embedding_dimensions: 768,
    chunking_config: Some(json!({
        "strategy": "syntactic",
        "max_chunk_size": 500,
        "tree_sitter_language": "rust"
    })),
    mrl_dimensions: Some(64),
};
```

**Separate storage:**

```sql
CREATE TABLE embedding_set_note (
    id UUID PRIMARY KEY,
    embedding_set_id UUID REFERENCES embedding_set(id),
    note_id UUID,
    embedding vector(768),
    coarse_embedding vector(64)  -- MRL
);
```

Each note embedded multiple times (once per full set it belongs to).

### Auto-Embed Rules

Automatically embed notes matching criteria:

```rust
let auto_embed_rules = json!({
    "tag_filter": {
        "required_tags": ["code"],
        "excluded_tags": ["archive"]
    },
    "created_after": "2024-01-01T00:00:00Z",
    "min_content_length": 100
});
```

On note create/update, check rules:

```rust
async fn on_note_updated(note: &Note) {
    for set in embedding_sets {
        if set.auto_embed_rules.matches(note) {
            queue_job(JobType::EmbedForSet, json!({
                "note_id": note.id,
                "embedding_set_id": set.id
            })).await?;
        }
    }
}
```

## Performance Tuning

### Chunking Optimization

**Goal:** Minimize chunks while preserving searchability

**Metrics:**
- Chunks per document
- Embedding generation time
- Search recall
- Storage overhead

**Tuning process:**

1. **Measure baseline:**
```bash
# Check average chunks per document
SELECT
    AVG(total_chunks) as avg_chunks,
    MIN(total_chunks) as min_chunks,
    MAX(total_chunks) as max_chunks,
    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY total_chunks) as median_chunks
FROM note_chunk_chain;
```

2. **Identify outliers:**
```sql
-- Documents with excessive chunks
SELECT
    ncc.chain_id,
    n.title,
    ncc.total_chunks,
    dt.name as doc_type,
    dt.chunk_size_default
FROM note_chunk_chain ncc
JOIN note n ON ncc.chain_id = n.id
JOIN document_type dt ON n.document_type_id = dt.id
WHERE ncc.total_chunks > 50
ORDER BY ncc.total_chunks DESC
LIMIT 20;
```

3. **Adjust chunk size:**
```sql
-- Increase chunk size for markdown
UPDATE document_type
SET chunk_size_default = 1500,
    chunk_overlap_default = 150
WHERE name = 'markdown';

-- Re-embed affected documents
INSERT INTO job_queue (id, note_id, job_type, priority)
SELECT gen_random_uuid(), n.id, 'embedding'::job_type, 80
FROM note n
JOIN document_type dt ON n.document_type_id = dt.id
WHERE dt.name = 'markdown';
```

4. **Validate recall:**
```bash
# Test search recall before/after change
# Ensure changes don't hurt search quality
```

### Embedding Model Optimization

**Goal:** Balance quality, speed, and storage

**Considerations:**

| Factor | Small Model (384d) | Large Model (1024d) |
|--------|-------------------|---------------------|
| Quality | Good (0.75 NDCG) | Excellent (0.82 NDCG) |
| Speed | 50 ms/chunk | 150 ms/chunk |
| Storage | 1.5 KB/chunk | 4 KB/chunk |
| Best For | High-volume, speed-critical | Quality-critical, smaller corpus |

**Switching models:**

```bash
# Update embedding set configuration
curl -X PATCH /api/v1/embedding-sets/default \
  -d '{
    "embedding_model": "e5-large-v2",
    "embedding_dimensions": 1024
  }'

# Re-embed all notes
curl -X POST /api/v1/jobs/re-embed-all
```

**Testing workflow:**

1. Create test embedding set with new model
2. Embed sample documents (1000+ notes)
3. Run benchmark queries
4. Compare recall@20, NDCG, latency
5. Decide to switch or keep existing model

### Job Queue Tuning

**Goal:** Maximize throughput, minimize latency

**Metrics:**
- Queue depth (pending jobs)
- Average processing time
- Job failure rate
- P50/P95/P99 latency

**Monitoring:**

```sql
-- Queue statistics
SELECT
    job_type,
    COUNT(*) FILTER (WHERE status = 'pending') as pending,
    COUNT(*) FILTER (WHERE status = 'running') as running,
    COUNT(*) FILTER (WHERE status = 'failed') as failed,
    AVG(actual_duration_ms) FILTER (WHERE status = 'completed') as avg_duration_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (
        ORDER BY actual_duration_ms
    ) FILTER (WHERE status = 'completed') as p95_duration_ms
FROM job_queue
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY job_type
ORDER BY pending DESC;
```

**Tuning knobs:**

1. **Worker concurrency:**
```bash
# In matric-jobs worker
export JOB_WORKER_CONCURRENCY=4  # Parallel job processing
```

2. **Batch processing:**
```rust
// Process multiple embeddings in single batch
let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
let embeddings = embedding_service.embed_batch(&texts).await?;
```

3. **Priority adjustment:**
```rust
// Lower priority for bulk operations
jobs.queue(note_id, JobType::ReEmbedAll, 5, None).await?;
```

### Index Optimization

**HNSW index tuning:**

```sql
-- Create HNSW index with tuned parameters
CREATE INDEX note_embedding_hnsw_idx
  ON note_embedding
  USING hnsw (embedding vector_cosine_ops)
  WITH (m = 16, ef_construction = 64);
```

**Parameters:**

| Parameter | Value | Effect |
|-----------|-------|--------|
| `m` | 16 | Connections per layer (higher = better recall, slower build) |
| `ef_construction` | 64 | Build quality (higher = better index, slower build) |
| `ef_search` | Dynamic | Query quality (set per search, not index creation) |

**Adaptive `ef_search`:**

```rust
// matric-search automatically adjusts based on corpus size
let ef = match corpus_size {
    n if n < 1_000 => 40,
    n if n < 10_000 => 80,
    n if n < 100_000 => 120,
    _ => 160,
};
```

See [Search Guide](./search-guide.md) for HNSW tuning details.

## Monitoring and Debugging

### Job Queue Metrics

**Key metrics to track:**

1. **Queue depth** - Pending jobs over time
2. **Processing rate** - Jobs/minute completed
3. **Failure rate** - % of jobs that fail
4. **Duration distribution** - P50, P95, P99 latency

**Monitoring queries:**

```sql
-- Real-time queue depth
SELECT
    job_type::text,
    COUNT(*) as pending_jobs,
    MIN(created_at) as oldest_job
FROM job_queue
WHERE status = 'pending'
GROUP BY job_type
ORDER BY pending_jobs DESC;

-- Recent failures
SELECT
    job_type::text,
    error_message,
    retry_count,
    created_at
FROM job_queue
WHERE status = 'failed'
  AND completed_at > NOW() - INTERVAL '1 hour'
ORDER BY completed_at DESC
LIMIT 20;

-- Processing throughput
SELECT
    DATE_TRUNC('minute', completed_at) as minute,
    job_type::text,
    COUNT(*) as jobs_completed,
    AVG(actual_duration_ms) as avg_duration
FROM job_queue
WHERE status = 'completed'
  AND completed_at > NOW() - INTERVAL '1 hour'
GROUP BY minute, job_type
ORDER BY minute DESC, job_type;
```

### Embedding Quality Metrics

**Measuring embedding quality:**

1. **Search recall** - % of relevant docs in top-K
2. **NDCG (Normalized Discounted Cumulative Gain)** - Ranking quality
3. **Cosine similarity distribution** - Embedding separability

**Example evaluation:**

```python
# Pseudo-code for embedding quality test
test_queries = load_test_queries()  # Known query-document pairs
results = []

for query, relevant_docs in test_queries:
    search_results = search(query, limit=20)

    # Calculate recall@20
    found = [doc for doc in search_results if doc in relevant_docs]
    recall = len(found) / len(relevant_docs)
    results.append(recall)

avg_recall = sum(results) / len(results)
print(f"Recall@20: {avg_recall:.3f}")
```

### Debugging Failed Jobs

**Common failure causes:**

1. **Ollama timeout** - Embedding service unreachable
2. **OOM (Out of Memory)** - Document too large
3. **Invalid content** - Malformed text
4. **Database constraint violation** - Orphaned references

**Debugging workflow:**

```bash
# 1. Get failed job details
curl /api/v1/jobs/{job_id}

# 2. Check error message
# "Ollama timeout after 30s" → Increase timeout or check Ollama health
# "OOM: Cannot allocate memory" → Reduce chunk size or add memory
# "Foreign key violation" → Check referential integrity

# 3. Retry manually (after fixing root cause)
curl -X POST /api/v1/jobs/{job_id}/retry

# 4. Monitor retry
curl /api/v1/jobs/{job_id}
```

**Ollama health check:**

```bash
# Test Ollama connection
curl http://localhost:11434/api/embeddings \
  -d '{
    "model": "nomic-embed-text",
    "prompt": "test"
  }'

# Check Ollama logs
docker logs ollama

# Restart Ollama if needed
docker restart ollama
```

### Performance Bottleneck Analysis

**Identify bottlenecks:**

```sql
-- Jobs spending most time in queue
SELECT
    job_type::text,
    AVG(EXTRACT(EPOCH FROM (started_at - created_at))) as avg_queue_time_sec,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_processing_time_sec,
    COUNT(*) as jobs
FROM job_queue
WHERE status = 'completed'
  AND completed_at > NOW() - INTERVAL '1 hour'
GROUP BY job_type
ORDER BY avg_queue_time_sec DESC;
```

**Bottleneck solutions:**

| Symptom | Bottleneck | Solution |
|---------|-----------|----------|
| High queue time | Too few workers | Increase `JOB_WORKER_CONCURRENCY` |
| High processing time (embedding) | Slow Ollama | Add GPU, increase Ollama concurrency |
| High processing time (AI revision) | Slow LLM | Use faster model, increase timeout |
| Memory errors | Large documents | Reduce chunk size, add memory |
| Database locks | Too many writes | Batch inserts, increase connection pool |

## Configuration Reference

### Environment Variables

```bash
# Embedding provider
EMBEDDING_PROVIDER=ollama          # ollama | openai | custom
OLLAMA_ENDPOINT=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text

# Job queue
JOB_WORKER_CONCURRENCY=4           # Parallel workers
JOB_WORKER_POLL_INTERVAL_MS=1000   # Queue polling rate
JOB_MAX_RETRIES=3                  # Retry failed jobs
JOB_TIMEOUT_MS=300000              # 5 minute timeout

# Chunking defaults
DEFAULT_CHUNK_SIZE=1000
DEFAULT_CHUNK_OVERLAP=100
DEFAULT_CHUNKING_STRATEGY=semantic

# MRL (Matryoshka)
MRL_COARSE_DIMENSIONS=64           # Coarse embedding size
MRL_ENABLED=true                   # Enable two-stage retrieval
```

### Document Type Configuration

```sql
-- View document type settings
SELECT
    name,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    tree_sitter_language
FROM document_type
WHERE is_active = TRUE
ORDER BY category, name;

-- Update chunking for specific type
UPDATE document_type
SET chunking_strategy = 'semantic',
    chunk_size_default = 1500,
    chunk_overlap_default = 200
WHERE name = 'markdown';
```

### Embedding Set Configuration

```bash
# List embedding sets
curl /api/v1/embedding-sets

# Create new embedding set
curl -X POST /api/v1/embedding-sets \
  -d '{
    "name": "research_papers",
    "embedding_model": "e5-large-v2",
    "embedding_dimensions": 1024,
    "auto_embed_rules": {
      "tag_filter": {
        "required_tags": ["research"]
      }
    },
    "mrl_dimensions": 64
  }'

# Update embedding set
curl -X PATCH /api/v1/embedding-sets/{id} \
  -d '{
    "embedding_model": "nomic-embed-text"
  }'
```

## Best Practices

### Chunking Strategy Selection

**Guidelines:**

1. **Code** - Use syntactic chunking with Tree-sitter
   - Preserves function/class boundaries
   - Better semantic coherence
   - Example: `chunking_strategy: syntactic`, `tree_sitter_language: rust`

2. **Markdown/Documentation** - Use semantic chunking
   - Respects heading hierarchy
   - Keeps sections together
   - Example: `chunking_strategy: semantic`, `chunk_size: 1500`

3. **Prose/Narrative** - Use paragraph or sentence chunking
   - Natural language boundaries
   - Good for long-form content
   - Example: `chunking_strategy: paragraph`, `chunk_size: 1200`

4. **Structured Data** - Use sliding window
   - Consistent chunk sizes
   - Good for CSV, logs, etc.
   - Example: `chunking_strategy: sliding_window`, `chunk_size: 1000`, `overlap: 100`

### Embedding Model Selection

**Decision matrix:**

| Requirement | Recommended Model | Reasoning |
|-------------|------------------|-----------|
| Long context (>2048 tokens) | `nomic-embed-text` | 8192 token limit |
| Multilingual | `multilingual-e5-base` | 100+ languages |
| Speed critical | `all-minilm` | Smallest, fastest (384d) |
| Quality critical | `e5-large-v2` | Best retrieval quality (1024d) |
| Balanced | `e5-base-v2` | Good quality/speed tradeoff (768d) |

### Job Queue Best Practices

**Do:**
- Use deduplication for frequently-updated notes
- Set appropriate priorities for different job types
- Monitor queue depth and processing times
- Retry failed jobs with exponential backoff

**Don't:**
- Queue duplicate jobs manually (use `queue_deduplicated`)
- Set all jobs to highest priority (defeats prioritization)
- Ignore failed jobs (investigate and fix root cause)
- Process jobs synchronously in request handlers (always async)

### Monitoring Best Practices

**Essential dashboards:**

1. **Queue health** - Pending jobs by type, oldest job age
2. **Processing rate** - Jobs/minute, by type
3. **Failure rate** - Failed jobs, error breakdown
4. **Latency** - P50/P95/P99 job duration
5. **Embedding quality** - Search recall, NDCG

**Alerting thresholds:**

- Queue depth > 1000 jobs
- Failure rate > 5%
- P95 latency > 5 minutes
- Oldest pending job > 30 minutes

## Related Documentation

- [Embedding Model Selection](./embedding-model-selection.md) - Model comparison and selection guide
- [Embedding Sets](./embedding-sets.md) - Isolated search contexts
- [Chunking Guide](./chunking.md) - Chunking strategies and configuration
- [Chunking Workflow](./chunking-workflow.md) - Document chunking and deduplication
- [Search Guide](./search-guide.md) - Search modes and optimization
- [Document Types Guide](./document-types-guide.md) - Document type registry

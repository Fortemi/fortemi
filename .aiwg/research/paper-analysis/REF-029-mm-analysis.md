# REF-029: Dense Passage Retrieval - matric-memory Analysis

**Paper:** Karpukhin, V., et al. (2020). Dense Passage Retrieval for Open-Domain Question Answering.

**Analysis Date:** 2026-01-25
**Relevance:** Critical - Semantic search architecture

---

## Implementation Mapping

| DPR Concept | matric-memory Implementation | Location |
|-------------|------------------------------|----------|
| Dual encoder | Query & passage embedding via Ollama | `crates/matric-inference/src/ollama.rs` |
| Passage encoder | nomic-embed-text model | `matric-inference` config |
| Query encoder | Same model (shared) | `matric-inference` config |
| FAISS index | pgvector HNSW index | PostgreSQL + pgvector |
| Dot product similarity | Cosine similarity (equivalent for normalized) | `crates/matric-db/src/embeddings.rs` |
| In-batch negatives | N/A (no training) | Pre-trained model used |
| Hard negatives | N/A (no training) | Pre-trained model used |

---

## matric-memory Semantic Search Architecture

### The Vocabulary Mismatch Problem

BM25 requires exact lexical overlap:

```
Query: "How do I fix database connectivity issues?"
Note: "Troubleshooting PostgreSQL connection failures"

BM25 match: Only "database" ↔ (no direct match)
Problem: User intent and note content are aligned, but words differ
```

Dense retrieval solves this by embedding both into a shared semantic space.

### Dual-Encoder Architecture in matric-memory

```
┌─────────────────────────────────────────────────────────────┐
│                    Indexing (Offline)                        │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Note Content                                                │
│  "Troubleshooting PostgreSQL connection failures..."         │
│                            │                                 │
│                            ▼                                 │
│  ┌──────────────────────────────────────┐                   │
│  │  Passage Encoder (nomic-embed-text)  │                   │
│  │  via Ollama                          │                   │
│  └──────────────────────────────────────┘                   │
│                            │                                 │
│                            ▼                                 │
│  Embedding: [0.023, -0.156, 0.089, ...] (768 dims)          │
│                            │                                 │
│                            ▼                                 │
│  ┌──────────────────────────────────────┐                   │
│  │  pgvector HNSW Index                 │                   │
│  └──────────────────────────────────────┘                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    Search (Online)                           │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Query: "database connectivity issues"                       │
│                            │                                 │
│                            ▼                                 │
│  ┌──────────────────────────────────────┐                   │
│  │  Query Encoder (same model)          │                   │
│  └──────────────────────────────────────┘                   │
│                            │                                 │
│                            ▼                                 │
│  Query Embedding: [0.019, -0.142, 0.095, ...] (768 dims)    │
│                            │                                 │
│                            ▼                                 │
│  ┌──────────────────────────────────────┐                   │
│  │  Approximate Nearest Neighbor        │                   │
│  │  (pgvector <=> operator)             │                   │
│  └──────────────────────────────────────┘                   │
│                            │                                 │
│                            ▼                                 │
│  Results: Notes with similar embeddings                      │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Details

```rust
// crates/matric-inference/src/ollama.rs

/// Embed text using Ollama's nomic-embed-text model
/// Following DPR dual-encoder pattern (REF-029)
pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
    let response = self.client
        .post(&format!("{}/api/embeddings", self.base_url))
        .json(&EmbedRequest {
            model: "nomic-embed-text".to_string(),
            prompt: text.to_string(),
        })
        .send()
        .await?;

    let result: EmbedResponse = response.json().await?;
    Ok(result.embedding)  // 768-dimensional vector
}

// crates/matric-db/src/embeddings.rs

/// Store embedding in pgvector
pub async fn store_embedding(
    pool: &PgPool,
    note_id: Uuid,
    chunk_index: i32,
    embedding: &[f32],
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO note_embeddings (note_id, chunk_index, embedding)
        VALUES ($1, $2, $3::vector)
        ON CONFLICT (note_id, chunk_index)
        DO UPDATE SET embedding = EXCLUDED.embedding
        "#,
        note_id,
        chunk_index,
        embedding as &[f32]
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Semantic search using cosine similarity
/// REF-029 uses dot product, but for normalized vectors these are equivalent
pub async fn semantic_search(
    pool: &PgPool,
    query_embedding: &[f32],
    limit: i32,
) -> Result<Vec<SearchResult>> {
    sqlx::query_as!(
        SearchResult,
        r#"
        SELECT
            note_id,
            1 - (embedding <=> $1::vector) as score  -- Cosine similarity
        FROM note_embeddings
        WHERE note_id IN (SELECT id FROM notes WHERE deleted_at IS NULL)
        ORDER BY embedding <=> $1::vector
        LIMIT $2
        "#,
        query_embedding as &[f32],
        limit
    )
    .fetch_all(pool)
    .await
}
```

---

## Benefits Mirroring DPR Research Findings

### 1. Semantic Matching Beyond Lexical Overlap

**Paper Finding:**
> "DPR achieves 9-19 percentage points higher top-20 retrieval accuracy compared to BM25." (Table 2)

| Dataset | BM25 Top-20 | DPR Top-20 | Improvement |
|---------|-------------|------------|-------------|
| Natural Questions | 59.1 | 78.4 | +32.7% |
| TriviaQA | 66.9 | 79.4 | +18.7% |
| WebQuestions | 55.0 | 63.2 | +14.9% |

**matric-memory Benefit:**
- "How do I connect to the database?" matches "PostgreSQL configuration guide"
- Synonyms handled: "authenticate" ↔ "log in" ↔ "sign in"
- Paraphrases matched without explicit rules

### 2. Efficient Batch Indexing

**Paper Finding:**
> "The passage encoder processes documents offline, enabling efficient index construction." (Section 3)

**matric-memory Benefit:**
- Notes embedded once at creation/update
- No real-time passage encoding during search
- Query encoding is single-vector, fast operation

### 3. Decoupled Query and Passage Processing

**Paper Finding:**
> "Separating query and passage encoders allows for different representations optimized for each task." (Section 2)

**matric-memory Adaptation:**
- Uses same model for both (nomic-embed-text)
- Simpler deployment, single model to manage
- Asymmetric encoders possible future enhancement

### 4. Approximate Nearest Neighbor Compatibility

**Paper Finding:**
> "Dense representations enable sub-linear time retrieval via FAISS or similar indexes." (Section 3)

**matric-memory Benefit:**
- pgvector HNSW provides O(log N) query time
- Scales to millions of note chunks
- No full collection scan required

---

## Comparison: DPR Paper vs matric-memory

| Aspect | DPR Paper | matric-memory |
|--------|-----------|---------------|
| Encoder | BERT-base (110M params) | nomic-embed-text (137M params) |
| Dimension | 768 | 768 |
| Similarity | Dot product | Cosine (equivalent for normalized) |
| Index | FAISS HNSW | pgvector HNSW |
| Training | In-batch + hard negatives | Pre-trained (no fine-tuning) |
| Query encoder | Separate fine-tuned | Shared with passage |
| Passages | 100-word Wikipedia chunks | Variable note chunks |

### Why These Differences?

**Shared encoder:** DPR trains separate encoders for queries vs passages. matric-memory uses a pre-trained model that works for both, trading theoretical optimal for simplicity.

**Cosine vs dot product:** For L2-normalized vectors (as nomic-embed-text produces), these are equivalent rank orderings. Cosine is more interpretable (0-1 similarity scale).

**pgvector vs FAISS:** PostgreSQL integration means one database for everything. Acceptable performance for knowledge base scale (<1M notes).

---

## DPR Training Insights (Informing Future Work)

### In-Batch Negatives

**Paper Finding:**
> "Using other passages in the batch as negatives provides effective training signal with no extra cost." (Section 3.2)

**Diagram:**
```
Batch of 32 (query, positive_passage) pairs:
- Query 1 → Passage 1 (positive)
          → Passage 2-32 (negatives, from other queries)
- Query 2 → Passage 2 (positive)
          → Passage 1, 3-32 (negatives)
...
```

**matric-memory Relevance:**
If fine-tuning domain-specific model, use in-batch negatives for efficient training.

### Hard Negatives

**Paper Finding:**
> "Including BM25 top-k passages that aren't the answer as hard negatives improves accuracy by 2-4%." (Table 4)

**matric-memory Relevance:**
For future fine-tuning:
1. Run BM25 search on user queries
2. Find passages ranked highly by BM25 but not clicked
3. Use as hard negatives in contrastive training

---

## Cross-References

### Related Papers

| Paper | Relationship to DPR |
|-------|---------------------|
| REF-027 (RRF) | Fuses DPR results with BM25 |
| REF-030 (SBERT) | Alternative embedding approach |
| REF-031 (HNSW) | Index structure for DPR vectors |
| REF-056 (ColBERT) | Alternative dense retrieval |
| REF-057 (Contriever) | Unsupervised DPR training |
| REF-058 (E5) | Alternative embedding model |

### Related Code Locations

| File | DPR Concept |
|------|-------------|
| `crates/matric-inference/src/ollama.rs` | Embedding generation |
| `crates/matric-db/src/embeddings.rs` | Vector storage and search |
| `crates/matric-jobs/src/embedding.rs` | Async embedding job |
| `crates/matric-search/src/hybrid.rs` | Semantic results into fusion |

---

## Improvement Opportunities

### 1. Separate Query Encoder

Deploy query-specific model for asymmetric search:

```rust
pub struct DualEncoder {
    query_model: String,     // e.g., "nomic-embed-text-query"
    passage_model: String,   // e.g., "nomic-embed-text-passage"
}

impl DualEncoder {
    pub async fn embed_query(&self, query: &str) -> Vec<f32> {
        self.ollama.embed(&self.query_model, query).await
    }

    pub async fn embed_passage(&self, passage: &str) -> Vec<f32> {
        self.ollama.embed(&self.passage_model, passage).await
    }
}
```

### 2. Query Augmentation

Expand short queries before embedding:

```rust
pub async fn augment_query(query: &str) -> String {
    // Short queries benefit from context
    if query.split_whitespace().count() < 5 {
        format!("Find notes about: {}", query)
    } else {
        query.to_string()
    }
}
```

### 3. Passage Chunking Strategy

DPR uses 100-word chunks. Experiment with:

```rust
pub enum ChunkStrategy {
    FixedTokens(usize),      // 100-200 tokens
    Sentences(usize),         // 3-5 sentences
    Paragraphs,               // Natural boundaries
    Sliding { size: usize, overlap: usize },  // Overlapping windows
}
```

### 4. Multi-Vector Retrieval

Store multiple embeddings per note:

```rust
// Current: Single embedding (or per-chunk)
// Enhanced: Title + summary + content embeddings

pub struct NoteEmbeddings {
    title_embedding: Vec<f32>,
    summary_embedding: Vec<f32>,
    content_embeddings: Vec<Vec<f32>>,  // Per chunk
}
```

### 5. Domain Fine-Tuning

Following DPR training recipe:

```python
# Pseudo-code for future fine-tuning
from transformers import DPRQuestionEncoder, DPRContextEncoder

# Collect training data from user interactions
positive_pairs = [
    (query, clicked_note) for query, clicked_note in search_logs
]

# Train with in-batch negatives
train_dpr(
    question_encoder=DPRQuestionEncoder.from_pretrained("..."),
    context_encoder=DPRContextEncoder.from_pretrained("..."),
    train_data=positive_pairs,
    batch_size=32,  # In-batch negatives
    hard_negatives=bm25_top_k_not_clicked
)
```

---

## Critical Insights for matric-memory Development

### 1. Dense Retrieval Complements, Doesn't Replace BM25

> "While DPR outperforms BM25 on average, BM25 still wins on some entity-heavy queries." (Section 5.2)

**Implication:** Hybrid search is correct. Don't go semantic-only.

### 2. Embedding Quality is Paramount

> "The choice of pre-training objective significantly impacts retrieval quality." (Section 4)

**Implication:** Model selection (nomic-embed-text vs alternatives) matters more than index tuning.

### 3. Passage Length Affects Retrieval

> "Shorter passages are more precise but may lack context; longer passages have more recall but less precision." (Section 3.1)

**Implication:** matric-memory's note-based retrieval differs from 100-word chunks. Consider chunking strategy for long notes.

### 4. Negative Sampling is Critical for Training

> "Random negatives provide weak signal; BM25 negatives that look relevant but aren't are most informative." (Section 3.2)

**Implication:** If ever fine-tuning, hard negatives from BM25 are essential.

---

## Key Quotes Relevant to matric-memory

> "Dense representations allow semantically similar text to be close in vector space, enabling retrieval without lexical overlap." (Section 1)
>
> **Relevance:** Core justification for semantic search in matric-memory.

> "The dual-encoder architecture enables efficient retrieval: passages encoded offline, queries encoded at search time." (Section 2)
>
> **Relevance:** matric-memory's architecture directly follows this pattern.

> "FAISS enables sub-linear time retrieval over millions of passages." (Section 3)
>
> **Relevance:** pgvector HNSW provides equivalent capability within PostgreSQL.

> "Combining dense and sparse retrieval often outperforms either alone." (Section 5.3)
>
> **Relevance:** Validates matric-memory's hybrid search architecture.

---

## Summary

REF-029 provides the blueprint for matric-memory's semantic search. The dual-encoder architecture—embedding passages offline and queries at search time—enables efficient semantic retrieval that captures meaning beyond keyword matching. Combined with BM25 via RRF, this creates a robust hybrid search system.

**Implementation Status:** Complete
**Model:** nomic-embed-text (768-dim)
**Index:** pgvector HNSW
**Test Coverage:** Semantic search tests verify similarity rankings
**Future Work:** Query augmentation, chunking strategy optimization, potential fine-tuning

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |

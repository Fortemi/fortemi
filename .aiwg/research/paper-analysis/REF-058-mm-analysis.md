# REF-058: E5 Text Embeddings - matric-memory Analysis

**Paper:** Wang, L., et al. (2022). Text Embeddings by Weakly-Supervised Contrastive Pre-training.

**Analysis Date:** 2026-01-25
**Relevance:** Future Enhancement - Potential embedding model upgrade

---

## Implementation Mapping (Proposed)

| E5 Concept | Proposed matric-memory Implementation | Location |
|------------|---------------------------------------|----------|
| Query prefix | "query: " prepended to searches | `crates/matric-inference/src/ollama.rs` |
| Passage prefix | "passage: " prepended to notes | `crates/matric-jobs/src/embedding.rs` |
| Asymmetric embeddings | Different prefixes for query/doc | Embedding pipeline |
| E5-base model | Replace nomic-embed-text | Model configuration |

**Current Status:** Not implemented
**Priority:** Medium (for embedding quality evaluation)

---

## E5 Architecture Overview

### The Weakly-Supervised Approach

E5 uses web-scale text pairs without manual annotation:

```
Training Data: CCPairs (Curated Contrastive Pairs)

Source 1: (title, passage) from web pages
Source 2: (question, answer) from forums
Source 3: (query, clicked result) from search logs

270 Million pairs total
No manual relevance labels
```

### Asymmetric Prefixing

```
E5 Key Innovation: Different prefixes for queries vs passages

Query Embedding:
Input: "query: database connection pooling"
       ↓
Model: E5-base (110M params)
       ↓
Output: [0.023, -0.156, 0.089, ...] (768 dims)

Passage Embedding:
Input: "passage: PgBouncer is a lightweight connection pooler..."
       ↓
Model: E5-base (same model)
       ↓
Output: [0.028, -0.142, 0.095, ...] (768 dims)

Why prefixes matter:
- Queries are short, seeking information
- Passages are long, providing information
- Prefixes help model distinguish roles
```

---

## Proposed matric-memory Integration

### Model Swap Approach

```rust
// crates/matric-inference/src/ollama.rs

pub struct E5Embedder {
    client: OllamaClient,
    model: String,  // "e5-base-v2" or "e5-large-v2"
}

impl E5Embedder {
    /// Embed a search query with E5 prefix
    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let prefixed = format!("query: {}", query);
        self.embed_text(&prefixed).await
    }

    /// Embed a note/passage with E5 prefix
    pub async fn embed_passage(&self, passage: &str) -> Result<Vec<f32>> {
        let prefixed = format!("passage: {}", passage);
        self.embed_text(&prefixed).await
    }

    async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let response = self.client
            .post(&format!("{}/api/embeddings", self.base_url))
            .json(&EmbedRequest {
                model: self.model.clone(),
                prompt: text.to_string(),
            })
            .send()
            .await?;

        let result: EmbedResponse = response.json().await?;
        Ok(result.embedding)
    }
}
```

### Backward Compatibility

Switching models requires re-embedding all notes:

```rust
// Migration plan for model switch

pub enum EmbeddingMigration {
    // Strategy 1: Parallel embeddings
    DualEmbedding {
        old_column: &'static str,  // "embedding_nomic"
        new_column: &'static str,  // "embedding_e5"
    },

    // Strategy 2: Full re-index
    ReindexAll,

    // Strategy 3: Gradual migration
    LazyMigration {
        on_access: bool,  // Re-embed when note accessed
    },
}

pub async fn migrate_to_e5(
    pool: &PgPool,
    strategy: EmbeddingMigration,
) -> Result<MigrationStats> {
    match strategy {
        EmbeddingMigration::ReindexAll => {
            // Background job to re-embed all notes
            let notes = get_all_note_ids(pool).await?;
            for note_id in notes {
                queue_embedding_job(pool, note_id, "e5").await?;
            }
        }
        // ...
    }
}
```

---

## E5 vs Current Model Comparison

### Performance Benchmarks

**Paper Finding:**

| Model | Params | BEIR Avg | MTEB Avg |
|-------|--------|----------|----------|
| BM25 | - | 0.428 | - |
| nomic-embed-text | 137M | ~0.45 | 0.621 |
| E5-base | 110M | 0.462 | 0.643 |
| E5-large | 335M | 0.478 | 0.665 |
| GTR-XXL | 4.8B | 0.458 | 0.634 |

**Key observation:** E5-base outperforms models 40x larger.

### Context Length Comparison

| Model | Max Tokens | matric-memory Implication |
|-------|------------|--------------------------|
| nomic-embed-text | 8192 | Long notes: single chunk |
| E5-base | 512 | Long notes: need chunking |
| E5-large | 512 | Same limitation |

**Trade-off:** E5's 512 token limit means more chunking for long notes.

### Prefix Importance

**Paper Finding:**
> "Using prefixes improves performance by 6.7% on average." (Table 3)

| Configuration | BEIR Avg |
|--------------|----------|
| E5 without prefix | 0.433 |
| E5 with prefix | **0.462** |

**Implication:** Must use prefixes to get full E5 quality.

---

## Benefits for matric-memory

### 1. State-of-the-Art Quality

**Paper Finding:**
> "E5 is the first embedding model to outperform BM25 on BEIR in zero-shot settings." (Abstract)

**matric-memory Benefit:**
- Better semantic matching out-of-the-box
- No fine-tuning required
- Stronger baseline for hybrid search

### 2. Smaller Model, Better Results

**Paper Finding:**
> "E5-base (110M) outperforms GTR-XXL (4.8B) while being 40x smaller." (Table 1)

**matric-memory Benefit:**
- Faster inference on CPU/small GPU
- Lower memory requirements
- Reduced hosting costs

### 3. Asymmetric Search Optimization

**Paper Finding:**
> "Query and passage prefixes help the model distinguish between search and indexing contexts." (Section 3)

**matric-memory Benefit:**
- Queries optimized for retrieval intent
- Passages optimized for content representation
- Better matching for question-like queries

---

## Implementation Considerations

### Context Length Management

E5's 512 token limit requires chunking:

```rust
// Current: nomic-embed-text handles 8192 tokens
// Proposed: E5 requires chunking at 512 tokens

pub struct E5ChunkConfig {
    max_tokens: usize,    // 512
    overlap_tokens: usize, // 50 (for context continuity)
}

pub fn chunk_for_e5(text: &str, config: &E5ChunkConfig) -> Vec<String> {
    let tokens = tokenize(text);
    let mut chunks = Vec::new();

    let mut start = 0;
    while start < tokens.len() {
        let end = (start + config.max_tokens).min(tokens.len());
        chunks.push(tokens[start..end].join(" "));

        // Overlap for next chunk
        start = end - config.overlap_tokens;
        if start >= tokens.len() - config.overlap_tokens {
            break;
        }
    }

    chunks
}
```

### Storage Impact

More chunks = more embeddings:

| Scenario | nomic (8K context) | E5 (512 context) |
|----------|-------------------|------------------|
| 500-word note | 1 embedding | 1 embedding |
| 2000-word note | 1 embedding | 4-5 embeddings |
| 5000-word note | 1 embedding | 10-12 embeddings |

**Mitigation:**
- Store primary + chunk embeddings
- Use chunk-level for long notes, note-level for short

### Ollama Availability

Check E5 availability in Ollama:

```bash
# Check if E5 is available
ollama list | grep e5

# If not, may need alternative:
# 1. Run E5 via sentence-transformers
# 2. Use HuggingFace Inference API
# 3. Self-host with text-embeddings-inference
```

---

## A/B Testing Framework

### Evaluation Protocol

```rust
pub struct ModelEvaluation {
    model: EmbeddingModel,
    test_queries: Vec<TestQuery>,
}

pub struct TestQuery {
    query: String,
    expected_notes: Vec<Uuid>,  // Known relevant
}

pub async fn evaluate_model(
    pool: &PgPool,
    eval: &ModelEvaluation,
) -> EvaluationMetrics {
    let mut recall_at_10 = 0.0;
    let mut mrr = 0.0;

    for test in &eval.test_queries {
        let results = search_with_model(
            pool,
            &test.query,
            &eval.model,
            10
        ).await;

        // Calculate recall@10
        let found = results.iter()
            .filter(|r| test.expected_notes.contains(&r.note_id))
            .count();
        recall_at_10 += found as f32 / test.expected_notes.len() as f32;

        // Calculate MRR
        if let Some(pos) = results.iter()
            .position(|r| test.expected_notes.contains(&r.note_id))
        {
            mrr += 1.0 / (pos + 1) as f32;
        }
    }

    EvaluationMetrics {
        recall_at_10: recall_at_10 / eval.test_queries.len() as f32,
        mrr: mrr / eval.test_queries.len() as f32,
    }
}
```

### Comparison Test Plan

1. **Collect test queries** from search logs (anonymized)
2. **Manual relevance labels** for top results
3. **Embed with both models**
4. **Compare metrics** on held-out set
5. **Decision threshold:** Switch if E5 shows >5% improvement

---

## Cross-References

### Related Papers

| Paper | Relationship to E5 |
|-------|-------------------|
| REF-029 (DPR) | Supervised baseline E5 improves |
| REF-030 (SBERT) | Training methodology inspiration |
| REF-057 (Contriever) | Unsupervised alternative |

### Planned Code Locations

| File | E5 Usage |
|------|----------|
| `crates/matric-inference/src/e5.rs` | E5 embedder |
| `crates/matric-jobs/src/embedding.rs` | Prefix handling |
| `scripts/eval_embedding_models.py` | A/B evaluation |
| `docs/embedding-models.md` | Configuration guide |

---

## Decision Framework

### Should matric-memory Switch to E5?

**Factors favoring E5:**
- Benchmarks show quality improvement
- Smaller model = faster inference
- Well-supported in ecosystem

**Factors against E5:**
- 512 token limit (vs nomic's 8192)
- More chunking = more storage
- Re-embedding required for all notes
- nomic-embed-text may already be sufficient

**Recommended approach:**
1. Run A/B evaluation on real queries
2. If E5 shows >5% improvement, plan migration
3. If marginal, stay with nomic-embed-text

---

## Migration Checklist

If decision is to migrate:

- [ ] Add E5 model to Ollama (or alternative host)
- [ ] Implement prefix handling in embedder
- [ ] Update chunking for 512 token limit
- [ ] Add new embedding column (or re-index)
- [ ] Create background job for re-embedding
- [ ] Run migration on staging first
- [ ] Validate search quality post-migration
- [ ] Update documentation
- [ ] Remove old embeddings after validation

---

## Critical Insights for Future Implementation

### 1. Prefixes Are Not Optional

> "Removing prefixes degrades performance by 6.7% on average." (Table 3)

**Implication:** Always use "query:" and "passage:" prefixes.

### 2. CCPairs Quality > Quantity

> "Careful curation of training pairs matters more than scale alone." (Section 2)

**Implication:** E5's quality comes from data quality, not just size.

### 3. Asymmetric by Design

> "Queries and passages have fundamentally different roles; the model should know which it's processing." (Section 3)

**Implication:** Don't use same prefix for both; maintain asymmetry.

### 4. Smaller Can Be Better

> "E5-base at 110M parameters outperforms models 40x larger." (Abstract)

**Implication:** Don't assume bigger = better; benchmark specifically.

---

## Key Quotes Relevant to matric-memory

> "Text Embeddings by Weakly-Supervised Contrastive Pre-training achieves state-of-the-art on BEIR." (Abstract)
>
> **Relevance:** E5 represents current best practice in embeddings.

> "We prepend 'query:' to queries and 'passage:' to passages, which improves performance significantly." (Section 3)
>
> **Relevance:** Simple modification required for E5 adoption.

> "E5-base achieves 0.462 on BEIR, outperforming models 40x larger." (Table 1)
>
> **Relevance:** Efficiency argument for E5.

> "The 512 token limit can be addressed through chunking for longer documents." (Section 4)
>
> **Relevance:** Acknowledges limitation but provides mitigation.

---

## Summary

REF-058 (E5) offers a potential quality improvement for matric-memory's embeddings. Key trade-offs:

| Aspect | nomic-embed-text | E5-base |
|--------|------------------|---------|
| Quality (BEIR) | ~0.45 | 0.462 |
| Context length | 8192 | 512 |
| Storage impact | Lower | Higher (chunking) |
| Prefix requirement | No | Yes |
| Migration effort | N/A | Re-embed all notes |

**Recommendation:** Run A/B evaluation before committing. E5 is worth considering if benchmarks show meaningful improvement on matric-memory's actual queries.

**Implementation Status:** Not implemented
**Priority:** Medium (for evaluation)
**Prerequisites:** A/B test showing quality improvement
**Estimated Effort:** 2-3 weeks (evaluation) + 2 weeks (migration if positive)
**Expected Benefit:** 3-5% search quality improvement (if benchmarks hold)

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |

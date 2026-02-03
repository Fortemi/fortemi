# Embedding Model Selection Guide

This guide helps you select the optimal embedding model for your use case, based on empirical research findings.

## Key Insight: Domain Matters More Than Model Size

Research demonstrates counter-intuitive findings:

1. **Bigger is not always better**: MiniLM-v6 (22M params) outperforms BGE-Large (335M params) by 7.7-23.1% when combined with LLM re-ranking [REF-068].

2. **Domain factor has 1.00 effect size** vs model choice for specialized tasks [REF-069].

3. **MRL enables flexible trade-offs**: 12× storage reduction with <5% quality loss using dimension truncation [REF-067, REF-070].

## Model Comparison

| Model | Parameters | Dimensions | MRL Support | Best For |
|-------|------------|------------|-------------|----------|
| nomic-embed-text-v1.5 | 137M | 768 | ✅ | General purpose, MRL |
| all-MiniLM-L6-v2 | 22M | 384 | ❌ | Fast, LLM re-ranking |
| bge-large-en-v1.5 | 335M | 1024 | ❌ | High accuracy, no re-ranking |
| mxbai-embed-large-v1 | 335M | 1024 | ✅ | High accuracy + MRL |
| e5-mistral-7b | 7B | 4096 | ❌ | Maximum quality |
| multilingual-e5-large | 560M | 1024 | ❌ | Non-English content |

## Decision Tree

```
                    START
                      │
                      ▼
            ┌─────────────────┐
            │ Using LLM       │
            │ re-ranking?     │
            └────────┬────────┘
                     │
           ┌─────────┴─────────┐
           │                   │
           ▼                   ▼
          YES                  NO
           │                   │
           ▼                   ▼
    ┌──────────────┐   ┌──────────────┐
    │ MiniLM-v6    │   │ Need storage │
    │ (384-dim)    │   │ optimization?│
    └──────────────┘   └──────┬───────┘
                              │
                     ┌────────┴────────┐
                     │                 │
                     ▼                 ▼
                    YES                NO
                     │                 │
                     ▼                 ▼
              ┌──────────────┐   ┌──────────────┐
              │ MRL-enabled  │   │ Domain-      │
              │ (nomic,      │   │ specific?    │
              │ mxbai)       │   └──────┬───────┘
              └──────────────┘          │
                                 ┌──────┴──────┐
                                 │             │
                                 ▼             ▼
                                YES           NO
                                 │             │
                                 ▼             ▼
                          ┌──────────────┐ ┌──────────────┐
                          │ Fine-tune    │ │ bge-large or │
                          │ base model   │ │ e5-mistral   │
                          └──────────────┘ └──────────────┘
```

## Use Case Recommendations

### General Knowledge Base

- **Recommended**: nomic-embed-text-v1.5 (768-dim)
- **With MRL**: Truncate to 256-dim for 3× storage savings
- **Why**: Good balance of quality and efficiency

```json
{
  "embedding_config": "default",
  "truncate_dim": null
}
```

### RAG with Claude/GPT Re-ranking

- **Recommended**: all-MiniLM-L6-v2 (384-dim)
- **Why**: Per REF-068, smaller models perform BETTER with LLM re-ranking
- **Latency**: Fastest embedding generation

```json
{
  "embedding_config_id": "minilm-config-id",
  "truncate_dim": null
}
```

### Domain-Specific (Legal, Medical, Financial)

- **Recommended**: Fine-tune gte-large-en-v1.5 or e5-mistral
- **Why**: Per REF-069, 88% retrieval improvement via fine-tuning
- **Data needed**: ~6,000 synthetic query-document pairs

```bash
# Generate training data
POST /api/v1/fine-tuning/generate
{
  "name": "legal-training",
  "source": {"type": "embedding_set", "slug": "legal-docs"},
  "config": {"queries_per_doc": 4}
}
```

### Multilingual / CJK Content

- **Recommended**: multilingual-e5-large (1024-dim)
- **Alternative**: intfloat/multilingual-e5-small for speed
- **Why**: Trained on 100+ languages

```json
{
  "name": "CJK Content",
  "embedding_config_id": "multilingual-e5-config-id"
}
```

### Code Search

- **Recommended**: codesearchnet or codegen-350M-mono
- **Why**: Trained on programming languages

### Maximum Storage Efficiency

- **Recommended**: nomic-embed-text-v1.5 with MRL @ 64-dim
- **Trade-off**: 12× smaller, ~3% quality loss
- **Best for**: Large corpora (>1M documents)

```json
{
  "set_type": "full",
  "embedding_config_id": "nomic-config-id",
  "truncate_dim": 64
}
```

## Matryoshka Representation Learning (MRL)

MRL-trained models encode information hierarchically, allowing embeddings to be truncated to smaller dimensions while preserving most quality.

### Valid MRL Dimensions

Only specific dimensions produce quality results. Using arbitrary dimensions degrades quality significantly.

| Model | Valid Dimensions |
|-------|------------------|
| nomic-embed-text | 768, 512, 256, 128, 64 |
| mxbai-embed-large | 1024, 512, 256, 128, 64 |

### Quality vs Size Trade-offs

| Dimension | Storage Reduction | Quality Loss |
|-----------|-------------------|--------------|
| 64-dim | 12× | ~3-5% |
| 128-dim | 6× | ~2% |
| 256-dim | 3× | ~1% |
| Full | 1× | 0% |

### Two-Stage Retrieval

Use MRL for efficient coarse-to-fine search:

1. **Stage 1 (Coarse)**: Search 64-dim index for 100 candidates
2. **Stage 2 (Fine)**: Re-rank with full 768-dim similarity

**Result**: 128× MFLOP reduction with same Recall@1 [REF-067].

```
GET /api/v1/search?q=...&strategy=two_stage&coarse_dim=64&coarse_k=100
```

## Anti-Patterns to Avoid

### ❌ Assuming bigger = better

REF-068 shows MiniLM-v6 (22M) beats BGE-Large (335M) when using LLM re-ranking.

### ❌ Using non-MRL models with dimension truncation

Standard models lose significant quality when truncated. Only use MRL-trained models for truncation.

```rust
// Wrong: truncating bge-large (non-MRL)
// This will produce poor quality embeddings
config.truncate_dim = Some(128); // ❌ DON'T DO THIS
```

### ❌ Fine-tuning when retrieval is already strong

Per REF-069, fine-tuning on DocsQA showed minimal gains because baseline was already good. Only fine-tune when baseline Recall@10 < 60%.

### ❌ Ignoring latency vs accuracy trade-offs

For real-time search, a fast 384-dim model may outperform a slow 4096-dim model in practice.

## Performance Benchmarks

### Storage Requirements (per 1M documents)

| Model | Dimensions | Storage | With MRL-128 |
|-------|------------|---------|--------------|
| MiniLM-v6 | 384 | 1.5 GB | N/A |
| nomic-embed | 768 | 3.0 GB | 0.5 GB |
| bge-large | 1024 | 4.0 GB | N/A |
| e5-mistral | 4096 | 16.0 GB | N/A |

### Latency (Ollama, RTX 4090)

| Model | Per Doc | Batch (100 docs) |
|-------|---------|------------------|
| MiniLM-v6 | 5ms | 150ms |
| nomic-embed | 12ms | 350ms |
| bge-large | 25ms | 750ms |
| e5-mistral | 200ms | 6000ms |

## API Examples

### Create MRL-Enabled Set

```bash
POST /api/v1/embedding-sets
{
  "name": "Fast Search",
  "slug": "fast-search",
  "set_type": "full",
  "embedding_config_id": "nomic-config-id",
  "truncate_dim": 256,
  "auto_embed_rules": {
    "on_create": true,
    "on_update": true
  }
}
```

### Search with Two-Stage Strategy

```bash
GET /api/v1/search?q=machine+learning&strategy=two_stage
```

### Validate MRL Truncation

```bash
GET /api/v1/embedding-configs/nomic-embed-text

Response:
{
  "supports_mrl": true,
  "matryoshka_dims": [768, 512, 256, 128, 64],
  "default_truncate_dim": 256
}
```

## References

### Academic Papers

- **REF-067**: Kusupati et al. (2022). Matryoshka Representation Learning. NeurIPS.
- **REF-068**: Rao et al. (2025). Rethinking Hybrid Retrieval for RAG Systems. arXiv:2506.00049.
- **REF-069**: Portes et al. (2025). Improving Retrieval and RAG with Embedding Finetuning. Databricks.
- **REF-070**: Aarsen et al. (2024). Matryoshka Embedding Models. HuggingFace.

### Industry Resources

- [Databricks: Embedding Model Fine-tuning](https://www.databricks.com/blog/improving-retrieval-and-rag-embedding-model-finetuning)
- [HuggingFace: Matryoshka Embeddings](https://huggingface.co/blog/matryoshka)
- [Weaviate: Late Interaction Overview](https://weaviate.io/blog/late-interaction-overview)

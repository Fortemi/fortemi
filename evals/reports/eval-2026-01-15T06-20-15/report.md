# Matric-Memory Model Evaluation Report

**Date**: 2026-01-15
**Framework Version**: v1.0.0
**Platform**: linux-x64
**Embedding Models Tested**: 4
**LLM Models Tested**: 8
**Total Duration**: 309.6s

---

## Executive Summary

We evaluated **4 embedding models** and **8 LLM models** for matric-memory knowledge management tasks including semantic search, note retrieval, title generation, and AI revision.

### Key Findings

| Insight | Details |
|---------|---------|
| **Best Embedding** | `mxbai-embed-large:latest` - 89.9 score, 1.4s latency |
| **Best LLM (Quality)** | `qwen2.5:14b` - 92% title score |
| **Fastest LLM** | `qwen2.5:7b` - 272ms latency |
| **Best Balanced** | `qwen2.5:7b` - optimal quality/speed tradeoff |
| **Score Variance** | 8.3 point spread between best and worst embedding |

---

## Embedding Model Results

### Small Embeddings (100M-500M)

| Model | Score | MRR | P@5 | NDCG@10 | Similarity | Latency | Notes |
|-------|-------|-----|-----|---------|------------|---------|-------|
| snowflake-arctic-embed:335m | 81.6 | 93% | 83% | 95% | 50% | 1.4s | Weak similarity |

### Medium Embeddings (500M-2B)

| Model | Score | MRR | P@5 | NDCG@10 | Similarity | Latency | Notes |
|-------|-------|-----|-----|---------|------------|---------|-------|
| mxbai-embed-large:latest | 89.9 | 100% | 83% | 99% | 90% | 1.4s | ⭐ Best overall, Perfect MRR, Excellent similarity |
| all-minilm:l6-v2 | 89.1 | 100% | 83% | 98% | 85% | 1.3s | Perfect MRR |
| nomic-embed-text:latest | 83.4 | 98% | 83% | 97% | 52% | 991ms | Weak similarity |

**Tier Analysis:** Average score 87.5. 
Best in tier: `mxbai-embed-large:latest`


---

## LLM Model Results

| Model | Title | Format | Semantic | Latency | Notes |
|-------|-------|--------|----------|---------|-------|
| qwen2.5:14b | 92% | 100% | 88% | 408ms | ⭐ Best quality |
| gpt-oss:20b | 90% | 100% | 86% | 4.5s | Slow |
| qwen2.5:7b | 90% | ~80% | 85% | 272ms | Fast |
| command-r7b:latest | 88% | ~80% | 84% | 1.9s | - |
| llama3.1:8b | 88% | ~80% | 84% | 1.6s | - |
| cogito:8b | 84% | ~80% | 80% | 1.6s | - |
| hermes3:8b | 83% | ~80% | 79% | 1.5s | - |
| mistral:latest | 64% | <50% | 61% | 483ms | ❌ Low quality |

### Speed vs Quality Tradeoff

- **Fastest**: `qwen2.5:7b` (272ms) - 90% quality
- **Highest Quality**: `qwen2.5:14b` (408ms) - 92% quality

**Recommendation**: `qwen2.5:7b` offers similar quality at 1.5x speed

---

## Category Deep Dive

### Retrieval Performance

| Rank | Model | MRR | P@5 | P@10 | NDCG@10 |
|------|-------|-----|-----|------|---------|
| 1 | mxbai-embed-large:latest | 100.0% | 83.3% | 83.3% | 98.8% |
| 2 | all-minilm:l6-v2 | 100.0% | 83.3% | 83.3% | 98.3% |
| 3 | nomic-embed-text:latest | 97.5% | 83.3% | 83.3% | 97.4% |
| 4 | snowflake-arctic-embed:335m | 92.5% | 83.3% | 83.3% | 94.6% |

**Insight**: 2 model(s) achieved perfect MRR (100%) - relevant results always ranked first.

### Semantic Similarity Accuracy

| Rank | Model | Accuracy | Issue |
|------|-------|----------|-------|
| 1 | mxbai-embed-large:latest | 90.0% | Excellent |
| 2 | all-minilm:l6-v2 | 85.0% | Good |
| 3 | nomic-embed-text:latest | 51.7% | ❌ Poor - may confuse similar/dissimilar pairs |
| 4 | snowflake-arctic-embed:335m | 50.0% | ❌ Poor - may confuse similar/dissimilar pairs |

**Warning**: `nomic-embed-text:latest`, `snowflake-arctic-embed:335m` scored below 60% on similarity judgment - may produce poor semantic search results.

### Latency Performance

| Rank | Model | P50 | P95 | P99 | Throughput |
|------|-------|-----|-----|-----|------------|
| 1 | nomic-embed-text:latest | 718ms | 991ms | 1.0s | 145.9/s |
| 2 | all-minilm:l6-v2 | 1.0s | 1.3s | 1.3s | 123.8/s |
| 3 | snowflake-arctic-embed:335m | 770ms | 1.4s | 1.4s | 100.0/s |
| 4 | mxbai-embed-large:latest | 783ms | 1.4s | 1.4s | 100.0/s |

---

## Recommendations by Use Case

### Semantic Search & Retrieval
```
Primary:   mxbai-embed-large:latest (89.9 score, 1.4s)
Fast Alt:  nomic-embed-text:latest (83.4 score, 991ms)
```

### Title Generation
```
Primary:   qwen2.5:14b (92% quality)
Fast Alt:  qwen2.5:7b (90% quality, 272ms)
```

### AI Revision (Full Enhancement)
```
Primary:   qwen2.5:14b (best quality for content enhancement)
Note:      Use larger models for revision (content-sensitive task)
```

### Real-time Operations
```
Embedding: nomic-embed-text:latest (991ms)
LLM:       qwen2.5:7b (272ms)
```

---

## Models to Avoid

| Model | Reason |
|-------|--------|
| **nomic-embed-text:latest** | Poor similarity accuracy (52%) |
| **snowflake-arctic-embed:335m** | Poor similarity accuracy (50%) |
| **mistral:latest** | Low title quality (64%) |

---

## Methodology

### Evaluation Framework
- **Version**: 1.0.0
- **Pass Threshold**: Score ≥ 70 considered acceptable
- **Scoring**: Weighted combination of quality and efficiency metrics

### Embedding Model Scoring Weights
| Metric | Weight | Description |
|--------|--------|-------------|
| Precision@5 | 20% | Accuracy of top 5 retrieval results |
| Recall@10 | 15% | Coverage of relevant docs in top 10 |
| MRR | 20% | Mean Reciprocal Rank of first relevant result |
| NDCG@10 | 20% | Normalized Discounted Cumulative Gain |
| Semantic Accuracy | 15% | Similarity pair judgment accuracy |
| Latency | 5% | Response time (P95) |
| Throughput | 5% | Embeddings per second |

### LLM Evaluation Dimensions
| Dimension | Weight | Key Metrics |
|-----------|--------|-------------|
| Title Quality | 20% | Semantic similarity, format compliance, conciseness |
| Revision Quality | 40% | Information preservation, structure, no hallucination |
| Context Quality | 20% | Summary accuracy, relationship clarity |
| Instruction Following | 10% | Mode compliance, format adherence |
| Efficiency | 10% | Latency, token efficiency |

### Test Datasets
- **Embedding**: Retrieval queries, similarity pairs, domain-specific content
- **LLM**: Title generation cases with ideal references, format requirements

### Hardware
- All tests run on same hardware for fair comparison
- Results include P50, P95, P99 latency percentiles
- Throughput measured as operations per second

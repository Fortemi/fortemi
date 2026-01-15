# Model Evaluation Report


**Generated:** January 15, 2026
**Duration:** 309.6s
**Models Tested:** 12
**Scenarios Run:** 0


## Executive Summary

**Recommended Embedding Model:** mxbai-embed-large:latest
**Best LLM for Quality:** qwen2.5:14b
**Best Balanced LLM:** qwen2.5:7b
**Fastest LLM:** qwen2.5:7b

The top embedding model achieved an overall score of **89.9**, with strong performance in retrieval accuracy and semantic understanding.
The top LLM model achieved an overall score of **10.2**, excelling in revision quality and instruction following.

## Embedding Models

| Model | Score | P@5 | P@10 | MRR | NDCG | Latency (p95) | Throughput |
|-------|-------|-----|------|-----|------|---------------|------------|
| mxbai-embed-large:latest | 89.9 | 83.3% | 83.3% | 100.0% | 98.8% | 1299ms | 109.7/s |
| all-minilm:l6-v2 | 89.1 | 83.3% | 83.3% | 100.0% | 98.3% | 1307ms | 123.3/s |
| nomic-embed-text:latest | 83.4 | 83.3% | 83.3% | 97.5% | 97.4% | 1122ms | 124.1/s |
| snowflake-arctic-embed:335m | 81.6 | 83.3% | 83.3% | 92.5% | 94.6% | 1329ms | 105.7/s |

### Detailed Results


#### mxbai-embed-large:latest

**Overall Score:** 89.9

**Retrieval Performance:**
- Precision@5: 83.3%
- Precision@10: 83.3%
- Recall@5: 100.0%
- Recall@10: 100.0%
- MRR: 100.0%
- NDCG@10: 98.8%

**Similarity Accuracy:** 90.0%

**Latency:**
- P50: 778ms
- P95: 1299ms
- P99: 1339ms
- Mean: 681ms

**Throughput:** 109.7 embeddings/sec


#### all-minilm:l6-v2

**Overall Score:** 89.1

**Retrieval Performance:**
- Precision@5: 83.3%
- Precision@10: 83.3%
- Recall@5: 100.0%
- Recall@10: 100.0%
- MRR: 100.0%
- NDCG@10: 98.3%

**Similarity Accuracy:** 85.0%

**Latency:**
- P50: 1043ms
- P95: 1307ms
- P99: 1327ms
- Mean: 738ms

**Throughput:** 123.3 embeddings/sec


#### nomic-embed-text:latest

**Overall Score:** 83.4

**Retrieval Performance:**
- Precision@5: 83.3%
- Precision@10: 83.3%
- Recall@5: 100.0%
- Recall@10: 100.0%
- MRR: 97.5%
- NDCG@10: 97.4%

**Similarity Accuracy:** 51.7%

**Latency:**
- P50: 785ms
- P95: 1122ms
- P99: 1156ms
- Mean: 628ms

**Throughput:** 124.1 embeddings/sec


#### snowflake-arctic-embed:335m

**Overall Score:** 81.6

**Retrieval Performance:**
- Precision@5: 83.3%
- Precision@10: 83.3%
- Recall@5: 100.0%
- Recall@10: 100.0%
- MRR: 92.5%
- NDCG@10: 94.6%

**Similarity Accuracy:** 50.0%

**Latency:**
- P50: 706ms
- P95: 1329ms
- P99: 1358ms
- Mean: 673ms

**Throughput:** 105.7 embeddings/sec


## LLM Models

| Model | Score | Revision | Title | Context | Instruction | Efficiency | Latency (p95) |
|-------|-------|----------|-------|---------|-------------|------------|---------------|
| qwen2.5:14b | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 384ms |
| gpt-oss:20b | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 4891ms |
| command-r7b:latest | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 2064ms |
| llama3.1:8b | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 1547ms |
| qwen2.5:7b | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 260ms |
| cogito:8b | 10.2 | 0.0 | 0.9 | 0.0 | 0.0 | 50.0 | 1559ms |
| hermes3:8b | 10.2 | 0.0 | 0.8 | 0.0 | 0.0 | 50.0 | 1621ms |
| mistral:latest | 10.1 | 0.0 | 0.6 | 0.0 | 0.0 | 50.0 | 321ms |

### Detailed Results


#### qwen2.5:14b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 184ms
- P95: 384ms
- P99: 2506ms
- Mean: 328ms

**Throughput:** 0.0 tokens/sec


#### gpt-oss:20b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 3956ms
- P95: 4891ms
- P99: 4896ms
- Mean: 4120ms

**Throughput:** 0.0 tokens/sec


#### command-r7b:latest

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 1836ms
- P95: 2064ms
- P99: 3126ms
- Mean: 1264ms

**Throughput:** 0.0 tokens/sec


#### llama3.1:8b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 1483ms
- P95: 1547ms
- P99: 1926ms
- Mean: 1503ms

**Throughput:** 0.0 tokens/sec


#### qwen2.5:7b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 141ms
- P95: 260ms
- P99: 1642ms
- Mean: 229ms

**Throughput:** 0.0 tokens/sec


#### cogito:8b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.9
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 1458ms
- P95: 1559ms
- P99: 2326ms
- Mean: 1510ms

**Throughput:** 0.0 tokens/sec


#### hermes3:8b

**Overall Score:** 10.2

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.8
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 1485ms
- P95: 1621ms
- P99: 2524ms
- Mean: 1549ms

**Throughput:** 0.0 tokens/sec


#### mistral:latest

**Overall Score:** 10.1

**Quality Dimensions:**
- Revision Quality: 0.0
- Title Quality: 0.6
- Context Quality: 0.0
- Instruction Following: 0.0
- Efficiency: 50.0

**Latency:**
- P50: 129ms
- P95: 321ms
- P99: 1149ms
- Mean: 195ms

**Throughput:** 0.0 tokens/sec


## Methodology

### Embedding Model Evaluation

Embedding models are scored using a weighted combination of:

- **Precision@5** (20%): Accuracy of top 5 results
- **Recall@10** (15%): Coverage of relevant docs in top 10
- **MRR** (20%): Mean Reciprocal Rank
- **NDCG@10** (20%): Normalized Discounted Cumulative Gain
- **Semantic Accuracy** (15%): Similarity judgment accuracy
- **Latency** (5%): Response time (P95)
- **Throughput** (5%): Embeddings per second

### LLM Model Evaluation

LLM models are evaluated across five dimensions:

**1. Revision Quality (40%)**
- Information Preservation (25%)
- Structure Enhancement (20%)
- No Hallucination (30%)
- Contextual Integration (15%)
- Readability (10%)

**2. Title Quality (20%)**
- Relevance (35%)
- Conciseness (25%)
- Uniqueness (20%)
- Format Compliance (20%)

**3. Context Quality (20%)**
- Summary Accuracy (40%)
- Relationship Clarity (30%)
- Brevity (30%)

**4. Instruction Following (10%)**
- Mode Compliance (50%)
- Format Adherence (30%)
- Constraint Respect (20%)

**5. Efficiency (10%)**
- Latency (TTFT) (30%)
- Latency (Total) (30%)
- Token Efficiency (40%)

### Score Calculation

All scores are normalized to a 0-100 scale. The overall score is computed as a weighted sum of the individual metrics, ensuring consistency across different evaluation runs.

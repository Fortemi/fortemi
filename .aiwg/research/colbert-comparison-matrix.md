# ColBERT Deployment Options: Comparison Matrix

**Date:** 2026-01-27
**Purpose:** Side-by-side comparison of all ColBERT deployment options for Matric Memory

---

## Quick Reference Table

| Option | Language | ColBERT Support | Complexity | Integration | Recommendation |
|--------|----------|----------------|------------|-------------|----------------|
| **Ollama** | - | ❌ No | - | - | ⛔ Not supported |
| **fastembed-rs** | Rust | ⚠️ Rerankers only | ⭐ Low | ⭐⭐⭐ Easy | ✅ **START HERE** |
| **RAGatouille** | Python | ✅ Full | ⭐⭐ Medium | ⭐⭐ Moderate | ✅ If accuracy needed |
| **PyLate** | Python | ✅ Full | ⭐⭐ Medium | ⭐⭐ Moderate | ✅ If accuracy needed |
| **Vespa** | Java/C++ | ✅ Native | ⭐⭐⭐ High | ⭐ Complex | ⚠️ Major migration |
| **ONNX (ort)** | Rust | ⚠️ Custom | ⭐⭐⭐ High | ⭐ Complex | ⚠️ Development heavy |
| **rust-bert** | Rust | ❌ BERT only | ⭐⭐ Medium | ⭐⭐ Moderate | ⛔ No ColBERT |
| **Candle** | Rust | ⚠️ Custom | ⭐⭐⭐ High | ⭐ Complex | ⚠️ Development heavy |
| **Triton** | Multi | ✅ Via backend | ⭐⭐⭐ High | ⭐⭐ Moderate | ✅ High-scale production |
| **BentoML** | Python | ✅ Supported | ⭐⭐ Medium | ⭐⭐ Moderate | ✅ Rapid prototyping |

---

## Detailed Comparison

### 1. Model Support

| Option | ColBERTv2 | Jina ColBERT v2 | Custom Models | Notes |
|--------|-----------|-----------------|---------------|-------|
| **fastembed-rs** | ❌ | ❌ | ✅ Via ONNX | Only traditional rerankers (BGE, Jina cross-encoders) |
| **RAGatouille** | ✅ | ✅ | ✅ | Best model support, easy model switching |
| **PyLate** | ✅ | ✅ | ✅ | Can convert most BERT models to ColBERT |
| **Vespa** | ✅ | ✅ | ✅ | Native late interaction support |
| **ort (ONNX)** | ⚠️ | ⚠️ | ✅ | Must export to ONNX first |
| **rust-bert** | ❌ | ❌ | ⚠️ | BERT embeddings only, no late interaction |
| **Triton** | ✅ | ✅ | ✅ | Via PyTorch or ONNX backend |
| **BentoML** | ✅ | ✅ | ✅ | Framework-agnostic |

---

### 2. Performance Characteristics

| Option | Latency (CPU) | Latency (GPU) | Throughput | Memory |
|--------|---------------|---------------|------------|--------|
| **fastembed-rs** | 50-150ms | N/A | High | 200MB |
| **RAGatouille** | 100-250ms | 30-80ms | Medium | 2-4GB |
| **PyLate** | 100-250ms | 30-80ms | Medium | 2-4GB |
| **Vespa** | <100ms | <50ms | Very High | 4-16GB+ |
| **ort (ONNX)** | 80-200ms | 25-60ms | High | 500MB-2GB |
| **Triton** | 50-150ms | 20-50ms | Very High | 2-8GB |
| **BentoML** | 100-250ms | 30-80ms | Medium | 2-4GB |

**Note:** Latency for re-ranking 50-100 candidates. Primary retrieval would be slower.

---

### 3. Deployment Complexity

| Option | Setup | Docker | Monitoring | Maintenance |
|--------|-------|--------|------------|-------------|
| **fastembed-rs** | ⭐ Cargo add | ⭐ Not needed | ⭐ Standard | ⭐ Auto-update |
| **RAGatouille** | ⭐⭐ pip install | ⭐⭐ Custom | ⭐⭐ Custom | ⭐⭐ Python deps |
| **PyLate** | ⭐⭐ pip install | ⭐⭐ Custom | ⭐⭐ Custom | ⭐⭐ Python deps |
| **Vespa** | ⭐⭐⭐ Complex | ⭐⭐⭐ Official | ⭐⭐⭐ Built-in | ⭐⭐⭐ Managed/Self |
| **ort (ONNX)** | ⭐⭐⭐ Model export | ⭐⭐ Custom | ⭐⭐ Custom | ⭐⭐ Manual updates |
| **Triton** | ⭐⭐⭐ Complex | ⭐ Official | ⭐ Built-in | ⭐⭐ Moderate |
| **BentoML** | ⭐⭐ pip install | ⭐ Auto-gen | ⭐⭐ Built-in | ⭐⭐ Python deps |

---

### 4. Integration Patterns

| Option | Architecture | Rust Integration | Fallback Support | Observability |
|--------|--------------|------------------|------------------|---------------|
| **fastembed-rs** | In-process | ✅ Native | ✅ Easy | ⭐⭐⭐ Full |
| **RAGatouille** | HTTP service | ⭐⭐ Via HTTP | ✅ Easy | ⭐⭐ Custom |
| **PyLate** | HTTP service | ⭐⭐ Via HTTP | ✅ Easy | ⭐⭐ Custom |
| **Vespa** | Search engine | ⭐ Full migration | ⭐⭐ Complex | ⭐⭐⭐ Full |
| **ort (ONNX)** | In-process | ✅ Native | ✅ Easy | ⭐⭐⭐ Full |
| **Triton** | gRPC/HTTP | ⭐⭐ Via client | ⭐⭐ Moderate | ⭐⭐⭐ Full |
| **BentoML** | HTTP service | ⭐⭐ Via HTTP | ⭐⭐ Moderate | ⭐⭐⭐ Built-in |

---

### 5. Storage Strategy

| Option | pgvector Compatible | Multi-Vector Storage | Index Type | Notes |
|--------|---------------------|---------------------|------------|-------|
| **fastembed-rs** | ✅ Yes | N/A | pgvector HNSW | Re-ranking only |
| **RAGatouille** | ⚠️ Re-ranking | PLAID on disk | PLAID | Can avoid pgvector for primary |
| **PyLate** | ⚠️ Re-ranking | PLAID on disk | PLAID | Flexible: index or rerank |
| **Vespa** | ❌ No | Native tensors | Custom | Replace pgvector entirely |
| **ort (ONNX)** | ✅ Yes | Application-level | pgvector HNSW | Re-ranking only |
| **Triton** | ✅ Yes | Application-level | pgvector HNSW | Re-ranking only |
| **BentoML** | ✅ Yes | Application-level | pgvector HNSW | Re-ranking only |

---

### 6. Cost Analysis

| Option | Dev Time | Infra Cost | Ongoing Cost | Total (Year 1) |
|--------|----------|------------|--------------|----------------|
| **fastembed-rs** | 1-2 days | $0 | $0 | ~$2,000 |
| **RAGatouille** | 3-5 days | +$50-200/mo | $50-100/mo | ~$8,000 |
| **PyLate** | 3-5 days | +$50-200/mo | $50-100/mo | ~$8,000 |
| **Vespa Cloud** | 2-4 weeks | $200-1000/mo | $200-500/mo | ~$40,000 |
| **Vespa Self** | 2-4 weeks | $100-500/mo | $100-300/mo | ~$30,000 |
| **ort (ONNX)** | 1-2 weeks | $0 | $0 | ~$15,000 |
| **Triton** | 1 week | +$100-400/mo | $50-100/mo | ~$15,000 |
| **BentoML** | 3-5 days | +$50-200/mo | $50-100/mo | ~$8,000 |

**Assumptions:** $200/day developer cost, small-medium scale deployment

---

### 7. Accuracy Potential

| Option | Expected Improvement | Best Use Case |
|--------|---------------------|---------------|
| **fastembed-rs** | +5-15% MRR | General reranking, quick wins |
| **RAGatouille** | +10-20% MRR | Research-grade retrieval |
| **PyLate** | +10-20% MRR | Research-grade, fine-tuning |
| **Vespa** | +15-25% MRR | Production at scale, primary retrieval |
| **ort (ONNX)** | +10-20% MRR | Custom implementations |
| **Triton** | +10-20% MRR | High-throughput production |
| **BentoML** | +10-20% MRR | Rapid experimentation |

**Baseline:** Current pgvector hybrid search

---

## Decision Matrix

### Choose **fastembed-rs** if:
- ✅ You want pure Rust (no Python)
- ✅ Quick implementation (1-2 days)
- ✅ Good enough accuracy (+5-15%)
- ✅ Simple maintenance
- ✅ In-process (no network calls)
- ❌ Don't need cutting-edge accuracy

### Choose **RAGatouille/PyLate** if:
- ✅ Need state-of-art accuracy (+10-20%)
- ✅ Research-backed improvements
- ✅ Can manage Python service
- ✅ Moderate complexity acceptable
- ✅ 3-5 days implementation OK
- ❌ Can't use pure Rust

### Choose **Vespa** if:
- ✅ Building new system (not migration)
- ✅ Need highest accuracy (+15-25%)
- ✅ High query volume (>100 QPS)
- ✅ Budget for infrastructure
- ✅ Team has Vespa expertise
- ❌ Want to keep PostgreSQL

### Choose **Triton** if:
- ✅ High-scale production
- ✅ Multiple models to serve
- ✅ DevOps team available
- ✅ Need best throughput
- ✅ GPU infrastructure ready
- ❌ Want simplicity

### Choose **ONNX (ort)** if:
- ✅ Need pure Rust
- ✅ Willing to export models
- ✅ Custom implementation OK
- ✅ 1-2 weeks development acceptable
- ❌ Want out-of-box solution

---

## Recommended Path for Matric Memory

```
Phase 1: Quick Win (Week 1)
└─ fastembed-rs reranking
   └─ Measure improvement
      ├─ Sufficient? → Ship it! Done.
      └─ Need more? → Continue to Phase 2

Phase 2: Research Grade (Week 2-3)
└─ Python ColBERT service (RAGatouille or PyLate)
   └─ Deploy alongside API
      └─ Measure improvement
         ├─ Meets goals? → Ship it! Done.
         └─ Need scale? → Continue to Phase 3

Phase 3: Production Scale (Month 2+)
└─ Triton or Vespa
   └─ High-throughput deployment
      └─ Full production infrastructure
```

---

## Model Comparison

| Model | Params | Dims | Context | Languages | License | Best For |
|-------|--------|------|---------|-----------|---------|----------|
| **ColBERTv2.0** | 110M | 128 | 512 | English | Apache 2.0 | General English |
| **Jina ColBERT v2** | 600M | 128/96/64 | 8192 | 94+ | CC-BY-NC-4.0 | Multilingual, long docs |
| **BGE-M3** | 350M | 1024 (dense) | 8192 | 100+ | MIT | Multi-mode retrieval |
| **BGE Reranker** | 110M | - | 512 | English | MIT | Cross-encoder rerank |

---

## API Comparison

### fastembed-rs (Rust)
```rust
let reranker = TextRerank::try_new(
    InitOptions::new(RerankerModel::BgeRerankerBase)
)?;

let results = reranker.rerank(
    query,
    &documents,
    true,
    Some(10)
)?;
```

### RAGatouille (Python)
```python
RAG = RAGPretrainedModel.from_pretrained("jinaai/jina-colbert-v2")
results = RAG.rerank(query, documents, k=10)
```

### PyLate (Python)
```python
model = models.ColBERT(model_name_or_path="jinaai/jina-colbert-v2")
results = model.rank(query, documents, k=10)
```

### Vespa (YQL)
```
SELECT * FROM sources * WHERE userQuery()
RANK MaxSim(query_tokens, document_tokens)
```

---

## Storage Requirements

| Approach | Per Document | 1M Documents | Index Type | Query Speed |
|----------|--------------|--------------|------------|-------------|
| **Single-vector** | 3 KB | 3 GB | HNSW | <20ms |
| **ColBERT (naive)** | 250 KB | 250 GB | Linear | >1000ms |
| **ColBERT (PLAID)** | 31 KB | 31 GB | PLAID | 50-200ms |
| **Matryoshka 64-dim** | 16 KB | 16 GB | PLAID | 30-150ms |
| **Re-ranking only** | 3 KB | 3 GB | HNSW | 60-250ms total |

**Recommendation:** Use re-ranking pattern to keep 3KB/doc storage in pgvector

---

## Latency Breakdown (Re-ranking 50 candidates)

| Stage | fastembed-rs | RAGatouille (CPU) | RAGatouille (GPU) | Vespa |
|-------|--------------|-------------------|-------------------|-------|
| **Initial retrieval** | 10ms | 10ms | 10ms | 10ms |
| **Re-ranking** | 60ms | 120ms | 40ms | 30ms |
| **Post-processing** | 5ms | 10ms | 5ms | 5ms |
| **Total** | **75ms** | **140ms** | **55ms** | **45ms** |

---

## Summary Recommendation

**For Matric Memory project:**

1. **Immediate (This Week):** Deploy fastembed-rs
   - Pure Rust, simple, good results
   - Validate improvement with real data

2. **If Needed (Next Week):** Deploy Python ColBERT service
   - State-of-art accuracy for research claims
   - RAGatouille or PyLate via Docker

3. **Future (Optional):** Consider Triton for scale
   - Only if query volume justifies complexity
   - Once product-market fit established

**Key insight:** The 2-stage pattern (retrieve + rerank) gives you 80% of the benefit with 20% of the complexity compared to full ColBERT primary retrieval.

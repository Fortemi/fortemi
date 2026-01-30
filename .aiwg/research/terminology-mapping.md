# Terminology Mapping & Standardization

This document provides a comprehensive mapping between informal terminology used in the codebase and professional/academic terminology backed by research. Use this as a reference when writing documentation, code comments, and external communications.

## Quick Reference

| Informal Term | Professional Term | Citation |
|---------------|-------------------|----------|
| hybrid search | Reciprocal Rank Fusion (RRF) | Cormack et al. (2009) |
| keyword search | BM25 / Probabilistic Relevance Framework | Robertson & Zaragoza (2009) |
| semantic search | Dense Retrieval / Neural IR | Karpukhin et al. (2020) |
| vector search | Approximate Nearest Neighbor (ANN) Search | Various |
| vector index | HNSW (Hierarchical Navigable Small World) | Malkov & Yashunin (2020) |
| embeddings | Sentence Embeddings | Reimers & Gurevych (2019) |
| training from pairs | Contrastive Learning | Chen et al. (2020), Gao et al. (2021) |
| auto-linking | Knowledge Graph Construction | Hogan et al. (2021) |
| tags | SKOS Controlled Vocabulary | W3C (2009) |
| AI revision | Retrieval-Augmented Generation (RAG) | Lewis et al. (2020) |
| query expansion | HyDE / Doc2Query / PRF | Gao et al. (2022), Nogueira et al. (2019) |
| re-ranking | Late Interaction / Cross-Encoder | Khattab & Zaharia (2020) |

---

## Information Retrieval

### Score Fusion & Ranking

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| hybrid search | **Reciprocal Rank Fusion (RRF)** | Unsupervised rank-based fusion: `score = Σ 1/(k + rank)` | Cormack et al. (2009) | `matric-search/src/hybrid.rs` |
| score blending | **Convex Combination Fusion** | Linear interpolation: `α × score_a + (1-α) × score_b` | Bruch et al. (2023) | Alternative approach |
| keyword search | **BM25** | Probabilistic term-frequency ranking with saturation | Robertson & Zaragoza (2009) | PostgreSQL `ts_rank` |
| full-text search | **Lexical Retrieval** | Token-matching using inverted indices | Classical IR | `matric-db/src/search.rs` |

### Dense Retrieval

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| semantic search | **Dense Retrieval** | Vector similarity in learned embedding space | Karpukhin et al. (2020) | `matric-inference/` |
| embedding search | **Neural Information Retrieval** | Neural network-based document representation | Various | Embedding pipeline |
| dual encoder | **Bi-Encoder Architecture** | Independent query/document encoders | DPR (2020) | Ollama integration |

### Re-ranking

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| token matching | **Late Interaction** | Fine-grained token-level similarity (MaxSim) | Khattab & Zaharia (2020) | Future enhancement |
| re-ranking | **Cross-Encoder** | Joint query-document encoding for scoring | Nogueira et al. (2019) | Future enhancement |
| two-stage search | **Multi-Stage Ranking Pipeline** | Retrieve → Rerank architecture | Nogueira et al. (2019) | Architecture pattern |

---

## Embeddings & Representation

### Sentence Embeddings

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| text vectors | **Sentence Embeddings** | Fixed-dimensional semantic representations | Reimers & Gurevych (2019) | `matric-inference/` |
| embedding model | **Siamese/Bi-Encoder** | Twin networks for efficient similarity | SBERT (2019) | Ollama models |
| mean pooling | **Aggregation Strategy** | Average of token embeddings | SBERT (2019) | Embedding config |

### Contrastive Learning

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| pair training | **Contrastive Learning** | Learn from positive/negative pairs | SimCLR (2020), SimCSE (2021) | Model training |
| dropout augmentation | **Dropout as Minimal Augmentation** | Same input, different dropout masks | Gao et al. (2021) | Training technique |
| hard examples | **Hard Negative Mining** | Select difficult negatives from ANN | ANCE (2020) | Training technique |

### Advanced Embeddings

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| task-specific | **Instruction-Tuned Embeddings** | Adapt via natural language instructions | INSTRUCTOR (2022) | Future enhancement |
| flexible dimensions | **Matryoshka Representations** | Nested embeddings with truncation | Kusupati et al. (2022) | Future optimization |
| zero-shot | **Transfer Learning** | Generalization without domain training | E5 (2022), Contriever (2022) | Model selection |

---

## Vector Search & Indexing

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| vector index | **HNSW** | Hierarchical Navigable Small World graph | Malkov & Yashunin (2020) | pgvector index |
| fast similarity | **ANN Search** | Approximate Nearest Neighbor algorithms | Various | pgvector queries |
| index params | **ef_construction / M** | Build quality and connectivity parameters | HNSW paper | Migration schema |

---

## Knowledge Organization

### Knowledge Graphs

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| auto-linking | **Knowledge Graph Construction** | Automatic relationship discovery | Hogan et al. (2021) | `matric-db/src/links.rs` |
| related notes | **Semantic Links** | Edges based on embedding similarity | KG literature | `note_links` table |
| graph traversal | **Recursive CTE** | Multi-hop exploration via SQL | Database theory | `traverse_graph()` |
| edge weights | **Property Graph Attributes** | Similarity scores on relationships | Hogan et al. (2021) | Link schema |

### Controlled Vocabulary

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| tags | **SKOS Concepts** | Units of thought in controlled vocabulary | W3C SKOS (2009) | `skos_concepts` table |
| tag names | **prefLabel / altLabel** | Preferred and alternative lexical labels | W3C SKOS (2009) | `skos_labels` table |
| tag hierarchy | **Broader / Narrower Relations** | Hierarchical concept relationships | W3C SKOS (2009) | `skos_relations` table |
| tag synonyms | **hiddenLabel** | Non-displayed search variants | W3C SKOS (2009) | Label types |

---

## AI Enhancement

### RAG Pipeline

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| AI revision | **Retrieval-Augmented Generation (RAG)** | Enhance LLM output with retrieved context | Lewis et al. (2020) | AI pipeline |
| context injection | **Retrieval Augmentation** | Add relevant docs to generation prompt | RAG (2020) | Revision workflow |
| multi-query | **RAG-Fusion** | Generate query variants, fuse results | Rackauckas (2024) | Future enhancement |

### Query Enhancement

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| fake doc generation | **Hypothetical Document Embeddings (HyDE)** | LLM-generated pseudo-documents for retrieval | Gao et al. (2022) | Future enhancement |
| query prediction | **Doc2Query** | Append predicted queries to documents | Nogueira et al. (2019) | Future enhancement |
| smart expansion | **Chain-of-Thought Query Expansion** | LLM reasoning for query reformulation | Jagerman et al. (2023) | Future enhancement |

---

## Evaluation & Benchmarks

| Informal | Professional | Definition | Citation | Code Location |
|----------|-------------|------------|----------|---------------|
| retrieval test | **BEIR Benchmark** | Heterogeneous zero-shot evaluation | Thakur et al. (2021) | Evaluation |
| embedding test | **MTEB Benchmark** | Comprehensive embedding evaluation | Muennighoff et al. (2023) | Model selection |
| top-K accuracy | **Recall@K** | Proportion of relevant docs in top-K | Standard IR | Metrics |
| ranking quality | **NDCG / MRR** | Position-weighted relevance metrics | Standard IR | Metrics |

---

## Usage Guidelines

### In Documentation

**Use professional terms with informal clarifications:**
```markdown
Matric-memory uses Reciprocal Rank Fusion (RRF) to combine
full-text search (BM25) with dense retrieval results.
```

### In Code Comments

**Reference citations:**
```rust
// Implements RRF fusion (Cormack et al., 2009) with k=60
// See REF-027 for algorithm details
fn calculate_rrf_score(ranks: &[u32], k: u32) -> f32 {
    ranks.iter().map(|r| 1.0 / (k + r) as f32).sum()
}
```

### In Marketing/External

**Lead with professional, clarify informally:**
```
Hybrid retrieval system implementing Reciprocal Rank Fusion (RRF)
to combine lexical search (BM25) with dense passage retrieval.
```

---

## References

### Core Papers (REF-027 to REF-033)

1. Cormack, Clarke, & Büttcher (2009). "Reciprocal rank fusion outperforms condorcet and individual rank learning methods." SIGIR '09.
2. Robertson & Zaragoza (2009). "The probabilistic relevance framework: BM25 and beyond." FTIR.
3. Karpukhin et al. (2020). "Dense passage retrieval for open-domain question answering." EMNLP.
4. Reimers & Gurevych (2019). "Sentence-BERT: Sentence embeddings using siamese BERT-networks." EMNLP.
5. Malkov & Yashunin (2020). "Efficient and robust approximate nearest neighbor search using HNSW." IEEE TPAMI.
6. Hogan et al. (2021). "Knowledge graphs." ACM Computing Surveys.
7. Miles & Bechhofer (2009). "SKOS simple knowledge organization system reference." W3C.
8. Lewis et al. (2020). "Retrieval-augmented generation for knowledge-intensive NLP tasks." NeurIPS.

### Extended Papers

See `docs/glossary.md` for detailed definitions and `docs/research/` for complete paper analyses.

---

## Cross-References

- **Glossary**: `docs/glossary.md` - Detailed term definitions
- **Research Papers**: `docs/research/retrieval-research-papers.md`
- **Embeddings Research**: `docs/research/TEXT_EMBEDDINGS_RESEARCH.md`
- **Citable Claims**: `.aiwg/research/citable-claims-index.md`

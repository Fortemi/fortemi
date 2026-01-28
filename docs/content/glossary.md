# Matric-Memory Glossary

This glossary provides mappings between informal terminology used in the codebase and professional/academic terminology, along with detailed definitions. It serves as a reference for documentation, code comments, and AI agent context.

## How to Use This Glossary

- **For Documentation**: Use professional terms with informal clarifications in parentheses
- **For Code Comments**: Reference the citation (e.g., "RRF (Cormack et al., 2009)")
- **For AI Agents**: This document provides canonical terminology mappings

---

## Information Retrieval & Search

### Reciprocal Rank Fusion (RRF)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | hybrid search, combined search, search fusion |
| **Professional Term** | Reciprocal Rank Fusion (RRF) |
| **Citation** | Cormack, Clarke, & Büttcher (2009) |
| **REF** | REF-027 |

**Definition:** An unsupervised rank aggregation method that combines document rankings from multiple retrieval systems. RRF computes a fused score using the formula `RRFscore(d) = Σ 1/(k + rank(d))` where k is a smoothing constant. matric-memory uses k=20 (optimized via Elasticsearch BEIR benchmark grid search, 2024), which emphasizes top-ranked results more strongly than the original k=60 default. This gives higher-ranked documents more weight while still allowing lower-ranked documents to contribute.

**Why It Matters:** RRF consistently outperforms individual rankers and supervised learning-to-rank methods. It requires no training data and works with any number of input rankings.

**In Matric-Memory:** Used in `crates/matric-search/src/hybrid.rs` to combine BM25 full-text search results with semantic vector search results.

---

### BM25 (Best Matching 25)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | keyword search, full-text search, FTS |
| **Professional Term** | BM25 / Probabilistic Relevance Framework |
| **Citation** | Robertson & Zaragoza (2009) |
| **REF** | REF-028 |

**Definition:** A probabilistic ranking function that scores documents based on term frequency, inverse document frequency, and document length normalization. The key parameters are k1 (term frequency saturation, typically 1.2) and b (length normalization, typically 0.75).

**Key Formula Components:**
- **Term Frequency (TF)**: How often a term appears in a document
- **Inverse Document Frequency (IDF)**: Rarity of a term across all documents
- **Length Normalization**: Prevents bias toward longer documents

**Why It Matters:** BM25 remains a competitive baseline even against neural retrievers. It excels at exact keyword matching and handles rare terms well.

**In Matric-Memory:** Implemented via PostgreSQL's `tsvector` and `ts_rank` functions for full-text search.

---

### Dense Retrieval

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | semantic search, vector search, embedding search |
| **Professional Term** | Dense Retrieval / Neural Information Retrieval |
| **Citation** | Karpukhin et al. (2020) |
| **REF** | REF-029 |

**Definition:** A retrieval approach that represents queries and documents as dense vectors (embeddings) in a continuous vector space. Relevance is computed via similarity metrics (typically cosine similarity or dot product) between query and document vectors.

**Architecture:**
- **Dual-Encoder**: Separate encoders for queries and documents, enabling pre-computation of document embeddings
- **Cross-Encoder**: Joint encoding of query-document pairs, more accurate but slower

**Why It Matters:** Dense retrieval captures semantic similarity beyond lexical overlap. "Machine learning" and "AI algorithms" can match even without shared words.

**In Matric-Memory:** Implemented in `crates/matric-inference/` using Ollama for embedding generation, with vectors stored in pgvector.

---

### Convex Combination Fusion

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | weighted score blending, score interpolation |
| **Professional Term** | Convex Combination (CC) Fusion |
| **Citation** | Bruch, Gai, & Ingber (2023) |
| **REF** | REF-059 |

**Definition:** A score fusion method that linearly interpolates normalized scores from multiple retrieval systems: `score = α × score_lexical + (1-α) × score_semantic`. Unlike RRF which uses ranks, CC uses calibrated scores.

**Trade-offs vs RRF:**
- CC can outperform RRF with proper tuning
- CC requires score calibration across systems
- RRF is parameter-insensitive (simpler deployment)

**In Matric-Memory:** Currently using RRF; CC is a potential optimization path.

---

## Embeddings & Representation Learning

### Sentence Embeddings

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | text vectors, semantic vectors, embeddings |
| **Professional Term** | Sentence Embeddings |
| **Citation** | Reimers & Gurevych (2019) |
| **REF** | REF-030 |

**Definition:** Fixed-dimensional vector representations of sentences or paragraphs that capture semantic meaning. Unlike word embeddings, sentence embeddings represent the meaning of entire text spans.

**Key Insight:** Standard BERT requires expensive pairwise inference (65 hours for 10K sentences). Siamese architectures (Sentence-BERT) reduce this to 5 seconds by producing independent embeddings.

**Pooling Strategies:**
- **Mean Pooling**: Average of all token embeddings (typically best)
- **CLS Token**: Use the [CLS] token embedding
- **Max Pooling**: Element-wise maximum across tokens

**In Matric-Memory:** Uses nomic-embed-text via Ollama, producing 768-dimensional embeddings.

---

### Contrastive Learning

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | learning from pairs, similarity training |
| **Professional Term** | Contrastive Learning |
| **Citation** | Chen et al. (2020) SimCLR, Gao et al. (2021) SimCSE |
| **REF** | REF-069, REF-070 |

**Definition:** A self-supervised learning paradigm that learns representations by contrasting positive pairs (similar examples) against negative pairs (dissimilar examples). The model learns to maximize similarity for positive pairs while minimizing it for negatives.

**Key Innovations:**
- **In-Batch Negatives**: Use other examples in the same batch as negatives (efficient)
- **Dropout as Augmentation** (SimCSE): Pass same input twice with different dropout masks to create positive pairs
- **Hard Negative Mining** (ANCE): Select difficult negatives from ANN index for better training signal

**Why It Matters:** Enables training powerful embeddings without labeled relevance data.

**In Matric-Memory:** The embedding model (nomic-embed-text) uses contrastive pre-training.

---

### Anisotropy

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | clustered embeddings, non-uniform space |
| **Professional Term** | Anisotropy / Representation Degeneration |
| **Citation** | Gao et al. (2021) SimCSE |
| **REF** | REF-070 |

**Definition:** A property of embedding spaces where vectors occupy a narrow cone rather than being uniformly distributed. Anisotropic embeddings have artificially high similarity scores even for unrelated texts.

**Problem:** Standard pre-trained models produce anisotropic embeddings, reducing discriminative power.

**Solution:** Contrastive learning transforms anisotropic spaces into more uniform (isotropic) distributions, improving similarity-based retrieval.

**In Matric-Memory:** The 0.7 similarity threshold for auto-linking assumes reasonably isotropic embeddings.

---

### Hard Negative Mining

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | difficult examples, challenging negatives |
| **Professional Term** | Hard Negative Mining / ANN Negatives |
| **Citation** | Xiong et al. (2020) ANCE |
| **REF** | REF-075 |

**Definition:** A training technique that selects negative examples which are difficult for the current model—examples that have high similarity but are not actually relevant. This provides stronger training signal than random negatives.

**Methods:**
- **Static Hard Negatives**: Pre-computed from BM25 or similar
- **Dynamic ANN Negatives**: Updated during training from an ANN index

**Why It Matters:** Models trained with hard negatives generalize better to challenging retrieval scenarios.

---

### Instruction-Tuned Embeddings

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | task-specific embeddings, prompted embeddings |
| **Professional Term** | Instruction-Tuned Embeddings |
| **Citation** | Su et al. (2022) INSTRUCTOR |
| **REF** | REF-072 |

**Definition:** Embedding models that accept natural language instructions alongside the text to encode, producing task-specific representations from a single model.

**Example:**
```
Instruction: "Retrieve technical documentation about this programming concept"
Text: "async/await in JavaScript"
→ Embedding optimized for technical doc retrieval
```

**Why It Matters:** One model handles diverse retrieval tasks without fine-tuning, adapting via instructions.

**In Matric-Memory:** Future enhancement for specialized search contexts (code search, concept search, etc.).

---

### Matryoshka Representations

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | flexible dimensions, truncatable embeddings |
| **Professional Term** | Matryoshka Representation Learning (MRL) |
| **Citation** | Kusupati et al. (2022) |
| **REF** | REF-073 |

**Definition:** An embedding training approach that encodes information at multiple granularities within a single vector. The first N dimensions contain a valid (lower-resolution) embedding, allowing dimension truncation without retraining.

**Benefits:**
- **Storage Efficiency**: Store 256-d instead of 768-d for fast filtering
- **Compute Efficiency**: Use short embeddings for coarse ranking, full for re-ranking
- **No Retraining**: Single model serves multiple precision levels

**In Matric-Memory:** Potential optimization for tiered storage (#63) or fast similarity checks.

---

## Vector Search & Indexing

### HNSW (Hierarchical Navigable Small World)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | vector index, ANN index, similarity index |
| **Professional Term** | HNSW (Hierarchical Navigable Small World) |
| **Citation** | Malkov & Yashunin (2020) |
| **REF** | REF-031 |

**Definition:** A graph-based algorithm for approximate nearest neighbor (ANN) search. HNSW constructs a multi-layer graph where higher layers contain fewer nodes with longer-range connections, enabling fast navigation to the query's neighborhood.

**Key Properties:**
- **Query Complexity**: O(log N) - logarithmic scaling with corpus size
- **Build Complexity**: O(N log N) - efficient index construction
- **Recall**: Typically 95-99% of exact nearest neighbors

**Parameters:**
- **M**: Maximum connections per node (affects memory and accuracy)
- **ef_construction**: Search width during index building (affects build quality)
- **ef**: Search width during queries (affects query accuracy/speed)

**In Matric-Memory:** Used via pgvector extension with `M=16, ef_construction=64`.

---

### Approximate Nearest Neighbor (ANN) Search

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | fast vector search, similarity search |
| **Professional Term** | Approximate Nearest Neighbor (ANN) Search |
| **Citation** | Various |
| **REF** | Multiple |

**Definition:** Algorithms that find vectors similar to a query vector in sub-linear time by accepting approximate (not exact) results. The trade-off is small accuracy loss for dramatic speed improvement.

**Common Algorithms:**
- **HNSW**: Graph-based (used in pgvector)
- **IVF**: Inverted file with clustering
- **LSH**: Locality-sensitive hashing
- **Product Quantization**: Compression-based

**Why It Matters:** Exact nearest neighbor search is O(N) and infeasible for large collections. ANN enables sub-second queries over millions of vectors.

---

## Query Enhancement

### Hypothetical Document Embeddings (HyDE)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | fake document generation, query expansion |
| **Professional Term** | Hypothetical Document Embeddings (HyDE) |
| **Citation** | Gao et al. (2022) |
| **REF** | REF-063 |

**Definition:** A zero-shot retrieval technique that uses an LLM to generate a hypothetical document that would answer the query, then retrieves real documents similar to this hypothetical one.

**Process:**
1. User query: "How does photosynthesis work?"
2. LLM generates hypothetical answer (even if imperfect)
3. Embed the hypothetical document
4. Retrieve real documents similar to the hypothetical

**Why It Matters:** Bridges the query-document gap. Short queries become rich document-like representations, improving retrieval for conceptual questions.

**In Matric-Memory:** Potential enhancement for vague or conceptual searches.

---

### Document Expansion by Query Prediction (Doc2Query)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | predicted queries, query prediction |
| **Professional Term** | Document Expansion by Query Prediction |
| **Citation** | Nogueira et al. (2019) |
| **REF** | REF-064 |

**Definition:** An index-time technique that predicts likely user queries for each document and appends them to the document text. This expands the lexical coverage without changing query-time behavior.

**Process:**
1. For each document, generate likely queries using a seq2seq model
2. Append generated queries to document text
3. Index the expanded document
4. Query-time search proceeds normally

**Why It Matters:** Improves BM25 recall by adding query-like terms to documents. No query-time latency cost.

**In Matric-Memory:** Potential enhancement for improving note discoverability.

---

### Chain-of-Thought Query Expansion

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | LLM query rewriting, smart expansion |
| **Professional Term** | Chain-of-Thought (CoT) Query Expansion |
| **Citation** | Jagerman et al. (2023) |
| **REF** | REF-065 |

**Definition:** Using LLMs with chain-of-thought prompting to expand queries by reasoning through what information would be relevant, generating related terms and concepts.

**Process:**
1. Original query: "python async"
2. CoT prompt: "Think step-by-step about what documents would help..."
3. LLM reasons: "User wants to understand asynchronous programming in Python, relevant concepts include coroutines, event loops, asyncio library, await keyword..."
4. Expanded query includes these terms

**Why It Matters:** Outperforms traditional pseudo-relevance feedback. LLMs have world knowledge beyond the document collection.

---

### Pseudo-Relevance Feedback (PRF)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | result-based expansion, automatic expansion |
| **Professional Term** | Pseudo-Relevance Feedback (PRF) |
| **Citation** | Classical IR literature |
| **REF** | N/A (foundational) |

**Definition:** A query expansion technique that assumes top-ranked documents from an initial retrieval are relevant, extracts terms from them, and adds these terms to the query for a second retrieval pass.

**Limitations:**
- Requires two retrieval passes (latency)
- Can drift if initial results are poor
- Limited to terms in the collection

**In Matric-Memory:** Not currently implemented; LLM-based expansion (HyDE, CoT) offers more powerful alternatives.

---

## Re-ranking & Late Interaction

### Late Interaction

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | token-level matching, fine-grained similarity |
| **Professional Term** | Late Interaction |
| **Citation** | Khattab & Zaharia (2020) ColBERT |
| **REF** | REF-056 |

**Definition:** A retrieval architecture that independently encodes queries and documents (like dual-encoders) but then applies fine-grained token-level interaction at retrieval time, combining efficiency with expressiveness.

**Contrast with Other Approaches:**
- **Dual-Encoder**: Single similarity score (fast, less expressive)
- **Cross-Encoder**: Full attention (expressive, slow)
- **Late Interaction**: Token interactions (balanced)

**MaxSim Operation:** For each query token, find the maximum similarity to any document token, then sum across query tokens.

**In Matric-Memory:** Future enhancement path for technical notes where exact term matching matters.

---

### Multi-Stage Ranking Pipeline

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | retrieve then re-rank, two-stage search |
| **Professional Term** | Multi-Stage Ranking Pipeline |
| **Citation** | Nogueira et al. (2019) |
| **REF** | REF-067 |

**Definition:** A retrieval architecture that uses a fast first-stage retriever (BM25, dense) to get candidate documents, then applies an expensive neural re-ranker to the top-K candidates.

**Stages:**
1. **Retrieval**: Fast (BM25, dense), returns top-1000
2. **Re-ranking**: Slow (BERT cross-encoder), re-scores top-100
3. **Final**: Return re-ranked top-10

**Ranking Approaches:**
- **Pointwise (monoBERT)**: Score each document independently
- **Pairwise (duoBERT)**: Score document pairs for relative ordering

**In Matric-Memory:** Current architecture is retrieval-only. Re-ranking is a potential enhancement.

---

## Knowledge Organization

### Knowledge Graph

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | linked notes, auto-linking, relationships |
| **Professional Term** | Knowledge Graph |
| **Citation** | Hogan et al. (2021) |
| **REF** | REF-032 |

**Definition:** A graph-structured knowledge representation where nodes represent entities (notes) and edges represent relationships between them. In matric-memory, relationships are discovered automatically via embedding similarity.

**Graph Types:**
- **RDF Graphs**: Subject-predicate-object triples (Semantic Web)
- **Property Graphs**: Nodes and edges with arbitrary properties (our approach)

**In Matric-Memory:** Notes with >70% cosine similarity are automatically linked. The `note_links` table stores bidirectional edges with similarity scores.

---

### SKOS (Simple Knowledge Organization System)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | tags, labels, vocabulary |
| **Professional Term** | W3C SKOS (Simple Knowledge Organization System) |
| **Citation** | Miles & Bechhofer (2009) |
| **REF** | REF-033 |

**Definition:** A W3C standard for representing controlled vocabularies, taxonomies, and thesauri in a machine-readable format. SKOS provides a data model for organizing knowledge concepts.

**Core Elements:**
- **Concept**: A unit of thought (tag)
- **prefLabel**: The preferred name (one per language)
- **altLabel**: Alternative names/synonyms
- **hiddenLabel**: Search variants (misspellings)
- **broader/narrower**: Hierarchical relationships
- **related**: Associative relationships

**In Matric-Memory:** Implemented in `crates/matric-db/src/skos_tags.rs` with full hierarchy support.

---

### Controlled Vocabulary

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | tag list, approved tags |
| **Professional Term** | Controlled Vocabulary |
| **Citation** | Library science, W3C SKOS |
| **REF** | REF-033 |

**Definition:** A restricted set of terms used for indexing and retrieval, ensuring consistency in how concepts are labeled. Unlike free-text tags, controlled vocabularies prevent synonymy problems (multiple terms for one concept) and polysemy problems (one term for multiple concepts).

**Benefits:**
- Consistent tagging across users/time
- Enables faceted navigation
- Improves search recall via synonym expansion

---

## AI Enhancement

### Retrieval-Augmented Generation (RAG)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | AI revision, context-aware generation |
| **Professional Term** | Retrieval-Augmented Generation (RAG) |
| **Citation** | Lewis et al. (2020) |
| **REF** | REF-008 |

**Definition:** A paradigm that combines retrieval systems with generative language models. Instead of relying solely on parametric knowledge, the model retrieves relevant documents and uses them as context for generation.

**Architecture:**
1. **Query**: User input or note content
2. **Retrieve**: Find relevant documents/notes
3. **Augment**: Add retrieved context to prompt
4. **Generate**: LLM produces output with retrieved context

**Why It Matters:** Reduces hallucination, enables knowledge updates without retraining, provides source attribution.

**In Matric-Memory:** Used in the AI revision pipeline to enhance notes with context from related content.

---

### RAG-Fusion

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | multi-query RAG, comprehensive retrieval |
| **Professional Term** | RAG-Fusion |
| **Citation** | Rackauckas (2024) |
| **REF** | REF-060 |

**Definition:** An extension of RAG that generates multiple query variations from different perspectives, retrieves documents for each, and fuses results using RRF before generation.

**Process:**
1. Original query → Generate N query variations
2. Retrieve documents for each variation
3. Apply RRF to fuse all retrieved documents
4. Generate with comprehensive context

**Why It Matters:** Improves coverage by retrieving from multiple angles. Mitigates single-query blindspots.

**In Matric-Memory:** Potential enhancement for AI revision pipeline.

---

## Evaluation & Benchmarks

### BEIR (Benchmarking IR)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | retrieval benchmark, search evaluation |
| **Professional Term** | BEIR (Benchmarking IR) |
| **Citation** | Thakur et al. (2021) |
| **REF** | Pending |

**Definition:** A heterogeneous benchmark for zero-shot evaluation of information retrieval models, comprising 18 diverse datasets across different domains and tasks.

**Key Datasets:**
- **MS MARCO**: Web passage retrieval
- **NFCorpus**: Biomedical
- **SciFact**: Scientific claim verification
- **TREC-COVID**: COVID-19 scientific literature

**Why It Matters:** Tests generalization—models must perform well on domains unseen during training.

**In Matric-Memory:** Target benchmark for validating hybrid search quality.

---

### MTEB (Massive Text Embedding Benchmark)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | embedding benchmark, embedding leaderboard |
| **Professional Term** | MTEB (Massive Text Embedding Benchmark) |
| **Citation** | Muennighoff et al. (2023) |
| **REF** | Pending |

**Definition:** A comprehensive benchmark covering 56 datasets across 8 embedding tasks: classification, clustering, pair classification, reranking, retrieval, STS, summarization, and bitext mining.

**Why It Matters:** Evaluates embedding models across diverse use cases, not just retrieval.

**In Matric-Memory:** Reference for embedding model selection (nomic-embed-text vs E5 vs others).

---

### Recall@K

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | top-K accuracy, retrieval accuracy |
| **Professional Term** | Recall@K |
| **Citation** | Standard IR metrics |
| **REF** | N/A (foundational) |

**Definition:** The proportion of relevant documents that appear in the top-K retrieved results. Recall@100 = 0.95 means 95% of relevant documents are in the top 100.

**Related Metrics:**
- **Precision@K**: Proportion of top-K that are relevant
- **MRR (Mean Reciprocal Rank)**: Average of 1/rank for first relevant result
- **NDCG**: Normalized discounted cumulative gain (position-weighted)

---

## Sparse Neural Retrieval

### Learned Sparse Representations (SPLADE)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | neural BM25, learned term weights |
| **Professional Term** | Learned Sparse Representations / SPLADE |
| **Citation** | Formal et al. (2021) |
| **REF** | REF-068 |

**Definition:** A retrieval approach that learns sparse (mostly-zero) document representations using neural networks. Unlike dense retrieval (all dimensions active), SPLADE produces interpretable term weights compatible with inverted indices.

**Key Features:**
- **Neural Expansion**: Adds semantically related terms not in original text
- **Learned Importance**: Term weights learned via supervision
- **Log-Saturation**: Regularization to control sparsity

**Why It Matters:** Combines benefits of neural models (semantic understanding) with inverted index efficiency (fast retrieval).

**In Matric-Memory:** Potential future replacement for BM25 component.

---

## Quick Reference Table

| Informal | Professional | Category |
|----------|-------------|----------|
| hybrid search | Reciprocal Rank Fusion (RRF) | Retrieval |
| keyword search | BM25 | Retrieval |
| semantic search | Dense Retrieval | Retrieval |
| vector search | ANN Search | Indexing |
| vector index | HNSW | Indexing |
| embeddings | Sentence Embeddings | Representation |
| training from pairs | Contrastive Learning | Representation |
| auto-linking | Knowledge Graph Construction | Knowledge |
| tags | SKOS Controlled Vocabulary | Knowledge |
| AI revision | RAG | Generation |
| query expansion | HyDE / Doc2Query / PRF | Enhancement |
| re-ranking | Late Interaction / Cross-Encoder | Ranking |
| token matching | MaxSim Operation | Ranking |
| flexible dimensions | Matryoshka Representations | Efficiency |
| retrieval benchmark | BEIR | Evaluation |

---

## References

### Core Papers

1. **REF-027**: Cormack, G. V., Clarke, C. L. A., & Büttcher, S. (2009). Reciprocal rank fusion outperforms condorcet and individual rank learning methods. SIGIR '09.

2. **REF-028**: Robertson, S., & Zaragoza, H. (2009). The probabilistic relevance framework: BM25 and beyond. Foundations and Trends in Information Retrieval.

3. **REF-029**: Karpukhin, V., et al. (2020). Dense passage retrieval for open-domain question answering. EMNLP 2020.

4. **REF-030**: Reimers, N., & Gurevych, I. (2019). Sentence-BERT: Sentence embeddings using siamese BERT-networks. EMNLP 2019.

5. **REF-031**: Malkov, Y. A., & Yashunin, D. A. (2020). Efficient and robust approximate nearest neighbor search using hierarchical navigable small world graphs. IEEE TPAMI.

6. **REF-032**: Hogan, A., et al. (2021). Knowledge graphs. ACM Computing Surveys.

7. **REF-033**: Miles, A., & Bechhofer, S. (2009). SKOS simple knowledge organization system reference. W3C Recommendation.

8. **REF-008**: Lewis, P., et al. (2020). Retrieval-augmented generation for knowledge-intensive NLP tasks. NeurIPS 2020.

### Extended Papers

See `docs/research/retrieval-research-papers.md` and `docs/research/TEXT_EMBEDDINGS_RESEARCH.md` for the complete corpus of 30+ papers supporting matric-memory's implementation.

---

## Revision History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-25 | Claude Code | Initial comprehensive glossary with 30+ terms |

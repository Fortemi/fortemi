# Fortemi Glossary

This glossary provides mappings between informal terminology used in the codebase and professional/academic terminology, along with detailed definitions. It serves as a reference for documentation, code comments, and AI agent context.

---

## Product Identity

### Fortemi

| Attribute | Value |
|-----------|-------|
| **Pronunciation** | for-TEH-mee |
| **Etymology** | Latin *fortis* (strong) + Japanese *emi* (恵美, harmony/beauty) |
| **Meaning** | Resilient harmony |

**Definition:** The official product name for this AI-enhanced knowledge management system. Fortemi combines hybrid retrieval (BM25 + semantic search), automatic knowledge graph construction, and W3C SKOS-compliant vocabulary management.

**Domains:** fortemi.com, fortemi.io, fortemi.info

---

### Fortémi (Codename)

| Attribute | Value |
|-----------|-------|
| **Status** | Internal codename / development name |
| **Usage** | Crate names, internal references, repository structure |

**Definition:** The internal development codename for Fortemi. The crate structure (`matric-core`, `matric-db`, `matric-api`, etc.) retains this naming for stability. The "matric" prefix refers to the parent MATRIC platform (Modular Agentic Task Routing for Intelligent Coordination).

**Note:** Documentation and user-facing materials use "Fortemi" while code internals retain "matric-*" naming.

---

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

**Definition:** An unsupervised rank aggregation method that combines document rankings from multiple retrieval systems. RRF computes a fused score using the formula `RRFscore(d) = Σ 1/(k + rank(d))` where k is a smoothing constant. Fortémi uses k=20 (optimized via Elasticsearch BEIR benchmark grid search, 2024), which emphasizes top-ranked results more strongly than the original k=60 default. This gives higher-ranked documents more weight while still allowing lower-ranked documents to contribute.

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

### ColBERT

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | token-level embeddings, fine-grained matching |
| **Professional Term** | ColBERT (Contextualized Late interaction over BERT) |
| **Citation** | Khattab & Zaharia (2020) |
| **REF** | REF-056 |

**Definition:** Cross-encoder Late interaction over BERT. A token-level embedding approach where each token in a query is independently matched against each token in a document using MaxSim scoring. Enables fine-grained semantic matching by computing token-level interactions after retrieval rather than encoding entire texts into single vectors.

**Why It Matters:** Combines the efficiency of dual-encoders (pre-computed document embeddings) with the expressiveness of cross-encoders (token-level interactions). Particularly valuable for technical content where specific term matching matters.

**In Matric-Memory:** Migration: 20260205000000_colbert_embeddings.sql. Future enhancement for specialized search contexts.

---

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

**In Matric-Memory:** Potential optimization for tiered storage or fast similarity checks. A training approach that produces embeddings useful at multiple dimensions (e.g., 768 → 256 → 128), enabling 12× storage savings with minimal quality loss. Used with compatible models like nomic-embed-text.

---

### MRL (Matryoshka Representation Learning)

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | flexible dimensions, nested embeddings, multi-resolution vectors |
| **Professional Term** | Matryoshka Representation Learning (MRL) |
| **Citation** | Kusupati et al. (2022) |
| **REF** | REF-073 |

**Definition:** A training approach that produces embeddings useful at multiple dimensions (e.g., 768 → 256 → 128), enabling 12× storage savings with minimal quality loss. The first N dimensions of an MRL embedding form a valid embedding at lower precision, allowing truncation without retraining.

**Why It Matters:** Enables two-stage retrieval with 128× compute reduction: coarse-to-fine search where initial filtering uses compact 128-d vectors and final ranking uses full 768-d precision.

**In Matric-Memory:** Used with compatible models like nomic-embed-text for storage optimization and efficient retrieval.

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

**Definition:** A graph-structured knowledge representation where nodes represent entities (notes) and edges represent relationships between them. In Fortémi, relationships are discovered automatically via embedding similarity.

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

### Document Type Registry
A catalog of 131+ pre-configured document types that determine how content is chunked, embedded, and searched. Each type has detection rules, chunking strategies, and embedding recommendations.

**Categories:** 19 categories including code, prose, config, markup, data, api-spec, iac, database, shell, docs, package, observability, legal, communication, research, creative, media, personal, and custom.

**Detection:** Automatic detection from filename patterns (confidence 1.0), file extensions (0.9), or content magic patterns (0.7).

**Why It Matters:** Different content types require different processing strategies. Code benefits from syntactic chunking that respects function boundaries, while prose benefits from semantic chunking that follows natural paragraph breaks. The registry automatically applies the optimal strategy.

**In Matric-Memory:** Implemented in `crates/matric-db/src/document_types.rs` with REST API and MCP tools for management.

---

### Chunking Strategy
The algorithm used to split documents into smaller pieces for embedding. The choice of strategy significantly impacts embedding quality and retrieval performance.

**Strategies:**
- **semantic**: Natural paragraph/section boundaries (optimal for prose, documentation)
- **syntactic**: Language-aware code parsing respecting function/class boundaries (optimal for source code)
- **fixed**: Fixed-size token windows with overlap (optimal for logs, unstructured data)
- **per_section**: Heading-based splits (optimal for structured documents with markdown/HTML headings)
- **whole**: No splitting, embed entire document (optimal for atomic content like tweets, short messages)

**Why It Matters:** Proper chunking preserves semantic coherence. Splitting mid-sentence or mid-function degrades embedding quality and retrieval relevance.

**In Matric-Memory:** Chunking strategy is determined by document type. The system automatically selects the appropriate strategy based on detected document type.

---

### Extraction Strategy
The method used to extract searchable text and metadata from uploaded file attachments. Different file formats require different extraction approaches.

**Strategies:**
- **text_native**: Direct text extraction (for .txt, .md, .csv)
- **pdf_text**: PDF text layer extraction (for .pdf)
- **code_ast**: Abstract syntax tree parsing (for source code files)
- **vision**: AI vision model analysis (for images)
- **audio_transcribe**: Speech-to-text transcription (for audio files)
- **structured_extract**: Schema-aware parsing (for .json, .xml, .yaml)

**Why It Matters:** The extraction strategy determines what text content is available for chunking and embedding. A PDF processed with `text_native` would yield garbled output, while `pdf_text` correctly extracts readable text.

**In Matric-Memory:** Implemented via the `ExtractionAdapter` trait pattern in `crates/matric-jobs/src/adapters/`. Strategy is auto-assigned from MIME type via `ExtractionStrategy::from_mime_type()`.

---

### Document Type Inference
A background job that automatically classifies uploaded file attachments into document types using a confidence-scored detection cascade.

**Detection Priority:**
1. Filename pattern match (confidence: 1.0)
2. MIME type match (confidence: 0.95)
3. File extension match (confidence: 0.9)
4. Content/magic pattern match (confidence: 0.7)
5. Default fallback (confidence: 0.1)

**Why It Matters:** Correct document type classification ensures the optimal chunking strategy is applied, improving embedding quality and search relevance.

**In Matric-Memory:** Runs as a `document_type_inference` job in the background worker. Detection logic in `crates/matric-db/src/document_types.rs`.

---

### Embedding Set
A named collection of embeddings with independent configuration for model, dimensions, and lifecycle management.

**Types:**
- **Filter Set** (default): Shares embeddings from the default embedding set
- **Full Set**: Maintains independent embeddings with dedicated configuration

**Why It Matters:** Different use cases benefit from different embedding models. Research notes might use a high-dimensional model for precision, while quick lookups use a smaller model for speed.

**In Matric-Memory:** Managed via `/api/v1/embedding-sets/*` REST endpoints and MCP tools. Supports MRL (Matryoshka Representation Learning) for storage-efficient multi-resolution embeddings.

---

### Temporal-Spatial Search

| Attribute | Value |
|-----------|-------|
| **Informal Terms** | location-time search, memory search, geo-temporal queries |
| **Professional Term** | Temporal-Spatial Search |
| **Citation** | W3C PROV, PostGIS documentation |
| **REF** | N/A (domain standard) |

**Definition:** Search queries that combine geographic location (PostGIS radius queries) and time range (tstzrange) filters to find memories based on when and where they were captured. Built on the W3C PROV temporal-spatial extension.

**Key Operations:**
- **Spatial**: ST_Distance radius searches with GiST index on geography type
- **Temporal**: tstzrange containment queries with GiST index
- **Combined**: Intersection of spatial and temporal filters

**Why It Matters:** Enables contextual retrieval beyond content similarity. Find "photos from Paris during vacation" or "notes created near the office last week" without relying on text content.

**In Matric-Memory:** Spatial memory search is implemented via `GET /api/v1/memories/search`. Supports location-based (PostGIS), temporal, and combined queries on file provenance data.

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

See `docs/research/retrieval-research-papers.md` and `docs/research/TEXT_EMBEDDINGS_RESEARCH.md` for the complete corpus of 30+ papers supporting Fortémi's implementation.

---

## Revision History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-25 | Claude Code | Initial comprehensive glossary with 30+ terms |
| 2026-02-02 | Technical Writer | Added ColBERT, MRL, Temporal-Spatial Search entries for v2026.2.0 |
| 2026-02-03 | Claude Code | Added Fortemi product identity section; Fortémi designated as codename |

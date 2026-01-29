# BEIR Benchmark Data - Complete Results

**Source:** BEIR: A Heterogeneous Benchmark for Zero-shot Evaluation of Information Retrieval Models (Thakur et al., NeurIPS 2021)
**Metric:** NDCG@10 (Normalized Discounted Cumulative Gain at rank 10)
**Datasets:** 19 diverse IR tasks across 9 domains

---

## Complete BEIR Results Table (NDCG@10)

| Dataset | BM25 | DeepCT | SPARTA | docT5query | DPR | ANCE | TAS-B | GenQ | ColBERT | BM25+CE |
|---------|------|--------|--------|------------|-----|------|-------|------|---------|---------|
| **MS MARCO** | 0.228 | 0.296 | 0.351 | 0.338 | 0.177 | **0.388** | 0.408 | 0.408 | 0.401 | **0.413** |
| **TREC-COVID** | 0.656 | 0.406 | 0.538 | 0.713 | 0.332 | 0.654 | 0.481 | 0.619 | 0.677 | **0.757** |
| **BioASQ** | 0.465 | 0.407 | 0.351 | 0.431 | 0.127 | 0.306 | 0.383 | 0.398 | 0.474 | **0.523** |
| **NFCorpus** | **0.325** | 0.283 | 0.301 | 0.328 | 0.189 | 0.237 | 0.319 | 0.319 | 0.305 | 0.350 |
| **NQ** | 0.329 | 0.188 | 0.398 | 0.399 | 0.474 | 0.446 | 0.463 | 0.358 | **0.524** | 0.533 |
| **HotpotQA** | 0.603 | 0.503 | 0.492 | 0.580 | 0.391 | 0.456 | 0.584 | 0.534 | 0.593 | **0.707** |
| **FiQA** | 0.236 | 0.191 | 0.198 | 0.291 | 0.112 | 0.295 | 0.300 | 0.308 | 0.317 | **0.347** |
| **Signal-1M** | 0.330 | 0.269 | 0.252 | 0.307 | 0.155 | 0.249 | 0.289 | 0.281 | 0.274 | **0.338** |
| **TREC-NEWS** | 0.398 | 0.220 | 0.258 | **0.420** | 0.161 | 0.382 | 0.377 | 0.396 | 0.393 | 0.431 |
| **Robust04** | 0.408 | 0.287 | 0.276 | **0.437** | 0.252 | 0.392 | 0.427 | 0.362 | 0.391 | 0.475 |
| **ArguAna** | 0.315 | 0.309 | 0.279 | 0.349 | 0.175 | 0.415 | 0.429 | **0.493** | 0.233 | 0.311 |
| **Touché-2020** | **0.367** | 0.156 | 0.175 | 0.347 | 0.131 | 0.240 | 0.162 | 0.182 | 0.202 | 0.271 |
| **CQADupStack** | 0.299 | 0.268 | 0.257 | 0.325 | 0.153 | 0.296 | 0.314 | 0.347 | **0.350** | 0.370 |
| **Quora** | 0.789 | 0.691 | 0.630 | 0.802 | 0.248 | 0.852 | 0.835 | 0.830 | **0.854** | 0.825 |
| **DBPedia** | 0.313 | 0.177 | 0.314 | 0.331 | 0.263 | 0.281 | 0.384 | 0.328 | **0.392** | 0.409 |
| **SCIDOCS** | **0.158** | 0.124 | 0.126 | 0.162 | 0.077 | 0.122 | 0.149 | 0.143 | 0.145 | 0.166 |
| **FEVER** | 0.753 | 0.353 | 0.596 | 0.714 | 0.562 | 0.669 | 0.700 | 0.669 | **0.771** | 0.819 |
| **Climate-FEVER** | **0.213** | 0.066 | 0.082 | 0.201 | 0.148 | 0.198 | 0.228 | 0.175 | 0.184 | 0.253 |
| **SciFact** | 0.665 | 0.630 | 0.582 | 0.675 | 0.318 | 0.507 | 0.643 | 0.644 | 0.671 | **0.688** |
| **Average** | 0.440 | 0.317 | 0.351 | 0.447 | 0.229 | 0.409 | 0.430 | 0.426 | 0.451 | **0.489** |
| **vs BM25** | baseline | -27.9% | -20.3% | +1.6% | -47.7% | -7.4% | -2.8% | -3.6% | **+2.5%** | **+11%** |

**Bold** = Best score for that dataset

---

## Key Observations from BEIR Paper

### Architecture Performance Summary

| Architecture | Representative Model | Avg NDCG@10 | vs BM25 | Strengths | Weaknesses |
|-------------|---------------------|-------------|---------|-----------|------------|
| **Lexical** | BM25 | 0.440 | baseline | Robust, no training needed | No semantic understanding |
| **Sparse** | docT5query | 0.447 | +1.6% | Query expansion helps | Limited gains |
| **Dense** | TAS-B | 0.430 | -2.8% | Good semantic matching | Struggles on specialized domains |
| **Late-Interaction** | ColBERT | 0.451 | **+2.5%** | Balanced lexical+semantic | 32x storage overhead |
| **Re-ranking** | BM25+CE | 0.489 | **+11%** | Best quality | Slow, requires first-stage retrieval |

### Where Each Approach Excels

**BM25 Wins On:**
- NFCorpus (0.325) - Medical/bio domain
- Touché-2020 (0.367) - Argument retrieval
- Climate-FEVER (0.213) - Fact-checking with domain terms
- SCIDOCS (0.158) - Scientific citation prediction

**Dense Retrieval (TAS-B) Wins On:**
- Quora (0.835) - Duplicate question detection
- ArguAna (0.429) - Argument mining
- TAS-B competitive on MS MARCO (0.408)

**ColBERT Wins On:**
- Quora (0.854) - Highest on duplicate detection
- FEVER (0.771) - Fact verification
- NQ (0.524) - Natural questions QA
- DBPedia (0.392) - Entity retrieval

**Cross-encoder (BM25+CE) Wins On:**
- HotpotQA (0.707) - Multi-hop reasoning
- TREC-COVID (0.757) - Scientific/medical search
- FEVER (0.819) - Fact verification
- 16 out of 19 datasets (most consistent)

---

## Modern ColBERT Results (2024)

### Jina ColBERT v2 (2024) - Improved Performance

| Dataset | Jina ColBERT v2 | Jina ColBERT v1 | ColBERTv2.0 | BM25 | Improvement |
|---------|-----------------|-----------------|-------------|------|-------------|
| **Average** | **0.531** | 0.502 | 0.496 | 0.440 | **+21%** |
| nfcorpus | 0.346 | 0.338 | 0.337 | 0.325 | +6% |
| fiqa | 0.408 | 0.368 | 0.354 | 0.236 | +73% |
| trec-covid | **0.834** | 0.750 | 0.726 | 0.656 | +27% |
| arguana | 0.366 | 0.494 | 0.465 | 0.315 | +16% |
| quora | **0.887** | 0.823 | 0.855 | 0.789 | +12% |
| scidocs | 0.186 | 0.169 | 0.154 | 0.158 | +18% |
| scifact | 0.678 | 0.701 | 0.689 | 0.665 | +2% |
| webis-touche | 0.274 | 0.270 | 0.260 | 0.367 | -25% |
| dbpedia-entity | 0.471 | 0.413 | 0.452 | 0.313 | +50% |
| fever | 0.805 | 0.795 | 0.785 | 0.753 | +7% |
| climate-fever | 0.239 | 0.196 | 0.176 | 0.213 | +12% |
| hotpotqa | **0.766** | 0.656 | 0.675 | 0.603 | +27% |
| nq | **0.640** | 0.549 | 0.524 | 0.329 | +95% |

**Key improvements:**
- **+3.5 points** average improvement over ColBERTv2.0
- **+9.1 points** absolute improvement over BM25 average
- Strong on: TREC-COVID, Quora, HotpotQA, NQ, FiQA
- Weak on: Webis-Touché (argument retrieval)

### Mixedbread mxbai-colbert-large-v1 (2024) - Re-ranking Performance

Evaluated as **re-ranker on top of BM25 retrieval** (not end-to-end):

| Dataset | mxbai-colbert | ColBERTv2 | Jina-ColBERT-v1 | BM25 baseline |
|---------|---------------|-----------|-----------------|---------------|
| **Average** | **0.504** | 0.431 | 0.498 | 0.440 |
| ArguAna | 0.331 | 0.300 | 0.334 | 0.315 |
| ClimateFEVER | **0.209** | 0.165 | 0.207 | 0.213 |
| DBPedia | 0.406 | 0.318 | **0.422** | 0.313 |
| FEVER | 0.808 | 0.651 | **0.811** | 0.753 |
| FiQA | **0.359** | 0.236 | 0.356 | 0.236 |
| HotPotQA | 0.676 | 0.633 | **0.688** | 0.603 |
| NFCorpus | 0.364 | 0.338 | **0.367** | 0.325 |
| NQ | **0.514** | 0.306 | 0.513 | 0.329 |
| Quora | **0.870** | 0.789 | 0.852 | 0.789 |
| SCIDOCS | **0.170** | 0.149 | 0.154 | 0.158 |
| SciFact | **0.715** | 0.679 | 0.702 | 0.665 |
| TREC-COVID | **0.810** | 0.595 | 0.750 | 0.656 |
| Webis-touché2020 | 0.317 | **0.442** | 0.321 | 0.367 |

**Note:** These are re-ranking results (BM25 first stage → ColBERT re-rank top-100), not end-to-end retrieval.

---

## Model Architecture Details

### BM25 (Baseline)
- **Type:** Lexical sparse retrieval
- **Training:** None (unsupervised)
- **Parameters:** k1=0.9, b=0.4 (Anserini defaults)
- **Storage:** TF-IDF index (~2-8 KB per document)
- **Speed:** Very fast (ms)

### Dense Retrieval (TAS-B)
- **Type:** Bi-encoder (dual-tower)
- **Model:** DistilBERT-based
- **Training:** MS MARCO with balanced topic sampling
- **Embedding dims:** 768
- **Storage:** 3 KB per document (vector only)
- **Speed:** Fast with ANN index (ms)

### ColBERT
- **Type:** Late-interaction multi-vector
- **Model:** BERT-base-uncased
- **Training:** MS MARCO for 300K steps
- **Embeddings:** 32-128 tokens × 128 dims = 16-64 KB per document
- **Storage:** 32x more than single-vector
- **Speed:** Medium (requires late-interaction scoring)

### BM25 + Cross-Encoder (BM25+CE)
- **Type:** Re-ranker (two-stage)
- **Model:** MiniLM 6-layer (384 hidden dims)
- **Training:** MS MARCO with knowledge distillation
- **First stage:** BM25 retrieves top-100
- **Second stage:** Cross-encoder re-ranks
- **Storage:** 0 (no indexing, inference only)
- **Speed:** Slow (~50-100ms per query-doc pair)

---

## Datasets by Domain

### Question Answering (QA)
- **NQ**: Natural Questions from Google Search
- **HotpotQA**: Multi-hop reasoning questions
- **FiQA**: Financial question answering

**Winner:** ColBERT (avg 0.478 vs BM25 0.389)

### Fact-Checking
- **FEVER**: Fact Extraction and VERification
- **Climate-FEVER**: Climate science claims
- **SciFact**: Scientific claim verification

**Winner:** Cross-encoder (avg 0.587 vs BM25 0.544)

### Biomedical IR
- **TREC-COVID**: COVID-19 research papers
- **NFCorpus**: Medical document retrieval
- **BioASQ**: Biomedical question answering

**Winner:** Cross-encoder (avg 0.543 vs BM25 0.482)

### Argument Retrieval
- **ArguAna**: Counter-argument retrieval
- **Touché-2020**: Argument search

**Winner:** Dense retrieval (avg 0.296 vs BM25 0.341) - **BM25 actually wins**

### Duplicate Question Retrieval
- **Quora**: Question pair similarity
- **CQADupStack**: Stack Exchange duplicates

**Winner:** ColBERT (avg 0.602 vs BM25 0.544)

### Other Tasks
- **DBPedia**: Entity retrieval
- **SCIDOCS**: Citation prediction
- **Signal-1M**: Tweet retrieval
- **TREC-NEWS**: News article retrieval
- **Robust04**: Classic ad-hoc retrieval

**Winner:** Mixed (task-dependent)

---

## Statistical Significance

From BEIR paper findings:

### Wins vs BM25 (number of datasets)

| Model | Wins | Ties | Losses | Win Rate |
|-------|------|------|--------|----------|
| **BM25+CE** | 16 | 0 | 3 | **84%** |
| **ColBERT** | 10 | 0 | 9 | 53% |
| **TAS-B** | 8 | 0 | 11 | 42% |
| **DPR** | 4 | 0 | 15 | 21% |
| **docT5query** | 11 | 0 | 8 | 58% |

**Key insight:** Cross-encoder re-ranking is the **most consistent** winner, beating BM25 on 84% of datasets.

### Performance Variance

| Model | Std Dev | Min | Max | Range |
|-------|---------|-----|-----|-------|
| **BM25** | 0.189 | 0.158 | 0.789 | 0.631 |
| **ColBERT** | 0.214 | 0.145 | 0.854 | 0.709 |
| **TAS-B** | 0.182 | 0.149 | 0.835 | 0.686 |
| **BM25+CE** | 0.207 | 0.166 | 0.819 | 0.653 |

**Key insight:** All models show high variance across datasets, reinforcing the need for diverse benchmarks.

---

## How to Interpret NDCG@10

**NDCG@10** (Normalized Discounted Cumulative Gain at rank 10):
- Measures ranking quality of top-10 results
- Accounts for both relevance and position
- Higher is better (max = 1.0)
- Penalizes relevant docs at lower ranks

### Score Interpretation

| NDCG@10 | Quality | Description |
|---------|---------|-------------|
| **0.0 - 0.2** | Poor | Most relevant docs not in top-10 |
| **0.2 - 0.4** | Fair | Some relevant docs in top-10 but poorly ranked |
| **0.4 - 0.6** | Good | Relevant docs consistently in top-10 |
| **0.6 - 0.8** | Very Good | Most relevant docs in top-5 |
| **0.8 - 1.0** | Excellent | Best relevant docs at rank 1-3 |

### Example

If your system scores **0.48 NDCG@10**:
- You're in "Good" territory
- Relevant notes usually appear in top-10
- May need improvement to get them in top-3 consistently
- Comparable to modern hybrid search systems

---

## Practical Takeaways

### For Small Corpus (<100K docs)

**Recommended stack:**
1. BM25 (baseline)
2. Single-vector dense (nomic-embed-text, BGE, E5)
3. RRF or relative score fusion
4. Optional: Cross-encoder re-ranker for top-20

**Expected NDCG@10:** 0.45-0.52 (Good to Very Good)

**Avoid:** ColBERT (diminishing returns on small corpus)

### For Medium Corpus (100K-1M docs)

**Recommended stack:**
1. Hybrid BM25 + dense with RRF
2. Consider ColBERT as re-ranker (not end-to-end)
3. Cross-encoder for critical queries

**Expected NDCG@10:** 0.50-0.58 (Good to Very Good)

### For Large Corpus (>1M docs)

**Recommended stack:**
1. Multi-stage: BM25/dense → ColBERT → Cross-encoder
2. Optimize for recall@1000 in first stage
3. Focus re-ranking budget on top-100

**Expected NDCG@10:** 0.55-0.65+ (Very Good to Excellent)

---

## Data Sources

1. **BEIR Paper:** Thakur et al., "BEIR: A Heterogeneous Benchmark for Zero-shot Evaluation of Information Retrieval Models," NeurIPS 2021
   - arXiv: https://arxiv.org/abs/2104.08663
   - GitHub: https://github.com/beir-cellar/beir

2. **Jina ColBERT v2:** https://huggingface.co/jinaai/jina-colbert-v2

3. **Mixedbread mxbai-colbert:** https://huggingface.co/mixedbread-ai/mxbai-colbert-large-v1

4. **BEIR Dataset:** https://huggingface.co/datasets/BeIR/beir

5. **BEIR Leaderboard (deprecated):** https://docs.google.com/spreadsheets/d/1L8aACyPaXrL8iEelJLGqlMqXKPX2oSP_R10pZoy77Ns

---

**Last Updated:** 2026-01-27

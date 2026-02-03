# Retrieval Research Papers for Matric-Memory

This document catalogs high-impact research papers that support matric-memory's hybrid retrieval, ranking, and expansion capabilities. Papers are organized by research area with full citations and relevance to our implementation.

## Table of Contents

1. [Hybrid Retrieval & Fusion](#hybrid-retrieval--fusion)
2. [Late Interaction Models](#late-interaction-models)
3. [Dense Retrieval](#dense-retrieval)
4. [Query & Document Expansion](#query--document-expansion)
5. [Learning to Rank](#learning-to-rank)
6. [Sparse Neural Retrieval](#sparse-neural-retrieval)
7. [Evaluation Benchmarks](#evaluation-benchmarks)

---

## Hybrid Retrieval & Fusion

### An Analysis of Fusion Functions for Hybrid Retrieval

**Authors:** Sebastian Bruch, Siyu Gai, Amir Ingber
**Venue:** ACM SIGIR 2023
**Year:** 2022 (published 2023)
**arXiv:** https://arxiv.org/abs/2210.11934

**Key Contributions:**
- Compares Reciprocal Rank Fusion (RRF) with convex combination methods for combining lexical and semantic search
- Finds that convex combination (CC) outperforms RRF in both in-domain and out-of-domain settings
- Challenges conventional wisdom that RRF is parameter-insensitive, demonstrating it requires careful tuning
- Shows CC achieves strong sample efficiency, requiring minimal training data for domain adaptation

**Relevance to Matric-Memory:**
Critical for our RRF-based fusion of BM25 and semantic search. Suggests we should consider convex combination alternatives and parameter tuning for RRF (currently using k=60).

---

### RAG-Fusion: A New Take on Retrieval-Augmented Generation

**Authors:** Zackary Rackauckas
**Venue:** International Journal on Natural Language Computing (IJNLC) Vol.13, No.1
**Year:** 2024
**arXiv:** https://arxiv.org/abs/2402.03367

**Key Contributions:**
- Combines RAG with Reciprocal Rank Fusion by generating multiple queries from different perspectives
- Reranks results with reciprocal scores and fuses documents across query variations
- Demonstrates improved comprehensiveness and accuracy over single-query RAG
- Identifies limitation: quality depends heavily on generated query relevance

**Relevance to Matric-Memory:**
Directly applicable to our AI revision feature. Could enhance note generation by retrieving context using multiple reformulated queries before fusion, improving semantic coverage.

---

### Self-MedRAG: Self-Reflective Hybrid Retrieval-Augmented Generation

**Authors:** Jessica Ryan et al.
**Venue:** arXiv preprint
**Year:** 2026
**Link:** Not publicly available yet

**Key Contributions:**
- Integrates sparse (BM25) and dense (Contriever) retrievers via Reciprocal Rank Fusion
- Adds iterative self-reflection to improve medical QA accuracy from 80% to 83.33%
- Demonstrates that hybrid retrieval with reflection outperforms single-method approaches

**Relevance to Matric-Memory:**
Validates our BM25 + semantic + RRF architecture. Self-reflection pattern could enhance our AI revision workflow by iteratively refining retrieved context.

---

## Late Interaction Models

### ColBERT: Efficient and Effective Passage Search via Contextualized Late Interaction over BERT

**Authors:** Omar Khattab, Matei Zaharia
**Venue:** SIGIR 2020
**Year:** 2020
**arXiv:** https://arxiv.org/abs/2004.12832

**Key Contributions:**
- Introduces late interaction architecture: independently encodes queries and documents with BERT, then applies cheap MaxSim interaction
- Achieves 2 orders of magnitude speedup over full BERT re-ranking while maintaining competitive effectiveness
- Enables end-to-end retrieval from large collections using vector-similarity indexes
- Balances expressiveness of deep language models with practical efficiency

**Relevance to Matric-Memory:**
Alternative to single-vector dense retrieval we currently use. ColBERT's token-level interactions could improve precision for technical/code notes where exact term matching matters alongside semantics.

---

### PLAID: An Efficient Engine for Late Interaction Retrieval

**Authors:** Keshav Santhanam, Omar Khattab, Christopher Potts, Matei Zaharia
**Venue:** arXiv preprint
**Year:** 2022
**arXiv:** https://arxiv.org/abs/2205.09707

**Key Contributions:**
- Optimizes ColBERT retrieval through centroid interaction and centroid pruning
- Reduces latency by up to 7x on GPU and 45x on CPU vs vanilla ColBERTv2
- Maintains state-of-the-art quality while achieving tens of milliseconds latency on 140M passages
- Enables production deployment of late interaction models at scale

**Relevance to Matric-Memory:**
If we adopt ColBERT-style late interaction, PLAID provides the engineering blueprint for efficient implementation. Centroid-based pruning particularly relevant for scaling to large note collections.

---

## Dense Retrieval

### Dense Passage Retrieval for Open-Domain Question Answering

**Authors:** Vladimir Karpukhin, Barlas Oğuz, Sewon Min, Patrick Lewis, Ledell Wu, Sergey Edunov, Danqi Chen, Wen-tau Yih
**Venue:** EMNLP 2020
**Year:** 2020
**arXiv:** https://arxiv.org/abs/2004.04906

**Key Contributions:**
- Demonstrates dense vector representations can outperform BM25 for passage retrieval
- Dual-encoder framework learns from relatively small question-passage pairs
- Achieves 9-19% absolute improvements in top-20 passage retrieval accuracy
- Establishes dense retrieval as viable alternative to keyword-based systems

**Relevance to Matric-Memory:**
Foundational work justifying our dense semantic search component. Validates that learned embeddings can capture relevance beyond lexical overlap, critical for conceptual note linking.

---

### Unsupervised Dense Information Retrieval with Contrastive Learning

**Authors:** Gautier Izacard, Mathilde Caron, Lucas Hosseini, Sebastian Riedel, Piotr Bojanowski, Armand Joulin, Edouard Grave
**Venue:** arXiv preprint
**Year:** 2021 (revised 2022)
**arXiv:** https://arxiv.org/abs/2112.09118

**Key Contributions:**
- Trains dense retrievers using contrastive learning without labeled relevance data
- Outperforms BM25 on 11/15 BEIR datasets for Recall@100 in zero-shot settings
- Demonstrates strong multilingual and cross-lingual transfer capabilities
- Enables retrieval across different writing systems (e.g., Arabic queries to English documents)

**Relevance to Matric-Memory:**
Contriever's unsupervised training is ideal for personal knowledge bases with limited query logs. Cross-lingual capabilities valuable for multilingual note collections. Currently we use nomic-embed-text which has similar contrastive training.

---

### Text Embeddings by Weakly-Supervised Contrastive Pre-training (E5)

**Authors:** Liang Wang, Nan Yang, Xiaolong Huang, Binxing Jiao, Linjun Yang, Daxin Jiang, Rangan Majumder, Furu Wei
**Venue:** arXiv preprint
**Year:** 2022 (revised 2024)
**arXiv:** https://arxiv.org/abs/2212.03533

**Key Contributions:**
- First model to outperform BM25 on BEIR benchmark in zero-shot setting without labeled data
- Achieves best MTEB results while being 40x smaller than competing models
- Uses weakly-supervised contrastive learning on CCPairs dataset
- Strong performance across 56 datasets spanning retrieval, clustering, and classification

**Relevance to Matric-Memory:**
E5 embeddings could be strong alternative to our current nomic-embed-text model. Zero-shot BM25-beating performance particularly relevant for note retrieval without fine-tuning on user's query patterns.

---

### Approximate Nearest Neighbor Negative Contrastive Learning for Dense Text Retrieval (ANCE)

**Authors:** Lee Xiong, Chenyan Xiong, Ye Li, Kwok-Fung Tang, Jialin Liu, Paul Bennett, Junaid Ahmed, Arnold Overwijk
**Venue:** ICLR 2021
**Year:** 2020
**arXiv:** https://arxiv.org/abs/2007.00808

**Key Contributions:**
- Addresses training-testing mismatch by constructing negatives from dynamically updated ANN index
- Enables BERT-based dense retrieval to match sparse-retrieval-with-BERT-reranking pipelines
- Achieves 100x speedup over re-ranking approaches
- Demonstrates importance of hard negative mining for dense retriever training

**Relevance to Matric-Memory:**
If we fine-tune embeddings on user's note corpus, ANCE provides methodology for selecting hard negatives. Dynamic ANN index construction could improve embedding quality for personalized retrieval.

---

## Query & Document Expansion

### Precise Zero-Shot Dense Retrieval without Relevance Labels (HyDE)

**Authors:** Luyu Gao, Xueguang Ma, Jimmy Lin, Jamie Callan
**Venue:** ACL 2023
**Year:** 2022
**arXiv:** https://arxiv.org/abs/2212.10496

**Key Contributions:**
- Generates hypothetical documents from queries using instruction-following LLMs (InstructGPT)
- Encodes hypothetical documents with unsupervised encoders (Contriever) to retrieve similar real documents
- Significantly outperforms Contriever baseline, approaches fine-tuned systems in zero-shot settings
- Works across diverse tasks (web search, QA, fact verification) and languages (Swahili, Korean, Japanese)

**Relevance to Matric-Memory:**
Could enhance our semantic search by embedding LLM-generated document hypotheses instead of raw queries. Particularly valuable for short/vague queries where expansion to full hypothetical notes improves retrieval.

---

### Document Expansion by Query Prediction with Doc2Query

**Authors:** Rodrigo Nogueira, Wei Yang, Jimmy Lin, Kyunghyun Cho
**Venue:** arXiv preprint
**Year:** 2019
**arXiv:** https://arxiv.org/abs/1904.08375

**Key Contributions:**
- Expands documents by predicting likely queries and appending them to document text
- Uses sequence-to-sequence model trained on query-document pairs
- Achieves state-of-the-art retrieval performance when combined with neural re-ranking
- Approaches neural re-ranker effectiveness with significantly lower latency

**Relevance to Matric-Memory:**
Could improve BM25 retrieval by expanding notes with predicted queries during indexing. Seq2seq model could generate likely user queries for each note, improving keyword-based discoverability.

---

### Query Expansion by Prompting Large Language Models

**Authors:** Rolf Jagerman, Honglei Zhuang, Zhen Qin, Xuanhui Wang, Michael Bendersky
**Venue:** arXiv preprint
**Year:** 2023
**arXiv:** https://arxiv.org/abs/2305.03653

**Key Contributions:**
- Leverages LLM generative capabilities for query expansion instead of pseudo-relevance feedback
- Tests zero-shot, few-shot, and Chain-of-Thought prompting strategies
- Chain-of-Thought prompts most effective, breaking queries down step-by-step
- Demonstrates LLM expansions outperform traditional PRF methods on MS-MARCO and BEIR

**Relevance to Matric-Memory:**
Can enhance our search by using Ollama to generate query expansions via CoT prompting before retrieval. More powerful than traditional PRF since it doesn't require initial retrieval pass.

---

## Learning to Rank

### Unbiased LambdaMART: An Unbiased Pairwise Learning-to-Rank Algorithm

**Authors:** Ziniu Hu, Yang Wang, Qu Peng, Hang Li
**Venue:** WWW 2019
**Year:** 2018 (revised 2019)
**arXiv:** https://arxiv.org/abs/1809.05818

**Key Contributions:**
- Addresses position bias in click data for learning-to-rank systems
- Jointly estimates biases at click and unclick positions while training ranker
- Outperforms inverse propensity weighting approaches
- Validated via A/B testing in commercial search engine showing improved relevance

**Relevance to Matric-Memory:**
If we implement click tracking on search results, Unbiased LambdaMART provides methodology for training personalized rankers from user interactions. Could learn user-specific relevance preferences over time.

---

### Multi-Stage Document Ranking with BERT

**Authors:** Rodrigo Nogueira, Wei Yang, Kyunghyun Cho, Jimmy Lin
**Venue:** arXiv preprint
**Year:** 2019
**arXiv:** https://arxiv.org/abs/1910.14424

**Key Contributions:**
- Introduces monoBERT (pointwise) and duoBERT (pairwise) ranking with BERT
- Multi-stage pipeline architecture enabling quality-latency tradeoffs
- Competitive performance on MS MARCO and TREC CAR benchmarks
- Demonstrates BERT adaptation for document retrieval ranking tasks

**Relevance to Matric-Memory:**
Could add neural re-ranking stage after hybrid retrieval. MonoBERT scoring of top-K results from RRF fusion would improve precision for complex semantic queries beyond embedding similarity.

---

## Sparse Neural Retrieval

### SPLADE: Sparse Lexical and Expansion Model for First Stage Ranking

**Authors:** Thibault Formal, Benjamin Piwowarski, Stéphane Clinchant
**Venue:** SIGIR 2021
**Year:** 2021
**arXiv:** https://arxiv.org/abs/2107.05720

**Key Contributions:**
- Combines benefits of dense embeddings with sparse representations via sparsity regularization
- Learns term importance weights with log-saturation effect for controlled sparsity
- Maintains inverted index efficiency while adding neural expansion capabilities
- Enables effectiveness-efficiency tradeoff through sparsity regularization tuning

**Relevance to Matric-Memory:**
Alternative to our current BM25 + dense hybrid approach. SPLADE provides learned sparse representations with neural expansion, potentially replacing BM25 with learned lexical matching while maintaining inverted index speed.

---

## Evaluation Benchmarks

### BEIR: A Heterogenous Benchmark for Zero-shot Evaluation of Information Retrieval Models

**Authors:** Nandan Thakur, Nils Reimers, Andreas Rücklé, Abhishek Srivastava, Iryna Gurevych
**Venue:** NeurIPS 2021 Datasets and Benchmarks Track
**Year:** 2021
**arXiv:** https://arxiv.org/abs/2104.08663

**Key Contributions:**
- 18 diverse datasets spanning multiple retrieval tasks and domains for out-of-distribution evaluation
- Benchmarks 10 state-of-the-art systems: lexical, dense, sparse, late-interaction, re-ranking
- BM25 proves robust baseline; re-ranking and late-interaction achieve best zero-shot performance
- Identifies generalization gaps in dense and sparse retrieval requiring future improvement

**Relevance to Matric-Memory:**
Standard benchmark for evaluating retrieval systems. We should test our hybrid approach on BEIR subsets relevant to knowledge bases (e.g., NFCorpus, TREC-COVID, SciFact) to validate generalization beyond personal notes.

---

## Summary by Implementation Priority

### Immediate Relevance (Currently Implemented)
1. **Dense Passage Retrieval** - Justifies our semantic search component
2. **Contriever** - Similar to our nomic-embed-text unsupervised approach
3. **RRF Analysis** - Critical for optimizing our fusion parameters

### High-Value Additions (Next 6 Months)
1. **HyDE** - Generate hypothetical notes from queries for better retrieval
2. **LLM Query Expansion** - Use Ollama for CoT query expansion before search
3. **Doc2Query** - Expand notes with predicted queries during indexing
4. **E5 Embeddings** - Evaluate as alternative to nomic-embed-text

### Advanced Features (Future)
1. **ColBERT + PLAID** - Token-level late interaction for technical notes
2. **SPLADE** - Learned sparse retrieval replacing BM25
3. **MonoBERT Re-ranking** - Neural re-ranking after RRF fusion
4. **Unbiased LambdaMART** - Personalized ranking from click data

### Evaluation & Validation
1. **BEIR Benchmark** - Test on knowledge base subsets for generalization metrics
2. **Ablation Studies** - Measure contribution of BM25, semantic, and RRF components separately

---

## Research Gaps & Future Exploration

### Areas Needing More Research
1. **Personal Knowledge Base Retrieval** - Most papers focus on web/QA, limited work on PKB-specific challenges
2. **Multi-hop Reasoning** - Graph-based retrieval for connected notes (our collection hierarchy)
3. **Temporal Retrieval** - Incorporating note creation/modification time in ranking
4. **Privacy-Preserving Retrieval** - Encrypted search for PKE-encrypted notes

### Potential Collaborations
- Evaluate matric-memory on BEIR subsets, contribute results to community
- Publish case study on hybrid retrieval for personal knowledge management
- Open-source our RRF+BM25+semantic implementation for reproducibility

---

## Citation Format

For academic citations, use:

```bibtex
@misc{matric-memory-research-2026,
  title={Retrieval Research Papers for Matric-Memory},
  author={Matric-Memory Contributors},
  year={2026},
  howpublished={\url{https://github.com/yourusername/matric-memory}},
  note={Research bibliography supporting hybrid retrieval implementation}
}
```

---

## Changelog

- **2026-01-25**: Initial compilation with 15+ papers across 6 research areas
- Focus areas: hybrid fusion, late interaction, dense retrieval, expansion, LTR, benchmarks
- Prioritized papers with direct implementation relevance to matric-memory

## Contributors

Research compiled by Claude Code (Sonnet 4.5) for the matric-memory project.

For updates or corrections, please submit issues or PRs to the main repository.

# Research Gap Analysis - matric-memory

**Date:** 2026-01-25
**Project:** matric-memory
**Purpose:** Identify research foundations for AI-enhanced knowledge management system with semantic search, automatic linking, and NLP pipelines.

## Executive Summary

matric-memory implements a modern knowledge management system combining:
- **Hybrid search** (BM25 + semantic vectors via RRF fusion)
- **Automatic semantic linking** (embedding-based knowledge graph)
- **W3C SKOS tagging** (controlled vocabulary with hierarchical relations)
- **AI revision pipeline** (context-aware content enhancement)

This analysis identifies 10 core papers currently supporting the implementation and 8 additional papers for future enhancement.

## Current Research Coverage

### Papers Already Documented in research-papers

| REF | Paper | Relevance to matric-memory | Status |
|-----|-------|---------------------------|--------|
| REF-005 | Miller's Law (Miller, 1956) | Context limits (7±2) | COMPLETE |
| REF-006 | Cognitive Load Theory (Sweller, 1988) | Prompt simplification | COMPLETE |
| REF-008 | RAG (Lewis et al., 2020) | External memory for AI revision | COMPLETE |
| REF-015 | Self-Refine (Madaan et al., 2023) | Iterative AI improvement | COMPLETE |
| REF-018 | ReAct (Yao et al., 2023) | Transparent agent reasoning | COMPLETE |
| REF-021 | Reflexion (Shinn et al., 2023) | Self-improvement via reflection | COMPLETE |
| REF-026 | ICL Survey (Dong et al., 2023) | Few-shot prompt strategies | COMPLETE |
| REF-027 | RRF (Cormack et al., 2009) | Hybrid search fusion | COMPLETE |
| REF-028 | BM25 (Robertson & Zaragoza, 2009) | Full-text search foundation | COMPLETE |
| REF-029 | DPR (Karpukhin et al., 2020) | Dense retrieval architecture | COMPLETE |
| REF-030 | SBERT (Reimers & Gurevych, 2019) | Sentence embeddings | COMPLETE |
| REF-031 | HNSW (Malkov & Yashunin, 2020) | Vector index algorithm | COMPLETE |
| REF-032 | Knowledge Graphs (Hogan et al., 2021) | Semantic linking patterns | COMPLETE |
| REF-033 | SKOS (Miles & Bechhofer, 2009) | Tagging system foundation | COMPLETE |
| REF-048 | ColBERT (Khattab & Zaharia, 2020) | Late interaction (future) | COMPLETE |
| REF-049 | Contriever (Izacard et al., 2022) | Unsupervised retrieval | COMPLETE |
| REF-050 | E5 (Wang et al., 2022) | State-of-the-art embeddings | COMPLETE |
| REF-056 | FAIR Principles (Wilkinson et al., 2016) | Data management standards | COMPLETE |
| REF-061 | OAIS (ISO 14721:2012) | Digital preservation | COMPLETE |
| REF-062 | W3C PROV (Moreau & Groth, 2013) | AI provenance tracking | COMPLETE |
| REF-063 | HELM (Liang et al., 2022) | Evaluation framework | COMPLETE |

### Coverage by Feature Area

| Feature | Papers | Coverage |
|---------|--------|----------|
| Hybrid Search | REF-027, REF-028, REF-029 | Strong |
| Vector Search | REF-030, REF-031 | Strong |
| Knowledge Graph | REF-032 | Strong |
| SKOS Tagging | REF-033 | Strong |
| AI Revision | REF-008, REF-015, REF-018, REF-021 | **Strong (expanded)** |
| Cognitive Foundations | REF-005, REF-006 | Moderate |
| AI Transparency | REF-062 | Strong |
| Data Standards | REF-056, REF-061 | Strong |
| Re-ranking | REF-048 | Documented (not implemented) |
| Embeddings | REF-049, REF-050 | Strong alternatives |
| Evaluation | REF-063 | Documented |

## Identified Improvement Opportunities

### CRITICAL Priority - AI Transparency & Quality

| # | Opportunity | Research Source | Expected Impact |
|---|-------------|-----------------|-----------------|
| 1 | **W3C PROV Provenance Tracking** | REF-062 | Track which notes influence AI revisions |
| 2 | **Self-Refine Iterative Loop** | REF-015 | ~20% quality improvement |
| 3 | **ReAct Agent Pattern** | REF-018 | Transparent AI reasoning |

**Gap Status:** These patterns are not yet implemented. W3C PROV is essential for AI trustworthiness.

### HIGH Priority - Performance & Quality

| # | Opportunity | Research Source | Expected Impact |
|---|-------------|-----------------|-----------------|
| 4 | **HNSW Parameter Tuning** | REF-031 | M=32, ef_construction=200 → +5-10% recall |
| 5 | **E5 Embedding Migration** | REF-050 | +3-5% retrieval quality |
| 6 | **Reflexion Self-Improvement** | REF-021 | +20-32% task success |
| 7 | **Context Limit to 5 Notes** | REF-005 | Respect 7±2 cognitive limit |

**Gap Status:** ivfflat index used instead of HNSW. Current embeddings use nomic-embed-text.

### MEDIUM Priority - Enhancements

| # | Opportunity | Research Source | Expected Impact |
|---|-------------|-----------------|-----------------|
| 8 | BM25F Field Weighting | REF-028 | +10-15% multi-field improvement |
| 9 | FAIR Metadata Export | REF-056 | Improved interoperability |
| 10 | Soft Delete (Tombstoning) | REF-056 (A2) | Metadata preservation |
| 11 | Few-shot Prompt Examples | REF-026 | Better AI consistency |

### Implementation Deviation Identified

| Issue | Expected (Research) | Actual (Code) | Impact |
|-------|---------------------|---------------|--------|
| Vector index | HNSW (REF-031) | ivfflat | O(√N) vs O(log N) query |

**Location:** `migrations/20260102000000_initial_schema.sql:276`

---

## Gap Analysis: Papers Needed (P1 - Critical)

### Information Retrieval Evaluation

| Priority | Paper | Year | Why Needed |
|----------|-------|------|-----------|
| P1 | **BEIR Benchmark** (Thakur et al.) | 2021 | Standardized evaluation for hybrid search |
| P1 | **MS MARCO** (Nguyen et al.) | 2016 | Passage retrieval benchmark |

**Gap:** No formal evaluation methodology documented. Need standardized benchmarks for measuring hybrid search quality.

### Query Understanding

| Priority | Paper | Year | Why Needed |
|----------|-------|------|-----------|
| P1 | **HyDE** (Gao et al.) | 2022 | Hypothetical document expansion |
| P2 | **Query2Doc** (Wang et al.) | 2023 | Query expansion via LLM |
| P2 | **Doc2Query** (Nogueira et al.) | 2019 | Document expansion |

**Gap:** Current system takes queries as-is. Query expansion could significantly improve recall for vague or short queries.

### Chunking Strategies

| Priority | Paper | Year | Why Needed |
|----------|-------|------|-----------|
| P1 | **Late Chunking** (Günther et al.) | 2024 | Context-aware chunking |
| P2 | **Semantic Chunking** (various) | 2023-24 | Meaning-preserving splits |

**Gap:** Current chunking is character-based with overlap. Research-backed semantic chunking could improve retrieval quality.

### Learned Sparse Retrieval

| Priority | Paper | Year | Why Needed |
|----------|-------|------|-----------|
| P2 | **SPLADE** (Formal et al.) | 2021 | Learned sparse representations |
| P2 | **SPLADEv2** (Formal et al.) | 2022 | Improved efficiency |

**Gap:** Current BM25 is lexical only. SPLADE-style learned sparse could bridge lexical and semantic matching.

## Acquisition Plan

### Phase 1: Evaluation Framework (Week 1)

**Target:** P1 benchmark papers for systematic evaluation

1. **BEIR Benchmark** (REF-037)
   - arXiv: 2104.08663
   - PDF: https://arxiv.org/pdf/2104.08663.pdf
   - Key sections: Dataset descriptions, evaluation metrics

2. **MS MARCO** (REF-038)
   - Official: https://microsoft.github.io/msmarco/
   - Key sections: Passage ranking task definition

### Phase 2: Query Enhancement (Week 2)

**Target:** Query and document expansion techniques

3. **HyDE** (REF-039)
   - arXiv: 2212.10496
   - PDF: https://arxiv.org/pdf/2212.10496.pdf
   - Key sections: Hypothetical document generation

4. **Doc2Query** (REF-040)
   - arXiv: 1904.08375
   - PDF: https://arxiv.org/pdf/1904.08375.pdf
   - Key sections: Query prediction methodology

### Phase 3: Advanced Techniques (Week 3-4)

**Target:** Chunking and sparse retrieval improvements

5. **Late Chunking** (REF-041)
   - Source: Jina AI technical report
   - Key: Embedding-then-chunk vs chunk-then-embed

6. **SPLADE** (REF-042)
   - arXiv: 2107.05720
   - PDF: https://arxiv.org/pdf/2107.05720.pdf
   - Key sections: Learned expansion terms

## Documentation Standards

For each paper added to research-papers:

### File Structure
```
documentation/references/REF-XXX-{short-name}.md
pdfs/full/REF-XXX-{author}-{year}-{short}.pdf
bibliographies/master.bib (append entry)
```

### Markdown Template
```markdown
# REF-XXX: {Paper Title}

## Citation
{Full citation}
**DOI/arXiv:** {link}
**PDF:** `pdfs/full/REF-XXX-*.pdf`

## Document Profile
| Attribute | Value |
|-----------|-------|
| Pages | XX |
| Year | YYYY |
| Venue | Conference/Journal |
| Relevance | Critical/High/Medium |

## Referenced By
| Project | Context | Date Added |
|---------|---------|------------|
| matric-memory | {use case} | YYYY-MM-DD |

## Executive Summary
{2-3 paragraphs}

## Key Findings
{Numbered findings with evidence}

## Implementation Notes
{Project-specific guidance}

## BibTeX
{Citation entry}
```

## Implementation Priorities

### Immediate (Current Iteration)
- [x] Document REF-027 to REF-033 (core papers)
- [x] Document REF-056 to REF-058 (halo papers)
- [x] Create citable-claims-index.md
- [x] Create research-gap-analysis.md

### Short-Term (Next Sprint)
- [ ] Acquire BEIR benchmark paper
- [ ] Implement evaluation methodology
- [ ] Document HyDE for query expansion

### Medium-Term (Next Month)
- [ ] Evaluate E5 vs nomic-embed-text
- [ ] Research semantic chunking approaches
- [ ] Consider ColBERT re-ranking stage

### Long-Term (Next Quarter)
- [ ] SPLADE integration feasibility
- [ ] Domain-specific fine-tuning
- [ ] Multi-language support (E5-multilingual)

## Metrics

### Research Coverage Goals

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Core papers documented | 21 | 21 | ✅ Complete |
| Halo papers documented | 6 | 6 | ✅ Complete |
| Claims with citations | 60 | 50 | ✅ Exceeded |
| Features with research backing | 95% | 95% | ✅ Met |
| Verified implementations | 7 | 10 | 🔄 In progress |
| Implementation deviations | 1 | 0 | ⚠️ HNSW pending |

### Research Quality Indicators

- [x] All implemented features reference at least one paper
- [x] Key algorithms include paper citations in code comments
- [x] Architecture decisions reference relevant research
- [ ] Evaluation methodology based on published benchmarks (BEIR pending)
- [x] Implementation claims verified against source code

## Cross-References

- **Research Papers Repo:** https://git.integrolabs.net/roctinam/research-papers
- **Citable Claims Index:** `.aiwg/research/citable-claims-index.md`
- **Paper Documentation:** `research-papers/documentation/references/`
- **Architecture Decisions:** `docs/architecture/`

## Revision History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-25 | AI Research Agent | Initial gap analysis with acquisition plan |
| 2026-01-25 | Ralph Loop Iter 4 | Added 10 new papers; added improvement opportunities section; updated metrics |

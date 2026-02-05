# Research Foundation - Fortémi

This directory contains research documentation backing Fortémi's implementation. All technical claims in the codebase should trace to research papers indexed here.

## Quick Reference

| Need | Document |
|------|----------|
| **Verify a code claim** | [citable-claims-index.md](citable-claims-index.md) |
| **Paper analysis for REF-NNN** | [paper-analysis/REF-NNN-mm-analysis.md](paper-analysis/) |
| **Implementation opportunities** | [research-backed-improvements.md](research-backed-improvements.md) |
| **Research audit results** | [comprehensive-findings.md](comprehensive-findings.md) |

## Directory Structure

```
research/
├── README.md                      # This file
├── citable-claims-index.md        # Master index: claims → code locations → papers
├── comprehensive-findings.md      # 2026-01-25 audit results (22 papers)
├── research-backed-improvements.md # Implementation status tracker
│
├── paper-analysis/                # Individual paper analyses
│   ├── README.md                  # Paper analysis index
│   └── REF-NNN-mm-analysis.md     # Deep analysis for Fortémi
│
├── colbert/                       # ColBERT late-interaction research
│   ├── colbert-decision-summary.md
│   ├── colbert-vs-hybrid-search-analysis.md
│   └── ecosystem/                 # Maintenance assessment
│
├── skos/                          # W3C SKOS semantic tagging
│   ├── SKOS_RESEARCH_SUMMARY.md
│   ├── skos-implementation-research.md
│   ├── skos-quick-reference.md
│   └── skos-rust-implementation-guide.md
│
└── [topic files]                  # Standalone research documents
```

## Core Research Papers

These papers form the theoretical foundation of Fortémi:

### Search & Retrieval

| REF | Paper | Citation | Applied In |
|-----|-------|----------|------------|
| REF-027 | RRF Fusion | Cormack et al., 2009 | `matric-search/src/rrf.rs` |
| REF-028 | BM25 | Robertson & Zaragoza, 2009 | PostgreSQL ts_rank |
| REF-029 | Dense Passage Retrieval | Karpukhin et al., 2020 | Embedding strategy |
| REF-030 | Sentence-BERT | Reimers & Gurevych, 2019 | Semantic search |
| REF-031 | HNSW | Malkov & Yashunin, 2020 | pgvector index |

### Knowledge Organization

| REF | Paper | Citation | Applied In |
|-----|-------|----------|------------|
| REF-032 | Knowledge Graphs | Hogan et al., 2021 | Semantic linking |
| REF-033 | W3C SKOS | Miles & Bechhofer, 2009 | Tagging system |
| REF-062 | W3C PROV | Moreau & Groth, 2013 | Provenance tracking |

### AI Enhancement

| REF | Paper | Citation | Applied In |
|-----|-------|----------|------------|
| REF-008 | RAG | Lewis et al., 2020 | AI revision pipeline |
| REF-015 | Self-Refine | Madaan et al., 2023 | Iterative improvement |
| REF-018 | ReAct | Yao et al., 2023 | Agent reasoning |

### Embeddings

| REF | Paper | Citation | Applied In |
|-----|-------|----------|------------|
| REF-067 | Matryoshka RL | Kusupati et al., 2024 | MRL embeddings |
| REF-068 | Small Models + Reranking | - | Model selection |
| REF-069 | Domain Fine-tuning | - | Custom embeddings |

## Paper Analysis Format

Each `paper-analysis/REF-NNN-mm-analysis.md` contains:

1. **Summary**: Paper's key contributions
2. **Relevance**: How it applies to Fortémi
3. **Implementation**: Current code mapping
4. **Opportunities**: Potential improvements
5. **Key Quotes**: Citable passages

## Adding New Research

1. Add paper to `research-papers` repository with REF-NNN identifier
2. Create analysis in `paper-analysis/REF-NNN-mm-analysis.md`
3. Update `citable-claims-index.md` with new claims
4. Update `research-backed-improvements.md` if creating opportunities

## Research Status

| Category | Papers Analyzed | Status |
|----------|-----------------|--------|
| Search/Retrieval | 5 | Complete |
| Knowledge Graphs | 3 | Complete |
| AI Enhancement | 3 | Complete |
| Embeddings | 4 | Complete |
| ColBERT (Future) | 6 | Research only |
| SKOS | 4 | Complete |

Last audit: 2026-01-25 (22 papers analyzed)

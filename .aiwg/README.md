# Internal Documentation - Fortémi

This directory contains internal specifications, research, and working documents for the Fortémi project. These are **internal documents** for development, troubleshooting, and system evolution.

Architecture Decision Records (ADRs) live in `docs/architecture/adr/`.

## Directory Structure

```
.aiwg/
├── README.md                 # This file
├── frameworks/               # AIWG framework registry
│   └── registry.json
├── research/                 # Research foundation
│   ├── citable-claims-index.md  # Master reference linking
│   ├── paper-analysis/       # Individual paper analyses (REF-*)
│   └── *.md                  # Topic-specific research synthesis
├── specs/                    # Promoted design specifications
├── testing/                  # Test plans and strategy
│   ├── test-strategy.md
│   ├── mcp-validation-test-plan.md
│   └── production-test-plan.md
└── reports/uat/              # UAT reports by release
    ├── RELEASE-UAT-REPORT.md
    └── v11/
```

## Quick Reference

| Need | Location |
|------|----------|
| **Why was X decided?** | `docs/architecture/adr/ADR-*.md` |
| **Research backing a feature** | `research/citable-claims-index.md` |
| **Paper analysis for REF-NNN** | `research/paper-analysis/REF-NNN-mm-analysis.md` |
| **Test coverage and strategy** | `testing/test-strategy.md` |
| **MCP integration tests** | `testing/mcp-validation-test-plan.md` |
| **Production deployment tests** | `testing/production-test-plan.md` |
| **Feature design specs** | `specs/` |

## ADR Categories

ADRs are in `docs/architecture/adr/` and organized by domain:

| Range | Domain |
|-------|--------|
| 001-005 | Inference Backend |
| 006-010 | Encryption |
| 011-016 | Core Architecture |
| 017-021 | Multilingual FTS |
| 022-027 | Embedding System |
| 028-030 | Backup/Migration |
| 031-036 | File Handling |
| 037-046 | Eventing & Streaming |
| 047-077 | Multi-Memory, Jobs, Search |
| 078-083 | Graph Quality Pipeline, Brand |

## Research Reference Format

Research papers are referenced as `REF-NNN` and linked in two places:

1. **`citable-claims-index.md`** - Maps claims to code locations
2. **`paper-analysis/REF-NNN-mm-analysis.md`** - Deep analysis for Fortémi

Key references:
- REF-027: RRF fusion (Cormack 2009)
- REF-030: SBERT embeddings (Reimers 2019)
- REF-031: HNSW indexing (Malkov 2020)
- REF-033: W3C SKOS (Miles 2009)

## Cleanup Policy

- Completed work (closed issues, shipped features) should be deleted, not archived
- Git history preserves all prior content
- Only keep actively referenced documents in the working tree

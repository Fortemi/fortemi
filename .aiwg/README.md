# Internal Documentation - Fortémi

This directory contains internal specifications, architecture decisions, research, and working documents for the Fortémi project. These are **internal documents** for development, troubleshooting, adaptation, and system evolution.

## Directory Structure

```
.aiwg/
├── README.md                 # This file - navigation guide
├── architecture/             # Architecture Decision Records (ADRs)
│   ├── README.md             # ADR index and template reference
│   └── ADR-*.md              # Individual decision records (001-036)
├── specs/                    # Promoted design specifications
│   └── *.md                  # Finalized system designs
├── research/                 # Research foundation
│   ├── citable-claims-index.md  # Master reference linking
│   ├── paper-analysis/       # Individual paper analyses (REF-*)
│   └── *.md                  # Topic-specific research synthesis
├── testing/                  # Test strategy and results
│   └── test-strategy.md      # Testing approach and coverage
├── intake/                   # Project intake documents (private)
├── working/                  # Active work-in-progress
│   ├── discovery/            # Feature discovery phase
│   ├── checklists/           # Operational checklists
│   └── synthesis-reports/    # Cross-cutting analysis
├── archive/                  # Historical records
│   └── YYYY-MM/              # Date-organized archives
└── frameworks/               # Registry configuration
```

## Quick Reference

| Need | Location |
|------|----------|
| **Why was X decided?** | `architecture/ADR-*.md` |
| **Research backing a feature** | `research/citable-claims-index.md` |
| **Paper analysis for REF-NNN** | `research/paper-analysis/REF-NNN-mm-analysis.md` |
| **Test coverage and strategy** | `testing/test-strategy.md` |
| **Feature design specs** | `specs/` |
| **Active feature work** | `working/discovery/` |
| **Release checklists** | `working/checklists/` |

## ADR Categories

Architecture Decision Records are organized by domain:

| Range | Domain | Examples |
|-------|--------|----------|
| 001-005 | Inference Backend | Trait abstraction, feature flags, config priority |
| 006-010 | Encryption | Symmetric, envelope, magic bytes, JSON headers |
| 011-016 | Core Architecture | Hybrid search, semantic linking, SKOS, HNSW, strict filtering |
| 017-021 | Multilingual FTS | Strategy, websearch, script detection, multi-index |
| 022-027 | Embedding System | Set types, MRL, auto-embed rules, document types, chunking |
| 028-030 | Backup/Migration | Shard migration, schema versioning, UX |
| 031-036 | File Handling | Attachments, provenance, storage, 3D, media, safety |

## Research Reference Format

Research papers are referenced as `REF-NNN` and linked in two places:

1. **`citable-claims-index.md`** - Maps claims to code locations
2. **`paper-analysis/REF-NNN-mm-analysis.md`** - Deep analysis for Fortémi

Key references:
- REF-027: RRF fusion (Cormack 2009)
- REF-030: SBERT embeddings (Reimers 2019)
- REF-031: HNSW indexing (Malkov 2020)
- REF-033: W3C SKOS (Miles 2009)

## Working Directory Lifecycle

```
working/discovery/   →   working/elaboration/   →   specs/   →   architecture/ADR-*.md
     (research)              (design)            (finalized)      (decision record)
```

Files move right as they mature. Completed features have:
1. ADR in `architecture/`
2. Optional spec in `specs/` (for complex features)
3. Discovery docs archived in `archive/YYYY-MM/`

## Archive Policy

- Ralph completion reports: `archive/YYYY-MM/ralph/`
- Status reports: `archive/YYYY-MM/status-reports/`
- Completed working docs: `archive/YYYY-MM/`
- Intake documents are retained indefinitely (private)

## Maintenance

When adding new documents:

1. **ADRs**: Copy `architecture/ADR-TEMPLATE.md`, use next available number
2. **Research**: Add paper analysis to `paper-analysis/`, update `citable-claims-index.md`
3. **Designs**: Start in `working/discovery/`, promote to `specs/` when stable
4. **Archive**: Date-prefix and move to `archive/YYYY-MM/` when superseded

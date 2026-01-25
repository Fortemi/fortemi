# Architecture Decision Records - matric-memory

This directory contains Architecture Decision Records (ADRs) documenting significant technical decisions made during the development of matric-memory.

## ADR Index

### Inference Backend (from ARCH-010)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-001](ADR-001-trait-based-backend-abstraction.md) | Trait-Based Backend Abstraction | Accepted | 2026-01-22 |
| [ADR-002](ADR-002-feature-flags-optional-backends.md) | Feature Flags for Optional Backends | Accepted | 2026-01-22 |
| [ADR-003](ADR-003-configuration-priority-order.md) | Configuration Priority Order | Accepted | 2026-01-22 |
| [ADR-004](ADR-004-unified-error-types.md) | Unified Error Types | Accepted | 2026-01-22 |
| [ADR-005](ADR-005-optional-streaming-support.md) | Optional Streaming Support | Accepted | 2026-01-22 |

### Encryption (from ARCH-015)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-006](ADR-006-symmetric-only-encryption.md) | Symmetric-Only Encryption for v1.0 | Accepted | 2026-01-22 |
| [ADR-007](ADR-007-envelope-encryption-e2e.md) | Envelope Encryption for E2E Multi-Recipient | Accepted | 2026-01-22 |
| [ADR-008](ADR-008-magic-bytes-format-detection.md) | Magic Bytes for Format Detection | Accepted | 2026-01-22 |
| [ADR-009](ADR-009-json-headers-over-binary.md) | JSON Headers Over Binary | Accepted | 2026-01-22 |
| [ADR-010](ADR-010-in-memory-encryption.md) | In-Memory Encryption vs Streaming | Accepted | 2026-01-22 |

### Core Architecture

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-011](ADR-011-hybrid-search-rrf-fusion.md) | Hybrid Search with RRF Fusion | Accepted | 2026-01-02 |
| [ADR-012](ADR-012-semantic-linking-threshold.md) | Semantic Linking with 0.7 Similarity Threshold | Accepted | 2026-01-02 |
| [ADR-013](ADR-013-skos-tagging-system.md) | W3C SKOS-Based Tagging System | Accepted | 2026-01-02 |
| [ADR-014](ADR-014-pgvector-hnsw-indexing.md) | pgvector with HNSW Indexing | Accepted | 2026-01-02 |
| [ADR-015](ADR-015-workspace-crate-structure.md) | Cargo Workspace with Domain-Driven Crates | Accepted | 2026-01-02 |

## ADR Template

See [ADR-TEMPLATE.md](ADR-TEMPLATE.md) for the template used when creating new ADRs.

## Cross-References

- **Detailed Architecture Documents:** `.aiwg/working/elaboration/ARCH-*.md`
- **Research Foundation:** `.aiwg/research/citable-claims-index.md`
- **Implementation:** `crates/` directory

## How to Add a New ADR

1. Copy `ADR-TEMPLATE.md` to `ADR-NNN-<short-name>.md`
2. Fill in all sections
3. Update this README with the new entry
4. Reference the ADR in code comments where relevant

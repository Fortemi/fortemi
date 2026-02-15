# Architecture Decision Records - Fortémi

This directory contains Architecture Decision Records (ADRs) documenting significant technical decisions made during the development of Fortémi.

## ADR Index

### Inference Backend (ADR-001 to ADR-005)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-001](ADR-001-trait-based-backend-abstraction.md) | Trait-Based Backend Abstraction | Accepted | 2026-01-22 |
| [ADR-002](ADR-002-feature-flags-optional-backends.md) | Feature Flags for Optional Backends | Accepted | 2026-01-22 |
| [ADR-003](ADR-003-configuration-priority-order.md) | Configuration Priority Order | Accepted | 2026-01-22 |
| [ADR-004](ADR-004-unified-error-types.md) | Unified Error Types | Accepted | 2026-01-22 |
| [ADR-005](ADR-005-optional-streaming-support.md) | Optional Streaming Support | Accepted | 2026-01-22 |

### Encryption (ADR-006 to ADR-010)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-006](ADR-006-symmetric-only-encryption.md) | Symmetric-Only Encryption for v1.0 | Accepted | 2026-01-22 |
| [ADR-007](ADR-007-envelope-encryption-e2e.md) | Envelope Encryption for E2E Multi-Recipient | Accepted | 2026-01-22 |
| [ADR-008](ADR-008-magic-bytes-format-detection.md) | Magic Bytes for Format Detection | Accepted | 2026-01-22 |
| [ADR-009](ADR-009-json-headers-over-binary.md) | JSON Headers Over Binary | Accepted | 2026-01-22 |
| [ADR-010](ADR-010-in-memory-encryption.md) | In-Memory Encryption vs Streaming | Accepted | 2026-01-22 |

### Core Architecture (ADR-011 to ADR-016)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-011](ADR-011-hybrid-search-rrf-fusion.md) | Hybrid Search with RRF Fusion | Accepted | 2026-01-02 |
| [ADR-012](ADR-012-semantic-linking-threshold.md) | Semantic Linking with 0.7 Similarity Threshold | Accepted | 2026-01-02 |
| [ADR-013](ADR-013-skos-tagging-system.md) | W3C SKOS-Based Tagging System | Accepted | 2026-01-02 |
| [ADR-014](ADR-014-pgvector-hnsw-indexing.md) | pgvector with HNSW Indexing | Accepted | 2026-01-02 |
| [ADR-015](ADR-015-workspace-crate-structure.md) | Cargo Workspace with Domain-Driven Crates | Accepted | 2026-01-02 |
| [ADR-016](ADR-016-strict-tag-filtering.md) | Strict Tag Filtering for Data Isolation | Accepted | 2026-01-24 |

### Multilingual FTS (ADR-017 to ADR-021)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-017](ADR-017-multilingual-fts-strategy.md) | Multilingual FTS Strategy | Accepted | 2026-02-01 |
| [ADR-018](ADR-018-websearch-query-parser.md) | Query Parser Migration to websearch_to_tsquery | Accepted | 2026-02-01 |
| [ADR-019](ADR-019-script-detection-strategy.md) | Script Detection Strategy | Accepted | 2026-02-01 |
| [ADR-020](ADR-020-multi-index-strategy.md) | Multi-Index Strategy | Accepted | 2026-02-01 |
| [ADR-021](ADR-021-migration-rollout-strategy.md) | Migration and Rollout Strategy | Accepted | 2026-02-01 |

### Embedding System (ADR-022 to ADR-027)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-022](ADR-022-embedding-set-types.md) | Embedding Set Types (Filter vs Full) | Accepted | 2026-01-25 |
| [ADR-023](ADR-023-matryoshka-representation-learning.md) | Matryoshka Representation Learning | Accepted | 2026-01-25 |
| [ADR-024](ADR-024-auto-embed-rules.md) | Auto-Embed Rules | Accepted | 2026-01-25 |
| [ADR-025](ADR-025-document-type-registry.md) | Document Type Registry | Accepted | 2026-01-26 |
| [ADR-026](ADR-026-dynamic-embedding-config-api.md) | Dynamic Embedding Config API | Accepted | 2026-01-26 |
| [ADR-027](ADR-027-code-aware-chunking.md) | Code-Aware Chunking | Accepted | 2026-01-26 |

### Backup & Migration (ADR-028 to ADR-030)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-028](ADR-028-shard-archive-migration-system.md) | Shard and Archive Migration System | Accepted | 2026-02-01 |
| [ADR-029](ADR-029-shard-schema-versioning.md) | Shard Schema Versioning | Accepted | 2026-02-01 |
| [ADR-030](ADR-030-migration-downgrade-upgrade-ux.md) | Migration Downgrade/Upgrade UX | Accepted | 2026-02-01 |

### File Handling (ADR-031 to ADR-036)

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-031](ADR-031-intelligent-attachment-processing.md) | Intelligent Attachment Processing | Accepted | 2026-02-02 |
| [ADR-032](ADR-032-temporal-spatial-provenance.md) | Temporal and Spatial Provenance | Accepted | 2026-02-02 |
| [ADR-033](ADR-033-file-storage-architecture.md) | File Storage Architecture | Accepted | 2026-02-02 |
| [ADR-034](ADR-034-3d-file-analysis-support.md) | 3D File Analysis Support | Accepted | 2026-02-02 |
| [ADR-035](ADR-035-structured-media-formats.md) | Structured Media Formats | Accepted | 2026-02-02 |
| [ADR-036](ADR-036-file-safety-validation.md) | File Safety Validation | Accepted | 2026-02-02 |

## Status Definitions

| Status | Meaning |
|--------|---------|
| **Proposed** | Under discussion, not yet implemented |
| **Accepted** | Approved and implemented |
| **Superseded** | Replaced by a newer ADR |
| **Deprecated** | No longer recommended |

## ADR Template

See [ADR-TEMPLATE.md](ADR-TEMPLATE.md) for the template used when creating new ADRs.

## Cross-References

- **Design Specifications:** `../.aiwg/specs/`
- **Research Foundation:** `../.aiwg/research/citable-claims-index.md`
- **Implementation:** `crates/` directory
- **User Documentation:** `docs/content/`

## How to Add a New ADR

1. Copy `ADR-TEMPLATE.md` to `ADR-NNN-<short-name>.md`
2. Use the next available number in the appropriate category
3. Fill in all sections (Context, Decision, Consequences)
4. Update this README with the new entry
5. Reference the ADR in code comments where relevant

## Decision Relationships

```
Core Architecture (011-016)
├── Multilingual FTS (017-021) - extends search capabilities
├── Embedding System (022-027) - extends semantic features
└── Backup/Migration (028-030) - operational concerns

Inference Backend (001-005)
└── Encryption (006-010) - secure data handling

File Handling (031-036)
├── Builds on 025 (Document Type Registry)
└── Builds on 032 (Provenance)
```

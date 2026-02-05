# ADR-017: Multilingual FTS Strategy

**Status:** Accepted (Implemented 2026-02-01)
**Date:** 2026-02-01
**Decision Makers:** @roctinam
**Technical Story:** Enable full-text search for CJK (Chinese, Japanese, Korean), emoji, and additional scripts

## Context

Matric Memory's current full-text search implementation uses a single PostgreSQL text search configuration (`matric_english`) optimized for English content:

```sql
CREATE TEXT SEARCH CONFIGURATION matric_english (COPY = english);
ALTER TEXT SEARCH CONFIGURATION matric_english
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, english_stem;
```

### Current Limitations

| Limitation | Impact | Issue |
|------------|--------|-------|
| English-only stemming | CJK characters not tokenized properly | #316 |
| No word boundaries for CJK | Cannot search Chinese/Japanese/Korean phrases | #316 |
| Emoji treated as noise | Emoji characters ignored in search | #319 |
| Single tsvector column | No language-specific optimization | - |
| No trigram fallback | Partial matches fail for non-Latin scripts | - |

### Problem Statement

Users with multilingual content cannot effectively search:
- Chinese text (simplified and traditional)
- Japanese text (kanji, hiragana, katakana)
- Korean text (hangul)
- Emoji and symbol characters
- Mixed-script content (e.g., "Python 机器学习")

The semantic search (pgvector) component handles multilingual queries well because the embedding model (nomic-embed-text) is multilingual. However, the FTS component fails completely for non-Latin scripts, degrading hybrid search quality.

## Decision

Adopt a **hybrid approach combining pg_trgm (trigram) as universal fallback with optional pg_bigm (bigram) for optimized CJK support**.

### Strategy Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      Query Input                                 │
└─────────────────────────┬───────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────────┐
│                 Script Detection (Unicode Analysis)              │
│    - Fast O(n) single pass                                       │
│    - Returns: Script enum (Latin, Han, Hangul, Cyrillic, etc.)  │
└─────────────────────────┬───────────────────────────────────────┘
                          │
         ┌────────────────┴────────────────┐
         │                                 │
┌────────▼────────┐               ┌────────▼────────┐
│ Latin/European  │               │   CJK/Other     │
│ (Primary)       │               │   (Primary)     │
└────────┬────────┘               └────────┬────────┘
         │                                 │
┌────────▼────────────────────────────────▼────────┐
│                FTS Strategy Selector              │
│                                                   │
│  Latin scripts  → matric_english tsvector        │
│  Han (Chinese)  → pg_bigm (or pg_trgm fallback)  │
│  Japanese       → pg_bigm (or pg_trgm fallback)  │
│  Korean         → pg_bigm (or pg_trgm fallback)  │
│  Cyrillic       → matric_russian tsvector        │
│  Emoji/Symbols  → pg_trgm exact match            │
│  Mixed/Unknown  → Multi-strategy OR              │
└──────────────────────────────────────────────────┘
```

### Index Configuration

| Script | Primary Index | Fallback Index | Extension |
|--------|--------------|----------------|-----------|
| Latin/English | GIN tsvector (matric_english) | GIN pg_trgm | pg_trgm (built-in) |
| German | GIN tsvector (matric_german) | GIN pg_trgm | pg_trgm |
| Russian | GIN tsvector (matric_russian) | GIN pg_trgm | pg_trgm |
| Chinese | GIN pg_bigm | GIN pg_trgm | pg_bigm (optional) |
| Japanese | GIN pg_bigm | GIN pg_trgm | pg_bigm (optional) |
| Korean | GIN pg_bigm | GIN pg_trgm | pg_bigm (optional) |
| Emoji/Symbols | GIN pg_trgm | - | pg_trgm |
| Mixed/Unknown | Multi-config search | GIN pg_trgm | pg_trgm |

### Graceful Degradation

If pg_bigm is not available (e.g., cloud PostgreSQL without extension support):
1. CJK search falls back to pg_trgm (trigram)
2. Trigram works for CJK but with slightly lower precision
3. Semantic search compensates for FTS limitations
4. Application logs warning, continues functioning

## Consequences

### Positive

- **CJK support**: Full coverage for Chinese, Japanese, Korean scripts
- **Emoji search**: Users can search by emoji characters
- **Backward compatible**: Existing English searches unchanged
- **Graceful degradation**: Works without pg_bigm (just suboptimal)
- **Semantic fallback**: pgvector handles multilingual even when FTS fails
- **No external dependencies**: Pure PostgreSQL solution (no Elasticsearch/MeiliSearch)

### Negative

- **Index size growth**: ~3-5x increase in total index size
  - Estimated: 50MB (current) → 150-250MB (10k notes)
- **Query complexity**: Multiple index lookups for mixed-script queries
- **Latency increase**: +10-50ms for non-English queries
- **pg_bigm optional dependency**: Best CJK performance requires extension
- **Maintenance overhead**: Multiple indexes to monitor and maintain

### Mitigations

1. **Index size**: Monitor growth; consider partial indexes if problematic
2. **Query latency**: Cache script detection results; use prepared statements
3. **pg_bigm dependency**: Implement clean fallback path; document requirements
4. **Maintenance**: Add index health monitoring to ops runbook

## Alternatives Considered

### 1. External Search Engine (Elasticsearch/MeiliSearch)

Dedicated search infrastructure with native multilingual support.

**Rejected because:**
- Massive infrastructure complexity (separate service)
- Data synchronization overhead (eventual consistency)
- Deployment complexity (Docker bundle would need additional container)
- Overkill for matric-memory's scale (single-user/small-team)
- Unknown CJK/emoji support quality in MeiliSearch

### 2. zhparser + MeCab (Language-Specific Parsers)

Install language-specific word segmentation libraries.

**Rejected because:**
- External dependencies (SCWS library for zhparser)
- Chinese-only (zhparser) or Japanese-only (MeCab)
- Complex installation and dictionary management
- Platform-specific build issues
- pg_bigm provides 80% of the benefit with 20% of the complexity

### 3. pg_trgm Only (No pg_bigm)

Use trigram indexing for all non-English content.

**Partially adopted:** pg_trgm is the fallback strategy. However:
- pg_bigm is optimized for 2-character patterns (common in CJK)
- pg_bigm handles 1-2 character keywords efficiently (trigram requires 3+)
- Worth the optional dependency for CJK-heavy workloads

### 4. Semantic Search Only (Disable FTS for Non-English)

Rely entirely on embedding-based search for multilingual content.

**Rejected because:**
- FTS provides exact keyword matching (valuable for precise searches)
- Hybrid search (FTS + semantic) outperforms either alone
- Users expect keyword search to work regardless of language
- Some queries are better suited to FTS (exact matches, short queries)

## Implementation

**Code Location:**
- `crates/matric-search/src/script_detection.rs` - Unicode script analysis
- `crates/matric-search/src/fts_strategy.rs` - Strategy selection
- `crates/matric-db/src/search_multilingual.rs` - Query execution
- `migrations/20260202000000_multilingual_fts.sql` - Schema changes

**Key Changes:**
1. Add script detection module using `unicode_script` crate
2. Implement FTS strategy selector based on query script profile
3. Create trigram indexes (pg_trgm) on content columns
4. Optionally create bigram indexes (pg_bigm) if extension available
5. Update search repository to route queries by script
6. Integrate with existing RRF fusion (FTS results + semantic results)

## References

- PostgreSQL pg_trgm: https://www.postgresql.org/docs/16/pgtrgm.html
- pg_bigm Documentation: https://github.com/pgbigm/pg_bigm
- Issue #316: CJK text search fails
- Issue #319: Emoji search fails
- Architecture Design: `.aiwg/working/discovery/multilingual-fts/designs/architecture-design.md`
- Technical Research: `.aiwg/working/discovery/multilingual-fts/spikes/technical-research.md`

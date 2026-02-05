# ADR-020: Multi-Index Strategy

**Status:** Accepted (Implemented 2026-02-01)
**Date:** 2026-02-01
**Decision Makers:** @roctinam
**Technical Story:** Balance index size, query performance, and multilingual coverage

## Context

Supporting multilingual full-text search requires multiple index types:
- **tsvector (GIN)**: Word-based indexing for stemmed languages (English, German, Russian)
- **pg_trgm (GIN)**: Trigram indexing for fuzzy matching and universal fallback
- **pg_bigm (GIN)**: Bigram indexing optimized for CJK languages

### Current State

```sql
-- Current: single tsvector index
CREATE INDEX idx_note_revised_tsv ON note_revised_current USING gin (tsv);
-- tsv column is: to_tsvector('matric_english', content)
```

### Problem Statement

How many indexes should we maintain, and what is the acceptable trade-off between:
- **Coverage**: Supporting all scripts/languages
- **Index size**: Storage and memory overhead
- **Query performance**: Latency for different query types
- **Maintenance**: Index rebuild time, vacuum overhead

### Estimated Index Sizes

| Index Type | Size (10k notes) | Size (100k notes) | Size (1M notes) |
|------------|------------------|-------------------|-----------------|
| tsvector (matric_english) | ~50MB | ~500MB | ~5GB |
| tsvector (matric_simple) | ~55MB | ~550MB | ~5.5GB |
| pg_trgm (content) | ~100MB | ~1GB | ~10GB |
| pg_bigm (content) | ~80MB | ~800MB | ~8GB |
| **Current total** | ~50MB | ~500MB | ~5GB |
| **Proposed total** | ~235MB | ~2.3GB | ~23GB |

**Growth factor**: ~4.6x (with pg_bigm) or ~3x (without pg_bigm)

## Decision

Adopt a **multi-index strategy with conditional bigram**:

1. **Always create**: pg_trgm trigram index (universal fallback)
2. **Conditionally create**: pg_bigm bigram index (if extension available)
3. **Keep existing**: matric_english tsvector index (unchanged)
4. **Add optionally**: Additional language configs (matric_german, matric_russian)

### Index Schema

```sql
-- =============================================================================
-- Required: Universal trigram index (pg_trgm)
-- =============================================================================
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Trigram index on note content (universal fallback)
CREATE INDEX CONCURRENTLY idx_note_revised_trgm
  ON note_revised_current
  USING gin (content gin_trgm_ops);

-- Trigram index on note title
CREATE INDEX CONCURRENTLY idx_note_title_trgm
  ON note
  USING gin (title gin_trgm_ops);

-- =============================================================================
-- Optional: CJK bigram index (pg_bigm, if available)
-- =============================================================================
DO $$
BEGIN
    -- Try to create pg_bigm extension
    CREATE EXTENSION IF NOT EXISTS pg_bigm;

    -- If successful, create bigram indexes
    CREATE INDEX CONCURRENTLY idx_note_revised_bigm
      ON note_revised_current
      USING gin (content gin_bigm_ops);

    CREATE INDEX CONCURRENTLY idx_note_title_bigm
      ON note
      USING gin (title gin_bigm_ops);

    RAISE NOTICE 'pg_bigm indexes created for optimized CJK search';
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'pg_bigm not available, CJK search will use pg_trgm fallback';
END $$;

-- =============================================================================
-- Existing: English tsvector index (unchanged)
-- =============================================================================
-- Already exists: idx_note_revised_tsv (matric_english)
```

### Query Routing

```rust
pub async fn search_multilingual(
    &self,
    query: &str,
    strategy: &FtsStrategyConfig,
    limit: i64,
) -> Result<Vec<SearchHit>> {
    match &strategy.primary {
        // Latin scripts: use tsvector with websearch_to_tsquery
        FtsStrategy::English | FtsStrategy::German | FtsStrategy::Russian => {
            self.search_tsvector(query, &strategy.config, limit).await
        }
        // CJK: prefer pg_bigm, fallback to pg_trgm
        FtsStrategy::Chinese | FtsStrategy::Japanese | FtsStrategy::Korean => {
            if self.has_bigm_extension {
                self.search_bigm(query, limit).await
            } else {
                self.search_trgm(query, limit).await
            }
        }
        // Emoji/symbols: use trigram exact match
        FtsStrategy::Trigram => {
            self.search_trgm(query, limit).await
        }
        // Mixed: search multiple indexes, merge results
        FtsStrategy::Multi(strategies) => {
            self.search_multi_index(query, strategies, limit).await
        }
    }
}
```

### Index Selection Guidelines

| Query Type | Primary Index | Fallback | Rationale |
|------------|---------------|----------|-----------|
| English keywords | tsvector (matric_english) | pg_trgm | Stemming benefits recall |
| CJK characters | pg_bigm | pg_trgm | Bigram optimal for 2+ chars |
| Single CJK char | pg_bigm | pg_trgm | Bigram handles 1-2 chars |
| Emoji | pg_trgm | - | Trigram matches exact |
| Partial match | pg_trgm | - | Trigram for fuzzy |
| Mixed script | Multi-index OR | pg_trgm | Union of relevant indexes |

## Consequences

### Positive

- **Comprehensive coverage**: All scripts searchable via some index
- **Optimized CJK**: pg_bigm provides best CJK performance (when available)
- **Graceful degradation**: Works without pg_bigm (falls back to trigram)
- **Backward compatible**: Existing English searches unchanged
- **Query planner efficiency**: PostgreSQL selects optimal index per query
- **Concurrent builds**: CONCURRENTLY avoids table locks during creation

### Negative

- **3-5x storage growth**: Multiple indexes increase disk usage
- **Write amplification**: Each insert/update touches multiple indexes
- **Vacuum overhead**: More indexes to maintain
- **Memory pressure**: Indexes compete for shared_buffers
- **Build time**: Initial index creation takes longer

### Mitigations

1. **Storage growth**:
   - Monitor disk usage with alerts
   - Consider partial indexes for archived notes
   - Acceptable for typical deployments (<10k notes)

2. **Write amplification**:
   - matric-memory is read-heavy (search >> writes)
   - Background index updates (GIN fastupdate)
   - Acceptable trade-off for search quality

3. **Vacuum overhead**:
   - Schedule regular maintenance windows
   - Monitor pg_stat_user_indexes for bloat
   - autovacuum handles most cases

4. **Memory pressure**:
   - Increase shared_buffers if needed
   - GIN indexes are memory-efficient (compressed)

5. **Build time**:
   - CONCURRENTLY builds avoid blocking
   - Background job for initial migration
   - <10 minutes for 100k notes

## Alternatives Considered

### 1. Single Universal Index (pg_trgm Only)

Use only trigram indexing for all content.

**Pros:**
- Simplest implementation
- Works for all scripts
- Smallest total index size

**Rejected because:**
- Suboptimal for English (no stemming)
- Suboptimal for CJK (trigrams worse than bigrams)
- Cannot do proximity search (phrase matching)
- tsvector provides significant benefits for Latin scripts

### 2. Per-Language Separate Indexes

Create dedicated tsvector index for each language.

```sql
-- Per-language indexes
CREATE INDEX idx_note_tsv_en ON note_revised_current USING gin (to_tsvector('english', content));
CREATE INDEX idx_note_tsv_de ON note_revised_current USING gin (to_tsvector('german', content));
CREATE INDEX idx_note_tsv_ru ON note_revised_current USING gin (to_tsvector('russian', content));
-- ... for every language
```

**Rejected because:**
- Massive index proliferation (20+ languages = 20+ indexes)
- Unclear which index to query (need per-note language metadata)
- Overkill for typical multilingual use case
- pg_trgm provides universal fallback more efficiently

### 3. Language Column with Generated tsvector

Store detected language per note, generate appropriate tsvector.

```sql
ALTER TABLE note ADD COLUMN detected_language TEXT;
ALTER TABLE note_revised_current ADD COLUMN tsv_dynamic tsvector
  GENERATED ALWAYS AS (
    to_tsvector(COALESCE(detected_language, 'simple')::regconfig, content)
  ) STORED;
```

**Partially adopted:** Language detection column is useful metadata.

**Not primary because:**
- Query-time config selection (not index-time)
- User queries may not match note language
- Adds complexity without proportional benefit

### 4. Materialized View per Script

Pre-compute search results per script in materialized views.

**Rejected because:**
- Refresh overhead (stale results between refreshes)
- Query routing complexity
- Cannot combine results from multiple scripts efficiently

## Implementation

**Code Location:**
- `migrations/20260202000000_multilingual_fts.sql` - Index creation
- `crates/matric-db/src/search_multilingual.rs` - Query routing
- `crates/matric-db/src/extensions.rs` - Extension availability check

**Key Changes:**

1. Migration creates trigram indexes (always) and bigram indexes (if available)
2. Search repository checks for pg_bigm availability at startup
3. Query router selects index based on detected script
4. Fallback path uses trigram for all non-tsvector queries

**Monitoring:**

```sql
-- Index size monitoring
SELECT
    indexrelname,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
WHERE relname IN ('note_revised_current', 'note')
ORDER BY pg_relation_size(indexrelid) DESC;

-- Index usage monitoring
SELECT
    indexrelname,
    idx_scan AS scans,
    idx_tup_read AS tuples_read
FROM pg_stat_user_indexes
WHERE relname IN ('note_revised_current', 'note')
ORDER BY idx_scan DESC;
```

## References

- PostgreSQL GIN Indexes: https://www.postgresql.org/docs/16/gin.html
- pg_trgm Index Options: https://www.postgresql.org/docs/16/pgtrgm.html#PGTRGM-INDEX
- pg_bigm GIN Operator Class: https://github.com/pgbigm/pg_bigm#gin-index
- ADR-017: Multilingual FTS Strategy (parent decision)
- Architecture Design: `.aiwg/working/discovery/multilingual-fts/designs/architecture-design.md`

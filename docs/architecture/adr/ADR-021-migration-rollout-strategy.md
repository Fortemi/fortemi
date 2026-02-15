# ADR-021: Migration and Rollout Strategy

**Status:** Accepted (Implemented 2026-02-01)
**Date:** 2026-02-01
**Decision Makers:** @roctinam
**Technical Story:** Enable safe, zero-downtime migration to multilingual FTS with rollback capability

## Context

The multilingual FTS feature (ADR-017 through ADR-020) introduces significant database changes:
- New PostgreSQL extensions (pg_trgm, optionally pg_bigm)
- Additional GIN indexes on content columns
- New text search configurations
- Query function changes (plainto_tsquery → websearch_to_tsquery)
- Application code changes (script detection, strategy routing)

### Risk Factors

| Risk | Likelihood | Impact | Description |
|------|------------|--------|-------------|
| Index build blocks writes | Medium | High | Non-CONCURRENTLY index creation locks table |
| Query latency regression | Medium | Medium | New indexes may cause planner confusion |
| Extension unavailable | Medium | High | pg_bigm not available on all PostgreSQL deployments |
| Behavioral change | Low | Medium | websearch_to_tsquery treats "OR" as operator |
| Migration timeout | Low | High | Large datasets may timeout during index build |

### Deployment Targets

| Environment | PostgreSQL Version | pg_bigm Available | Notes |
|-------------|-------------------|-------------------|-------|
| Docker Bundle | PostgreSQL 16 | Yes (compiled) | Primary target |
| AWS RDS | PostgreSQL 15/16 | Limited | Trusted extension only |
| Google Cloud SQL | PostgreSQL 15/16 | No | Extension not available |
| Self-hosted | Varies | Manual install | Depends on admin |

## Decision

Adopt a **phased rollout strategy with feature flags** enabling gradual risk reduction and instant rollback.

### Phase Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Phase 1: Infrastructure (Zero Downtime)                                      │
│ Duration: ~2 minutes                                                         │
│                                                                              │
│ ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐        │
│ │ Enable     │───>│ Create     │───>│ Add        │───>│ Create     │        │
│ │ pg_trgm    │    │ Text Cfgs  │    │ Columns    │    │ Functions  │        │
│ └────────────┘    └────────────┘    └────────────┘    └────────────┘        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase 2: Index Creation (Background, Non-Blocking)                           │
│ Duration: ~5-30 minutes (depends on data size)                               │
│                                                                              │
│ ┌──────────────────────────────────────────────────────────────────┐        │
│ │        CREATE INDEX CONCURRENTLY (parallel)                       │        │
│ │                                                                   │        │
│ │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │        │
│ │  │ idx_note_       │  │ idx_note_       │  │ idx_note_       │   │        │
│ │  │ revised_trgm    │  │ title_trgm      │  │ revised_bigm    │   │        │
│ │  └─────────────────┘  └─────────────────┘  └─────────────────┘   │        │
│ │                                                                   │        │
│ └──────────────────────────────────────────────────────────────────┘        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase 3: Feature Flag Rollout (Gradual)                                      │
│ Duration: 1-7 days                                                           │
│                                                                              │
│ ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    │
│ │ 10% queries │───>│ 25% queries │───>│ 50% queries │───>│ 100%        │    │
│ │ (canary)    │    │ (expansion) │    │ (majority)  │    │ (full)      │    │
│ └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘    │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase 4: Cleanup (Optional)                                                  │
│ Duration: After validation                                                   │
│                                                                              │
│ ┌────────────────────────────────────────────────────────────────────┐      │
│ │ Remove feature flag code, update documentation, archive old code   │      │
│ └────────────────────────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Infrastructure

**Migration: `20260202000000_multilingual_fts_infrastructure.sql`**

```sql
-- =============================================================================
-- Phase 1: Infrastructure (non-blocking, fast)
-- =============================================================================

-- 1. Enable pg_trgm extension (trusted, no superuser required)
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- 2. Try pg_bigm (may fail on cloud PostgreSQL)
DO $$
BEGIN
    CREATE EXTENSION IF NOT EXISTS pg_bigm;
    RAISE NOTICE 'pg_bigm extension enabled';
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'pg_bigm not available: %', SQLERRM;
END $$;

-- 3. Create additional text search configurations
CREATE TEXT SEARCH CONFIGURATION IF NOT EXISTS matric_simple (COPY = simple);
CREATE TEXT SEARCH CONFIGURATION IF NOT EXISTS matric_german (COPY = german);
ALTER TEXT SEARCH CONFIGURATION matric_german
  ALTER MAPPING FOR hword, hword_part, word WITH unaccent, german_stem;

CREATE TEXT SEARCH CONFIGURATION IF NOT EXISTS matric_russian (COPY = russian);
ALTER TEXT SEARCH CONFIGURATION matric_russian
  ALTER MAPPING FOR hword, hword_part, word WITH unaccent, russian_stem;

-- 4. Add language metadata columns
ALTER TABLE note
  ADD COLUMN IF NOT EXISTS detected_language TEXT,
  ADD COLUMN IF NOT EXISTS language_confidence REAL;

-- 5. Create script detection helper function
CREATE OR REPLACE FUNCTION detect_dominant_script(text_content TEXT)
RETURNS TEXT AS $$
DECLARE
    han_count INTEGER := 0;
    latin_count INTEGER := 0;
    cyrillic_count INTEGER := 0;
    hangul_count INTEGER := 0;
    total_count INTEGER := 0;
    code_point INTEGER;
BEGIN
    IF text_content IS NULL OR length(text_content) = 0 THEN
        RETURN 'unknown';
    END IF;

    FOR i IN 1..length(text_content) LOOP
        code_point := ascii(substring(text_content FROM i FOR 1));
        IF code_point > 64 THEN
            total_count := total_count + 1;
            CASE
                WHEN code_point BETWEEN 65 AND 90 OR code_point BETWEEN 97 AND 122
                     OR code_point BETWEEN 192 AND 687 THEN latin_count := latin_count + 1;
                WHEN code_point BETWEEN 19968 AND 40959 THEN han_count := han_count + 1;
                WHEN code_point BETWEEN 1024 AND 1279 THEN cyrillic_count := cyrillic_count + 1;
                WHEN code_point BETWEEN 44032 AND 55215 THEN hangul_count := hangul_count + 1;
                ELSE NULL;
            END CASE;
        END IF;
    END LOOP;

    IF total_count = 0 THEN RETURN 'unknown'; END IF;
    IF han_count::FLOAT / total_count > 0.5 THEN RETURN 'han'; END IF;
    IF hangul_count::FLOAT / total_count > 0.5 THEN RETURN 'hangul'; END IF;
    IF cyrillic_count::FLOAT / total_count > 0.5 THEN RETURN 'cyrillic'; END IF;
    IF latin_count::FLOAT / total_count > 0.5 THEN RETURN 'latin'; END IF;
    RETURN 'mixed';
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

### Phase 2: Index Creation

**Migration: `20260202000001_multilingual_fts_indexes.sql`**

```sql
-- =============================================================================
-- Phase 2: Index Creation (CONCURRENTLY - non-blocking)
-- =============================================================================
-- NOTE: Run as separate migration to allow Phase 1 to complete

-- Trigram indexes (always)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_revised_trgm
  ON note_revised_current USING gin (content gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_title_trgm
  ON note USING gin (title gin_trgm_ops);

-- Bigram indexes (if pg_bigm available)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm') THEN
        EXECUTE 'CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_revised_bigm
                 ON note_revised_current USING gin (content gin_bigm_ops)';
        EXECUTE 'CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_title_bigm
                 ON note USING gin (title gin_bigm_ops)';
        RAISE NOTICE 'pg_bigm indexes created';
    ELSE
        RAISE NOTICE 'pg_bigm not available, skipping bigram indexes';
    END IF;
END $$;
```

### Phase 3: Feature Flag

**Configuration:**

```rust
/// Feature flag for multilingual FTS
pub struct MultilingualFtsConfig {
    /// Enable multilingual search (vs legacy English-only)
    pub enabled: bool,
    /// Percentage of queries to route to new system (0-100)
    pub rollout_percentage: u8,
    /// Force specific strategy for testing
    pub force_strategy: Option<FtsStrategy>,
    /// Enable detailed logging for debugging
    pub debug_logging: bool,
}

impl Default for MultilingualFtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,  // Disabled by default until Phase 3
            rollout_percentage: 0,
            force_strategy: None,
            debug_logging: false,
        }
    }
}
```

**Environment Variables:**

```bash
# Phase 3 rollout control
MULTILINGUAL_FTS_ENABLED=true
MULTILINGUAL_FTS_ROLLOUT_PERCENTAGE=10  # Start at 10%

# Debugging
MULTILINGUAL_FTS_DEBUG=true

# Force fallback (emergency)
MULTILINGUAL_FTS_FORCE_LEGACY=true
```

**Rollout Schedule:**

| Day | Percentage | Monitoring Focus |
|-----|------------|------------------|
| 1 | 10% | Error rates, latency p99 |
| 2 | 25% | Memory usage, index scans |
| 3 | 50% | Query planner behavior |
| 5 | 75% | Full coverage testing |
| 7 | 100% | Remove flag after validation |

### Rollback Plan

**Instant Rollback (feature flag):**

```bash
# Disable multilingual FTS immediately
MULTILINGUAL_FTS_ENABLED=false

# Or force legacy behavior
MULTILINGUAL_FTS_FORCE_LEGACY=true
```

**Full Rollback (if needed):**

```sql
-- Drop new indexes (safe, quick)
DROP INDEX IF EXISTS idx_note_revised_trgm;
DROP INDEX IF EXISTS idx_note_title_trgm;
DROP INDEX IF EXISTS idx_note_revised_bigm;
DROP INDEX IF EXISTS idx_note_title_bigm;

-- Drop language columns (optional)
ALTER TABLE note DROP COLUMN IF EXISTS detected_language;
ALTER TABLE note DROP COLUMN IF EXISTS language_confidence;

-- Extensions can remain (no harm)
-- Text search configs can remain (no harm)
```

## Consequences

### Positive

- **Zero downtime**: CONCURRENTLY index builds, feature flag control
- **Instant rollback**: Feature flag disables without migration
- **Gradual validation**: Percentage-based rollout catches issues early
- **Risk isolation**: Each phase can be validated independently
- **Backward compatible**: Legacy behavior always available

### Negative

- **Longer timeline**: Phased rollout takes 1-2 weeks vs big-bang
- **Dual code paths**: Feature flag adds branching complexity
- **Monitoring overhead**: Need to track both old and new paths
- **Index maintenance**: New indexes consume resources even when disabled

### Mitigations

1. **Timeline**: Automated rollout progression (if metrics healthy)
2. **Code complexity**: Clear separation between strategies, remove flag after validation
3. **Monitoring**: Add metrics for both code paths, dashboards
4. **Index overhead**: Acceptable given storage costs; defer drops until confident

## Alternatives Considered

### 1. Big-Bang Migration

Deploy all changes at once without feature flag.

**Rejected because:**
- No rollback without downtime (index drops take time)
- Cannot validate incrementally
- Higher risk for production incidents
- Debugging harder (all changes in one commit)

### 2. Shadow Mode

Run new system in parallel, compare results, but use old results.

**Rejected because:**
- Double resource consumption (run both searches)
- Complex comparison logic
- Still need rollout phase after shadow validation
- Feature flag achieves similar validation with less overhead

### 3. Blue-Green Database

Maintain two database instances, switch traffic.

**Rejected because:**
- Massive infrastructure overhead
- Data synchronization complexity
- Overkill for matric-memory's scale
- Docker bundle deployment incompatible

### 4. Canary Deployment

Deploy to subset of servers, route traffic.

**Partially adopted:** Feature flag achieves query-level canary.

**Not infrastructure canary because:**
- Single-instance deployment (Docker bundle)
- Query-level sampling more granular
- Simpler to implement and monitor

## Implementation

**Code Location:**
- `migrations/20260202000000_multilingual_fts_infrastructure.sql` - Phase 1
- `migrations/20260202000001_multilingual_fts_indexes.sql` - Phase 2
- `crates/matric-search/src/config.rs` - Feature flag configuration
- `crates/matric-search/src/search.rs` - Search routing with flag

**Key Changes:**

1. Split migration into infrastructure and index phases
2. Add feature flag configuration and environment variable support
3. Implement routing based on flag and rollout percentage
4. Add metrics for old vs new search paths
5. Document rollout procedure in ops runbook

**Monitoring:**

```rust
// Metrics to track during rollout
pub struct MultilingualFtsMetrics {
    /// Queries routed to new multilingual system
    pub multilingual_queries: Counter,
    /// Queries routed to legacy system
    pub legacy_queries: Counter,
    /// Latency histogram (multilingual)
    pub multilingual_latency: Histogram,
    /// Latency histogram (legacy)
    pub legacy_latency: Histogram,
    /// Error count (multilingual)
    pub multilingual_errors: Counter,
    /// Error count (legacy)
    pub legacy_errors: Counter,
}
```

## References

- Feature Flags Best Practices: https://martinfowler.com/articles/feature-toggles.html
- PostgreSQL CREATE INDEX CONCURRENTLY: https://www.postgresql.org/docs/16/sql-createindex.html#SQL-CREATEINDEX-CONCURRENTLY
- ADR-017: Multilingual FTS Strategy (parent decision)
- Architecture Design: `.aiwg/working/discovery/multilingual-fts/designs/architecture-design.md`

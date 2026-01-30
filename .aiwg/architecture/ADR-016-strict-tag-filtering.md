# ADR-016: Strict Tag Filtering for Search

**Status:** Accepted (Implemented 2026-01-24)
**Date:** 2026-01-24
**Decision Makers:** @roctinam
**Technical Story:** Enable guaranteed data segregation via strict SKOS tag/scheme filtering

## Context

Matric Memory has a comprehensive W3C SKOS-compliant tag system with:
- Concept schemes (vocabulary namespaces)
- Concepts with labels, facets, relations
- Note-to-concept tagging via `note_skos_concept` table

The hybrid search system combines FTS + semantic search with RRF fusion. However, current filtering has critical gaps:

### Current State

1. **Legacy tag filtering**: Uses deprecated `note_tag` table, not SKOS concepts
2. **Soft filtering**: Filters parsed from query string mixed into fuzzy search
3. **No AND/OR logic**: Can only filter by single tag at a time
4. **No scheme isolation**: Cannot guarantee results from specific vocabulary
5. **Semantic search bypass**: Vector search doesn't apply filters, only FTS does

### Problem Statement

Users need **guaranteed segregation** of results by project, client, or domain. Current fuzzy-only filtering cannot provide this guarantee. Without strict filtering:

- Data tenancy is impossible (cross-client contamination risk)
- Project isolation requires separate databases
- Remote access/sharing features would leak data
- Compliance requirements (GDPR, SOC2) cannot be met

## Decision

Implement strict tag filtering as a **pre-search WHERE clause system** applied at the database level before any fuzzy matching occurs.

### Filtering Model

```rust
/// Strict tag filter configuration
#[derive(Debug, Clone, Default)]
pub struct StrictTagFilter {
    /// Notes MUST have ALL these concepts (AND logic)
    pub required_concepts: Vec<Uuid>,

    /// Notes MUST have AT LEAST ONE of these concepts (OR logic)
    pub any_concepts: Vec<Uuid>,

    /// Notes MUST NOT have ANY of these concepts (exclusion)
    pub excluded_concepts: Vec<Uuid>,

    /// Notes MUST belong to concepts within these schemes only
    pub required_schemes: Vec<Uuid>,

    /// Notes MUST NOT have concepts from these schemes
    pub excluded_schemes: Vec<Uuid>,
}
```

### Alternative Notation Support

For ergonomic API usage, also support notation-based filtering:

```rust
pub struct StrictTagFilterInput {
    /// Concept notations/labels (resolved to UUIDs)
    pub required_tags: Vec<String>,      // AND logic
    pub any_tags: Vec<String>,           // OR logic
    pub excluded_tags: Vec<String>,      // NOT logic

    /// Scheme notations (resolved to UUIDs)
    pub required_schemes: Vec<String>,   // scheme isolation
    pub excluded_schemes: Vec<String>,   // scheme exclusion
}
```

### SQL Implementation

Filters generate SQL JOINs/WHERE clauses applied **before** search:

```sql
-- Required concepts (AND): note must have ALL
SELECT DISTINCT n.id FROM note n
WHERE EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    WHERE nsc.note_id = n.id AND nsc.concept_id = $1
)
AND EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    WHERE nsc.note_id = n.id AND nsc.concept_id = $2
);

-- Any concepts (OR): note must have AT LEAST ONE
SELECT DISTINCT n.id FROM note n
WHERE EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    WHERE nsc.note_id = n.id AND nsc.concept_id = ANY($1::uuid[])
);

-- Excluded concepts (NOT): note must have NONE
SELECT DISTINCT n.id FROM note n
WHERE NOT EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    WHERE nsc.note_id = n.id AND nsc.concept_id = ANY($1::uuid[])
);

-- Required schemes: note must only have concepts from these schemes
SELECT DISTINCT n.id FROM note n
WHERE EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    JOIN skos_concept sc ON sc.id = nsc.concept_id
    WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY($1::uuid[])
)
AND NOT EXISTS (
    SELECT 1 FROM note_skos_concept nsc
    JOIN skos_concept sc ON sc.id = nsc.concept_id
    WHERE nsc.note_id = n.id AND sc.primary_scheme_id != ALL($1::uuid[])
);
```

## Consequences

### Positive

- **Guaranteed isolation**: No false positives - results strictly match criteria
- **Foundation for tenancy**: Scheme-level isolation enables multi-tenant features
- **Composable with fuzzy**: Strict filters + semantic search work together
- **Auditable**: Filter criteria can be logged for compliance
- **Backward compatible**: Existing searches unaffected (filters are optional)

### Negative

- **Performance overhead**: Additional JOINs on every filtered search
- **Complexity**: More parameters to manage in API
- **Resolution cost**: Notation-to-UUID resolution adds latency

### Mitigations

1. **Index optimization**: Add composite indexes on `note_skos_concept(note_id, concept_id)`
2. **Caching**: Cache notation-to-UUID mappings with TTL
3. **Query planning**: Use CTEs for efficient subquery execution
4. **Batch resolution**: Resolve all notations in single query

## Alternatives Considered

### 1. Post-Search Filtering

Filter results after FTS/semantic search completes.

**Rejected because:**
- Cannot guarantee result counts (may return 0 after filtering full set)
- Wastes compute on results that will be discarded
- Leaks existence of filtered-out notes via timing

### 2. Separate Indexes per Scheme

Create PostgreSQL schemas or separate indexes per concept scheme.

**Rejected because:**
- Massive operational complexity
- Cannot filter across schemes
- Duplication of infrastructure

### 3. Materialized View Approach

Pre-compute note-scheme membership in materialized view.

**Partially adopted:** May use materialized views for complex scheme isolation queries, but core filtering remains dynamic.

### 4. Embedding-Level Isolation

Store separate embedding sets per scheme.

**Complementary:** Already supported via `embedding_set_id`. Strict filtering adds note-level guarantee.

## Implementation Plan

See related issues:
- Issue #XXX: Core `StrictTagFilter` types and builder
- Issue #XXX: Database query generation
- Issue #XXX: API endpoint updates
- Issue #XXX: MCP server tool updates
- Issue #XXX: Performance optimization and indexing
- Issue #XXX: Integration tests

## References

- W3C SKOS Reference: https://www.w3.org/TR/skos-reference/
- PostgreSQL EXISTS optimization: https://wiki.postgresql.org/wiki/Slow_Counting
- Prior art: Elasticsearch term queries vs match queries

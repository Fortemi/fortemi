# ADR-013: W3C SKOS-Based Tagging System

**Status:** Accepted
**Date:** 2026-01-02
**Deciders:** roctinam
**Research:** REF-033 (Miles & Bechhofer, 2009)

## Context

matric-memory needs a tagging system that goes beyond simple string tags. Requirements include:
- Synonyms (multiple labels for same concept)
- Hierarchies (broader/narrower relationships)
- Misspelling tolerance (hidden labels for search)
- Cross-vocabulary mapping
- Internationalization (labels per language)

Flat string tags cannot express these relationships.

## Decision

Implement tagging based on W3C SKOS (Simple Knowledge Organization System) standard. SKOS provides:
- **skos:prefLabel** - Primary display name (one per language)
- **skos:altLabel** - Synonyms and variations (multiple allowed)
- **skos:hiddenLabel** - Common misspellings for search expansion
- **skos:broader/narrower** - Hierarchical relationships
- **skos:related** - Associative (non-hierarchical) relationships
- **skos:ConceptScheme** - Grouping of related concepts

This enables faceted navigation, intelligent tag suggestions, and taxonomy-based filtering.

## Consequences

### Positive
- (+) Rich taxonomy support with hierarchies
- (+) Synonym resolution improves search recall
- (+) Misspelling tolerance without fuzzy search overhead
- (+) Standards-based, interoperable with other systems
- (+) Supports multi-language deployments
- (+) Enables strict tag filtering with scheme isolation

### Negative
- (-) More complex than flat tags
- (-) Requires concept management (not just strings)
- (-) Query complexity increases for hierarchical lookups
- (-) Learning curve for users expecting simple tags

## Implementation

**Code Location:** `crates/matric-db/src/skos_tags.rs`

**Schema:**

```sql
-- Concept schemes (tag namespaces)
CREATE TABLE skos_scheme (
    id UUID PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    created_at_utc TIMESTAMPTZ DEFAULT NOW()
);

-- Concepts (the actual tags)
CREATE TABLE skos_concept (
    id UUID PRIMARY KEY,
    scheme_id UUID REFERENCES skos_scheme(id),
    notation VARCHAR(100),  -- Short code (e.g., "rust", "pg")
    created_at_utc TIMESTAMPTZ DEFAULT NOW()
);

-- Labels (names for concepts)
CREATE TYPE label_type AS ENUM ('preferred', 'alternate', 'hidden');

CREATE TABLE skos_label (
    id UUID PRIMARY KEY,
    concept_id UUID REFERENCES skos_concept(id),
    label TEXT NOT NULL,
    label_type label_type NOT NULL,
    language VARCHAR(5) DEFAULT 'en',
    UNIQUE (concept_id, label_type, language)  -- One prefLabel per language
);

-- Hierarchical relationships
CREATE TABLE skos_relation (
    id UUID PRIMARY KEY,
    from_concept_id UUID REFERENCES skos_concept(id),
    to_concept_id UUID REFERENCES skos_concept(id),
    relation_type VARCHAR(20) NOT NULL,  -- 'broader', 'narrower', 'related'
    UNIQUE (from_concept_id, to_concept_id, relation_type)
);
```

**Tag Resolution:**

```rust
pub async fn resolve_tag(
    pool: &PgPool,
    tag_input: &str,
) -> Result<Option<SkosConceptId>> {
    // Try exact match on prefLabel first
    if let Some(concept) = find_by_pref_label(pool, tag_input).await? {
        return Ok(Some(concept));
    }

    // Try altLabel (synonyms)
    if let Some(concept) = find_by_alt_label(pool, tag_input).await? {
        return Ok(Some(concept));
    }

    // Try hiddenLabel (misspellings)
    if let Some(concept) = find_by_hidden_label(pool, tag_input).await? {
        return Ok(Some(concept));
    }

    // Try notation (short code)
    find_by_notation(pool, tag_input).await
}
```

**Strict Tag Filtering:**

```rust
pub struct StrictTagFilter {
    pub required_tags: Vec<Uuid>,      // Notes must have ALL
    pub any_tags: Vec<Uuid>,           // Notes must have AT LEAST ONE
    pub excluded_tags: Vec<Uuid>,      // Notes must NOT have
    pub required_schemes: Vec<Uuid>,   // Must have tag from these schemes
    pub excluded_schemes: Vec<Uuid>,   // Must NOT have tag from these schemes
}
```

## Research Citations

> "SKOS provides a model for expressing basic structure and content of concept schemes such as thesauri, classification schemes, subject heading lists." (REF-033, Miles & Bechhofer, 2009, Section 1)

> "One prefLabel per concept per language ensures unambiguous identification." (REF-033, Section 4)

## References

- `.aiwg/research/paper-analysis/REF-033-mm-analysis.md`
- `.aiwg/research/citable-claims-index.md` (SKOS Tagging section)
- W3C SKOS Reference: https://www.w3.org/TR/skos-reference/

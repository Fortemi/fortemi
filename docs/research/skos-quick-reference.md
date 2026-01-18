# SKOS Implementation Quick Reference

**Date:** 2025-01-17
**Purpose:** Quick lookup for SKOS implementation in Matric Memory

---

## Validated Sources Summary

| Source | Status | URL | Use Case |
|--------|--------|-----|----------|
| **W3C SKOS Reference** | ✓ Current (2009) | https://www.w3.org/TR/skos-reference/ | Normative spec |
| **W3C SKOS Primer** | ✓ Current (2009) | https://www.w3.org/TR/skos-primer/ | Tutorial |
| **ANSI/NISO Z39.19-2024** | ✓ Updated 2024 | https://www.niso.org/publications/z3919-2024 | Best practices |
| **ISO 25964-1:2011** | ✓ Current | https://www.iso.org/standard/53657.html | Thesaurus standard |
| **ISO 25964-2:2013** | ✓ Current | https://www.iso.org/standard/55460.html | Interoperability |
| **Library of Congress** | ✓ Production | https://id.loc.gov/ | Reference impl |
| **Getty Vocabularies** | ✓ Production | http://vocab.getty.edu/ | Large-scale impl |

---

## Recommended Rust Libraries

### Primary Choice: Sophia

```toml
[dependencies]
sophia = { version = "0.8", features = ["all-parsers", "all-serializers"] }
```

**Why:**
- Idiomatic Rust
- Comprehensive format support (Turtle, RDF/XML, N-Triples)
- Active maintenance
- Good performance
- Type-safe API

**Use for:**
- Parsing SKOS Turtle files
- Building RDF graphs
- SKOS export

### Alternative: Rio (for performance-critical parsing)

```toml
[dependencies]
rio_turtle = "0.8"
rio_api = "0.8"
```

**Why:**
- Fastest parsing performance
- Streaming API (memory efficient)
- Part of Oxigraph ecosystem

**Use for:**
- Large SKOS file imports
- Low-level RDF processing

### Optional: Oxigraph (if SPARQL needed)

```toml
[dependencies]
oxigraph = "0.3"
```

**Why:**
- Full SPARQL 1.1 support
- Persistent RDF store
- HTTP server with SPARQL endpoint

**Use for:**
- Complex SKOS queries
- SPARQL endpoint exposure
- RDF database backend

---

## SKOS Core Properties

### Concept Identification

```turtle
ex:Concept1 a skos:Concept ;
    skos:inScheme ex:MyScheme ;
    skos:topConceptOf ex:MyScheme .
```

### Labeling

```turtle
ex:Concept1
    skos:prefLabel "Preferred Label"@en ;      # Required, one per language
    skos:altLabel "Alternative Label"@en ;     # Optional, multiple allowed
    skos:hiddenLabel "Hidden Label"@en .       # For search, not display
```

### Documentation

```turtle
ex:Concept1
    skos:definition "Definition text"@en ;
    skos:scopeNote "Usage notes"@en ;
    skos:example "Example usage"@en ;
    skos:historyNote "Historical context"@en ;
    skos:editorialNote "Editorial notes"@en ;
    skos:changeNote "Change log"@en .
```

### Semantic Relations

```turtle
ex:Concept1
    skos:broader ex:ParentConcept ;            # More general
    skos:narrower ex:ChildConcept ;            # More specific
    skos:related ex:RelatedConcept .           # Associative
```

### Transitive Relations

```turtle
ex:Concept1
    skos:broaderTransitive ex:Ancestor ;       # Any ancestor
    skos:narrowerTransitive ex:Descendant .    # Any descendant
```

### Mapping Properties

```turtle
ex:Concept1
    skos:exactMatch <http://example.org/vocab/match> ;    # Equivalent
    skos:closeMatch <http://example.org/vocab/close> ;    # Similar
    skos:broadMatch <http://example.org/vocab/broad> ;    # More general
    skos:narrowMatch <http://example.org/vocab/narrow> ;  # More specific
    skos:relatedMatch <http://example.org/vocab/related> . # Associated
```

---

## SQL Schema Quick Reference

### Core Tables

```sql
-- Concept Schemes
skos_schemes (uri PK, title, description, creator, properties JSONB)

-- Concepts
skos_concepts (id UUID PK, uri UNIQUE, pref_label, scheme_uri FK, definition)

-- Labels
skos_labels (concept_id FK, label_type, label_text, language, PK(all))

-- Relations
skos_relations (source_id FK, target_id FK, relation_type, PK(all))

-- Mappings
skos_mappings (concept_id FK, target_uri, mapping_type, confidence, PK(...))

-- Hierarchy Cache
skos_hierarchy_paths (ancestor_id FK, descendant_id FK, depth, PK(...))

-- Tag Integration
tag_skos_mapping (tag_id FK, concept_id FK, PK(both))
```

### Key Queries

**Search concepts:**
```sql
SELECT DISTINCT c.*
FROM skos_concepts c
LEFT JOIN skos_labels l ON l.concept_id = c.id
WHERE c.pref_label ILIKE '%query%'
   OR l.label_text ILIKE '%query%';
```

**Get ancestors:**
```sql
SELECT c.*
FROM skos_concepts c
JOIN skos_hierarchy_paths p ON p.ancestor_id = c.id
WHERE p.descendant_id = $1 AND p.depth > 0
ORDER BY p.depth;
```

**Get descendants:**
```sql
SELECT c.*
FROM skos_concepts c
JOIN skos_hierarchy_paths p ON p.descendant_id = c.id
WHERE p.ancestor_id = $1 AND p.depth > 0
ORDER BY p.depth;
```

**Validate (no cycles):**
```sql
WITH RECURSIVE cycles AS (
    SELECT source_id, target_id, ARRAY[source_id] AS path
    FROM skos_relations WHERE relation_type = 'broader'
    UNION ALL
    SELECT c.source_id, r.target_id, c.path || r.source_id
    FROM cycles c
    JOIN skos_relations r ON r.source_id = c.target_id
    WHERE r.relation_type = 'broader'
      AND r.source_id = ANY(c.path)
)
SELECT * FROM cycles WHERE source_id = ANY(path[2:]);
```

---

## Common Anti-Patterns

| Anti-Pattern | Detection | Prevention |
|--------------|-----------|------------|
| **Cyclic hierarchies** | Recursive CTE | Pre-import validation |
| **Orphan concepts** | No scheme, no relations | Require inScheme |
| **Label conflicts** | Duplicate prefLabels | UNIQUE constraint |
| **Missing broader/narrower symmetry** | One-way relations | Auto-create inverse |
| **Reflexive relations** | source_id = target_id | CHECK constraint |
| **Missing top concepts** | No broader relations | Auto-detect |
| **Overlapping labels** | prefLabel = altLabel | Normalization |

---

## Validation Tools

### qSKOS (Java)
- **URL:** https://github.com/cmader/qSKOS
- **Purpose:** 28+ quality checks
- **Usage:** Pre-import validation
- **Command:** `java -jar qSKOS.jar analyze -dc vocab.ttl`

### Skosify (Python)
- **URL:** https://github.com/NatLibFi/Skosify
- **Purpose:** Auto-fix common issues
- **Usage:** Normalization pipeline
- **Command:** `skosify -o fixed.ttl input.ttl`

### SKOS Play (Web)
- **URL:** http://labs.sparna.fr/skos-play/
- **Purpose:** Visualization and conversion
- **Usage:** Manual validation and docs

---

## Implementation Phases

### Phase 1: Foundation (2 weeks)
- [ ] Create `matric-skos` crate
- [ ] Add Sophia dependency
- [ ] Implement SKOS schema migration
- [ ] Build basic Turtle parser
- [ ] Create repository layer

### Phase 2: Hierarchy (2 weeks)
- [ ] Implement relations CRUD
- [ ] Build hierarchy materialization
- [ ] Add cycle detection
- [ ] Create hierarchy query API
- [ ] Test with sample vocab

### Phase 3: Integration (2 weeks)
- [ ] Link SKOS to tags
- [ ] Extend tag search with altLabels
- [ ] Add SKOS hierarchy to tag API
- [ ] Build SKOS export endpoint

### Phase 4: Validation (2 weeks)
- [ ] Implement anti-pattern detection
- [ ] Add quality metrics
- [ ] Build validation reporting
- [ ] Create admin dashboard

### Phase 5: External Mapping (2 weeks)
- [ ] Support external mappings
- [ ] Build reconciliation API
- [ ] Add mapping suggestions
- [ ] Create mapping UI

---

## Sample SKOS Vocabularies for Testing

### Small (< 1,000 concepts)

**STW Thesaurus for Economics**
- Size: ~6,000 concepts
- URL: http://zbw.eu/stw/
- Format: SKOS/RDF
- License: CC BY 4.0
- Good for: Initial testing

### Medium (1,000 - 10,000 concepts)

**UNESCO Thesaurus**
- Size: ~7,000 concepts
- URL: http://vocabularies.unesco.org/
- Multilingual: 40+ languages
- Good for: Language testing

**EuroVoc**
- Size: ~7,000 concepts
- URL: https://op.europa.eu/en/web/eu-vocabularies
- Multilingual: 24 EU languages
- Good for: Multilingual testing

### Large (> 100,000 concepts)

**Getty AAT (Art & Architecture Thesaurus)**
- Size: ~370,000 concepts
- URL: http://vocab.getty.edu/aat/
- Format: SKOS/RDF
- Good for: Performance testing

**Library of Congress Subject Headings (LCSH)**
- Size: ~450,000 concepts
- URL: https://id.loc.gov/authorities/subjects.html
- Format: SKOS/RDF
- Good for: Scale testing

---

## Performance Considerations

### Indexing Strategy

```sql
-- Full-text search
CREATE INDEX idx_skos_concepts_pref_label_fts
    ON skos_concepts USING gin(to_tsvector('english', pref_label));

CREATE INDEX idx_skos_labels_text_fts
    ON skos_labels USING gin(to_tsvector('english', label_text));

-- Hierarchy traversal
CREATE INDEX idx_skos_relations_source
    ON skos_relations(source_id, relation_type);

CREATE INDEX idx_skos_hierarchy_ancestor
    ON skos_hierarchy_paths(ancestor_id, depth);
```

### Caching Strategy

1. **Materialized Hierarchy:** Pre-compute ancestor/descendant paths
2. **Scheme Lookup:** Cache scheme metadata in memory
3. **Top Concepts:** Cache top concepts per scheme
4. **Search Results:** Cache common search queries (Redis)

### Import Optimization

```rust
// Batch insert for large vocabularies
let batch_size = 1000;
for chunk in concepts.chunks(batch_size) {
    // Use COPY or multi-row INSERT
    repository.batch_create_concepts(chunk).await?;
}
```

---

## Common Integration Patterns

### Pattern 1: Tag Enhancement

```rust
// When user creates tag, suggest SKOS concept
async fn create_tag(name: &str) -> Result<Tag> {
    let tag = tag_repo.create(name).await?;

    // Search SKOS for matching concept
    if let Some(concept) = skos_repo.search_exact(name).await? {
        // Link tag to concept
        tag_skos_repo.link(tag.id, concept.id).await?;
    }

    Ok(tag)
}
```

### Pattern 2: Hierarchical Tag Navigation

```rust
// Get tag hierarchy via SKOS
async fn get_tag_hierarchy(tag_id: Uuid) -> Result<Vec<Tag>> {
    // Get linked SKOS concept
    let concept_id = tag_skos_repo.get_concept(tag_id).await?;

    // Get SKOS ancestors
    let ancestor_concepts = skos_repo.get_ancestors(concept_id).await?;

    // Map back to tags
    let tags = tag_skos_repo.concepts_to_tags(&ancestor_concepts).await?;

    Ok(tags)
}
```

### Pattern 3: Vocabulary-based Search

```rust
// Expand search with SKOS relations
async fn search_notes(query: &str) -> Result<Vec<Note>> {
    // Find matching concepts
    let concepts = skos_repo.search_concepts(query).await?;

    // Get related concepts (broader, narrower, related)
    let mut all_concepts = concepts.clone();
    for concept in &concepts {
        all_concepts.extend(skos_repo.get_related(concept.id).await?);
    }

    // Find tags linked to these concepts
    let tags = tag_skos_repo.concepts_to_tags(&all_concepts).await?;

    // Search notes with expanded tags
    note_repo.search_by_tags(tags).await
}
```

---

## Testing Checklist

### Unit Tests
- [ ] Parse simple SKOS Turtle
- [ ] Parse complex hierarchies
- [ ] Detect cycles
- [ ] Handle malformed input
- [ ] Validate label uniqueness
- [ ] Test relation symmetry

### Integration Tests
- [ ] Import small vocabulary (< 100 concepts)
- [ ] Import medium vocabulary (1,000 concepts)
- [ ] Query hierarchy (ancestors/descendants)
- [ ] Search across labels
- [ ] Link tags to concepts
- [ ] Export to SKOS

### Performance Tests
- [ ] Import large vocabulary (100K+ concepts)
- [ ] Hierarchy query performance (< 100ms)
- [ ] Search performance (< 50ms)
- [ ] Concurrent imports
- [ ] Cache effectiveness

### Validation Tests
- [ ] Detect all anti-patterns
- [ ] Generate quality report
- [ ] Handle validation errors gracefully

---

## Troubleshooting

### Issue: Slow hierarchy queries

**Solution:**
```sql
-- Ensure hierarchy paths are materialized
SELECT refresh_skos_hierarchy();

-- Verify indexes exist
\d skos_hierarchy_paths
```

### Issue: Import fails with encoding error

**Solution:**
```rust
// Ensure UTF-8 encoding
let content = std::fs::read_to_string(path)?;
// or
let content = String::from_utf8_lossy(&bytes);
```

### Issue: Cycle detected on import

**Solution:**
```rust
// Run validation before hierarchy materialization
let validation_results = service.validate().await?;
let errors: Vec<_> = validation_results.iter()
    .filter(|r| r.severity == "error")
    .collect();

if !errors.is_empty() {
    return Err(anyhow!("Validation failed: {:?}", errors));
}
```

### Issue: Label conflicts

**Solution:**
```sql
-- Find conflicts
SELECT pref_label, scheme_uri, COUNT(*)
FROM skos_concepts
GROUP BY pref_label, scheme_uri
HAVING COUNT(*) > 1;

-- Option 1: Add unique constraint
ALTER TABLE skos_concepts
ADD CONSTRAINT unique_label_per_scheme
UNIQUE (pref_label, scheme_uri);

-- Option 2: Append notation to disambiguate
UPDATE skos_concepts
SET pref_label = pref_label || ' [' || notation || ']'
WHERE (pref_label, scheme_uri) IN (
    SELECT pref_label, scheme_uri
    FROM skos_concepts
    GROUP BY pref_label, scheme_uri
    HAVING COUNT(*) > 1
);
```

---

## Resources

### Documentation
- W3C SKOS Primer: https://www.w3.org/TR/skos-primer/
- W3C SKOS Reference: https://www.w3.org/TR/skos-reference/
- NISO Z39.19-2024: https://www.niso.org/publications/z3919-2024

### Tools
- qSKOS: https://github.com/cmader/qSKOS
- Skosify: https://github.com/NatLibFi/Skosify
- SKOS Play: http://labs.sparna.fr/skos-play/
- VocBench: http://vocbench.uniroma2.it/

### Libraries
- Sophia (Rust): https://github.com/pchampin/sophia_rs
- Rio (Rust): https://github.com/oxigraph/rio
- Oxigraph (Rust): https://github.com/oxigraph/oxigraph

### Example Vocabularies
- Library of Congress: https://id.loc.gov/
- Getty Vocabularies: http://vocab.getty.edu/
- UNESCO Thesaurus: http://vocabularies.unesco.org/
- BARTOC (registry): https://bartoc.org/

---

**Last Updated:** 2025-01-17
**See Also:**
- `skos-implementation-research.md` - Full research report
- `skos-rust-implementation-guide.md` - Detailed code examples

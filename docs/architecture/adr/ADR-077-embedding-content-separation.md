# ADR-077: Embedding Content Separation

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Issues:** #475, #479

## Context

Fortemi enriches note embeddings by prepending SKOS concept labels to the content before embedding (#424). This improves retrieval by encoding topic classification into the vector. However, the initial implementation included ALL assigned concepts, which worsened the "seashell pattern":

- High-frequency concepts (e.g., "Programming", "Software") appear on 80%+ of notes
- Including these "stopword concepts" in the embedding text pushes all vectors toward a shared centroid
- Concept relationships (broader/narrower/related from SKOS hierarchy) add further noise -- "Rust is-narrower-than Programming" makes Rust notes more similar to Python notes via the shared parent
- The result: embedding enrichment that was intended to improve discrimination was actively making vectors more uniform

The note record still needs full concept metadata for display, search filtering, and tag-boost scoring in the linking pipeline. The problem is conflating what the embedding should encode (discriminating signal) with what the record should store (complete metadata).

## Decision

Separate the embedding payload (optimized for vector discrimination) from the record payload (full metadata):

**Embedding payload** (input to the embedding model):
1. **Instruction prefix:** `clustering: ` (nomic-embed-text task prefix, #472) -- maximizes inter-cluster distance
2. **Title**
3. **Discriminating concepts only:** Filtered by TF-IDF document frequency (#475)
4. **Note content** (revised if available, otherwise original)

**Record payload** (stored on the note):
- All SKOS concepts (including high-frequency ones)
- Concept relationships (broader, narrower, related)
- Full metadata for display and filtering

**TF-IDF filtering rule:** Concepts with document frequency above `EMBED_CONCEPT_MAX_DOC_FREQ` (default 0.8, meaning concepts appearing in >80% of notes) are excluded from the embedding text. The threshold is configurable and clamped to [0.01, 1.0].

**Concept relationships** (broader/narrower/related) are always excluded from the embedding payload. They are fetched from the DB for potential future use but not prepended to the embedding text, because they add shared semantic content that makes vectors more uniform.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Weight-based concept inclusion | Arbitrary weight selection; does not address the frequency problem |
| No enrichment at all | Loses the genuine benefit of topic-discriminating concepts |
| Separate embedding sets per strategy | Storage overhead; operational complexity of maintaining parallel sets |
| IDF weighting in embedding text | Embedding models do not process numeric weights meaningfully in free text |

## Consequences

### Positive
- (+) Discriminating concepts improve vector separation; stopword concepts no longer collapse distances
- (+) `clustering:` prefix shifts embedding geometry for graph-optimal neighbor selection
- (+) Configurable threshold (`EMBED_CONCEPT_MAX_DOC_FREQ`) adapts to corpus characteristics
- (+) Full metadata remains on note record for display, filtering, and tag-boost scoring
- (+) Clean architectural boundary: embedding is a derived signal, not a copy of the record

### Negative
- (-) Re-embedding required when threshold changes (vectors depend on which concepts were included)
- (-) TF-IDF filtering depends on global note count, which shifts as corpus grows
- (-) Concept relationships computed but not used in embedding (wasted query work, kept for future use)
- (-) `clustering:` prefix is model-specific (only works with nomic-embed-text instruction format)

## Implementation

**Code Location:**
- TF-IDF filtering: `crates/matric-api/src/handlers/jobs.rs` (`EmbeddingHandler::execute`)
- Threshold constant: `crates/matric-core/src/defaults.rs` (`EMBED_CONCEPT_MAX_DOC_FREQ`)
- Prefix constant: `crates/matric-core/src/defaults.rs` (`EMBED_INSTRUCTION_PREFIX`)

**Configuration:**

| Variable | Default | Description |
|----------|---------|-------------|
| `EMBED_CONCEPT_MAX_DOC_FREQ` | 0.8 | Max document frequency ratio for concept inclusion |
| `EMBED_INSTRUCTION_PREFIX` | `clustering: ` | Task prefix for embedding model |

**TF-IDF Filter Query:**

```sql
SELECT l.value FROM note_skos_concept nc
JOIN skos_concept_label l ON nc.concept_id = l.concept_id
JOIN skos_concept c ON nc.concept_id = c.id
WHERE nc.note_id = $1 AND l.label_type = 'pref_label'
  AND c.note_count::float / GREATEST(
    (SELECT COUNT(*) FROM note WHERE deleted_at IS NULL), 1
  ) <= $2
ORDER BY nc.is_primary DESC, nc.relevance_score DESC
```

**Embedding Text Assembly:**

```rust
// prefix + title + discriminating tags + content
let payload = format!(
    "clustering: {title}\n\nTags: {concepts}\n\n{content}",
    title = note.title,
    concepts = discriminating_concepts.join(", "),
    content = note.content,
);
```

## References

- ADR-073: Graph Quality Pipeline Architecture
- ADR-023: Matryoshka Representation Learning
- Issue #475: TF-IDF Concept Filtering
- Issue #479: Embedding vs. Record Separation
- Issue #472: Embedding Instruction Prefix

# ADR-012: Semantic Linking with 0.7 Similarity Threshold

**Status:** Accepted
**Date:** 2026-01-02
**Deciders:** roctinam
**Research:** REF-030 (Reimers & Gurevych, 2019), REF-032 (Hogan et al., 2021)

## Context

matric-memory automatically discovers relationships between notes based on semantic similarity. When a note is created or updated, the system finds other notes with similar content and creates bidirectional links.

The key question: What similarity threshold should trigger link creation?
- Too low (0.5): Links between vaguely related notes, noisy knowledge graph
- Too high (0.9): Only near-duplicates linked, misses valuable connections

## Decision

Use **0.7 cosine similarity** as the threshold for automatic semantic link creation.

This threshold is based on SBERT research (REF-030) which found:
- 0.7+ indicates "strong semantic relatedness"
- 0.9+ indicates "near-paraphrase"
- 0.5-0.7 is "topically similar" but not strongly related

Links are bidirectional (if A links to B, B links to A) with the similarity score stored as edge weight.

## Consequences

### Positive
- (+) Meaningful connections that aid navigation
- (+) Backlinks provide discovery of related content
- (+) Score indicates relationship strength
- (+) Research-backed threshold choice
- (+) Knowledge graph grows automatically

### Negative
- (-) Fixed threshold may not suit all content types
- (-) Dense topics may create many links
- (-) Threshold tuning may be needed per collection
- (-) Embedding quality directly affects link quality

## Implementation

**Code Location:**
- Threshold: `crates/matric-db/src/links.rs`
- Linking job: `crates/matric-jobs/src/linking.rs`

**Threshold Constant:**

```rust
// crates/matric-db/src/links.rs

/// Semantic linking threshold based on SBERT research (REF-030)
/// 0.7 captures strong semantic relatedness without excessive noise
pub const SEMANTIC_LINK_THRESHOLD: f32 = 0.7;
```

**Link Discovery Query:**

```sql
-- Find notes above similarity threshold
SELECT
    ne.note_id as to_note_id,
    1 - (ne.embedding <=> $1::vector) as similarity
FROM note_embeddings ne
JOIN notes n ON ne.note_id = n.id
WHERE ne.note_id != $2
  AND n.deleted_at IS NULL
  AND 1 - (ne.embedding <=> $1::vector) >= 0.7
ORDER BY similarity DESC
LIMIT 20
```

**Link Creation:**

```rust
pub async fn create_semantic_links(
    pool: &PgPool,
    note_id: Uuid,
    embedding: &[f32],
) -> Result<Vec<Uuid>> {
    let related = find_related_notes(pool, note_id, embedding).await?;

    for target in &related {
        // Create bidirectional links
        create_reciprocal_link(
            pool,
            note_id,
            target.note_id,
            "semantic",
            target.similarity,
        ).await?;
    }

    Ok(related.iter().map(|r| r.note_id).collect())
}
```

**Similarity Scale Reference:**

| Range | Interpretation | Action |
|-------|----------------|--------|
| 0.9-1.0 | Near-duplicate/paraphrase | Strong link |
| 0.7-0.9 | Semantically related | Create link |
| 0.5-0.7 | Topically similar | No automatic link |
| 0.0-0.5 | Unrelated | No link |

## Research Citations

> "We observe that a cosine-similarity threshold of around 0.7 works well for identifying semantically similar sentences." (REF-030, Reimers & Gurevych, 2019, Section 4.2)

> "Property graphs with weighted edges enable expressive knowledge representation; recursive traversal supports multi-hop reasoning." (REF-032, Hogan et al., 2021)

## References

- `.aiwg/research/paper-analysis/REF-030-mm-analysis.md`
- `.aiwg/research/paper-analysis/REF-032-mm-analysis.md`
- `.aiwg/research/citable-claims-index.md` (Sentence Embeddings section)

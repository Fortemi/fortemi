# ADR-081: DocumentComposition — Configurable Embedding Text

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam

## Context

Each embedding set has a single most-important characteristic: what text is assembled from a note and fed to the embedding model. Different use cases require different compositions:

- A **topic clustering** set wants: instruction prefix + title + discriminating concepts + content
- A **title-only** set for fast approximate search wants: title alone
- A **semantic search** set optimized for retrieval wants: `search_document:` prefix + content only
- A **tag-augmented** set wants: title + SKOS tags + content

Previously, the embedding text was hardcoded inside `EmbeddingHandler::execute`. Changing the composition required code changes and redeployment. Creating multiple embedding sets with different compositions for A/B comparison or specialized retrieval was not possible without forking the handler.

## Decision

Introduce `DocumentComposition`, a serialized struct stored in the `embedding_config` table's `document_composition` JSON column. The struct controls what note properties are assembled into the embedding text:

```rust
pub struct DocumentComposition {
    pub include_title: bool,          // default: true
    pub include_content: bool,        // default: true
    pub tag_strategy: TagStrategy,    // default: None (no tags)
    pub include_concepts: bool,       // default: false
    pub concept_max_doc_freq: f64,    // default: 0.8 (TF-IDF filter from ADR-077)
    pub instruction_prefix: String,   // default: "clustering: "
}
```

`DocumentComposition::build_text(title, content, concept_labels)` assembles the final embedding input from the struct's settings. The embedding handler reads the composition from the active embedding set's config and delegates text assembly to this method.

`TagStrategy` controls how SKOS tags are included:
- `None` — no tags in embedding text (default)
- `All` — all tag names prepended
- `Primary` — only primary tag

**Auto re-embed on composition change:** When `PATCH /api/v1/embedding-configs/:id` changes `document_composition`, the API automatically queues `RefreshEmbeddingSet` to re-embed all notes with the new text composition.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Hardcode composition per embedding set type | Inflexible; every new composition requires a code change |
| Store composition as separate DB columns | Schema changes required to add new composition fields |
| Separate embedding set subtypes | Multiplies handler code; compositions are orthogonal to storage strategy |
| Environment variable composition config | Cannot differ per embedding set; precludes A/B testing |

## Consequences

### Positive
- (+) Embedding sets are self-describing: composition is stored alongside the set, not in code
- (+) Multiple embedding sets can use different compositions in the same archive
- (+) A/B testing of composition strategies is possible without code changes
- (+) TF-IDF concept filtering (ADR-077) is integrated as a field rather than a separate mechanism
- (+) Auto re-embed on change ensures vectors stay consistent with their declared composition

### Negative
- (-) Changing composition invalidates all existing vectors in the set (requires full re-embed)
- (-) `DocumentComposition` is serialized as JSON; schema changes to the struct require migration care
- (-) `build_text` is called at embedding time, not index time; composition changes affect future embeddings only
- (-) `instruction_prefix` is model-specific; wrong prefix for a different model degrades quality

## Implementation

**Code Location:**
- Struct: `crates/matric-core/src/models.rs` (`DocumentComposition`, `TagStrategy`)
- Text assembly: `crates/matric-core/src/models.rs` (`DocumentComposition::build_text`)
- DB storage: `embedding_config.document_composition` column (JSONB, nullable — NULL means default composition)
- Handler: `crates/matric-api/src/handlers/jobs.rs` (`EmbeddingHandler::execute`)
- Auto re-embed: `crates/matric-api/src/main.rs` (PATCH handler for embedding configs)

**Default Composition:**

```rust
DocumentComposition {
    include_title: true,
    include_content: true,
    tag_strategy: TagStrategy::None,
    include_concepts: false,
    concept_max_doc_freq: 0.8,
    instruction_prefix: "clustering: ".to_string(),
}
```

**Configuration:**

| Variable | Default | Description |
|----------|---------|-------------|
| `EMBED_INSTRUCTION_PREFIX` | `clustering: ` | Default instruction prefix for new embedding sets |
| `EMBED_CONCEPT_MAX_DOC_FREQ` | `0.8` | Default TF-IDF threshold for concept filtering |

## References

- ADR-022: Embedding Set Types
- ADR-023: Matryoshka Representation Learning
- ADR-077: Embedding Content Separation (TF-IDF concept filtering)
- Issue #475: TF-IDF Concept Filtering
- Issue #479: Embedding vs. Record Separation

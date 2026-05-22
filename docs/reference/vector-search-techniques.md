# Vector Search Techniques in Fortemi

**Generated:** 2026-05-22
**Source of truth:** `crates/matric-search/` and migrations on `main` at the time of writing.
**Scope:** Inventory of vector / hybrid search techniques actively used in the Fortemi codebase, with module references and citations where present.

This is a reference snapshot. For the latest, query the corpus directly (`aiwg discover "<topic>"`) or read the linked modules.

---

## Indexing & Storage

| Technique | Where | Notes |
|---|---|---|
| **pgvector** | All embeddings live in Postgres via the `pgvector/pgvector:pg18` bundle image | Distance op `<=>` (cosine) used throughout |
| **HNSW indexes** | Default for `embedding`, `note_graph_embedding`, `embedding_coarse`, attachment vision/clip vectors | `vector_cosine_ops` — cosine similarity |
| **IVFFlat indexes** | Legacy on `embedding` table; SKOS `skos_concept.embedding` | Older index; HNSW preferred for new tables |
| **`halfvec` storage** | Several tables (initial schema, full_embedding_sets, skos_tags, attachment_doctype) | Half-precision float — ~2× memory savings vs full `vector` |
| **Per-archive embedding sets** | `embedding_sets` migration (2026-01-17) + `full_embedding_sets` (2026-02-01) | Multiple embedding spaces per memory archive — different models can coexist |

Distance operator across the codebase: `<=>` (cosine). No L2 (`<->`) or inner-product (`<#>`) primary use observed in the search engine code path, though pgvector supports them.

---

## Retrieval — Single-Vector

| Technique | Module | Notes |
|---|---|---|
| **Cosine similarity** (`<=>`) | Used by `hybrid.rs`, `colbert.rs`, all migrations creating HNSW indexes | Default everywhere |
| **HNSW `ef_search` runtime tuning** | `crates/matric-search/src/hnsw_tuning.rs` | Dynamic `ef_search` per query based on `corpus_size` and `RecallTarget`. Includes `estimated_recall()` and `estimated_latency_ms()` heuristics. Cites **REF-031 Malkov & Yashunin "HNSW"**. |

`hnsw_tuning.rs` exposes:

```rust
pub use hnsw_tuning::{
    compute_ef, estimated_latency_ms, estimated_recall, HnswTuningConfig, RecallTarget,
};
```

---

## Hybrid / Multi-Signal Retrieval

| Technique | Module | Notes |
|---|---|---|
| **Hybrid search (FTS + semantic)** | `hybrid.rs` — `HybridSearchEngine`, `HybridSearchConfig`, `SearchStrategy` | Combines Postgres `tsvector`/GIN with pgvector |
| **Reciprocal Rank Fusion (RRF)** | `rrf.rs` | Classic Cormack RRF for fusing FTS and semantic ranks |
| **Adaptive RRF** | `adaptive_rrf.rs` — `AdaptiveRrfConfig`, `QueryCharacteristics`, `select_k()` | Tunes the `k` denominator per query characteristics |
| **Adaptive fusion weights** | `adaptive_weights.rs` — `AdaptiveWeightConfig`, `FusionWeights` | Adjusts FTS vs semantic balance per query |
| **Reciprocal Score Fusion (RSF)** | `rsf.rs` | Alternative score-based fusion |
| **Semantic floor thresholds** | `hybrid.rs` constants `MIN_SEMANTIC_SCORE` / `MIN_SEMANTIC_SCORE_WITH_FTS` | Rejects low-similarity semantic results — cosine 0.3–0.5 is noise on most embedding models because all vectors occupy a similar region of the embedding manifold |

Strategy selection happens via `SearchStrategy` on `SearchRequest`; callers can force FTS-only, semantic-only, or hybrid.

---

## Re-Ranking

| Technique | Module | Notes |
|---|---|---|
| **ColBERT late-interaction reranking** | `colbert.rs` — `ColBERTReranker`, `ColBERTConfig` + migration `2026-02-05 colbert_embeddings.sql` | MaxSim over per-token vectors for precision rerank. For each query token, finds the most-similar document token and sums max similarities. |
| **MMR (Maximal Marginal Relevance) diversity rerank** | `mmr.rs` — shipped per #561 (Q1 2026) | Re-orders results for diversity: `λ * relevance − (1−λ) * max_similarity_to_selected`. Configurable per query (`λ` and `top_k` to consider). |
| **Cold-spot / access-frequency signals** | Not in `matric-search` directly; exposed via `GET /api/v1/graph/cold-spots` (#564) and `GET /api/v1/health/access-frequency` (#562) | Surfaces graph-isolated + rarely-accessed notes. Not a rerank per se but a complementary retrieval mode. |

---

## Multilingual & Text Preprocessing

| Technique | Where | Notes |
|---|---|---|
| **Script detection** | `script_detection.rs` | Latin / Cyrillic / CJK / etc. detection per query — drives FTS config selection |
| **Per-script FTS configs** | `fts_flags.rs` + migrations `2026-02-01 multilingual_fts_phase1.sql`, `_phase3`, Unicode-normalization phases | Postgres `tsvector` configs per script |
| **Unicode normalization** | `2026-01-31 fts_unicode_normalization.sql` | NFKC-style normalization before tokenization |
| **FTS config qualification** | `2026-02-15 fts_qualify_config_names.sql` | Schema-qualified config names for per-archive FTS |

---

## Deduplication

| Technique | Module | Notes |
|---|---|---|
| **Chunk-aware result dedup** | `deduplication.rs` — `EnhancedSearchHit`, `ChainSearchInfo`, `DeduplicationConfig` | Long documents are chunked for embedding; this stitches results back to one hit per source document. |

The `deduplication` step runs after RRF/RSF fusion and before final re-ranking.

---

## Out-of-Band / Supporting

- **Per-call ASR transcript vectors**: not yet embedded as part of the real-time provider work (#837). Post-call batch processing via the existing `AudioTranscriptionHandler` will embed via the standard pipeline once that lands.
- **Cold-spot detection** (#564 — graph isolation + access coldness) is a graph + access-log technique, not strictly vector search, but lives adjacent in the discovery surface.

---

## Pipeline Composition (Conceptual)

```
                  Query
                    │
        ┌───────────┴────────────┐
        │                        │
        ▼                        ▼
   Script + FTS            Embed query
   config select           (per archive's
        │                   embedding set)
        ▼                        │
   FTS (tsvector             ┌───┴────┐
   GIN scan)                 │        │
        │                    ▼        ▼
        │                HNSW      ColBERT
        │                ANN +     (when
        │                ef_tune   enabled)
        │                    │        │
        └────────┬───────────┴────────┘
                 ▼
         Fusion (RRF / Adaptive RRF / RSF)
         + adaptive weights + semantic floor
                 │
                 ▼
            Deduplication
            (chunk → source doc)
                 │
                 ▼
         MMR diversity rerank
         (optional, per request)
                 │
                 ▼
         Final ranked results
```

---

## Cited Research Foundations

Captured in module docs / comments at time of writing:

- **REF-031** — Malkov & Yashunin, "Efficient and robust approximate nearest neighbor search using HNSW" (cited in `hnsw_tuning.rs`)
- **Cormack et al.** RRF formulation (in `rrf.rs` per convention)
- **Khattab & Zaharia, ColBERT** — late interaction over per-token vectors (in `colbert.rs`)

Other REF references likely appear in `.aiwg/research/findings/`. Full provenance accessible via:

```bash
aiwg discover "HNSW tuning"
aiwg discover "ColBERT late interaction"
aiwg discover "RRF reciprocal rank fusion"
aiwg discover "MMR diversity"
aiwg discover "hybrid search"
```

---

## Module Layout

```
crates/matric-search/src/
├── lib.rs                # Re-exports + module docs
├── hybrid.rs             # HybridSearchEngine, SearchRequest, SearchStrategy
├── rrf.rs                # Reciprocal Rank Fusion
├── adaptive_rrf.rs       # Adaptive k selection
├── adaptive_weights.rs   # Adaptive FTS/semantic weighting
├── rsf.rs                # Reciprocal Score Fusion
├── colbert.rs            # ColBERT late-interaction reranking
├── mmr.rs                # Maximal Marginal Relevance diversity
├── deduplication.rs      # Chunk → source dedup
├── hnsw_tuning.rs        # Dynamic ef_search tuning
├── script_detection.rs   # Per-query script detection
└── fts_flags.rs          # FTS config feature flags
```

11 modules; ~thousands of LOC. All within `crates/matric-search/`.

---

## Quick Audit Commands

```bash
# List all vector ops in migrations
grep -hE "USING (hnsw|ivfflat)" migrations/*.sql

# Find all FTS-related migrations
ls migrations/*fts*.sql migrations/*multilingual*.sql

# Inspect a specific reranker
cat crates/matric-search/src/colbert.rs

# Run search unit tests
cargo test -p matric-search
```

---

## Related Reference

- HotM `/chat` consumer contract — calls `GET /search?q=...&mode=hybrid|fts|semantic` (per #549 closed contract)
- MCP `search_notes` tool — exposes the same search surface to agents
- Per-archive embedding sets — `POST /api/v1/embedding-sets`

## See also

- [`docs/architecture/`](../architecture/) — full architecture references
- [`docs/deployment/extraction-services.md`](../deployment/extraction-services.md) — extraction/embedding pipeline details
- [`.aiwg/research/findings/`](../../.aiwg/research/findings/) — REF-XXX research findings powering these techniques

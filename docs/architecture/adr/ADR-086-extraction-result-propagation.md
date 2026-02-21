# ADR-086: Extraction Result Propagation to Notes and Downstream Jobs

**Status:** Accepted (implemented 2026-02-20)
**Date:** 2026-02-20
**Deciders:** Architecture team
**Related:** ADR-048 (Extraction Adapter Pattern), ADR-034 (3D File Analysis — partially superseded), ADR-079 (Global Job Deduplication), ADR-082 (Queue-Based Tier Escalation), Issue #492

## Context

The extraction adapter pattern (ADR-048) defines how file attachments are processed: each `ExtractionStrategy` maps to a concrete `ExtractionAdapter` that returns an `ExtractionResult`:

```rust
pub struct ExtractionResult {
    pub extracted_text: Option<String>,
    pub metadata: serde_json::Value,
    pub ai_description: Option<String>,
    pub preview_data: Option<Vec<u8>>,
}
```

The `ExtractionHandler` persists `extracted_text` and `metadata` on the attachment record but **ignores `ai_description`**. This is a critical gap for adapters where the primary useful output is AI-generated rather than mechanically extracted:

| Adapter | `extracted_text` | `ai_description` | Primary output |
|---------|-----------------|-------------------|----------------|
| TextNative | content | None | `extracted_text` |
| PdfText | page text | None | `extracted_text` |
| CodeAst | source code | None | `extracted_text` |
| AudioTranscribe | transcript | None | `extracted_text` |
| Vision | None | image description | **`ai_description` (lost)** |
| VideoMultimodal | transcript | None | `extracted_text` |
| Glb3DModel | None | composite synthesis | **`ai_description` (lost)** |

For 3D models and images, the most valuable extraction output is the AI-generated description, which is currently discarded.

Additionally, when a note is created with an attachment, downstream jobs (Embedding, Linking, ConceptTagging) are queued simultaneously with the Extraction job. These downstream jobs run on the note's original content — which is empty or minimal for attachment-primary notes (3D models, images, audio). By the time extraction completes, downstream jobs have already finished processing empty content, producing useless embeddings and missing link opportunities.

### Constraints

1. The `file_attachment` table currently has `extracted_content TEXT` and `extracted_metadata JSONB` — no `ai_description` column
2. The job queue has no dependency mechanism (`depends_on` column)
3. Downstream jobs use deduplication (ADR-079) so re-queuing after extraction won't create duplicates if the original job already ran — but deduplication only blocks if a job is still `pending`, not if it already `completed`
4. The Three.js renderer for 3D models may not be available in all deployments — adapter registration should reflect this

## Decision

### 1. Add `ai_description` column to `file_attachment`

Add a new migration:

```sql
ALTER TABLE file_attachment ADD COLUMN ai_description TEXT;
ALTER TABLE file_attachment ADD COLUMN ai_model TEXT;
```

This mirrors the schema originally proposed in ADR-034's `model_3d_metadata` table but keeps it on the attachment record rather than a separate table, consistent with the existing `extracted_content`/`extracted_metadata` pattern.

### 2. Persist `ai_description` in ExtractionHandler

Update `ExtractionHandler::execute()` to store `ai_description` when present:

```rust
// After existing update_extracted_content_tx call:
if let Some(ref description) = result.ai_description {
    file_storage.update_ai_description_tx(&mut tx, att_id, description, model_name).await?;
}
```

### 3. Propagate extraction results to note content

After extraction completes for an attachment, if `ai_description` or `extracted_text` produced content, update the parent note's content and re-queue downstream jobs:

- If the note content is empty or contains only a filename stub, replace it with the extracted/described content
- If the note already has substantive content, append a `---\n## Attachment: {filename}\n{description}` section
- After updating note content, re-queue `Embedding`, `ConceptTagging`, `Linking`, and `TitleGeneration` jobs using `queue_deduplicated`. Since the original batch already completed (not pending), deduplication won't block the re-queue

This "extract then enrich" pattern means:
1. Note created → immediate jobs process whatever text content exists
2. Extraction job runs → produces `ai_description` or `extracted_text`
3. ExtractionHandler updates note content → re-queues downstream jobs
4. Downstream jobs run again on enriched content

### 4. Gate adapter registration on health check

Only register adapters whose backends are actually available:

```rust
if renderer_available {
    extraction_registry.register(Arc::new(adapter));
} else {
    warn!("Glb3DModel adapter NOT registered — renderer unavailable");
}
```

This applies to any adapter with external dependencies. When an unavailable adapter's strategy is requested (e.g., a `model/*` file is uploaded without the renderer running), the ExtractionHandler returns "No adapter registered for strategy" — a clear, actionable error.

### 5. Remove dead `ThreeDAnalysisHandler` and `JobType::ThreeDAnalysis`

The legacy placeholder handler and its associated job type enum variant are dead code from the pre-ADR-048 era. Remove:
- `ThreeDAnalysisHandler` struct and `JobHandler` impl
- `JobType::ThreeDAnalysis` enum variant
- `"3d_analysis"` string mapping in `PgJobRepository`

> Note: No database migration is needed for the enum removal — `3d_analysis` was never added to the PostgreSQL `job_type` enum via migration.

## Consequences

### Positive

- (+) **3D models become searchable**: AI-generated descriptions feed into FTS and semantic search via note content
- (+) **Images get descriptions persisted**: The `VisionAdapter`'s `ai_description` is no longer lost
- (+) **Downstream jobs see enriched content**: Embeddings, concepts, and links are based on actual extracted content rather than empty notes
- (+) **Clear failure modes**: Unavailable adapters produce explicit "not registered" errors rather than opaque connection failures
- (+) **Reduced dead code**: Removing `ThreeDAnalysisHandler` eliminates confusion about the legacy vs current approach
- (+) **Schema alignment**: `ai_description` column aligns with ADR-034's original schema intent

### Negative

- (-) **Double processing**: Downstream jobs run twice — once on original content (which may be wasted), once on enriched content
- (-) **Content mutation**: Extraction modifying note content could surprise users who set specific content before the attachment was processed
- (-) **Migration required**: New column on `file_attachment` requires a migration in existing deployments

### Mitigations

- **Double processing**: The first run is fast on empty content (embeddings are tiny, no concepts extracted). The cost is negligible compared to the extraction itself.
- **Content mutation**: Only mutate content when the note's text is empty/minimal (< 50 chars). Notes with substantive user-written content get an appended section instead of replacement.
- **Migration**: `ALTER TABLE ADD COLUMN` with no default is instant in PostgreSQL (no table rewrite).

## Implementation

**Migration:** `migrations/20260220100000_add_ai_description_to_attachment.sql`

**Code Changes:**

| File | Change |
|------|--------|
| `crates/matric-db/src/file_storage.rs` | Add `update_ai_description_tx()`, add column to queries |
| `crates/matric-jobs/src/extraction_handler.rs` | Persist `ai_description`, update note content, re-queue downstream jobs |
| `crates/matric-api/src/main.rs` | Gate adapter registration on health check result |
| `crates/matric-api/src/handlers/jobs.rs` | Remove `ThreeDAnalysisHandler` |
| `crates/matric-core/src/models.rs` | Remove `JobType::ThreeDAnalysis` |
| `crates/matric-db/src/jobs.rs` | Remove `3d_analysis` string mapping |

**Testing:**

- Unit test: `ExtractionHandler` persists `ai_description` when present
- Unit test: `ExtractionHandler` updates note content when extraction produces description
- Unit test: Adapter registration skipped when health check fails
- Integration test: Upload 3D file → extraction → note content updated → re-embedding succeeds

## References

- ADR-034: 3D File Analysis Support (partially superseded)
- ADR-048: Extraction Adapter Pattern
- ADR-079: Global Job Deduplication
- Issue #492: 3D model extraction pipeline broken
- `crates/matric-jobs/src/extraction_handler.rs` — ExtractionHandler
- `crates/matric-jobs/src/adapters/glb_3d_model.rs` — Glb3DModelAdapter
- `crates/matric-jobs/src/adapters/vision.rs` — VisionAdapter (also affected)

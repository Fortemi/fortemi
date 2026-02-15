# ADR-049: Separate File Type from Document Type (Two-Phase Detection)

**Status:** Accepted
**Date:** 2026-02-07
**Deciders:** Architecture team
**Supersedes:** ADR-048 Section 3 (upload handler auto-detection wiring)
**Related:** ADR-025 (Document Type Registry), ADR-031 (Intelligent Attachment Processing), ADR-033 (File Storage Architecture), ADR-048 (Extraction Adapter Pattern)

## Context

ADR-048 wired auto-detection into `upload_attachment` by calling `DocumentTypeRepository::detect()` at upload time, which finds a document type from MIME/filename and assigns both `document_type_id` AND `extraction_strategy` from that single match. This conflates two orthogonal concerns:

1. **File type** (container format): PDF, JPEG, MP4, DOCX -- determines HOW to extract content. Deterministic from MIME/extension.
2. **Document type** (semantic purpose): contract, meeting notes, research paper -- determines HOW AI processes/templates content. Requires reading the actual content.

A PDF could be a contract, thesis, invoice, or meeting agenda. The MIME type tells us nothing about semantic purpose. The current code picks whichever document type first matches `application/pdf` with `LIMIT 1` -- nondeterministic and wrong.

### Problem Example

```
Upload: report.pdf (application/pdf)

ADR-048 approach: detect() -> "pdf" document type -> extraction_strategy = pdf_text
  - Correct extraction strategy (PDF needs pdftotext)
  - Wrong document type (might be a legal contract, not generic "pdf")
  - Nondeterministic: LIMIT 1 picks arbitrarily among matching types

ADR-049 approach:
  Phase 1 (sync):  ExtractionStrategy::from_mime_type("application/pdf") -> PdfText
  Phase 2 (async): AI reads extracted text -> "legal_contract" document type (0.87 confidence)
```

## Decision

### Two-Phase Detection Model

**Phase 1 (upload time, synchronous, pure function):**

Determine `extraction_strategy` from MIME type via `ExtractionStrategy::from_mime_type()`. No database lookup needed. Set on attachment immediately. The mapping is a pure function:

| MIME Pattern | Extraction Strategy |
|---|---|
| `application/pdf` | PdfText |
| `image/*` | Vision |
| `audio/*` | AudioTranscribe |
| `video/*` | VideoMultimodal |
| `*officedocument*`, `*msword*` | OfficeConvert |
| `application/json`, `text/xml`, etc. | StructuredExtract |
| `text/*` | TextNative |
| default | TextNative |

When the MIME type is ambiguous (`application/octet-stream`), the file extension provides refinement via `ExtractionStrategy::from_mime_and_extension()`.

**Phase 2 (post-extraction, async, AI-assisted):**

After content extraction completes, a `DocumentTypeInference` job classifies the extracted text into a semantic document type. The result is stored as:
- `detected_document_type_id` -- the AI's classification
- `detection_confidence` -- float 0.0-1.0
- `detection_method` -- "content" (AI-based)

If confidence >= threshold (configurable, default 0.8), auto-promote to `document_type_id`.

### What Changed from ADR-048

1. **Removed**: The `detect()` call in `upload_attachment` that assigned both `document_type_id` and `extraction_strategy` from a single DB lookup
2. **Added**: `ExtractionStrategy::from_mime_type()` and `from_mime_and_extension()` pure functions
3. **Added**: `set_extraction_strategy()` method on file storage (sets only strategy, not document type)
4. **Added**: `set_detected_document_type()` method for AI classification results
5. **Added**: `DocumentTypeInference` job type for async classification
6. **Added**: Optional `document_type_id` field on upload request for user overrides
7. **Changed**: `store_file()` always uses filesystem storage (no inline threshold)
8. **Kept**: `detect()` endpoint for explicit user queries ("what type is this file?")
9. **Kept**: MIME backfill migration (reference data for heuristic detection)

### Attachment Detection Fields

The `attachment` table already has these columns (from `20260203000000_attachment_doctype_integration.sql`):

```sql
document_type_id UUID              -- User-confirmed or auto-promoted type
detected_document_type_id UUID     -- AI-detected type (may differ from confirmed)
detection_confidence FLOAT         -- 0.0-1.0
detection_method TEXT              -- 'mime', 'extension', 'magic', 'content'
```

## Consequences

### Positive

- (+) **Correct extraction strategy immediately**: MIME-to-strategy is a pure function, always deterministic, no DB needed
- (+) **Document type classification uses actual content**: AI reads the extracted text, not just the filename
- (+) **User can override**: Optional `document_type_id` on upload for cases where the user knows the type
- (+) **Decoupled concerns**: Extraction strategy and document type evolve independently
- (+) **No nondeterminism**: `from_mime_type()` is a pure function, always returns the same result

### Negative

- (-) **Two-step process**: Document type is not available immediately at upload time (only after extraction + inference)
- (-) **Requires Ollama for Phase 2**: AI classification needs the inference engine running

### Mitigations

- **Immediate feedback**: The extraction strategy IS available immediately, which is what the processing pipeline needs
- **Graceful degradation**: If Ollama is unavailable, `detected_document_type_id` stays NULL; the attachment still processes correctly via extraction strategy
- **User override**: Users who know the document type can set it explicitly at upload

## Implementation

**Code Locations:**
- `ExtractionStrategy::from_mime_type()` / `from_mime_and_extension()`: `crates/matric-core/src/models.rs`
- Upload handler: `crates/matric-api/src/main.rs` (`upload_attachment`)
- `set_extraction_strategy()`: `crates/matric-db/src/file_storage.rs`
- `set_detected_document_type()`: `crates/matric-db/src/file_storage.rs`
- `DocumentTypeInference` job type: `crates/matric-core/src/models.rs`, `crates/matric-db/src/jobs.rs`
- Job type migration: `migrations/20260207000000_add_document_type_inference_job.sql`
- MCP tool update: `mcp-server/index.js` (`upload_attachment` tool)

## References

- ADR-025: Document Type Registry
- ADR-031: Intelligent Attachment Processing
- ADR-033: File Storage Architecture
- ADR-048: Extraction Adapter Pattern (partially superseded)

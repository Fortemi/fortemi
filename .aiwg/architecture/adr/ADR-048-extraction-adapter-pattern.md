# ADR-048: Extraction Adapter Pattern with MIME-Based Document Type Detection

**Status:** Accepted (Section 3 superseded by ADR-049)
**Date:** 2026-02-06
**Deciders:** Architecture team
**Related:** ADR-025 (Document Type Registry), ADR-031 (Intelligent Attachment Processing), ADR-033 (File Storage Architecture)
**Superseded by:** ADR-049 (Section 3: upload handler auto-detection wiring replaced with two-phase detection)

## Context

Matric Memory has a Document Type Registry (ADR-025) with 161+ pre-configured document types spanning 19 categories and 9 extraction strategies (`text_native`, `pdf_text`, `pdf_ocr`, `vision`, `audio_transcribe`, `video_multimodal`, `code_ast`, `office_convert`, `structured_extract`). The file attachment system (ADR-031, ADR-033) provides content-addressable blob storage with BLAKE3 deduplication and a pluggable storage backend.

However, three critical gaps prevent these systems from working together effectively:

### 1. Missing MIME Types on Document Types

The `document_type` table schema includes a `mime_types TEXT[]` column, but the majority of the 161+ seeded document types have empty MIME type arrays. Only the specialized media types added in migration `20260204300001` (3D models, SVG, MIDI, diagrams) were seeded with MIME types. Core types like `markdown`, `pdf`, `rust`, `python`, and `json` lack MIME mappings entirely.

### 2. Incomplete Detection Pipeline

The `DocumentTypeRepository::detect()` trait signature accepts `mime_type: Option<&str>` as a third parameter (defined in `crates/matric-core/src/traits.rs:525-531`), but the PostgreSQL implementation in `crates/matric-db/src/document_types.rs:294-383` only accepts `filename` and `content` -- it has no MIME-based detection step at all. The current detection priority is:

1. `filename_pattern` match (1.0 confidence)
2. `file_extension` match (0.9 confidence)
3. `content_pattern` / magic match (0.7 confidence)
4. Default to `plaintext` (0.1 confidence)

For binary file uploads where the extension may be absent or ambiguous (e.g., a `.bin` file that is actually a PDF, or an upload via API where only the MIME Content-Type header is reliable), the system cannot determine the document type.

### 3. Upload Handler Does Not Auto-Detect

The `upload_attachment` handler in `crates/matric-api/src/main.rs:7207-7229` calls `store_file()` which creates the `attachment` record with `document_type_id = NULL` and `extraction_strategy = NULL`. Despite having the filename and MIME `content_type` available from the upload request body, no detection is performed and no `document_type_id` is assigned. This means:

- Uploaded files never get a `document_type_id` on the attachment record
- The extraction pipeline cannot route the file to the correct adapter (no `extraction_strategy`)
- The attachment stays in `uploaded` status indefinitely with no processing path

### 4. No Adapter Abstraction for Extraction

Each `extraction_strategy` value implies a specific external tool or model (pdftotext, Whisper, LLaVA, pandoc, tree-sitter), but there is no adapter abstraction. When the processing pipeline is implemented, it needs a clean pattern for dispatching to the correct extractor based on the strategy.

## Decision

### 1. Backfill MIME Types via Seed Migration

Create a migration that populates `mime_types` for all 161+ document types. Examples:

| Document Type | MIME Types |
|---------------|------------|
| `markdown` | `text/markdown`, `text/x-markdown` |
| `plaintext` | `text/plain` |
| `pdf` | `application/pdf` |
| `rust` | `text/x-rust` |
| `python` | `text/x-python`, `application/x-python-code` |
| `json` | `application/json` |
| `yaml` | `application/x-yaml`, `text/yaml` |
| `html` | `text/html` |
| `typescript` | `text/typescript`, `application/typescript` |
| `image-*` | `image/png`, `image/jpeg`, `image/webp`, etc. |
| `audio` | `audio/mpeg`, `audio/ogg`, `audio/wav`, etc. |
| `video` | `video/mp4`, `video/webm`, `video/quicktime`, etc. |

Use `ON CONFLICT (name) DO UPDATE SET mime_types = EXCLUDED.mime_types` to safely merge with existing data, preserving MIME types already set by earlier migrations (e.g., `20260204300001`).

### 2. Add MIME-Based Detection to detect() Pipeline

Insert a MIME-type matching step between `filename_pattern` (1.0) and `file_extension` (0.9) in the `PgDocumentTypeRepository::detect()` implementation. The updated detection priority becomes:

1. **filename_pattern** match -- confidence 1.0
2. **MIME type** match (NEW) -- confidence 0.95
3. **file_extension** match -- confidence 0.9
4. **content_pattern** / magic match -- confidence 0.7
5. Default to `plaintext` -- confidence 0.1

The MIME detection step queries `document_type` where `$1 = ANY(mime_types)` and `is_active = TRUE`. MIME matching is placed above extension matching because MIME types are more authoritative for binary files and API uploads where the Content-Type header is explicitly set by the client or inferred by the HTTP framework.

The implementation must also accept the `mime_type` parameter to satisfy the existing trait contract:

```rust
async fn detect(
    &self,
    filename: Option<&str>,
    content: Option<&str>,
    mime_type: Option<&str>,  // Currently ignored in implementation
) -> Result<Option<DetectDocumentTypeResult>>
```

### 3. Wire Auto-Detection into upload_attachment

> **Note:** This section is superseded by ADR-049 (Two-Phase Detection). The upload handler now uses `ExtractionStrategy::from_mime_and_extension()` for synchronous extraction strategy assignment, with document type classification deferred to an async `DocumentTypeInference` job.

After `store_file()` completes in the upload handler, call `detect()` with the filename and content_type from the request. If detection succeeds:

1. Set `document_type_id` on the attachment record
2. Copy the document type's `extraction_strategy` to the attachment record
3. Copy the document type's `extraction_config` to the attachment record
4. Update attachment `status` from `uploaded` to `queued` (signaling readiness for processing)

If detection fails (returns `None` or confidence below a configurable threshold), the attachment remains with `document_type_id = NULL` and `status = uploaded`, requiring manual classification or a future background re-detection job.

### 4. Extraction Adapter Pattern for Future Processing

Define a trait-based adapter pattern where each `ExtractionStrategy` maps to a concrete adapter implementation:

```
ExtractionStrategy         Adapter                  External Tool
--------------------       ---------------------    ----------------
text_native                TextNativeAdapter        (none - direct read)
pdf_text                   PdfTextAdapter           pdftotext
pdf_ocr                    PdfOcrAdapter            tesseract
vision                     VisionAdapter            Ollama (LLaVA)
audio_transcribe           AudioTranscribeAdapter   Whisper
video_multimodal           VideoMultimodalAdapter   FFmpeg + Whisper + LLaVA
code_ast                   CodeAstAdapter           tree-sitter
office_convert             OfficeConvertAdapter     pandoc
structured_extract         StructuredExtractAdapter (serde parsers)
```

Each adapter implements:

```rust
#[async_trait]
pub trait ExtractionAdapter: Send + Sync {
    /// The strategy this adapter handles.
    fn strategy(&self) -> ExtractionStrategy;

    /// Extract text content and metadata from file data.
    async fn extract(
        &self,
        data: &[u8],
        config: &serde_json::Value,
    ) -> Result<ExtractionResult>;

    /// Check if the required external tool is available.
    async fn health_check(&self) -> Result<bool>;
}

pub struct ExtractionResult {
    pub extracted_text: Option<String>,
    pub metadata: serde_json::Value,
    pub ai_description: Option<String>,
    pub preview_data: Option<Vec<u8>>,
}
```

A dispatcher selects the adapter based on the attachment's `extraction_strategy`:

```rust
pub struct ExtractionDispatcher {
    adapters: HashMap<ExtractionStrategy, Box<dyn ExtractionAdapter>>,
}

impl ExtractionDispatcher {
    pub async fn extract(&self, attachment: &Attachment, data: &[u8]) -> Result<ExtractionResult> {
        let strategy = attachment.extraction_strategy
            .unwrap_or(ExtractionStrategy::TextNative);
        let adapter = self.adapters.get(&strategy)
            .ok_or_else(|| Error::UnsupportedStrategy(strategy))?;
        adapter.extract(data, &attachment.extraction_config()).await
    }
}
```

This pattern is not implemented immediately but establishes the contract that the processing pipeline (matric-jobs) will use when extraction adapters are built.

## Consequences

### Positive

- (+) **All uploaded files get a document_type_id**: MIME detection closes the gap for binary files and API uploads where extension matching is insufficient
- (+) **Extraction pipeline can route by strategy**: With `extraction_strategy` set on the attachment at upload time, the job worker knows exactly which adapter to invoke
- (+) **Detection is more robust**: Five-stage detection (filename, MIME, extension, content, default) covers the full spectrum from exact matches to graceful fallback
- (+) **Adapter pattern decouples detection from extraction**: Adding a new extraction tool (e.g., a new OCR engine) requires only implementing the adapter trait, not changing detection or routing logic
- (+) **Health checks enable graceful degradation**: If an external tool (Whisper, LLaVA) is unavailable, the adapter's `health_check()` can report this, allowing the dispatcher to skip or defer processing rather than failing hard
- (+) **Backward compatible**: Existing attachments with `document_type_id = NULL` remain valid; a background job can retroactively detect and classify them

### Negative

- (-) **MIME type data maintenance**: MIME types must be kept up-to-date as new file formats emerge; incorrect MIME mappings can cause misclassification
- (-) **MIME ambiguity**: Some MIME types map to multiple document types (e.g., `application/octet-stream` is generic and matches nothing useful; `text/plain` could be plaintext, CSV, or log files)
- (-) **Adapter complexity**: Each adapter wraps an external tool with different invocation patterns, error modes, and resource requirements
- (-) **External tool dependencies**: The extraction pipeline requires pdftotext, tesseract, pandoc, FFmpeg, Whisper, and LLaVA to be available at runtime for the full set of strategies

### Mitigations

- **MIME ambiguity**: When multiple document types match the same MIME type, prefer the one whose `file_extensions` also match (if a filename is available), or fall through to the next detection stage
- **External tool availability**: Adapters report availability via `health_check()`; the processing pipeline skips unavailable strategies and sets attachment status to `failed` with an actionable error message
- **Maintenance burden**: MIME types are seeded via migrations and can be updated by users through the Document Type API for custom types

## Alternatives Considered

### A. MIME Detection as Highest Priority (Above Filename Patterns)

**Rejected.** Filename patterns like `Dockerfile`, `Makefile`, and `Justfile` are exact matches with no ambiguity. MIME types for these files are typically `application/octet-stream` or `text/plain`, which would produce false or unhelpful matches. Keeping filename patterns at the top preserves the most specific detection method.

### B. Lazy Detection (Detect Only When Processing Job Runs)

**Rejected.** Deferring detection to the processing job means the upload response cannot tell the client what document type was detected or what processing will occur. Immediate detection at upload time enables the `202 Accepted` response to include `document_type` and `estimated_completion_seconds`, which is essential for the MCP server to provide useful feedback to AI agents.

### C. Content Sniffing Instead of MIME Trust

**Considered but deferred.** Magic byte / content sniffing (e.g., using the `infer` crate) is more reliable than trusting the client-provided MIME type. However, it requires reading the file content at detection time, which is already handled by stage 4 (content_pattern). A future enhancement could add an `infer`-based detection step at confidence 0.85, between MIME (0.95) and extension (0.9). This is not included in the current decision to keep scope manageable.

### D. Single Monolithic Extraction Function

**Rejected.** A single `extract(strategy, data, config)` function with a large match statement would work initially but becomes difficult to test, extend, and deploy independently. The adapter pattern allows each extraction tool to be:
- Tested in isolation with mock data
- Feature-flagged independently (e.g., disable vision extraction if no GPU)
- Deployed with different resource requirements (audio transcription is CPU-heavy, vision is GPU-heavy)

## Implementation

**Code Locations:**
- MIME backfill migration: `migrations/YYYYMMDD_backfill_mime_types.sql`
- Detection update: `crates/matric-db/src/document_types.rs` (add MIME step to `detect()`, accept `mime_type` param)
- Upload handler update: `crates/matric-api/src/main.rs` (`upload_attachment` function, ~line 7207)
- Adapter trait: `crates/matric-core/src/traits.rs` (new `ExtractionAdapter` trait)
- Adapter dispatcher: `crates/matric-jobs/src/extraction/` (future implementation)

**Key Changes:**
1. Add seed migration backfilling `mime_types` for all 161+ document types
2. Update `PgDocumentTypeRepository::detect()` to accept `mime_type` parameter and add MIME matching step at 0.95 confidence
3. Add `get_by_mime_type()` method to `PgDocumentTypeRepository`
4. Update `upload_attachment` handler to call `detect()` after `store_file()` and set `document_type_id` + `extraction_strategy` on the attachment
5. Define `ExtractionAdapter` trait and `ExtractionDispatcher` in matric-core (implementation of concrete adapters is deferred)

**Migration Dependencies:**
- Depends on: `20260202000000_document_types.sql` (base schema), `20260203000000_attachment_doctype_integration.sql` (attachment table)
- Depended on by: Future extraction pipeline jobs

## References

- ADR-025: Document Type Registry (`/.aiwg/architecture/ADR-025-document-type-registry.md`)
- ADR-031: Intelligent Attachment Processing (`/.aiwg/architecture/ADR-031-intelligent-attachment-processing.md`)
- ADR-033: File Storage Architecture (`/.aiwg/architecture/ADR-033-file-storage-architecture.md`)
- ADR-036: File Safety Validation (`/.aiwg/architecture/ADR-036-file-safety-validation.md`)
- Document Type Schema: `migrations/20260202000000_document_types.sql`
- Attachment Integration Schema: `migrations/20260203000000_attachment_doctype_integration.sql`
- Detection Implementation: `crates/matric-db/src/document_types.rs`
- Upload Handler: `crates/matric-api/src/main.rs` (line ~7207)
- File Storage Repository: `crates/matric-db/src/file_storage.rs`
- Architecture Design: `docs/architecture/attachment-doctype-integration.md`

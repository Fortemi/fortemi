# ADR-085: Null Byte Sanitization in Extraction Pipeline

**Status:** Accepted
**Date:** 2026-02-20
**Deciders:** Engineering team

## Context

Old PDFs (particularly those created with Acrobat 3.0/4.0 scanners) embed null bytes (`\u0000`) in metadata fields like `Creator`, `Producer`, and sometimes within the extracted text content itself.

PostgreSQL rejects null bytes in `text` and `jsonb` columns with error code `22P05`: "unsupported Unicode escape sequence: \u0000 cannot be converted to text". This caused 3 PDF extraction jobs to fail silently — the extraction succeeded but the database persist failed, leaving jobs stuck in `running` status indefinitely.

### Impact

- Any PDF with null bytes in metadata would fail extraction permanently
- The error occurs at the database layer, not the extraction layer, making diagnosis difficult
- `pdfinfo` output faithfully preserves null bytes from the original PDF metadata

## Decision

Strip all null bytes (`\0`) from both extraction outputs before database storage:

1. **Metadata values** — in `parse_pdfinfo()`, each value has `.replace('\0', "")` applied
2. **Extracted text** — after `pdftotext` output is collected, apply `.replace('\0', "")`

This is applied at the adapter level (in `PdfTextAdapter`) rather than at the database layer, because:
- The adapter knows the data source is untrusted (external PDF files)
- Other adapters may need different sanitization strategies
- Null bytes carry no meaningful information in text/metadata contexts

## Consequences

### Positive
- (+) PDFs with null bytes in metadata no longer cause permanent extraction failures
- (+) Fix is localized to the PdfTextAdapter — other adapters are unaffected
- (+) Null bytes carry no semantic meaning in PDF text or metadata

### Negative
- (-) Silent sanitization — if null bytes ever carried meaning, it would be lost
- (-) Other adapters processing untrusted data may need similar treatment

## Implementation

**Code Location:** `crates/matric-jobs/src/adapters/pdf_text.rs`

**Key Changes:**
- `parse_pdfinfo()`: Added `.replace('\0', "")` to each metadata value before insertion
- `extract()`: Added `text.replace('\0', "")` after text extraction, before metadata insertion

## References

- PostgreSQL error code `22P05`: "unsupported Unicode escape sequence"
- [ADR-048: Extraction Adapter Pattern](ADR-048-extraction-adapter-pattern.md)

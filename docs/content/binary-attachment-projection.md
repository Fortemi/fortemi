# Binary Attachment Projection Contract

**Status:** Canonical
**Applies to:** search index records, JSON backup export, knowledge shards, embedding-set source text, `fortemi-react`, and AIWG `aiwg-fortemi-index-export.json`

## Contract

Binary attachments are attached to notes as data sources. Search, index, export, and embedding-set projections must never inline raw binary bytes.

For each binary attachment associated with a note, the projection carries extracted text plus an attachment reference:

```json
{
  "extracted_text": "Text produced by the extraction job, or null when extraction is pending.",
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000001",
    "path": "research-paper.pdf",
    "mime": "application/pdf",
    "checksum": "blake3-content-hash",
    "bytes": 911442
  }
}
```

The `bytes` field is a byte count. It is not a byte array, base64 string, blob handle payload, or serialized file body.

## Flow

1. A client creates or selects a note.
2. The binary is uploaded through the attachment pipeline.
3. Fortemi stores the binary in attachment storage and records metadata on the attachment row.
4. Extraction jobs consume the attachment as their data source.
5. The extraction pipeline writes `extracted_text` and extraction metadata back to the attachment.
6. Index, export, shard, and embedding-set builders read `extracted_text` and attachment metadata. They do not read or serialize blob bytes.

When extraction has not completed, the projection still exports a valid bounded record: `extracted_text` is `null` or the best available partial text, and `attachment` contains the id, path, MIME type, checksum, and byte count.

## Required Consumer Behavior

Consumers must treat the note body plus attachment `extracted_text` values as searchable and embeddable text. Attachment metadata is provenance and fetch metadata, not search text except where explicitly useful for filename or MIME filtering.

Consumers must reject or drop fields that attempt to carry raw binary in projection records, including byte arrays, base64 blob strings, or generic `data` / `raw` / `content_bytes` fields under an attachment projection.

Large or not-yet-extracted binaries must not make export or indexing fail. A record with attachment metadata and no extracted text is valid and bounded.

## AIWG Index Export Parity

`aiwg-fortemi-index-export.json` records should mirror this contract:

- Put note text and attachment `extracted_text` into the record text/chunk source fields.
- Put the attachment reference under record metadata or per-chunk provenance.
- Leave `skos_concepts[]`, `chunks[]`, and `embeddings[]` available for derived concepts, chunk records, and vector data.
- Do not put PDF, Office, audio, video, archive, or code bytes into any JSON string field.

If AIWG has only a local file path and extraction has not run yet, it should emit the attachment reference with `extracted_text: null`, then refresh the record after extraction.

## Browser Edition Parity

`fortemi-react` should use the same shape for browser-side notes and local index adapters. The browser may hold a `File` or `Blob` for upload and preview, but that object is not part of the search/index/export projection. Browser search indexes use extracted text, OCR/transcript/caption text, or a bounded placeholder until extraction finishes.

## Server Edition

The server edition already follows the data-source pattern for summary and extraction jobs: jobs read the stored attachment, extract text, and write derived text/metadata back to attachment records. The JSON export and knowledge-shard note records expose attachment projections with metadata and extracted text only.

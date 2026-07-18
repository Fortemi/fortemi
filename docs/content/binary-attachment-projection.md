# Binary Attachment Projection Contract

**Status:** Canonical format contract; implementation coverage varies by surface
**Applies to:** search index records, JSON backup export, knowledge shards, embedding-set source text, `fortemi-react`, and AIWG `aiwg-fortemi-index-export.json`

## Contract

Binary attachments are attached to notes as data sources. Search, index, export, and embedding-set projections must never inline raw binary bytes.

For each binary attachment associated with a note, the projection carries extracted text plus an attachment reference:

```json
{
  "extracted_text": "Text produced by the extraction job, or null when extraction is pending.",
  "extraction_status": "extracted",
  "reason": null,
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000001",
    "path": "research-paper.pdf",
    "mime": "application/pdf",
    "checksum": "blake3:6f1ed002ab5595859014ebf0951522d9f74523c1f8a3a6954e52483a880c24a9",
    "bytes": 911442
  }
}
```

The `bytes` field is a byte count. It is not a byte array, base64 string, blob handle payload, or serialized file body.

The `checksum` field is the whole-content BLAKE3 digest encoded as
`blake3:<hex>`, where `<hex>` is exactly 64 lowercase hexadecimal characters.
The `path` field is the attachment's display filename, such as
`research-paper.pdf`. It is never a filesystem path, managed blob path, OPFS
key, S3 object key, or other physical storage locator.

When extracted text is unavailable, `extraction_status` and `reason` use stable classes only:

| State | `extraction_status` | `reason` |
|---|---|---|
| Extracted text exists | `extracted` | `null` |
| Uploaded, queued, or processing | `pending` | `extraction_pending` |
| Large binary with no extracted text | `deferred` | `large_binary` |
| Unsupported generic MIME | `deferred` | `unsupported_mime` |
| Completed but no text was produced | `deferred` | `no_extracted_text` |
| Extraction failed | `failed` | `extractor_failed` |
| Quarantined | `blocked` | `quarantined` |

Projection payloads do not expose raw extractor diagnostics, backend errors, temporary filesystem paths, stack traces, storage paths, or connection strings.

## Portable Shard Byte Sidecar

This section defines the interoperable format for implementations that carry
attachment bytes in a shard. A portable Knowledge Shard may include attachment bytes in an optional
content-addressed sidecar within its `tar.gz` archive. Each distinct blob is a
tar entry named:

```text
blobs/<hex>
```

`<hex>` is the bare 64-character lowercase hexadecimal BLAKE3 digest of the
entry bytes. The corresponding projection record uses
`checksum: "blake3:<hex>"`; resolving the record to the sidecar strips the
`blake3:` prefix and looks for the exact `blobs/<hex>` entry. An exporter must
write at most one entry for each distinct digest, so attachments with identical
bytes share one sidecar entry.

The sidecar does not change the JSON projection shape. Projection records must
never contain byte arrays, base64 strings, or generic `data`, `raw`, or
`content_bytes` fields, whether or not a sidecar is present.

Sidecars are optional. A missing matching entry makes the attachment
reference-only and is valid; it does not make the shard malformed. Readers
must ignore unreferenced or otherwise unknown entries under `blobs/` without
failing shard import. Exporters and readers may stream large entries, but the
entry name is always derived from the BLAKE3 digest of the complete content.

## Flow

1. A client creates or selects a note.
2. The binary is uploaded through the attachment pipeline.
3. Fortemi stores the binary in attachment storage and records metadata on the attachment row.
4. Extraction jobs consume the attachment as their data source.
5. The extraction pipeline writes `extracted_text` and extraction metadata back to the attachment.
6. JSON projection builders read `extracted_text` and attachment metadata. They do not read or serialize blob bytes.
7. A sidecar-capable archive writer may separately stream stored attachment bytes into `blobs/<hex>` entries without changing the JSON records.

When extraction has not completed, the projection still exports a valid bounded record: `extracted_text` is `null` or the best available partial text, `extraction_status` and `reason` carry a stable class, and `attachment` contains the id, path, MIME type, checksum, and byte count.

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

The server edition follows the data-source pattern for summary and extraction
jobs: jobs read the stored attachment, extract text, and write derived
text/metadata back to attachment records. JSON records remain byte-free.

REST shard export is reference-only by default. Call
`GET /api/v1/backup/knowledge-shard?include_blobs=true` to add every available,
verified database- or filesystem-backed attachment as one digest-deduplicated
`blobs/<hex>` entry. Export fails closed if stored bytes do not match their
declared digest or length, or if the bounded archive budget cannot carry them.

Shard import accepts both forms. Missing referenced entries restore
reference-only attachment metadata. Present referenced entries must match the
declared BLAKE3 digest and byte count before any database write. Verified bytes
are staged outside the final blob namespace, associated with attachment rows in
the schema transaction, and promoted only after all selected components apply.
Repeated imports reuse an existing materialized digest. Late component or
commit failures roll back database state and discard or compensate newly
promoted bytes.

The current HTTP implementation buffers the bounded compressed archive and its
entries in memory. It uses streaming verification within the filesystem
storage primitive, but it is not a streaming archive reader/writer and does not
claim `full-v1` conformance, signature support, or complete disaster recovery.

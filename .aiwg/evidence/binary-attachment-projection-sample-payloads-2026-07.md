# Binary Attachment Projection Sample Payloads - 2026-07

## Purpose

Evidence companion for `Fortemi/fortemi#1013`. These samples mirror the Fortemi-local `BinaryAttachmentExportProjection` contract tests in `crates/matric-api/src/main.rs` and show the payload shape downstream React/browser and AIWG index/export consumers should expect. The Fortemi-side CI receipt is PR #1023 / Actions runs 4094 and 4096 on commit `f12a2df9`, merged to `main` as `79600fc2`.

## Extracted Text Case

```json
{
  "extracted_text": "extracted pdf text",
  "extraction_status": "extracted",
  "reason": null,
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000001",
    "path": "research/paper.pdf",
    "mime": "application/pdf",
    "checksum": "blake3-checksum",
    "bytes": 911442
  }
}
```

## No-Text Binary Cases

Large binary awaiting extraction or intentionally deferred:

```json
{
  "extracted_text": null,
  "extraction_status": "pending",
  "reason": "extraction_pending",
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000002",
    "path": "large-video.mp4",
    "mime": "video/mp4",
    "checksum": "blake3-video-checksum",
    "bytes": 910163968
  }
}
```

Unsupported binary MIME:

```json
{
  "extracted_text": null,
  "extraction_status": "deferred",
  "reason": "unsupported_mime",
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000004",
    "path": "archive.bin",
    "mime": "application/octet-stream",
    "checksum": "blake3-unsupported-checksum",
    "bytes": 512
  }
}
```

Extractor failure with stable reason class only:

```json
{
  "extracted_text": null,
  "extraction_status": "failed",
  "reason": "extractor_failed",
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000003",
    "path": "failed.pdf",
    "mime": "application/pdf",
    "checksum": "blake3-failed-checksum",
    "bytes": 4096
  }
}
```

Quarantined binary:

```json
{
  "extracted_text": null,
  "extraction_status": "blocked",
  "reason": "quarantined",
  "attachment": {
    "id": "018fd1a0-0000-7000-8000-000000000005",
    "path": "quarantined.pdf",
    "mime": "application/pdf",
    "checksum": "blake3-quarantined-checksum",
    "bytes": 4096
  }
}
```

## Negative Payload Invariants

Projection payloads must not contain raw binary bytes, base64 payloads, raw buffers, `attachment_blob`, `storage_path`, raw temporary paths, backend error strings, connection strings, SQL diagnostics, or stack traces. Search, index, export, embedding, and API projection consumers must use `extracted_text` only when present and treat no-text binary records as bounded metadata records with stable `extraction_status` and `reason` values.

## Closure Boundary

- `Fortemi/fortemi#1013` has Fortemi-side proof via PR #1023 / Actions runs 4094 and 4096 on commit `f12a2df9`, merged to `main` as `79600fc2`.
- `Fortemi/fortemi-react#227` remains open for browser parity verification under the 2026-07-07 takeover-owned React checkpoint boundary.
- `roctinam/aiwg#1719` remains the AIWG index/export crash companion.
- npm `@fortemi/react@2026.7.3` availability is already complete and is not proof of binary attachment projection readiness.

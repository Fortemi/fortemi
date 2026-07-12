# Binary Attachment Projection Contract - 2026-07

## Status

Accepted Fortemi-side contract for `Fortemi/fortemi#1013`; required before `Fortemi/fortemi-react#227` can treat browser binary-handling parity as release-ready.

## Context

The July 2026 checkpoint identified binary attachments as a release blocker for downstream React/browser consumers and AIWG index/export consumers. Fortemi must own the canonical projection rule because attachment ingestion, extracted text, metadata, checksums, search indexing, export generation, and embedding preparation originate in the Fortemi core.

Current checkpoint tracking requires:

- `Fortemi/fortemi#1013` as the Fortemi-side source of truth.
- `Fortemi/fortemi-react#227` as the React/browser parity consumer.
- `roctinam/aiwg#1719` as the AIWG index/export crash companion.
- `Fortemi/fortemi-react#252` to remain separate and limited to local release metadata/docs reconciliation for the already-published npm `2026.7.3` cut.

## Decision

Fortemi projections must model binary attachments as note data sources with extracted text plus bounded attachment metadata. Search, index, export, embedding, and API projection surfaces must never inline raw binary bytes.

The canonical metadata envelope is:

```json
{
  "id": "attachment-id",
  "path": "display-filename.ext",
  "mime": "application/octet-stream",
  "checksum": "blake3:<64-char-lowercase-hex>",
  "bytes": 12345
}
```

Projection records may include extracted text when available. They must not include raw file contents, base64 payloads, raw binary buffers, unbounded parser diagnostics, temporary filesystem paths, or backend error strings.

`checksum` is the whole-content BLAKE3 digest encoded as `blake3:` followed by
exactly 64 lowercase hexadecimal characters. `path` is the user-facing display
filename only and must not expose a physical filesystem path, managed blob
path, OPFS key, or object-store key.

Portable Knowledge Shards may carry an optional `blobs/<hex>` tar sidecar,
where `<hex>` is the bare digest from the corresponding record checksum. The
sidecar contains at most one entry per distinct digest. JSON records remain
byte-free. Missing matching entries are valid reference-only attachments, and
readers ignore unknown or unreferenced `blobs/` entries. Large entries may be
streamed, with the key derived from the digest of the complete content.

Each projection record also carries stable extraction classification fields:

```json
{
  "extraction_status": "pending|extracted|deferred|failed|blocked",
  "reason": "extraction_pending|large_binary|unsupported_mime|no_extracted_text|extractor_failed|quarantined|null"
}
```

`reason` is `null` only when `extraction_status` is `extracted`. Failure records expose the stable `extractor_failed` class only; raw backend diagnostics, storage paths, stack traces, temporary filesystem paths, and connection strings stay out of projection payloads.

## Required Projection States

| State | Search/index/export/embedding behavior | Required proof |
|---|---|---|
| Extracted text available | Include extracted text, `extraction_status: "extracted"`, `reason: null`, and the canonical metadata envelope. | Contract tests show text is searchable/exportable while binary bytes are absent. |
| Large binary, extraction pending, unsupported MIME, completed-without-text, extractor failure, or quarantine | Emit a bounded valid record with the canonical metadata envelope, extraction status, and stable reason class. | Contract tests show the record remains valid and bounded while binary bytes and raw diagnostics are absent. |
| Attachment metadata incomplete | Fail closed for release readiness, or emit an explicit invalid-contract error outside normal projection output. | Tests prove malformed projection output cannot be produced silently. |

## Surface Requirements

- API export and backup export paths must use the same metadata envelope.
- Search and embedding preparation must use extracted text only, never binary bytes.
- Index/export code must preserve attachment identity through `id` while treating `path` as a display filename, not a file-read instruction or physical storage locator for downstream consumers.
- Debug output must continue to report lengths, counts, presence flags, checksums, MIME classes, stable reason classes, and IDs only where already allowed by the existing redaction posture.
- React/browser consumers must receive enough metadata to render attachment affordances and error states without inspecting binary payloads.
- AIWG index/export consumers must be able to skip, summarize, or defer binary attachments without panicking or attempting to embed raw bytes.

## Migration Plan

1. Inventory current Fortemi projection and export surfaces that can emit attachment content or attachment-derived records.
2. Add a shared projection helper or contract fixture that emits the canonical metadata envelope.
3. Add tests for extracted text, extraction-pending binary, large binary, unsupported MIME, and extractor-failure records.
4. Wire React/browser parity against the same fixture or documented response sample.
5. Passing Fortemi CI receipts are attached through PR #1023 / Actions runs 4094 and 4096 on commit `f12a2df9`, merged to `main` as `79600fc2`; coordinate `Fortemi/fortemi-react#227` release verification under the 2026-07-07 takeover-owned React checkpoint boundary.

## Closure Criteria

- Fortemi docs and tests prove `{ id, path, mime, checksum, bytes }` metadata plus extracted text for eligible attachments.
- Tests prove raw binary bytes never enter search, index, export, embedding, or API projection payloads.
- Tests prove each large or not-yet-extracted binary, including unsupported, pending, completed-without-text, failed-extraction, and quarantined binaries, still produces a bounded valid record with stable `extraction_status` and `reason` values.
- The Fortemi issue `Fortemi/fortemi#1013` links the passing CI receipt and references this contract.
- React package-release reconciliation is complete under the 2026-07-07 takeover sync; this Fortemi contract still does not close `Fortemi/fortemi-react#227` until the Fortemi-side binary projection proof is accepted.

## References

- `Fortemi/fortemi#1013`
- `Fortemi/fortemi-react#227`
- `roctinam/aiwg#1719`
- `Fortemi/fortemi-react#252`
- `.aiwg/testing/binary-attachment-projection-contract-test-plan-2026-07.md`

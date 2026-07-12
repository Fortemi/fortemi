# Binary Attachment Projection Contract Test Plan - 2026-07

## Purpose

Define the minimum proof package required to close `Fortemi/fortemi#1013` without treating completed `fortemi-react` package-release reconciliation as binary projection proof.

## Scope

This plan covers Fortemi-side projection behavior for binary attachments across search, index, export, embedding preparation, and API projection surfaces. It does not close React/browser parity; `Fortemi/fortemi-react#227` remains the owning React follow-up after the Fortemi contract is proven.

## Required Fixtures

| Fixture | Required properties |
|---|---|
| Text-extractable binary | Has an attachment `id`, logical `path`, `mime`, `checksum`, `bytes`, and extracted text. |
| Large binary | Exceeds extraction/index inline limits and must produce a bounded valid record. |
| Unsupported MIME | Has metadata but no extracted text; emits a stable reason class. |
| Extraction pending | Has metadata and an explicit pending state. |
| Extractor failure | Has metadata and a stable failure reason without raw backend diagnostics. |

## Assertions

- Every projection record includes `{ id, path, mime, checksum, bytes }`.
- Every projection record includes stable `extraction_status` and `reason` fields; extracted records use `extracted` plus `null`, no-text records use one of `extraction_pending`, `large_binary`, `unsupported_mime`, `no_extracted_text`, `extractor_failed`, or `quarantined`.
- Extracted text is included only when extraction succeeded.
- Raw binary bytes, base64 payloads, raw buffers, raw temporary paths, and backend error strings are absent from search/index/export/embedding payloads.
- Large, unsupported, pending, and failed-extraction cases return bounded valid records instead of panics or malformed JSON.
- Export and backup export use the same metadata envelope as search/index projection.
- Embedding preparation consumes extracted text only and skips or defers no-text binary records with a stable reason class.
- Debug output uses lengths, counts, presence flags, MIME classes, checksums, stable reason classes, or IDs only where allowed by existing redaction rules.
- Checksums use `blake3:<64-char-lowercase-hex>` and `path` contains only the display filename, never a physical storage locator.
- A portable shard sidecar maps each referenced checksum to `blobs/<64-char-lowercase-hex>` and deduplicates identical content by digest.
- Reference-only shards remain valid when the matching sidecar entry is absent, and readers ignore unknown or unreferenced `blobs/` entries.
- Sidecar presence never adds raw byte, base64, `data`, `raw`, or `content_bytes` fields to JSON records.

## Candidate Test Targets

The implementation owner should add or extend tests near the actual projection code. Candidate Fortemi targets from the current repository shape:

- `crates/matric-api/tests/analytics_memory_attachments_archive_routing_test.rs`
- `crates/matric-api/tests/archives_api_test.rs`
- `crates/matric-api/tests/backup_api_test.rs`
- `crates/matric-db/tests/file_storage_blob_refcount_test.rs`
- `crates/matric-db/tests/memory_search_test.rs`
- A new focused contract test named `binary_attachment_projection_contract_test.rs` if the projection seam is not already isolated.

## Evidence Required

To close `Fortemi/fortemi#1013`, attach:

- Fortemi CI receipt for the focused contract test target.
- A short sample projection payload for extracted-text and no-text binary cases, maintained in `.aiwg/evidence/binary-attachment-projection-sample-payloads-2026-07.md`.
- Confirmation that failed and quarantined records expose stable classes only and do not serialize raw extractor diagnostics.
- Confirmation that `Fortemi/fortemi-react#227` can consume the payload shape without raw binary inspection.
- Confirmation that `roctinam/aiwg#1719` no longer reproduces raw-byte index/export crashes against the sample payloads.

## Closure Evidence

Fortemi-side proof is attached through PR #1023, commit `f12a2df9`, merged to `main` as `79600fc2`.

- Actions run 4094 passed lint, dependency audit, cargo-deny policy, lockfile sync, build/unit tests, Docker build, isolated container test, and GPU integration.
- Actions run 4096 passed fast unit tests, integration tests, code coverage, and test summary.
- Focused local verification passed:
  - `cargo test -p matric-api --bin matric-api binary_attachment_export_projection -- --nocapture`
  - `cargo test -p matric-api --bin matric-api binary_attachment_projection_state -- --nocapture`

The remaining React/browser and AIWG index/export adoption work stays in `Fortemi/fortemi-react#227` and `roctinam/aiwg#1719`; those downstream tickets are not reopened as Fortemi-side blockers.

## Non-Goals

- Do not claim React/browser release parity from this Fortemi plan alone.
- Do not use `fortemi-react` package-release reconciliation as proof that the Fortemi binary attachment projection contract is accepted.
- Do not use npm `2026.7.3` availability as proof of binary attachment projection readiness.

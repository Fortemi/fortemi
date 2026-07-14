# MCP API and UAT Audit (2026-07-14)

## Scope

Audited the v2026.7.1 MCP core/full tool definitions, production filtering, inference and bulk-reprocess REST handlers, public MCP documentation, REST parity documentation, automated MCP schema tests, and manual MCP UAT phases.

## Findings and Disposition

| Finding | Severity | Disposition |
|---|---|---|
| Production exposed 43 core tools while schema tests independently asserted a stale 38-tool list | High | Resolved: one shared core inventory now drives production and tests |
| UAT claimed 27 core tools and 100% coverage | High | Resolved: inventory updated to 43; manual coverage is reported as 41/43 and combined coverage as 43/43 |
| `manage_inference` omitted llama.cpp, OpenRouter, independent embedding routing, update flags, provider inventory, config audit, and connection timeout | High | Resolved in schema, handler mapping, tests, and docs |
| UAT bulk-reprocess expectations used obsolete `processed_count`, `failed_count`, and `job_id` fields | Medium | Resolved: UAT now asserts `notes_count` and `jobs_queued` |
| No regression test proved archive-wide bulk reprocessing beyond the repository's 100-note page cap | High | Resolved: isolated 105-note archive test added to Phase 14 |
| Graph diagnostics, quality operations, access analytics, jobs, related notes, and purge tools lacked manual UAT coverage | Medium | Resolved with operations Phase 14 and cleanup Phase 15 |
| REST parity documentation contained obsolete endpoint mappings and false equivalence claims | Medium | Resolved: replaced with an audited capability-level map and intentional REST-only boundary |
| Shard documentation could imply self-contained attachment portability | Medium | Resolved: current reference-only export/import limitation is explicit |

## Intentional Gaps

- `manage_encryption` and `manage_backups` remain automated-integration-only because manual execution requires PKE and backup infrastructure.
- `purge_all_notes` is tested only with `confirm: false`; confirming it on a shared UAT instance could purge unrelated deleted notes.
- Streaming inference/chat/ingest/health, realtime WebSockets, inbound webhooks, TUS/raw binary transfer, and OAuth protocol endpoints remain REST-only by design.
- Portable shard blob sidecars are a documented contract direction, not current server behavior.

## Verification Targets

- 205 full-mode tool schemas validate.
- Core filtering returns exactly the shared 43-tool inventory.
- Inference request mapping preserves explicit `null`, all provider blocks, validation flags, audit filters, and connection timeout.
- The manual UAT suite executes 184 tests across 16 phases, with Phase 15 always last.

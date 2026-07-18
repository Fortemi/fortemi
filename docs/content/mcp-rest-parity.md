# MCP and REST API Parity

This document records the audited boundary between Fortemi's REST API and the MCP server as of 2026.7.1.

## Surface Modes

- **Core MCP mode** exposes 43 agent-oriented tools. Thirteen are consolidated tools with an `action` discriminator.
- **Full MCP mode** exposes 205 tools, including low-level administrative operations.
- **REST** remains the canonical transport API and includes streaming, realtime, webhook, upload, and OAuth surfaces that are intentionally not modeled as request/response MCP tools.

The core inventory is defined once in `mcp-server/constants/core-tools.js`; production filtering and schema tests import that same list.

## Core REST Mapping

| Core capability | MCP tools | REST endpoint families |
|---|---|---|
| Notes | `list_notes`, `get_note`, `update_note`, `delete_note`, `restore_note`, `capture_knowledge` | `/api/v1/notes*`, templates, upload helpers |
| Search | `search` | `/api/v1/search`, `/api/v1/search/federated`, `/api/v1/memories/search` |
| Provenance | `record_provenance` | `/api/v1/provenance/*` |
| Organization | `manage_tags`, `manage_collection`, `manage_concepts`, `manage_embeddings` | tags, collections, concepts/schemes, embedding sets |
| Archives | `manage_archives`, `select_memory`, `get_active_memory` | `/api/v1/archives*` plus MCP session memory headers |
| Attachments | `manage_attachments` | `/api/v1/notes/{id}/attachments*` and upload endpoints |
| Encryption and backup | `manage_encryption`, `manage_backups` | `/api/v1/pke/*`, `/api/v1/backup/*`, `/api/v1/memory/*` |
| Graph | 13 graph/link tools | `/api/v1/graph/*`, `/api/v1/notes/{id}/links`, `/related` |
| Jobs | `manage_jobs` | `/api/v1/jobs*`, `/api/v1/extraction/stats` |
| Inference config | `manage_inference` | models, embedding configs, `/api/v1/inference/config*`, `/providers`, `/test-connection` |
| Health and system | `health_check`, `get_system_info`, `get_knowledge_health`, `get_access_frequency` | `/health`, health analytics, memory info, queue stats |
| Export and bulk | `export_note`, `bulk_reprocess_notes` | note export and `/api/v1/notes/reprocess` |
| Permanent deletion | `purge_note`, `purge_notes`, `purge_all_notes` | `/api/v1/notes/{id}/purge` |
| Agent guidance | `get_documentation` | MCP-only static workflow guidance |

Consolidated MCP tools may compose several REST calls or return safe transfer instructions. They are parity at the workflow level, not necessarily one tool per endpoint.

## Inference Contract

`manage_inference` mirrors the current non-streaming inference administration contract:

- Effective configuration includes `default_backend`, optional independent `embedding_backend`, Ollama, OpenAI-compatible, llama.cpp, and OpenRouter blocks with source attribution.
- Partial updates support all four provider blocks and explicit JSON `null` to clear `embedding_backend`.
- `validate`, `dry_run`, and `atomic` update flags are forwarded.
- Provider inventory and redacted config audit history are readable through `list_providers` and `get_config_audit`.
- Connection tests forward the bounded `timeout_secs` option.

`POST /api/v1/inference/complete` and `/api/v1/inference/stream` remain REST-only. They are transport-level generation endpoints, not configuration tools.

## Intentional REST-Only Surfaces

These endpoints do not belong in the core request/response MCP surface:

- Server-sent event and streaming inference/chat/ingest/health responses
- WebSocket realtime call transports
- Inbound webhook receivers and provider callbacks
- TUS resumable upload protocol and raw binary downloads
- OAuth authorization, callback, token, client-registration, and revocation flows
- Browser/admin UI routes and OpenAPI/AsyncAPI documents

Some non-streaming administration remains available in full MCP mode even when it is not in core mode, including detailed versioning, document types, API keys, and low-level SKOS relations.

## Known Portability Limitation

The REST export route is reference-only by default and supports verified
attachment sidecars through `include_blobs=true`. Shard import restores present
valid sidecars and preserves missing ones as references. `manage_backups`
currently exposes component selection but not the REST sidecar opt-in, so its
generated `export_shard` command remains reference-only unless the query is
amended by the caller. This is `core-v1` portability, not `full-v1` disaster
recovery.

## Verification

- `npm run validate:schemas` validates all 205 schemas.
- `npm run test:schema` verifies that every one of the 43 core names exists and that filtering returns exactly 43 tools.
- `node --test tests/inference-requests.test.js` verifies provider fields, dry-run/atomic flags, explicit-null embedding routing, audit filters, and connection timeout mapping.
- `tests/uat/phases/phase-14-mcp-operations.md` covers current operational parity and the >100-note bulk pagination regression.

# MCP vs REST API Parity

This document clarifies which features are available via the REST API, the MCP (Model Context Protocol) server, or both.

## Overview

The Fortemi API exposes functionality through two interfaces:

- **REST API** (`/api/v1/*`): Standard HTTP endpoints for programmatic access
- **MCP Server** (port 3001): AI agent interface with tool-based access

Most features are available through both interfaces. Some MCP tools are **composite** — they aggregate multiple REST endpoints into a single tool call for agent convenience.

## MCP-Only Features

These tools have no single REST endpoint equivalent. They aggregate multiple REST calls or provide agent-specific functionality.

| MCP Tool | What It Does | REST Equivalent |
|---|---|---|
| `get_system_info` | Aggregated system overview | `GET /health` + `/api/v1/memory/info` + `/api/v1/jobs/stats` + `/api/v1/embedding-sets` |
| `get_documentation` | Static AI agent guidance text | None (agent-only help text) |
| `memory_info` | Archive storage and system diagnostics | `GET /api/v1/memory/info` (direct equivalent exists) |
| `search_memories_federated` | Cross-archive search | No REST equivalent (single-archive search only) |
| `get_memories_overview` | Overview of all archives | `GET /api/v1/archives` (partial — less detail) |

## Composite MCP Tools

These tools work by orchestrating multiple REST operations in a single call.

| MCP Tool | REST Operations Composed |
|---|---|
| `reprocess_note` | Queues multiple jobs (ai_revision, embedding, linking, title_generation) |
| `clone_memory` | Schema clone + data copy + FK restoration |
| `export_all_notes` | Search + export per note |
| `search_with_dedup` | Search + chunk deduplication |
| `knowledge_shard` | Export schema + data + embeddings |
| `knowledge_shard_import` | Import schema + data + embeddings |
| `backup_import` | Validate + restore from JSON/archive |

## REST Endpoint to MCP Tool Mapping

### Notes

| REST Endpoint | MCP Tool |
|---|---|
| `POST /api/v1/notes` | `create_note` |
| `GET /api/v1/notes` | `list_notes` |
| `GET /api/v1/notes/:id` | `get_note` |
| `PATCH /api/v1/notes/:id` | `update_note` |
| `DELETE /api/v1/notes/:id` | `delete_note` |
| `POST /api/v1/notes/:id/restore` | `restore_note` |
| `DELETE /api/v1/notes/:id/purge` | `purge_note` |
| `POST /api/v1/notes/bulk` | `bulk_create_notes` |
| `PUT /api/v1/notes/:id/tags` | `set_note_tags` |
| `GET /api/v1/notes/:id/export` | `export_note` |
| `POST /api/v1/notes/:id/move` | `move_note_to_collection` |

### Search

| REST Endpoint | MCP Tool |
|---|---|
| `GET /api/v1/search` | `search_notes` |
| `GET /api/v1/search/dedup` | `search_with_dedup` |
| `POST /api/v1/search/location` | `search_memories_by_location` |
| `POST /api/v1/search/time` | `search_memories_by_time` |
| `POST /api/v1/search/combined` | `search_memories_combined` |

### Collections

| REST Endpoint | MCP Tool |
|---|---|
| `GET /api/v1/collections` | `list_collections` |
| `GET /api/v1/collections/:id` | `get_collection` |
| `POST /api/v1/collections` | `create_collection` |
| `PATCH /api/v1/collections/:id` | `update_collection` |
| `DELETE /api/v1/collections/:id` | `delete_collection` |
| `GET /api/v1/collections/:id/notes` | `get_collection_notes` |

### SKOS Concepts

| REST Endpoint | MCP Tool |
|---|---|
| `GET /api/v1/concepts/schemes` | `list_concept_schemes` |
| `POST /api/v1/concepts/schemes` | `create_concept_scheme` |
| `GET /api/v1/concepts/schemes/:id` | `get_concept_scheme` |
| `POST /api/v1/concepts` | `create_concept` |
| `GET /api/v1/concepts/:id` | `get_concept` |
| `GET /api/v1/concepts/:id/full` | `get_concept_full` |
| `PATCH /api/v1/concepts/:id` | `update_concept` |
| `DELETE /api/v1/concepts/:id` | `delete_concept` |
| `GET /api/v1/concepts/search` | `search_concepts` |
| `GET /api/v1/concepts/autocomplete` | `autocomplete_concepts` |
| `POST /api/v1/concepts/:id/broader` | `add_broader` |
| `POST /api/v1/concepts/:id/narrower` | `add_narrower` |
| `POST /api/v1/concepts/:id/related` | `add_related` |
| `GET /api/v1/concepts/:id/broader` | `get_broader` |
| `GET /api/v1/concepts/:id/narrower` | `get_narrower` |
| `GET /api/v1/concepts/:id/related` | `get_related` |
| `DELETE /api/v1/concepts/:id/broader/:target` | `remove_broader` |
| `DELETE /api/v1/concepts/:id/narrower/:target` | `remove_narrower` |
| `DELETE /api/v1/concepts/:id/related/:target` | `remove_related` |
| `GET /api/v1/concepts/schemes/:id/export/turtle` | `export_skos_turtle` |
| `POST /api/v1/notes/:id/concepts/:concept_id` | `tag_note_concept` |
| `DELETE /api/v1/notes/:id/concepts/:concept_id` | `untag_note_concept` |
| `GET /api/v1/notes/:id/concepts` | `get_note_concepts` |

### Archives (Multi-Memory)

| REST Endpoint | MCP Tool |
|---|---|
| `GET /api/v1/archives` | `list_archives` |
| `POST /api/v1/archives` | `create_archive` |
| `GET /api/v1/archives/:name` | `get_archive` |
| `PATCH /api/v1/archives/:name` | `update_archive` |
| `DELETE /api/v1/archives/:name` | `delete_archive` |
| `POST /api/v1/archives/:name/clone` | `clone_memory` |
| `POST /api/v1/archives/:name/default` | `set_default_archive` |

### Health & Analytics

| REST Endpoint | MCP Tool |
|---|---|
| `GET /health` | `health_check` |
| `GET /api/v1/health/knowledge` | `get_knowledge_health` |
| `GET /api/v1/health/orphan-tags` | `get_orphan_tags` |
| `GET /api/v1/health/stale-notes` | `get_stale_notes` |
| `GET /api/v1/health/unlinked-notes` | `get_unlinked_notes` |
| `GET /api/v1/health/tag-cooccurrence` | `get_tag_cooccurrence` |
| `GET /api/v1/notes/timeline` | `get_notes_timeline` |
| `GET /api/v1/notes/activity` | `get_notes_activity` |
| `GET /api/v1/memory/info` | `memory_info` |

### Other Features

| REST Endpoint | MCP Tool |
|---|---|
| `GET /api/v1/templates` | `list_templates` |
| `POST /api/v1/templates` | `create_template` |
| `POST /api/v1/templates/:id/instantiate` | `instantiate_template` |
| `GET /api/v1/notes/:id/versions` | `list_note_versions` |
| `POST /api/v1/notes/:id/versions/:vid/restore` | `restore_note_version` |
| `GET /api/v1/notes/:id/versions/diff` | `diff_note_versions` |
| `GET /api/v1/notes/:id/graph` | `explore_graph` |
| `GET /api/v1/notes/:id/links` | `get_note_links` |
| `GET /api/v1/notes/:id/backlinks` | `get_note_backlinks` |
| `GET /api/v1/notes/:id/provenance` | `get_note_provenance` |
| `GET /api/v1/notes/:id/chunks` | `get_chunk_chain` |
| `GET /api/v1/notes/:id/full` | `get_full_document` |
| `POST /api/v1/backup/now` | `backup_now` |
| `GET /api/v1/backup/status` | `backup_status` |
| `GET /api/v1/backup/download` | `backup_download` |
| `POST /api/v1/backup/import` | `backup_import` |
| `POST /api/v1/jobs` | `create_job` |
| `GET /api/v1/jobs` | `list_jobs` |
| `GET /api/v1/jobs/stats` | `get_queue_stats` |

## HTTP Caching Behavior

The API sets `Cache-Control` headers on all responses:

| Endpoint Category | Cache-Control | Rationale |
|---|---|---|
| Mutations (POST/PUT/PATCH/DELETE) | `no-store` | Never cache write operations |
| Document types, concept schemes | `public, max-age=300` | Stable reference data (5 min) |
| Health endpoints | `no-cache, max-age=0` | Always fresh |
| All other API GETs | `private, no-cache` | Client may cache, must revalidate |
| Static assets (docs, openapi.yaml) | `public, max-age=3600` | Rarely changes (1 hour) |

All `/api/v1/*` responses include `Vary: X-Fortemi-Memory` since responses depend on the selected archive.

## When to Use REST vs MCP

| Use Case | Recommended Interface |
|---|---|
| AI agent integration | MCP (purpose-built for agents) |
| Web application | REST API |
| CLI scripts | REST API |
| Cross-archive operations | MCP (`search_memories_federated`) |
| System diagnostics | MCP (`get_system_info` aggregates 4 endpoints) |
| Bulk data operations | REST API (streaming, pagination) |
| Webhook/event integration | REST API (SSE, WebSocket) |

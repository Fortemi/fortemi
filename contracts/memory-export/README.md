# Memory Export Contract (`memory-export/1.0.0`)

Fortemi produces `memory-export/1.0.0`. It is a read-only, point-in-time
projection of one memory, designed for downstream systems that rebuild graph or
knowledge projections deterministically. It is separate from the Knowledge
Shard transfer profiles (`contracts/knowledge-shard`) and from backups. It
restores nothing and creates no archive.

- REST: `POST /api/v1/memory/export`. Requires the `read` scope and returns
  `Cache-Control: no-store`. The `X-Fortemi-Memory` header selects the memory.
- MCP: `export_memory_snapshot`, available in full tool mode.
- Request schema: `1.0.0/request.schema.json`
- Response schema: `1.0.0/response.schema.json`, made up of
  `manifest.schema.json` and one `record.schema.json` per line
- Architecture decision: `docs/architecture/adr/ADR-109-point-in-time-memory-export.md`
- Producer tests:
  - `cargo test -p matric-db --features migrations --test memory_export_contract_test`
  - `cargo test -p matric-api --features hosted-auth --bin matric-api memory_export`

## Entity types and keys

| Entity | Key | Fields |
|--------|-----|--------|
| `collection` | `[id]` | id, name, description, parent_id, created_at_utc |
| `note` | `[id]` | id, collection_id, format, source, title, metadata, starred, archived, visibility, document_type_id, owner_id, created_at_utc, updated_at_utc, deleted_at |
| `note_original` | `[note_id]` | note_id, content, hash, version_number, user_created_at, user_last_edited_at |
| `note_revised_current` | `[note_id]` | note_id, content, last_revision_id, ai_metadata |
| `note_tag` | `[note_id, tag_name]` | note_id, tag_name, source |
| `link` | `[id]` | id, from_note_id, to_note_id, to_url, kind, score, metadata, created_at_utc |

Key fields are always included, even when `fields` selects a subset. A
soft-deleted note is exported as a live record with `deleted_at` set. A delete
line is emitted only for a hard delete, a purge, or a key change. Access
counters, embeddings, and derived search vectors are not exported.

## Request

```json
{"mode": "full", "entity_types": ["note", "note_tag"], "fields": {"note": ["title"]}}
{"mode": "incremental", "since": "48213"}
```

- `mode` defaults to `full`.
- `since` is required for `incremental` and forbidden for `full`.
- A `since` greater than the memory's current high-water mark is rejected with
  400.

## Output

Each record is one line: `{"entity", "key", "op": "upsert"|"delete", "record"?}`.
`record` is present only for `upsert`.

- **Canonical form.** Object keys are sorted, there is no insignificant
  whitespace, and timestamps are RFC 3339 in UTC (`+00:00`).
- **Order.** Lines are ordered by entity type in the table order above, then by
  the canonical JSON text of `key` compared bytewise. Each `(entity, key)`
  appears at most once.
- **Hashes.** `manifest.entities[type].sha256` is
  `sha256:` + hex(SHA-256(concatenation of that type's lines, each followed by
  `\n`)). `manifest.content_sha256` is the same digest over every line. Every
  selected type is listed, including types with no records.
- **Determinism.** Every manifest field except `generated_at` is a function of
  the memory's contents at the high-water mark, the selection, and the producer
  build (`producer.version`, `producer.git_sha`, `schema_version`). Two exports
  with the same mark and selection are byte-identical after `generated_at` is
  removed.

## High-water mark

`high_water_mark` is a monotonic 64-bit PostgreSQL transaction id, sent as a
decimal string. The export window is `[since, high_water_mark)`: incrementals
take `since` from the previous export's `high_water_mark`.

- **Snapshot.** The export reads one MVCC snapshot. Writes that commit while it
  runs are not included, and the export never mixes old and new states.
- **No gaps.** Every change the snapshot could not see is stamped
  `>= high_water_mark`, even when it was made by a transaction that started
  earlier and committed later. The next incremental is therefore guaranteed to
  include it.
- **Duplicates.** When an older transaction was still running at export time,
  the mark is held back, and a later incremental may repeat records that are
  already applied. Repeats are identical to the current state.

## Apply semantics

Treat the export as a map from `(entity, canonical key)` to `record`:

- `upsert` replaces the whole record for its key.
- `delete` removes the key. Deleting an absent key is a no-op.
- Lines can be applied in order. No key appears twice in one export.

**Guarantee.** Let `full@M1` be a full export. Applying
`incremental(since = M1)` taken at mark `M2` to the state of `full@M1` yields
exactly the state of `full@M2` for the same selection. Repeated incrementals
compose the same way. The reference implementation is
`matric_core::apply_export_records`.

## Lifecycle notes

- Rows that existed before the change-tracking migration are stamped `0`. They
  appear in full exports and in no incremental until they change.
- Tombstones are retained. Version 1.0.0 publishes no tombstone floor, so any
  previous mark stays valid.
- A database restore or `pg_dump` load replaces the transaction-id lineage.
  Take a new full export after any restore. Marks from before the restore are
  not comparable.
- `TRUNCATE` is not tracked. Fortemi does not truncate these tables.
- Exports are materialized and returned as one JSON document. For very large
  memories, export one entity type at a time. Each export is a separate
  snapshot, so use the same `since` for every type, or use one full export.

## Relationship to archive transfer (#905)

Fortemi/fortemi#905 defines clone, backup, and import semantics for Referenced
archives. A memory export is not one of those operations. It copies only
Fortemi-owned rows of the listed entity types, never operator source bytes or
host paths, and it cannot recreate or reactivate an archive. #905's behaviour
matrix should list memory export as metadata-only for every storage mode.

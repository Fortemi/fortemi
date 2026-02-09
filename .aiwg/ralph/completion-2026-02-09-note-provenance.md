# Ralph Loop Completion Report

**Task**: Implement note-level provenance (Issue #262)
**Status**: SUCCESS
**Iterations**: 4 (migration fix, query priority fix, default archive fix)
**Duration**: ~45 minutes

## Iteration History

| # | Action | Result | Duration |
|---|--------|--------|----------|
| 1 | Full implementation: migration + core + DB + API + MCP | 8 test failures (table not migrated locally) | 20m |
| 2 | Applied migration to local DB | 1 test failure (JSONB fallback flooding results) | 2m |
| 3 | Added priority ordering + NOT EXISTS for file provenance in JSONB fallback | 5 test failures (stale default archive) | 3m |
| 4 | Reset default archive to public in registry | All tests pass | 2m |

## Verification Output

```
$ cargo test --workspace
test result: ok. 109 passed; 0 failed ... (matric-api binary)
test result: ok. 65 passed; 0 failed ... (matric-core)
... (all crates pass)

$ cargo clippy -- -D warnings
Finished `dev` profile [unoptimized + debuginfo]

$ node /tmp/validate_mcp.mjs
Total tools: 168
create_note_provenance found: true
Required: [ 'note_id' ]
Schema issues: 0
```

## Files Modified

### New Files
- `migrations/20260209300000_note_level_provenance.sql` - Migration: rename table, add note_id, XOR constraint, expanded CHECK constraints, compatibility view, archive loop

### Rust (Backend)
- `crates/matric-core/src/models.rs` - ProvenanceRecord (renamed from FileProvenanceRecord), CreateNoteProvenanceRequest, MemoryProvenance.note field
- `crates/matric-db/src/memory_search.rs` - All SQL updated: provenance table, LEFT JOIN attachment, note-level provenance CRUD, priority ordering
- `crates/matric-api/src/handlers/provenance.rs` - create_note_provenance handler
- `crates/matric-api/src/main.rs` - Route registration, test SQL updates
- `crates/matric-db/tests/memory_search_test.rs` - Table name updates

### MCP Server
- `mcp-server/tools.js` - create_note_provenance tool schema
- `mcp-server/index.js` - Handler + DOCUMENTATION help text update

### OpenAPI
- `crates/matric-api/src/openapi.yaml` - POST /api/v1/provenance/notes endpoint

### Documentation
- `docs/content/memory-search.md` - Note provenance overview, API docs, field reference updates
- `docs/content/mcp.md` - Tool count (168), Memory Search category (9 tools)
- `docs/content/mcp-rest-parity.md` - New endpoint mapping

### UAT
- `tests/uat/phases/phase-3b-memory-search.md` - 5 new test cases (UAT-3B-021 through UAT-3B-025)

## Architecture Summary

**Approach**: Polymorphic unified `provenance` table (renamed from `file_provenance`)

Key design decisions:
1. **XOR constraint**: `(attachment_id IS NOT NULL) != (note_id IS NOT NULL)` - exactly one target per record
2. **Backward compatibility**: `CREATE VIEW file_provenance AS SELECT * FROM provenance WHERE attachment_id IS NOT NULL`
3. **Type alias**: `type FileProvenanceRecord = ProvenanceRecord;`
4. **Unique index**: One provenance per note (`idx_provenance_note_id WHERE note_id IS NOT NULL`)
5. **Route pattern**: `POST /api/v1/provenance/notes` (body-based, matching existing `/api/v1/provenance/files`)
6. **Multi-archive**: Archive loop applies all DDL changes to non-public schemas

## Gitea Issues

- #262 (Epic): Note-level provenance
- #263: Migration - DONE
- #264: Core + DB - DONE
- #265: API + MCP - DONE
- #266: Docs + UAT - DONE

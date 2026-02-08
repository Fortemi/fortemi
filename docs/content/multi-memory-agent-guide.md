# Multi-Memory Agent Guide

**AI Agent Quick Reference for Fortemi Multi-Memory Architecture**

## Quick Reference

```
Memory = isolated PostgreSQL schema. Select via X-Fortemi-Memory header or MCP select_memory.
Default = public schema. Omitting header = default memory (archive_public).
All CRUD/tag/collection/template/version/attachment operations are memory-scoped.
Search (FTS+semantic) currently limited to default archive only (federated search planned).
Max memories configurable via MAX_MEMORIES env var (default: 100, recommended: <50).
```

## When to Create a New Memory (Decision Matrix)

| Scenario | Use Memory? | Instead Use | Notes |
|----------|-------------|-------------|-------|
| Separate client data with legal isolation requirements | **YES** | - | Hard schema-level isolation |
| Separate work vs personal | **YES** | - | If privacy/legal boundary required |
| Separate by project (>1000 notes per project) | **YES** | - | If projects are truly independent |
| Multi-tenant SaaS deployment | **YES** | - | One memory per tenant |
| Separate by topic within same project | **NO** | Tags or Collections | Memories too heavyweight |
| Temporary grouping of notes | **NO** | Collections | Memories are permanent structures |
| Filter search results by category | **NO** | Strict tag filtering | Search is memory-scoped already |
| Archive old project data | **YES** | - | Clone to archive memory |
| Test/staging environment isolation | **YES** | - | Separate memories for test data |
| Separate by time period (quarterly) | **MAYBE** | Tags if <5000 notes/quarter | Use rolling archives for >10k notes |
| Different embedding models per domain | **YES** | - | Each memory has independent embedding config |
| Team knowledge base (single team) | **NO** | Single memory + tags | Unless >50k notes |

## Segmentation Strategies

### Strategy A: By Client/Tenant (Multi-Tenant SaaS)

**Pattern:** One memory per client (`client-acme`, `client-globex`)

**Best for:** Multi-tenant deployments, legal data isolation, SaaS platforms

**Tradeoffs:**
- Cannot search across clients without federated search (not yet implemented)
- Perfect data isolation (schema-level)
- Simplifies backup/restore per client
- Overhead: ~1MB per empty memory, scales with data

**Naming convention:** `client-{slug}` or `tenant-{id}`

**When to use:** Always use for SaaS/multi-tenant scenarios where data isolation is legally or contractually required.

### Strategy B: By Project (Independent Work Streams)

**Pattern:** One memory per major project (`project-alpha`, `project-beta`)

**Best for:** Teams working on distinct, unrelated projects with minimal knowledge overlap

**Tradeoffs:**
- Cross-project knowledge discovery requires explicit effort
- Simplifies project archival (delete entire memory)
- Search limited to single project context

**When to merge:** If projects share >30% of the same concepts/tags, use single memory with project tags instead.

**Naming convention:** `project-{name}` or `proj-{slug}`

**When to use:** Projects with >1000 notes each, minimal shared concepts, or need for independent lifecycle management.

### Strategy C: By Knowledge Domain (Content Type)

**Pattern:** Separate domains (`code-docs`, `research-papers`, `meeting-notes`)

**Best for:** Different content types with different search patterns, embedding models, or retention policies

**Tradeoffs:**
- Same topic across domains requires federated search
- Risk: Domains often overlap, prefer tags within single memory
- Enables domain-specific embedding models (e.g., code-specific for `code-docs`)

**Naming convention:** `domain-{type}` or `kb-{category}`

**When to use:** Only when content types have fundamentally different characteristics (different embedding models, drastically different note sizes, separate retention policies).

**Warning:** Over-segmentation by domain reduces cross-domain discovery. Prefer single memory with document type metadata.

### Strategy D: By Time Period (Rolling Archives)

**Pattern:** Rolling archives (`archive-2025-q4`, `archive-2026-q1`, plus `active` for current work)

**Best for:** Compliance, data retention, performance (smaller active set), historical archival

**Tradeoffs:**
- Historical search requires federated query across periods (not yet supported)
- Excellent for compliance (immutable historical archives)
- Reduces active search space for performance

**Operations:** Clone current memory → archive at period boundary, then prune active memory.

**Naming convention:** `archive-{YYYY}-q{N}` or `archive-{YYYY}-{MM}`

**When to use:** Regulatory compliance requirements, >50k notes with clear temporal boundaries, or need to reduce active search space.

### Strategy E: Single Memory (Default, Recommended for Most)

**Pattern:** Everything in default memory, use tags/collections for organization

**Best for:** Individual users, small teams, <50,000 notes, no regulatory isolation needs

**Tradeoffs:**
- No data isolation (soft boundaries via tags only)
- Larger search space (but still performant <50k notes)
- Simplest operational model

**When to use:** Default strategy unless you have specific requirements driving multi-memory adoption.

**Recommended when:** No regulatory/legal isolation requirements, team size <20, or total notes <50,000.

## Tradeoffs Table

| Factor | Single Memory | Multiple Memories |
|--------|--------------|-------------------|
| **Search speed** | Scales with total notes | Scales with per-memory notes (smaller scope = faster) |
| **Search scope** | All notes searchable | Per-memory only (federated search planned, not implemented) |
| **Data isolation** | Tags only (soft) | Schema-level (hard, PostgreSQL enforced) |
| **Storage overhead** | Baseline | +~1MB per memory (41 tables × indexes per schema) |
| **Backup granularity** | All or nothing | Per-memory backup/restore possible |
| **Cross-referencing** | Links work anywhere | Links memory-scoped only (no cross-memory links) |
| **Migration complexity** | None | Export/import required between memories |
| **Max recommended** | 50,000 notes | 50,000 notes per memory |
| **Operational overhead** | Single VACUUM | Per-memory VACUUM ANALYZE needed |
| **Embedding configs** | Shared embedding sets | Independent per-memory embedding configs |
| **Tag isolation** | Shared tag namespaces | Independent tag/collection namespaces per memory |
| **Template sharing** | Global templates | Templates memory-scoped (duplicate across memories) |

## API Patterns for Agents

### HTTP API (cURL examples)

```bash
# Create memory
curl -X POST http://localhost:3000/api/v1/memories \
  -H "Content-Type: application/json" \
  -d '{"name": "project-alpha", "description": "Alpha project knowledge base"}'

# List all memories
curl http://localhost:3000/api/v1/memories

# Get memory stats
curl http://localhost:3000/api/v1/archives/project-alpha/stats

# Create note in specific memory
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -H "X-Fortemi-Memory: project-alpha" \
  -d '{"title": "Note", "content": "Content", "metadata": {}}'

# Search in default memory only (other memories return 400)
curl http://localhost:3000/api/v1/search/combined?q=query

# Clone memory (backup/snapshot)
curl -X POST http://localhost:3000/api/v1/archives/project-alpha/clone \
  -H "Content-Type: application/json" \
  -d '{"new_name": "project-alpha-backup-2026-02", "description": "Monthly backup"}'

# Delete memory (IRREVERSIBLE, CASCADE)
curl -X DELETE http://localhost:3000/api/v1/memories/project-alpha
```

### MCP Tool Patterns

```javascript
// Select active memory for MCP session (persists across calls)
await select_memory({ name: "project-alpha" });

// Check current active memory
const active = await get_active_memory();
// Returns: { name: "project-alpha" }

// All subsequent MCP calls use selected memory automatically
await add_note({ title: "Note", content: "..." }); // goes to project-alpha
await search_notes({ query: "..." }); // searches project-alpha only

// List all memories
const memories = await list_archives();

// Get memory statistics
const stats = await get_archive_stats({ archive_name: "project-alpha" });

// Create new memory
await create_archive({
  name: "project-beta",
  description: "Beta project knowledge base"
});

// Delete memory (DESTRUCTIVE)
await delete_archive({ archive_name: "project-beta" });
```

## Common Mistakes

| Mistake | Why It's Wrong | Do This Instead |
|---------|---------------|-----------------|
| Creating memory per tag | Memories are heavyweight schema isolation (41 tables + indexes) | Use tags within single memory |
| Forgetting X-Fortemi-Memory header in HTTP API | Operations go to default memory silently, no error | Always set header explicitly or use select_memory in MCP |
| Searching non-default archive | Returns HTTP 400 "Search only available in default archive" | Use default archive for search, or wait for federated search feature |
| Creating 100+ memories | Each has 41 tables with indexes, massive overhead | Limit to <50 memories for performance, prefer tags/collections |
| Not vacuuming per-memory | Auto-vacuum may not reach archive schemas promptly | Schedule per-schema `VACUUM ANALYZE archive_{name}` |
| Assuming cross-memory links work | Links are memory-scoped, no foreign keys across schemas | Export/import notes or use same memory |
| Using memories for temporary grouping | Memories are permanent structures, expensive to delete | Use collections for temporary grouping |
| Over-segmenting by topic | Reduces discoverability, creates knowledge silos | Use single memory with tags unless >10k notes per topic |
| Mixing embedding models without memories | Embedding configs are global in single-memory mode | Use separate memories for different embedding models |
| Expecting auto-migration between memories | No built-in migration, must export/import | Plan memory structure upfront, migrations are manual |

## MCP Tool Quick Reference

| Tool | Purpose | Uses Active Memory? | Returns Error if Non-Default? |
|------|---------|---------------------|------------------------------|
| `select_memory` | Set active memory for MCP session | Sets it (no active required) | No |
| `get_active_memory` | Check current active memory | No (retrieves state) | No |
| `list_archives` / `list_memories` | List all memories | No (system-wide) | No |
| `create_archive` / `create_memory` | Create new memory | No (system-wide) | No |
| `delete_archive` / `delete_memory` | Delete memory (destructive) | No (system-wide) | No |
| `get_archive_stats` | Get memory statistics | No (specify by name) | No |
| `memory_info` | System info for active memory | Yes | No |
| `add_note` / `update_note` / `delete_note` | Note CRUD | Yes | No |
| `search_notes` / `semantic_search` | Search notes | Yes | **YES** (only default searchable) |
| `add_tag` / `list_tags` | Tag management | Yes | No |
| `create_collection` / `list_collections` | Collection management | Yes | No |
| `add_template` / `list_templates` | Template management | Yes | No |
| `link_notes` / `get_backlinks` | Link management | Yes | No |
| `attach_file` / `list_attachments` | Attachment management | Yes | No |
| `add_version` / `list_versions` | Version history | Yes | No |

## Performance Characteristics

### Memory Creation
- **Time:** ~200ms (41 tables + indexes)
- **Storage:** ~1MB empty, scales with data
- **Limit:** 100 memories by default (MAX_MEMORIES env var)

### Memory Cloning
- **Time:** Proportional to data size (indexes rebuilt)
- **Storage:** Doubles (full copy including embeddings)
- **Use case:** Backups, snapshots, archival

### Memory Deletion
- **Time:** ~500ms (CASCADE drop)
- **IRREVERSIBLE:** No recovery, no soft delete
- **WARNING:** Deletes all notes, tags, collections, embeddings, attachments

### Search Performance
- **Default memory:** <100ms typical for <50k notes
- **Non-default memory:** Returns HTTP 400 (not yet supported)
- **Federated search:** Planned, not implemented (will query multiple memories)

## Decision Flow for Agents

```
START: Does user need multi-memory?
  |
  +--> NO legal/regulatory isolation required?
  |    +--> <50k total notes expected?
  |         +--> Use SINGLE MEMORY (default)
  |
  +--> Multi-tenant SaaS?
  |    +--> YES --> Strategy A: One memory per tenant
  |
  +--> Independent projects with <30% concept overlap?
  |    +--> YES --> Strategy B: One memory per project
  |
  +--> Need different embedding models per domain?
  |    +--> YES --> Strategy C: One memory per domain
  |
  +--> Compliance requires historical immutability?
  |    +--> YES --> Strategy D: Rolling time-based archives
  |
  +--> Default to SINGLE MEMORY, use tags/collections
```

## Migration Path

### From Single Memory to Multi-Memory

1. **Create target memory:** `POST /api/v1/memories`
2. **Export notes:** Use bulk export with tag filter
3. **Import to new memory:** `POST /api/v1/notes` with `X-Fortemi-Memory` header
4. **Verify:** Check note counts match
5. **Update agents:** Change MCP `select_memory` or API headers
6. **Archive old memory:** Clone to backup, then delete (optional)

### From Multi-Memory to Single Memory

1. **Export all memories:** Per-memory bulk export
2. **Tag for provenance:** Add `source-memory:{name}` tag to all notes
3. **Import to default memory:** Omit `X-Fortemi-Memory` header
4. **Update links:** Re-link notes (links don't migrate across memories)
5. **Verify:** Check total note counts
6. **Delete old memories:** After verification period

## Operational Checklist

- [ ] Plan memory structure upfront (migrations are manual)
- [ ] Limit to <50 memories for performance
- [ ] Set `X-Fortemi-Memory` header explicitly in all HTTP API calls
- [ ] Use `select_memory` at start of MCP sessions
- [ ] Search only works in default memory (federated planned)
- [ ] Schedule per-memory `VACUUM ANALYZE` for large memories
- [ ] Clone memories before risky operations (deletion is irreversible)
- [ ] Monitor memory count (`list_archives`) against MAX_MEMORIES limit
- [ ] Use tags/collections for organization within memories
- [ ] Document memory naming conventions for team

## Future Features (Not Yet Implemented)

- **Federated search:** Cross-memory search in single query
- **Cross-memory links:** Reference notes across schema boundaries
- **Memory templates:** Pre-configured memories with tags/collections
- **Auto-archival:** Scheduled cloning to time-based archives
- **Memory-level permissions:** Fine-grained access control per memory

## Summary

**Default recommendation:** Use single memory with tags/collections unless you have:
1. Legal/regulatory data isolation requirements (multi-tenant, client separation)
2. >50k notes with clear segmentation boundaries (projects, time periods)
3. Need for different embedding models per knowledge domain

**Key principle:** Memories are heavyweight. Over-segmentation reduces discoverability. When in doubt, use tags.

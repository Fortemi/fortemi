# MCP Integration Test Suite

Comprehensive integration test suite for validating ALL Fortémi MCP functionality. Designed for agentic execution with supporting management scripts.

## Overview

This test suite validates:
- **70+ MCP tools** across 12 feature categories
- **End-to-end workflows** including async job processing
- **Data isolation** via strict filtering
- **Security features** including PKE encryption
- **Backup/restore** with sharding and snapshots

## Test Environment

### Prerequisites
- MCP server connected and functional
- PostgreSQL with pgvector extension
- Ollama service running for embeddings
- Sufficient disk space for backup tests

### Management Scripts
- `scripts/mcp-test-setup.sh` - Creates test data fixtures
- `scripts/mcp-test-cleanup.sh` - Removes all test artifacts

---

## Quick Smoke Test (5 minutes)

Run these 10 tests to verify basic MCP connectivity:

| # | Test | Tool | Validation |
|---|------|------|------------|
| 1 | List notes | `list_notes` | Returns `{notes: [], total: N}` |
| 2 | Search (hybrid) | `search_notes` | Returns `{results: [], query: "..."}` |
| 3 | List tags | `list_tags` | Returns array |
| 4 | Queue stats | `get_queue_stats` | Returns `{pending, processing, ...}` |
| 5 | List concept schemes | `list_concept_schemes` | Contains default scheme |
| 6 | List embedding sets | `list_embedding_sets` | Contains default set |
| 7 | Backup status | `backup_status` | Returns status object |
| 8 | Memory info | `memory_info` | Returns summary and storage |
| 9 | List collections | `list_collections` | Returns array |
| 10 | Get documentation | `get_documentation` | Returns content |

---

## Full Integration Test Suite

### Category 1: Note Operations (15 tests)

#### 1.1 Create Note - Full Revision
```
Tool: create_note
Input: {
  content: "# Integration Test Note\n\nThis tests the full AI revision pipeline with contextual enhancement.",
  tags: ["mcp-test", "integration"],
  revision_mode: "full"
}
Validation:
- Returns {id: "uuid"}
- Wait for jobs: ai_revision, embedding, title_generation, linking
- get_note returns both original and revised content
- revised content may differ from original
```

#### 1.2 Create Note - Light Revision
```
Tool: create_note
Input: {
  content: "Quick fact: The test was run at [timestamp]",
  tags: ["mcp-test", "light"],
  revision_mode: "light"
}
Validation:
- revised content preserves original meaning
- No invented details added
```

#### 1.3 Create Note - No Revision
```
Tool: create_note
Input: {
  content: "EXACT: This content must not be modified",
  tags: ["mcp-test", "raw"],
  revision_mode: "none"
}
Validation:
- original_content === revised_content
```

#### 1.4 Bulk Create Notes
```
Tool: bulk_create_notes
Input: {
  notes: [
    {content: "Bulk note 1", tags: ["mcp-test", "bulk"], revision_mode: "none"},
    {content: "Bulk note 2", tags: ["mcp-test", "bulk"], revision_mode: "none"},
    {content: "Bulk note 3", tags: ["mcp-test", "bulk"], revision_mode: "none"}
  ]
}
Validation:
- Returns {ids: [...], count: 3}
- All notes retrievable via get_note
```

#### 1.5 Get Note
```
Tool: get_note
Input: {id: "[note_id]"}
Validation:
- Returns complete note object with:
  - note: {id, title, format, source, starred, archived, ...}
  - original: {content, hash, user_created_at}
  - revised: {content, last_revision_id, ai_metadata}
  - tags: [...]
  - links: [...]
```

#### 1.6 Update Note Content
```
Tool: update_note
Input: {id: "[note_id]", content: "Updated content - [timestamp]"}
Validation:
- Triggers new revision pipeline
- Version history increases
- Original content preserved in history
```

#### 1.7 Update Note Status - Star
```
Tool: update_note
Input: {id: "[note_id]", starred: true}
Validation:
- get_note shows starred: true
- list_notes with filter: "starred" includes note
```

#### 1.8 Update Note Status - Archive
```
Tool: update_note
Input: {id: "[note_id]", archived: true}
Validation:
- get_note shows archived: true
- Default list_notes excludes archived
- list_notes with filter: "archived" includes note
```

#### 1.9 Set Note Tags
```
Tool: set_note_tags
Input: {id: "[note_id]", tags: ["mcp-test", "updated", "new-tag"]}
Validation:
- get_note shows new tags array
- Old tags removed, new tags present
```

#### 1.10 Export Note
```
Tool: export_note
Input: {id: "[note_id]", include_frontmatter: true, content: "revised"}
Validation:
- Returns markdown with YAML frontmatter
- Frontmatter contains id, title, tags, dates
```

#### 1.11 List Notes with Pagination
```
Tool: list_notes
Input: {limit: 2, offset: 0}
Then: {limit: 2, offset: 2}
Validation:
- First call returns first 2 notes
- Second call returns next 2 notes
- Total count consistent
```

#### 1.12 List Notes with Filters
```
Tool: list_notes
Input: {filter: "starred"}
Then: {filter: "archived"}
Then: {tags: ["mcp-test"]}
Validation:
- Each filter returns appropriate subset
```

#### 1.13 Delete Note (Soft)
```
Tool: delete_note
Input: {id: "[note_id]"}
Validation:
- Returns success
- Note no longer in default list_notes
- Note can be restored (if implemented)
```

#### 1.14 Purge Note (Hard Delete)
```
Tool: purge_note
Input: {id: "[note_id]"}
Validation:
- Returns {job_id, status: "queued"}
- After job completes, get_note returns not found
```

#### 1.15 Purge Multiple Notes
```
Tool: purge_notes
Input: {note_ids: ["[id1]", "[id2]"]}
Validation:
- Returns summary of queued/failed
- All specified notes removed
```

---

### Category 2: Search Operations (10 tests)

#### 2.1 Hybrid Search (Default)
```
Tool: search_notes
Input: {query: "integration test", limit: 10}
Validation:
- Returns {results: [...], query: "...", total: N}
- Results have: note_id, score, snippet, title, tags
```

#### 2.2 Full-Text Search
```
Tool: search_notes
Input: {query: "EXACT", mode: "fts", limit: 10}
Validation:
- Only returns notes containing literal "EXACT"
- Keyword matching, not semantic
```

#### 2.3 Semantic Search
```
Tool: search_notes
Input: {query: "testing software quality", mode: "semantic", limit: 10}
Validation:
- Returns conceptually related results
- May include notes without exact keywords
```

#### 2.4 Search with Embedding Set
```
Tool: search_notes
Input: {query: "test", mode: "semantic", set: "mcp-test-set", limit: 10}
Validation:
- Only returns notes in specified set
- Respects set boundaries
```

#### 2.5 Strict Search - Required Tags (AND)
```
Tool: search_notes_strict
Input: {
  query: "test",
  required_tags: ["mcp-test", "integration"]
}
Validation:
- ALL results have BOTH tags
- Guaranteed isolation
```

#### 2.6 Strict Search - Any Tags (OR)
```
Tool: search_notes_strict
Input: {
  query: "test",
  any_tags: ["bulk", "light"]
}
Validation:
- Results have AT LEAST ONE of the tags
```

#### 2.7 Strict Search - Excluded Tags
```
Tool: search_notes_strict
Input: {
  query: "test",
  required_tags: ["mcp-test"],
  excluded_tags: ["archived-test"]
}
Validation:
- No results have excluded tags
```

#### 2.8 Strict Search - Required Schemes
```
Tool: search_notes_strict
Input: {
  required_schemes: ["mcp-test-scheme"]
}
Validation:
- Only returns notes with concepts from specified scheme
```

#### 2.9 Search with Deduplication
```
Tool: search_with_dedup
Input: {query: "chunk", limit: 10}
Validation:
- Chunked documents appear once
- chain_info shows chunks_matched, total_chunks
```

#### 2.10 Search Empty Results
```
Tool: search_notes
Input: {query: "xyznonexistent123", limit: 10}
Validation:
- Returns {results: [], total: 0}
- No errors
```

---

### Category 3: Collection Operations (8 tests)

#### 3.1 Create Root Collection
```
Tool: create_collection
Input: {name: "MCP Test Root", description: "Root collection for MCP tests"}
Validation:
- Returns {id: "uuid"}
- Appears in list_collections
```

#### 3.2 Create Nested Collection
```
Tool: create_collection
Input: {name: "MCP Test Child", parent_id: "[root_id]", description: "Nested collection"}
Validation:
- Returns {id: "uuid"}
- list_collections with parent_id shows child
```

#### 3.3 Get Collection
```
Tool: get_collection
Input: {id: "[collection_id]"}
Validation:
- Returns full collection details
- Includes parent_id, note_count
```

#### 3.4 List Root Collections
```
Tool: list_collections
Input: {}
Validation:
- Returns root-level collections
- Does not include nested collections
```

#### 3.5 List Child Collections
```
Tool: list_collections
Input: {parent_id: "[root_id]"}
Validation:
- Returns only children of specified parent
```

#### 3.6 Move Note to Collection
```
Tool: move_note_to_collection
Input: {note_id: "[note_id]", collection_id: "[collection_id]"}
Validation:
- get_note shows new collection_id
- get_collection_notes includes note
```

#### 3.7 Get Collection Notes
```
Tool: get_collection_notes
Input: {id: "[collection_id]", limit: 10}
Validation:
- Returns notes in collection
- Supports pagination
```

#### 3.8 Delete Collection
```
Tool: delete_collection
Input: {id: "[collection_id]"}
Validation:
- Returns success
- Notes moved to uncategorized
- Child collections moved to root
```

---

### Category 4: Template Operations (6 tests)

#### 4.1 Create Template
```
Tool: create_template
Input: {
  name: "MCP Test Template",
  content: "# {{title}}\n\nDate: {{date}}\nAuthor: {{author}}\n\n## Content\n{{body}}",
  description: "Test template with variables",
  default_tags: ["mcp-test", "from-template"]
}
Validation:
- Returns {id: "uuid"}
- Appears in list_templates
```

#### 4.2 Get Template
```
Tool: get_template
Input: {id: "[template_id]"}
Validation:
- Returns full template with content
- Shows default_tags, description
```

#### 4.3 List Templates
```
Tool: list_templates
Input: {}
Validation:
- Returns all templates
- Includes test template
```

#### 4.4 Instantiate Template - All Variables
```
Tool: instantiate_template
Input: {
  id: "[template_id]",
  variables: {
    title: "Generated Note",
    date: "2026-01-29",
    author: "MCP Test",
    body: "This note was generated from a template."
  },
  revision_mode: "none"
}
Validation:
- Creates note with substituted content
- Default tags applied
- Variables replaced correctly
```

#### 4.5 Instantiate Template - Partial Variables
```
Tool: instantiate_template
Input: {
  id: "[template_id]",
  variables: {title: "Partial Test"}
}
Validation:
- Creates note
- Unsubstituted variables remain as {{var}}
```

#### 4.6 Delete Template
```
Tool: delete_template
Input: {id: "[template_id]"}
Validation:
- Returns success
- No longer in list_templates
```

---

### Category 5: SKOS Taxonomy Operations (18 tests)

#### 5.1 List Concept Schemes
```
Tool: list_concept_schemes
Input: {}
Validation:
- Returns array of schemes
- Contains default system scheme (is_system: true)
```

#### 5.2 Create Concept Scheme
```
Tool: create_concept_scheme
Input: {
  notation: "mcp-test-scheme",
  title: "MCP Test Scheme",
  description: "Scheme for MCP integration tests"
}
Validation:
- Returns {id: "uuid"}
- Appears in list_concept_schemes
```

#### 5.3 Get Concept Scheme
```
Tool: get_concept_scheme
Input: {id: "[scheme_id]"}
Validation:
- Returns full scheme details
- Shows concept_count
```

#### 5.4 Create Root Concept
```
Tool: create_concept
Input: {
  scheme_id: "[mcp_scheme_id]",
  pref_label: "MCP Testing",
  definition: "Root concept for MCP integration tests",
  scope_note: "Use for all MCP test-related content"
}
Validation:
- Returns {id: "uuid"}
- No broader relations
```

#### 5.5 Create Child Concept (Broader)
```
Tool: create_concept
Input: {
  scheme_id: "[mcp_scheme_id]",
  pref_label: "Unit Tests",
  broader_ids: ["[root_concept_id]"]
}
Validation:
- Returns {id: "uuid"}
- Has broader relation to parent
```

#### 5.6 Add Narrower Relation
```
Tool: add_narrower
Input: {id: "[parent_id]", target_id: "[child_id]"}
Validation:
- get_narrower shows child
- Inverse of broader
```

#### 5.7 Get Broader Concepts
```
Tool: get_broader
Input: {id: "[child_concept_id]"}
Validation:
- Returns parent concept(s)
- Max 3 parents (polyhierarchy limit)
```

#### 5.8 Get Narrower Concepts
```
Tool: get_narrower
Input: {id: "[parent_concept_id]"}
Validation:
- Returns child concepts
```

#### 5.9 Add Related Concept
```
Tool: add_related
Input: {id: "[concept_a]", target_id: "[concept_b]"}
Validation:
- get_related shows association
- Symmetric relationship
```

#### 5.10 Get Related Concepts
```
Tool: get_related
Input: {id: "[concept_id]"}
Validation:
- Returns non-hierarchical associations
```

#### 5.11 Search Concepts
```
Tool: search_concepts
Input: {q: "MCP"}
Validation:
- Returns matching concepts
- Searches pref/alt/hidden labels
```

#### 5.12 Autocomplete Concepts
```
Tool: autocomplete_concepts
Input: {q: "MC", limit: 5}
Validation:
- Returns prefix-matched concepts
- Fast for UI autocomplete
```

#### 5.13 Get Concept Full
```
Tool: get_concept_full
Input: {id: "[concept_id]"}
Validation:
- Returns concept with ALL relationships
- Includes labels, notes, broader, narrower, related
```

#### 5.14 Update Concept
```
Tool: update_concept
Input: {id: "[concept_id]", status: "approved"}
Validation:
- Status updated
- Can change notation, facet_type
```

#### 5.15 Tag Note with Concept
```
Tool: tag_note_concept
Input: {note_id: "[note_id]", concept_id: "[concept_id]", is_primary: true}
Validation:
- get_note_concepts shows concept
- Primary flag set
```

#### 5.16 Get Note Concepts
```
Tool: get_note_concepts
Input: {note_id: "[note_id]"}
Validation:
- Returns concepts tagged on note
- Shows is_primary
```

#### 5.17 Untag Note Concept
```
Tool: untag_note_concept
Input: {note_id: "[note_id]", concept_id: "[concept_id]"}
Validation:
- Concept removed from note
- get_note_concepts no longer includes it
```

#### 5.18 Get Governance Stats
```
Tool: get_governance_stats
Input: {scheme_id: "[mcp_scheme_id]"}
Validation:
- Returns taxonomy health metrics
- total_concepts, candidates, approved, deprecated
- orphans, under_used, avg_note_count
```

---

### Category 6: Embedding Set Operations (8 tests)

#### 6.1 List Embedding Sets
```
Tool: list_embedding_sets
Input: {}
Validation:
- Contains default system set
- Shows document_count, embedding_count, index_status
```

#### 6.2 Create Embedding Set (Auto Mode)
```
Tool: create_embedding_set
Input: {
  name: "MCP Test Set",
  slug: "mcp-test-set",
  description: "Embedding set for MCP tests",
  purpose: "Isolate MCP test notes for focused search",
  mode: "auto",
  criteria: {tags: ["mcp-test"]}
}
Validation:
- Returns {id: "uuid"}
- Mode is "auto"
```

#### 6.3 Create Embedding Set (Manual Mode)
```
Tool: create_embedding_set
Input: {
  name: "MCP Manual Set",
  slug: "mcp-manual-set",
  mode: "manual"
}
Validation:
- Returns {id: "uuid"}
- Starts empty
```

#### 6.4 Get Embedding Set
```
Tool: get_embedding_set
Input: {slug: "mcp-test-set"}
Validation:
- Returns full set details
- Shows criteria, model, dimension
```

#### 6.5 Add Set Members (Manual)
```
Tool: add_set_members
Input: {slug: "mcp-manual-set", note_ids: ["[note_id1]", "[note_id2]"]}
Validation:
- Notes added to set
- list_set_members shows notes
```

#### 6.6 List Set Members
```
Tool: list_set_members
Input: {slug: "mcp-test-set", limit: 10}
Validation:
- Returns notes matching criteria
- Supports pagination
```

#### 6.7 Refresh Embedding Set
```
Tool: refresh_embedding_set
Input: {slug: "mcp-test-set"}
Validation:
- Re-evaluates criteria
- Updates membership
```

#### 6.8 Remove Set Member
```
Tool: remove_set_member
Input: {slug: "mcp-manual-set", note_id: "[note_id]"}
Validation:
- Note removed from set
- Not in list_set_members
```

---

### Category 7: Version Management (6 tests)

#### 7.1 List Note Versions
```
Tool: list_note_versions
Input: {note_id: "[note_id]"}
Validation:
- Returns original_versions and revised_versions arrays
- Shows current version numbers
```

#### 7.2 Get Original Version
```
Tool: get_note_version
Input: {note_id: "[note_id]", version: 1, track: "original"}
Validation:
- Returns original content at version 1
- Includes hash, created_at
```

#### 7.3 Get Revised Version
```
Tool: get_note_version
Input: {note_id: "[note_id]", version: 1, track: "revision"}
Validation:
- Returns AI-enhanced content
- Shows model, ai_metadata
```

#### 7.4 Diff Versions
```
Tool: diff_note_versions
Input: {note_id: "[note_id]", from_version: 1, to_version: 2}
Validation:
- Returns unified diff format
- Shows additions/removals
```

#### 7.5 Restore Version
```
Tool: restore_note_version
Input: {note_id: "[note_id]", version: 1, restore_tags: false}
Validation:
- Creates new version with old content
- Version count increases
```

#### 7.6 Delete Version
```
Tool: delete_note_version
Input: {note_id: "[note_id]", version: 1}
Validation:
- Version removed from history
- Cannot delete current version
```

---

### Category 8: Link and Graph Operations (4 tests)

#### 8.1 Get Note Links
```
Tool: get_note_links
Input: {id: "[note_id]"}
Validation:
- Returns outgoing and incoming arrays
- Each link has: id, from_note_id, to_note_id, kind, score
```

#### 8.2 Explore Graph (Depth 1)
```
Tool: explore_graph
Input: {id: "[note_id]", depth: 1, max_nodes: 20}
Validation:
- Returns {nodes: [...], edges: [...]}
- Nodes have id, title, depth
- Edges have from/to, score
```

#### 8.3 Explore Graph (Depth 2)
```
Tool: explore_graph
Input: {id: "[note_id]", depth: 2, max_nodes: 50}
Validation:
- Includes second-degree connections
- Respects max_nodes limit
```

#### 8.4 Get Full Document (Chunked)
```
Tool: get_full_document
Input: {id: "[chunked_note_id]"}
Validation:
- Reconstructs full document from chunks
- Shows chunk metadata if chunked
```

---

### Category 9: Job Queue Operations (4 tests)

#### 9.1 Get Queue Stats
```
Tool: get_queue_stats
Input: {}
Validation:
- Returns {pending, processing, completed_last_hour, failed_last_hour, total}
```

#### 9.2 List All Jobs
```
Tool: list_jobs
Input: {limit: 20}
Validation:
- Returns recent jobs
- Shows status, job_type, note_id
```

#### 9.3 List Jobs by Status
```
Tool: list_jobs
Input: {status: "completed", limit: 10}
Validation:
- Only returns completed jobs
```

#### 9.4 List Jobs by Type
```
Tool: list_jobs
Input: {job_type: "embedding", limit: 10}
Validation:
- Only returns embedding jobs
```

---

### Category 10: Backup Operations (12 tests)

#### 10.1 Backup Status
```
Tool: backup_status
Input: {}
Validation:
- Returns backup health info
- Shows backup_count, latest_backup
```

#### 10.2 List Backups
```
Tool: list_backups
Input: {}
Validation:
- Returns available backup files
- Shows filename, size, modified, sha256
```

#### 10.3 Memory Info
```
Tool: memory_info
Input: {}
Validation:
- Returns storage summary
- Shows database_total_bytes, embedding_table_bytes
- Includes recommendations
```

#### 10.4 Export All Notes (JSON)
```
Tool: export_all_notes
Input: {}
Validation:
- Returns {manifest, notes, collections, tags, templates}
- Portable JSON format
```

#### 10.5 Export Filtered Notes
```
Tool: export_all_notes
Input: {filter: {tags: ["mcp-test"], starred_only: false}}
Validation:
- Only exports matching notes
```

#### 10.6 Create Knowledge Shard
```
Tool: knowledge_shard
Input: {include: ["notes", "collections", "tags", "templates", "links"]}
Validation:
- Returns {filename, size_bytes, base64_data}
- tar.gz format
```

#### 10.7 Create Knowledge Shard with Embeddings
```
Tool: knowledge_shard
Input: {include: "all"}
Validation:
- Includes embeddings (larger file)
- Complete restore capability
```

#### 10.8 Import Knowledge Shard (Dry Run)
```
Tool: knowledge_shard_import
Input: {shard_base64: "[data]", dry_run: true, on_conflict: "skip"}
Validation:
- Reports what would be imported
- No actual changes
```

#### 10.9 Database Snapshot
```
Tool: database_snapshot
Input: {name: "mcp-test", title: "MCP Test Snapshot", description: "Created during integration tests"}
Validation:
- Creates pg_dump backup
- Returns filename, path, size
```

#### 10.10 Get Backup Info
```
Tool: get_backup_info
Input: {filename: "[backup_filename]"}
Validation:
- Returns file details
- Shows sha256 hash
```

#### 10.11 Get Backup Metadata
```
Tool: get_backup_metadata
Input: {filename: "[backup_filename]"}
Validation:
- Returns human-readable metadata
- Shows title, description, note_count
```

#### 10.12 Update Backup Metadata
```
Tool: update_backup_metadata
Input: {filename: "[backup_filename]", title: "Updated Title"}
Validation:
- Metadata updated
- get_backup_metadata shows new title
```

---

### Category 11: PKE Encryption (6 tests)

#### 11.1 Generate Keypair
```
Tool: pke_generate_keypair
Input: {passphrase: "test-passphrase-12chars", label: "MCP Test Key"}
Validation:
- Returns public_key_path, private_key_path, address
- Address format: mm:...
```

#### 11.2 Get Address from Public Key
```
Tool: pke_get_address
Input: {public_key_path: "[path]"}
Validation:
- Returns mm:... address
- Matches generated address
```

#### 11.3 Verify Address
```
Tool: pke_verify_address
Input: {address: "[mm_address]"}
Validation:
- Returns {valid: true, version: ...}
- Checksum verification passes
```

#### 11.4 Encrypt File
```
Tool: pke_encrypt
Input: {
  input_path: "[test_file]",
  output_path: "[encrypted_file]",
  recipients: ["[public_key_path]"]
}
Validation:
- Creates encrypted file
- MMPKE01 format
```

#### 11.5 List Recipients
```
Tool: pke_list_recipients
Input: {input_path: "[encrypted_file]"}
Validation:
- Returns array of mm:... addresses
- Shows who can decrypt
```

#### 11.6 Decrypt File
```
Tool: pke_decrypt
Input: {
  input_path: "[encrypted_file]",
  output_path: "[decrypted_file]",
  private_key_path: "[private_key]",
  passphrase: "test-passphrase-12chars"
}
Validation:
- Decrypted content matches original
- Metadata preserved
```

---

### Category 12: Documentation (3 tests)

#### 12.1 Get Overview
```
Tool: get_documentation
Input: {topic: "overview"}
Validation:
- Returns system overview
- Includes quick start
```

#### 12.2 Get Specific Topic
```
Tool: get_documentation
Input: {topic: "search"}
Validation:
- Returns search documentation
- Includes modes, tips
```

#### 12.3 Get All Documentation
```
Tool: get_documentation
Input: {topic: "all"}
Validation:
- Returns complete documentation
- All topics included
```

---

## Test Data Requirements

The test suite requires specific test data created by `scripts/mcp-test-setup.sh`:

### Notes (created in order)
1. `mcp-test-full-revision` - Full AI revision test
2. `mcp-test-light-revision` - Light revision test
3. `mcp-test-raw` - No revision test
4. `mcp-test-bulk-1`, `mcp-test-bulk-2`, `mcp-test-bulk-3` - Bulk create test
5. `mcp-test-chunked` - Large document for chunk testing

### Collections
1. `mcp-test-root` - Root collection
2. `mcp-test-child` - Nested collection

### Templates
1. `mcp-test-template` - Template with variables

### SKOS
1. `mcp-test-scheme` - Test concept scheme
2. `mcp-test-root-concept` - Root concept
3. `mcp-test-child-concept` - Child concept
4. `mcp-test-related-concept` - Related concept

### Embedding Sets
1. `mcp-test-set` - Auto mode set
2. `mcp-manual-set` - Manual mode set

### PKE
1. Test keypair in scratchpad directory
2. Test file for encryption

---

## Cleanup Procedure

After testing, run `scripts/mcp-test-cleanup.sh` or execute manually:

1. Purge all notes with tag `mcp-test`
2. Delete test collections
3. Delete test templates
4. Delete test concepts (requires removing from notes first)
5. Delete test concept scheme
6. Delete test embedding sets
7. Remove PKE test files
8. Remove test backups

---

## Agentic Execution Guide

### Running Full Suite

```
Execute the full MCP integration test suite per docs/content/mcp-validation-test-plan.md:

1. Run scripts/mcp-test-setup.sh to create test data
2. Execute all 12 categories of tests
3. Track pass/fail for each test
4. Run scripts/mcp-test-cleanup.sh
5. Report results summary
```

### Running Single Category

```
Run MCP integration tests for Category 5 (SKOS) only.
Use existing test data or create minimal required fixtures.
```

### Debugging Failures

```
Test X.Y failed with error: [error]
Investigate the failure and suggest fixes.
```

---

## Success Criteria

| Level | Requirements |
|-------|--------------|
| **Smoke** | All 10 quick tests pass |
| **Basic** | Categories 1-4 pass (Notes, Search, Collections, Templates) |
| **Full** | All 12 categories pass |
| **Complete** | All tests pass + cleanup successful |

---

## Related Documentation

- [Production Test Plan](production-test-plan.md) - Infrastructure validation
- [Operations Guide](operations.md) - Service management
- [MCP Server README](../../mcp-server/README.md) - MCP configuration

# MCP Validation Test Plan

This document provides a comprehensive test suite for validating Matric Memory MCP (Model Context Protocol) functionality agentically. Claude can execute these tests directly using the available MCP tools.

## Purpose

Validate that the MCP server is functioning correctly by testing all major tool categories through direct tool invocation. This is designed for agentic validation during sessions.

## Prerequisites

- MCP server connected and functional
- Database accessible with valid data
- Ollama service running (for embedding operations)

---

## Quick Validation (5 tests)

Minimal tests to confirm basic MCP functionality:

| # | Test | Tool | Expected |
|---|------|------|----------|
| 1 | List notes | `mcp__matric-memory__list_notes` | Returns notes array |
| 2 | Search notes | `mcp__matric-memory__search_notes` | Returns results array |
| 3 | List tags | `mcp__matric-memory__list_tags` | Returns tags with counts |
| 4 | Queue stats | `mcp__matric-memory__get_queue_stats` | Returns queue health |
| 5 | Get documentation | `mcp__matric-memory__get_documentation` | Returns overview |

---

## Full Validation Suite

### 1. Note Operations

#### 1.1 Create Note
```
Tool: mcp__matric-memory__create_note
Input: { content: "MCP validation test note - created at [timestamp]", tags: ["test", "mcp-validation"] }
Expected: Returns note with id, triggers AI enhancement pipeline
```

#### 1.2 Get Note
```
Tool: mcp__matric-memory__get_note
Input: { id: "[note_id from 1.1]" }
Expected: Returns full note with original_content, revised_content, title, tags
```

#### 1.3 Update Note
```
Tool: mcp__matric-memory__update_note
Input: { id: "[note_id]", content: "Updated content - [timestamp]" }
Expected: Returns success, triggers pipeline re-run
```

#### 1.4 List Notes
```
Tool: mcp__matric-memory__list_notes
Input: { limit: 5 }
Expected: Returns notes array with summaries
```

#### 1.5 Set Note Tags
```
Tool: mcp__matric-memory__set_note_tags
Input: { id: "[note_id]", tags: ["test", "mcp-validation", "updated"] }
Expected: Returns success
```

#### 1.6 Delete Note (soft)
```
Tool: mcp__matric-memory__delete_note
Input: { id: "[note_id]" }
Expected: Returns success, note is soft-deleted
```

### 2. Search Operations

#### 2.1 Hybrid Search (default)
```
Tool: mcp__matric-memory__search_notes
Input: { query: "knowledge", limit: 5 }
Expected: Returns results with note_id, score, snippet, title
```

#### 2.2 Full-Text Search
```
Tool: mcp__matric-memory__search_notes
Input: { query: "test", mode: "fts", limit: 5 }
Expected: Returns FTS-only results
```

#### 2.3 Semantic Search
```
Tool: mcp__matric-memory__search_notes
Input: { query: "learning concepts", mode: "semantic", limit: 5 }
Expected: Returns semantically similar results
```

#### 2.4 Strict Search
```
Tool: mcp__matric-memory__search_notes_strict
Input: { query: "test", required_tags: ["test"] }
Expected: Results guaranteed to have specified tags
```

#### 2.5 Deduplicated Search
```
Tool: mcp__matric-memory__search_with_dedup
Input: { query: "documentation", limit: 10 }
Expected: Returns deduplicated results with chain metadata
```

### 3. Collection Operations

#### 3.1 List Collections
```
Tool: mcp__matric-memory__list_collections
Input: {}
Expected: Returns collections array
```

#### 3.2 Create Collection
```
Tool: mcp__matric-memory__create_collection
Input: { name: "MCP Test Collection", description: "Created during MCP validation" }
Expected: Returns collection with id
```

#### 3.3 Get Collection
```
Tool: mcp__matric-memory__get_collection
Input: { id: "[collection_id]" }
Expected: Returns collection details
```

#### 3.4 Get Collection Notes
```
Tool: mcp__matric-memory__get_collection_notes
Input: { id: "[collection_id]" }
Expected: Returns notes in collection
```

#### 3.5 Delete Collection
```
Tool: mcp__matric-memory__delete_collection
Input: { id: "[collection_id]" }
Expected: Returns success, notes moved to uncategorized
```

### 4. Template Operations

#### 4.1 List Templates
```
Tool: mcp__matric-memory__list_templates
Input: {}
Expected: Returns templates array
```

#### 4.2 Create Template
```
Tool: mcp__matric-memory__create_template
Input: { name: "MCP Test Template", content: "# {{title}}\n\nCreated: {{date}}\n\n{{content}}", description: "Test template" }
Expected: Returns template with id
```

#### 4.3 Get Template
```
Tool: mcp__matric-memory__get_template
Input: { id: "[template_id]" }
Expected: Returns template with content and placeholders
```

#### 4.4 Instantiate Template
```
Tool: mcp__matric-memory__instantiate_template
Input: { id: "[template_id]", variables: { "title": "Test Note", "date": "2026-01-29", "content": "Test content" } }
Expected: Creates note from template with substitutions
```

#### 4.5 Delete Template
```
Tool: mcp__matric-memory__delete_template
Input: { id: "[template_id]" }
Expected: Returns success
```

### 5. SKOS Taxonomy Operations

#### 5.1 List Concept Schemes
```
Tool: mcp__matric-memory__list_concept_schemes
Input: {}
Expected: Returns schemes including default system scheme
```

#### 5.2 Search Concepts
```
Tool: mcp__matric-memory__search_concepts
Input: { q: "test" }
Expected: Returns matching concepts
```

#### 5.3 Autocomplete Concepts
```
Tool: mcp__matric-memory__autocomplete_concepts
Input: { q: "te", limit: 5 }
Expected: Returns concept suggestions
```

#### 5.4 Get Governance Stats
```
Tool: mcp__matric-memory__get_governance_stats
Input: {}
Expected: Returns taxonomy health metrics
```

#### 5.5 Create Concept
```
Tool: mcp__matric-memory__create_concept
Input: { scheme_id: "[default_scheme_id]", pref_label: "mcp-test-concept" }
Expected: Returns concept with id
```

#### 5.6 Get Concept Full
```
Tool: mcp__matric-memory__get_concept_full
Input: { id: "[concept_id]" }
Expected: Returns concept with all relationships
```

### 6. Embedding Set Operations

#### 6.1 List Embedding Sets
```
Tool: mcp__matric-memory__list_embedding_sets
Input: {}
Expected: Returns sets including 'default' system set
```

#### 6.2 Get Embedding Set
```
Tool: mcp__matric-memory__get_embedding_set
Input: { slug: "default" }
Expected: Returns set details with criteria
```

#### 6.3 List Set Members
```
Tool: mcp__matric-memory__list_set_members
Input: { slug: "default", limit: 5 }
Expected: Returns notes in the set
```

### 7. Version Management

#### 7.1 List Note Versions
```
Tool: mcp__matric-memory__list_note_versions
Input: { note_id: "[any_note_id]" }
Expected: Returns version history
```

#### 7.2 Get Note Version
```
Tool: mcp__matric-memory__get_note_version
Input: { note_id: "[note_id]", version: 1, track: "original" }
Expected: Returns specific version content
```

### 8. Link and Graph Operations

#### 8.1 Get Note Links
```
Tool: mcp__matric-memory__get_note_links
Input: { id: "[note_id_with_links]" }
Expected: Returns outgoing and incoming links
```

#### 8.2 Explore Graph
```
Tool: mcp__matric-memory__explore_graph
Input: { id: "[note_id]", depth: 2, max_nodes: 10 }
Expected: Returns graph with nodes and edges
```

### 9. Job Queue Operations

#### 9.1 Get Queue Stats
```
Tool: mcp__matric-memory__get_queue_stats
Input: {}
Expected: Returns pending, processing, completed_last_hour, failed_last_hour
```

#### 9.2 List Jobs
```
Tool: mcp__matric-memory__list_jobs
Input: { limit: 10 }
Expected: Returns recent jobs with status
```

### 10. Backup Operations

#### 10.1 Backup Status
```
Tool: mcp__matric-memory__backup_status
Input: {}
Expected: Returns backup health status
```

#### 10.2 List Backups
```
Tool: mcp__matric-memory__list_backups
Input: {}
Expected: Returns available backup files
```

#### 10.3 Memory Info
```
Tool: mcp__matric-memory__memory_info
Input: {}
Expected: Returns storage sizing and recommendations
```

### 11. Documentation

#### 11.1 Get Overview Documentation
```
Tool: mcp__matric-memory__get_documentation
Input: { topic: "overview" }
Expected: Returns system overview
```

#### 11.2 Get Search Documentation
```
Tool: mcp__matric-memory__get_documentation
Input: { topic: "search" }
Expected: Returns search documentation
```

---

## Cleanup Procedure

After running full validation, clean up test artifacts:

1. Delete any test notes created (use `purge_note` for permanent deletion)
2. Delete test collections
3. Delete test templates
4. Delete test concepts

---

## Agentic Execution Instructions

When running this validation agentically:

1. **Start with Quick Validation** - Run the 5 quick tests first
2. **Track results** - Use TodoWrite to track pass/fail for each test
3. **Create test data** - Create test notes/collections/templates as needed
4. **Run category tests** - Execute each category in order
5. **Clean up** - Delete test artifacts after validation
6. **Report results** - Summarize pass/fail counts

### Example Agentic Command

```
Run the MCP validation test suite per docs/content/mcp-validation-test-plan.md.
Execute quick validation first, then full validation. Report results.
```

---

## Success Criteria

- **Quick Validation**: All 5 tests pass
- **Full Validation**: All test categories return expected results
- **No Errors**: No unexpected errors or exceptions
- **Cleanup Complete**: All test artifacts removed

---

## Troubleshooting

| Issue | Possible Cause | Resolution |
|-------|----------------|------------|
| MCP tools not available | MCP server not connected | Verify MCP connection |
| Search returns empty | No indexed content | Create notes, wait for embedding jobs |
| Embedding operations fail | Ollama not running | Start Ollama service |
| Database errors | Connection issue | Check PostgreSQL status |

---

## Related Documentation

- [Production Test Plan](production-test-plan.md) - Infrastructure validation
- [Operations Guide](operations.md) - Service management
- [MCP Server README](../../mcp-server/README.md) - MCP configuration

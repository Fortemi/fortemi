# MCP Tool Permissions System

This document describes the MCP tool permission classification system used by the Fortémi MCP server. Proper annotation of tools enables Claude Code and other AI agents to work effectively in headless/automated modes.

## Background

Issue #223 identified that most MCP tools were being auto-denied in Claude Code's headless/automated sessions because they lacked proper annotations. The MCP specification defines annotation hints that permission systems use to determine tool safety.

## MCP Annotation Specification

```typescript
annotations: {
  readOnlyHint?: boolean;      // true = doesn't modify environment
  destructiveHint?: boolean;   // true = may cause data loss (permanent)
  idempotentHint?: boolean;    // true = safe to retry with same args
  openWorldHint?: boolean;     // true = interacts with external systems
}
```

### Permission Behavior

| Annotation | Claude Code Behavior |
|------------|---------------------|
| `readOnlyHint: true` | Auto-approved in all modes |
| `destructiveHint: false` | Auto-approved in "acceptEdits" mode |
| `destructiveHint: true` | Always requires explicit approval |
| No annotations | Treated conservatively (may be auto-denied) |

## Tool Classification Tiers

Fortémi exposes 205 tools in full mode. Of these, **197 carry annotations** classifying them into the tiers below based on their data modification characteristics. **8 tools are currently unannotated** (treated conservatively by permission systems until annotations are added):

- `manage_embeddings`
- `manage_attachments`
- `capture_diagnostics_snapshot`
- `pfnet_sparsify`
- `recompute_snn_scores`
- `coarse_community_detection`
- `trigger_graph_maintenance`
- `manage_jobs`

The authoritative source for tier classification is the `annotations` blocks in `mcp-server/tools.js`.

### Tier 1: Read-Only (`readOnlyHint: true`)

**102 tools** (`readOnlyHint: true`) - Auto-approved, no data modification risk.

These tools only read data and never modify state. Safe for any automated workflow.

```javascript
// Example annotation
{
  name: "get_note",
  description: "...",
  inputSchema: { ... },
  annotations: {
    readOnlyHint: true,
  },
}
```

**Tools in this tier:**
- `list_notes`, `get_note`, `search_notes`, `list_tags`, `get_note_links`, `export_note`
- `list_collections`, `get_collection`, `get_collection_notes`, `explore_graph`
- `list_templates`, `get_template`
- `list_embedding_sets`, `get_embedding_set`, `list_set_members`
- `list_embedding_configs`, `get_default_embedding_config`
- `export_all_notes`, `backup_status`, `backup_download`
- `knowledge_archive_download`, `list_backups`, `get_backup_info`, `get_backup_metadata`
- `memory_info`
- `list_concept_schemes`, `get_concept_scheme`, `search_concepts`, `get_concept`, `get_concept_full`
- `autocomplete_concepts`, `get_broader`, `get_narrower`, `get_related`
- `get_note_concepts`, `get_governance_stats`, `get_top_concepts`
- `list_note_versions`, `get_note_version`, `diff_note_versions`
- `get_full_document`, `search_with_dedup`, `get_chunk_chain`, `get_documentation`
- `pke_get_address`, `pke_list_recipients`, `pke_verify_address`
- `pke_list_keysets`, `pke_get_active_keyset`
- `list_jobs`, `get_queue_stats`

### Tier 2: Non-Destructive Write (`destructiveHint: false`)

Creates/modifies data but changes are recoverable or don't cause data loss.

These tools write data but either create new resources (can be deleted) or modify existing resources in ways that preserve history (versioning) or are easily reversible.

> **Counting note:** Tier 2 (non-destructive write) and Tier 3 (soft delete) both carry `destructiveHint: false`. The annotation boolean alone does not distinguish them — together they account for **67 tools** (all `destructiveHint: false`). The Tier 2 vs Tier 3 split below is a semantic sub-classification, not a separately-annotated count. For exact membership, consult the `annotations` blocks in `mcp-server/tools.js`.

```javascript
// Example annotation
{
  name: "create_note",
  description: "...",
  inputSchema: { ... },
  annotations: {
    destructiveHint: false,
    idempotentHint: false,  // creates new resource each time
  },
}
```

**Representative tools in this tier** (full list lives in `mcp-server/tools.js`):
- `create_note`, `bulk_create_notes`, `update_note`, `set_note_tags`
- `create_collection`, `move_note_to_collection`
- `create_embedding_set`, `add_set_members`, `refresh_embedding_set`
- `create_concept`, `update_concept`, `tag_note_concept`
- `pke_generate_keypair`, `pke_encrypt`, `pke_decrypt`

### Tier 3: Soft Delete (`destructiveHint: false`)

Marks as deleted but recoverable.

These tools perform soft deletion - resources are marked as deleted but can be restored. They also carry `destructiveHint: false` and are part of the combined 67-tool `destructiveHint: false` group noted under Tier 2.

```javascript
// Example annotation
{
  name: "delete_note",
  description: "Soft delete a note (can be restored later).",
  inputSchema: { ... },
  annotations: {
    destructiveHint: false,
  },
}
```

**Tools in this tier:**
- `delete_note` (soft delete - recoverable)
- `delete_collection` (notes moved to uncategorized, not deleted)
- `delete_template` (template removed but no data loss)

### Tier 4: Destructive (`destructiveHint: true`)

**27 tools** (`destructiveHint: true`) - Permanent data loss or irreversible state changes. **Always requires explicit approval.**

These tools can cause unrecoverable data loss. Even in automated modes, they should require user confirmation.

```javascript
// Example annotation
{
  name: "purge_note",
  description: "Permanently delete a note and ALL related data...",
  inputSchema: { ... },
  annotations: {
    destructiveHint: true,  // ALWAYS requires approval
  },
}
```

**Representative tools in this tier** (full list lives in `mcp-server/tools.js`):
- `purge_note`, `purge_notes`, `purge_all_notes` - Permanent deletion
- `remove_set_member` - Permanent removal from embedding set
- `delete_concept` - Permanent concept deletion
- `delete_note_version` - Permanent version history removal
- `database_restore` - Overwrites entire database state
- `backup_import` - Can overwrite existing data with imported data
- `knowledge_shard_import` - Imports external data, may conflict
- `pke_delete_keyset` - Permanent key deletion (data encrypted with key becomes unrecoverable)
- `pke_import_keyset` - Imports external keys, security implications

## Adding New Tools

When adding a new MCP tool, follow this process:

1. **Determine the tier** based on the tool's behavior:
   - Does it only read data? → Tier 1 (readOnlyHint: true)
   - Does it create/modify data but changes are recoverable? → Tier 2 (destructiveHint: false)
   - Does it soft-delete data? → Tier 3 (destructiveHint: false)
   - Can it cause permanent data loss? → Tier 4 (destructiveHint: true)

2. **Add the annotations block** after the `inputSchema` in the tool definition:

```javascript
{
  name: "new_tool",
  description: "...",
  inputSchema: { ... },
  annotations: {
    // Choose ONE of these patterns:

    // Tier 1: Read-only
    readOnlyHint: true,

    // Tier 2/3: Non-destructive write or soft delete
    destructiveHint: false,
    idempotentHint: true,  // if safe to retry

    // Tier 4: Destructive
    destructiveHint: true,
  },
}
```

3. **Verify the annotation counts** against `mcp-server/tools.js` (see commands below).

## Verification

Tool definitions and their annotations now live in `mcp-server/tools.js` (they were moved out of `index.js`). As of this writing, **197 of the 205 tools are annotated** (8 are pending — see the list under "Tool Classification Tiers" above).

> **Note:** The legacy verification script `mcp-server/test-verify-annotations.js` is currently broken — it still looks for the tools array in `index.js`, but the tools moved to `tools.js`. The script needs updating for the `tools.js` split (follow-up). Until then, use the manual counts below.

Verify the annotation counts directly against `mcp-server/tools.js`:

```bash
# Tier 1: read-only tools (expect 102)
grep -c '"readOnlyHint":true' mcp-server/tools.js

# Tiers 2+3 combined: non-destructive write + soft delete (expect 67)
grep -c '"destructiveHint":false' mcp-server/tools.js

# Tier 4: destructive tools (expect 27)
grep -c '"destructiveHint":true' mcp-server/tools.js
```

These three flags are mutually exclusive in practice (read-only vs. non-destructive write/soft-delete vs. destructive), so the counts sum to the bulk of the 197 annotated tools; the remaining 8 tools have no annotations block at all.

## References

- [MCP Specification - Tool Annotations](https://spec.modelcontextprotocol.io/specification/server/tools/)
- Issue #223: MCP tools auto-denied in headless mode
- Issue #360: Tool annotation verification
- Issue #345: Agentic operation support

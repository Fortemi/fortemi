# UAT Phase 20: Data Export

**Purpose**: Verify backup, export, and data portability features
**Duration**: ~8 minutes
**Phase Number**: 20 (second-to-last phase)
**Prerequisites**: All phases 0-19 completed
**Tools Tested**: `backup_status`, `backup_now`, `export_all_notes`, `export_note`, `knowledge_shard`, `knowledge_shard_import`, `list_backups`, `get_backup_info`, `get_backup_metadata`, `update_backup_metadata`, `database_snapshot`, `backup_download`, `knowledge_archive_download`, `knowledge_archive_upload`, `database_restore`, `backup_import`, `memory_info`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Backup Status

### BACK-001: Backup Status

**MCP Tool**: `backup_status`

```javascript
backup_status()
```

**Pass Criteria**: Returns status info including last backup time

---

### BACK-002: Trigger Backup

**MCP Tool**: `backup_now`

```javascript
backup_now()
```

**Pass Criteria**: Returns job ID or immediate success

---

## Export Operations

### BACK-003: Export All Notes

**MCP Tool**: `export_all_notes`

```javascript
export_all_notes()
```

**Pass Criteria**: Returns manifest with note count and notes array

---

### BACK-004: Export Single Note

**MCP Tool**: `export_note`

```javascript
export_note({
  id: "<ml_note_id>",
  content: "revised",
  include_frontmatter: true
})
```

**Pass Criteria**: Returns markdown with YAML frontmatter

---

### BACK-005: Export Note - Original Content

**MCP Tool**: `export_note`

```javascript
export_note({
  id: "<note_id>",
  content: "original"
})
```

**Pass Criteria**: Returns original (unrevised) content

---

## Knowledge Shards

### BACK-006: Create Knowledge Shard

**MCP Tool**: `knowledge_shard`

```javascript
knowledge_shard({
  include_embeddings: false,
  tags: ["uat"]
})
```

**Pass Criteria**: Returns shard manifest with UAT notes

---

### BACK-007: Knowledge Shard with Components

**MCP Tool**: `knowledge_shard`

```javascript
knowledge_shard({
  components: ["notes", "concepts", "links"]
})
```

**Pass Criteria**: Shard includes specified components

---

### BACK-008: Import Knowledge Shard

**MCP Tools**: `knowledge_shard`, `knowledge_shard_import`

```javascript
// First export to file
knowledge_shard({
  output_path: "/tmp/uat/shard.tar.gz"
})
// Expected: returns { saved_to: "/tmp/uat/shard.tar.gz", ... }

// Then import from file
knowledge_shard_import({
  file_path: "/tmp/uat/shard.tar.gz",
  merge_strategy: "skip_existing"
})
```

**Pass Criteria**: Notes imported successfully

---

## Backup Browser

### BACK-009: List Backups

**MCP Tool**: `list_backups`

```javascript
list_backups()
```

**Pass Criteria**: Returns array of backup files with metadata

---

### BACK-010: Get Backup Info

**MCP Tool**: `get_backup_info`

```javascript
get_backup_info({ filename: "<backup_filename>" })
```

**Pass Criteria**: Returns detailed backup information

---

### BACK-011: Get Backup Metadata

**MCP Tool**: `get_backup_metadata`

```javascript
get_backup_metadata({ filename: "<backup_filename>" })
```

**Pass Criteria**: Returns user-defined metadata for backup

---

### BACK-012: Update Backup Metadata

**MCP Tool**: `update_backup_metadata`

```javascript
update_backup_metadata({
  filename: "<backup_filename>",
  label: "UAT Test Backup",
  description: "Backup created during UAT testing",
  tags: ["uat", "test"]
})
```

**Pass Criteria**: Metadata updated successfully

---

## Database Operations

### BACK-013: Database Snapshot

**MCP Tool**: `database_snapshot`

```javascript
database_snapshot({
  label: "uat-pre-test"
})
```

**Pass Criteria**: Snapshot created with label

---

### BACK-014: Download Backup

**MCP Tool**: `backup_download`

```javascript
backup_download({
  output_dir: "/tmp/uat"
})
// Returns: { saved_to: "/tmp/uat/backup-2026-02-07.sql.gz", ... }
```

**Pass Criteria**: Backup file saved to output directory

---

## Knowledge Archives

### BACK-015: Knowledge Archive Download

**MCP Tool**: `knowledge_archive_download`

```javascript
knowledge_archive_download({
  filename: "<archive_filename>",
  output_dir: "/tmp/uat"
})
// Returns: { saved_to: "/tmp/uat/<archive_filename>", ... }
```

**Pass Criteria**: Archive file saved to output directory

---

### BACK-016: Knowledge Archive Upload

**MCP Tool**: `knowledge_archive_upload`

```javascript
knowledge_archive_upload({
  file_path: "/tmp/uat/uat-test-archive.archive"
})
```

**Pass Criteria**: Archive uploaded successfully

---

## Database Restore

### BACK-017: Database Restore

**MCP Tool**: `database_restore`

```javascript
// WARNING: This is destructive - use with caution
database_restore({
  filename: "<snapshot_filename>",
  skip_snapshot: false  // Create backup before restore
})
```

**Pass Criteria**: Database restored from snapshot

**WARNING**: This replaces all data. Only run in test environments.

---

### BACK-018: Memory Info

**MCP Tool**: `memory_info`

```javascript
memory_info()
```

**Expected Response**:
```json
{
  "summary": {
    "total_notes": 100,
    "total_size_bytes": 1024000
  },
  "embedding_sets": [...],
  "storage": {...},
  "recommendations": [...]
}
```

**Pass Criteria**: Returns comprehensive memory/storage info

---

### BACK-019: Import with Conflict Resolution

**MCP Tool**: `backup_import`

```javascript
backup_import({
  backup: { notes: [...] },
  dry_run: true,
  on_conflict: "skip"  // skip | replace | merge
})
```

**Pass Criteria**: Dry run reports what would be imported

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| BACK-001 | Backup Status | `backup_status` | |
| BACK-002 | Trigger Backup | `backup_now` | |
| BACK-003 | Export All Notes | `export_all_notes` | |
| BACK-004 | Export Single Note | `export_note` | |
| BACK-005 | Export Original Content | `export_note` | |
| BACK-006 | Create Knowledge Shard | `knowledge_shard` | |
| BACK-007 | Shard with Components | `knowledge_shard` | |
| BACK-008 | Import Knowledge Shard | `knowledge_shard`, `knowledge_shard_import` | |
| BACK-009 | List Backups | `list_backups` | |
| BACK-010 | Get Backup Info | `get_backup_info` | |
| BACK-011 | Get Backup Metadata | `get_backup_metadata` | |
| BACK-012 | Update Metadata | `update_backup_metadata` | |
| BACK-013 | Database Snapshot | `database_snapshot` | |
| BACK-014 | Download Backup | `backup_download` | |
| BACK-015 | Knowledge Archive Download | `knowledge_archive_download` | |
| BACK-016 | Knowledge Archive Upload | `knowledge_archive_upload` | |
| BACK-017 | Database Restore | `database_restore` | |
| BACK-018 | Memory Info | `memory_info` | |
| BACK-019 | Import Conflict Resolution | `backup_import` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

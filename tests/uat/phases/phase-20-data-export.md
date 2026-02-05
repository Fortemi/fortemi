# UAT Phase 20: Data Export

**Purpose**: Verify backup, export, and data portability features
**Duration**: ~8 minutes
**Phase Number**: 20 (second-to-last phase)
**Prerequisites**: All phases 0-19 completed

---

## Backup Status

### BACK-001: Backup Status

```javascript
backup_status()
```

**Pass Criteria**: Returns status info including last backup time

---

### BACK-002: Trigger Backup

```javascript
backup_now()
```

**Pass Criteria**: Returns job ID or immediate success

---

## Export Operations

### BACK-003: Export All Notes

```javascript
export_all_notes()
```

**Pass Criteria**: Returns manifest with note count and notes array

---

### BACK-004: Export Single Note

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

```javascript
knowledge_shard({
  include_embeddings: false,
  tags: ["uat"]
})
```

**Pass Criteria**: Returns shard manifest with UAT notes

---

### BACK-007: Knowledge Shard with Components

```javascript
knowledge_shard({
  components: ["notes", "concepts", "links"]
})
```

**Pass Criteria**: Shard includes specified components

---

### BACK-008: Import Knowledge Shard

```javascript
// First export
const shard = knowledge_shard({ tags: ["uat/export-test"] })

// Then import (to different instance or after delete)
knowledge_shard_import({
  shard: shard,
  merge_strategy: "skip_existing"
})
```

**Pass Criteria**: Notes imported successfully

---

## Backup Browser

### BACK-009: List Backups

```javascript
list_backups()
```

**Pass Criteria**: Returns array of backup files with metadata

---

### BACK-010: Get Backup Info

```javascript
get_backup_info({ filename: "<backup_filename>" })
```

**Pass Criteria**: Returns detailed backup information

---

### BACK-011: Get Backup Metadata

```javascript
get_backup_metadata({ filename: "<backup_filename>" })
```

**Pass Criteria**: Returns user-defined metadata for backup

---

### BACK-012: Update Backup Metadata

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

```javascript
database_snapshot({
  label: "uat-pre-test"
})
```

**Pass Criteria**: Snapshot created with label

---

### BACK-014: Download Backup

```javascript
backup_download()
```

**Pass Criteria**: Returns downloadable backup file content

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| BACK-001 | Backup Status | |
| BACK-002 | Trigger Backup | |
| BACK-003 | Export All Notes | |
| BACK-004 | Export Single Note | |
| BACK-005 | Export Original Content | |
| BACK-006 | Create Knowledge Shard | |
| BACK-007 | Shard with Components | |
| BACK-008 | Import Knowledge Shard | |
| BACK-009 | List Backups | |
| BACK-010 | Get Backup Info | |
| BACK-011 | Get Backup Metadata | |
| BACK-012 | Update Metadata | |
| BACK-013 | Database Snapshot | |
| BACK-014 | Download Backup | |

---

## Knowledge Archives

### BACK-015: Knowledge Archive Download

```javascript
knowledge_archive_download({ filename: "<archive_filename>" })
```

**Pass Criteria**: Returns archive bundle with metadata

---

### BACK-016: Knowledge Archive Upload

```javascript
knowledge_archive_upload({
  archive_base64: "<base64_encoded_archive>",
  filename: "uat-test-archive.archive"
})
```

**Pass Criteria**: Archive uploaded successfully

---

## Database Restore

### BACK-017: Database Restore

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

| Test ID | Name | Status |
|---------|------|--------|
| BACK-001 | Backup Status | |
| BACK-002 | Trigger Backup | |
| BACK-003 | Export All Notes | |
| BACK-004 | Export Single Note | |
| BACK-005 | Export Original Content | |
| BACK-006 | Create Knowledge Shard | |
| BACK-007 | Shard with Components | |
| BACK-008 | Import Knowledge Shard | |
| BACK-009 | List Backups | |
| BACK-010 | Get Backup Info | |
| BACK-011 | Get Backup Metadata | |
| BACK-012 | Update Metadata | |
| BACK-013 | Database Snapshot | |
| BACK-014 | Download Backup | |
| BACK-015 | Knowledge Archive Download | |
| BACK-016 | Knowledge Archive Upload | |
| BACK-017 | Database Restore | |
| BACK-018 | Memory Info | |
| BACK-019 | Import Conflict Resolution | |

**MCP Tools Covered**: `backup_status`, `backup_now`, `export_all_notes`, `export_note`, `knowledge_shard`, `knowledge_shard_import`, `list_backups`, `get_backup_info`, `get_backup_metadata`, `update_backup_metadata`, `database_snapshot`, `backup_download`, `knowledge_archive_download`, `knowledge_archive_upload`, `database_restore`, `backup_import`, `memory_info`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

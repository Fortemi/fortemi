# CRUD Retest Results - 2026-02-10

**Target**: Matric Memory v2026.2.8 at https://memory.integrolabs.net
**Scope**: CRUD-004 (Bulk Create visibility) and CRUD-008 (Tag Filter on bulk notes)
**Previously failed as**: Issue #275

---

## CRUD-004: Bulk Create Notes - Immediate Visibility

**Result: PASS**

### Step 1: Bulk Create

Called `bulk_create_notes` with 3 notes, each tagged `uat/retest` plus a unique `bulk/N` tag. Used `revision_mode: "none"` to avoid AI pipeline delays.

**Response:**
```json
{
  "count": 3,
  "ids": [
    "019c453e-8ad0-7c42-af77-f6c15e4af86d",
    "019c453e-8adc-7881-a1bc-7b1c0b3407d3",
    "019c453e-8adf-7312-8369-9bc461992915"
  ]
}
```

### Step 2: Immediate list_notes (no delay)

Called `list_notes` with `tags: ["uat/retest"]` immediately after creation.

**Response:**
```json
{
  "notes": [
    {
      "id": "019c453e-8adc-7881-a1bc-7b1c0b3407d3",
      "title": "Retest bulk note 2 - testing visibility",
      "created_at_utc": "2026-02-10T01:50:52.624881Z",
      "tags": ["bulk/2", "uat/retest"],
      "has_revision": false
    },
    {
      "id": "019c453e-8adf-7312-8369-9bc461992915",
      "title": "Retest bulk note 3 - testing visibility",
      "created_at_utc": "2026-02-10T01:50:52.624881Z",
      "tags": ["bulk/3", "uat/retest"],
      "has_revision": false
    },
    {
      "id": "019c453e-8ad0-7c42-af77-f6c15e4af86d",
      "title": "Retest bulk note 1 - testing visibility",
      "created_at_utc": "2026-02-10T01:50:52.624881Z",
      "tags": ["bulk/1", "uat/retest"],
      "has_revision": false
    }
  ],
  "total": 3
}
```

### Verdict

All 3 bulk-created notes appeared immediately in `list_notes` with no delay. The race condition (if any existed) is not present. **PASS**.

---

## CRUD-008: Tag Filter on Bulk-Created Notes

**Result: PASS**

### Step 1: Search with tag filter

Called `search_notes` with `query: "Retest bulk note"`, `required_tags: ["bulk/1"]`, `mode: "fts"`.

**Response:**
```json
{
  "results": [
    {
      "note_id": "019c453e-8ad0-7c42-af77-f6c15e4af86d",
      "score": 1,
      "snippet": "Retest bulk note 1 - testing visibility",
      "tags": ["bulk/1", "uat/retest"],
      "chain_info": {
        "chain_id": "019c453e-8ad0-7c42-af77-f6c15e4af86d",
        "chunks_matched": 1,
        "total_chunks": 1
      }
    }
  ],
  "query": "Retest bulk note",
  "total": 1
}
```

### Verdict

Search with `required_tags: ["bulk/1"]` returned exactly 1 result - the correct note with tag `bulk/1`. Tag filtering works correctly on bulk-created notes. **PASS**.

---

## Cleanup

All 3 test notes deleted successfully via `delete_note`:

| Note ID | Delete Result |
|---------|---------------|
| `019c453e-8ad0-7c42-af77-f6c15e4af86d` | `{"success": true}` |
| `019c453e-8adc-7881-a1bc-7b1c0b3407d3` | `{"success": true}` |
| `019c453e-8adf-7312-8369-9bc461992915` | `{"success": true}` |

---

## Summary

| Test | Result | Notes |
|------|--------|-------|
| CRUD-004: Bulk Create Visibility | **PASS** | All 3 notes visible immediately after creation |
| CRUD-008: Tag Filter on Bulk Notes | **PASS** | Tag filter correctly isolates single note |
| Cleanup | **PASS** | All test data removed |

**Overall: 2/2 PASS** - Both previously-failed tests now pass. Issue #275 can be closed.

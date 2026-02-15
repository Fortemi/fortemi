# Phase 1: Knowledge Capture — Results

**Date**: 2026-02-15
**Suite**: v10

| Test ID | Tool | Action | Focus | Result |
|---------|------|--------|-------|--------|
| CK-001 | capture_knowledge | create | Basic note creation | PASS |
| CK-002 | capture_knowledge | create | Metadata preservation | PASS |
| CK-003 | capture_knowledge | create | Hierarchical tags | PASS |
| CK-004 | capture_knowledge | create | Revision mode control | PASS |
| CK-005 | capture_knowledge | bulk_create | Batch creation (3) | PASS |
| CK-006 | capture_knowledge | bulk_create | Result validation (2) | PASS |
| CK-007 | capture_knowledge | from_template | Template instantiation | PASS |
| CK-008 | capture_knowledge | upload | Upload guidance | PASS |
| CK-009 | capture_knowledge | (invalid) | Error handling | PASS |
| CK-010 | capture_knowledge | create | Validation error | PASS |

**Phase Result**: PASS (10/10)

## Stored IDs

- `basic_note_id`: 019c5fd6-2aa3-7a01-a285-dc905d830185
- `metadata_note_id`: 019c5fd6-4082-7072-a7a4-cd8614e49b00
- `hierarchy_note_id`: 019c5fd6-5618-7791-84fb-494513d69b98
- `revision_note_id`: 019c5fd6-59ff-7640-ae66-626ad770c01c
- `bulk_note_ids`: [019c5fd6-6fa0-7053-a60b-b8588970c1b9, 019c5fd6-6fa2-76f2-a7d7-41eb2ac67982, 019c5fd6-6fa3-7743-bfd6-448a2e7d98a4]
- `validation_note_ids`: [019c5fd6-7fd2-7c83-934c-1174626378f6, 019c5fd6-7fd5-7911-83e9-e8f3a7187e24]
- `accidental_note_id`: 019c5fd6-b3ed-7e32-90f8-69818f671c24

## Notes
- CK-007: No templates exist in fresh system. Error message clear: "template_id must be a valid UUID"
- CK-008: Upload action requires note_id first. Clear guidance provided.
- CK-009: MCP schema enum validation prevents invalid actions at protocol level
- CK-010: API returns 422 with "missing field `content`"

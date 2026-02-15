# Phase 1: Knowledge Capture — Results

**Date**: 2026-02-14
**Result**: 8 PASS, 2 PARTIAL (10 tests)

| Test ID | Tool | Action | Focus | Result | Issue |
|---------|------|--------|-------|--------|-------|
| CK-001 | capture_knowledge | create | Basic note | PASS | |
| CK-002 | capture_knowledge | create | Metadata | PASS | |
| CK-003 | capture_knowledge | create | Hierarchical tags | PASS | |
| CK-004 | capture_knowledge | create | Revision mode none | PASS | |
| CK-005 | capture_knowledge | bulk_create | Batch (3 notes) | PASS | |
| CK-006 | capture_knowledge | bulk_create | Result validation (2 notes) | PASS | |
| CK-007 | capture_knowledge | from_template | Template instantiation | PARTIAL | #383 |
| CK-008 | capture_knowledge | upload | Upload guidance | PARTIAL | #382 |
| CK-009 | capture_knowledge | (invalid) | Invalid action rejected | PASS | |
| CK-010 | capture_knowledge | create | Missing content error | PASS | |

## Stored IDs
- basic_note_id: 019c5e67-4261-7122-b1ec-88bede99ee92
- metadata_note_id: 019c5e67-42ab-7730-bf23-2369eaa1e4ae
- hierarchy_note_id: 019c5e67-431d-7f73-be0e-f9fe66248e30
- bulk_note_ids: [019c5e67-7691-79a2-a772-a698610dab5b, 019c5e67-7694-7403-b4a3-22bc3016a3dd, 019c5e67-7697-7771-a028-a1974fae4a84]
- validation_note_ids: [019c5e67-8ea7-79b0-b8d8-6a67d6ac958a, 019c5e67-8ea9-76d2-ae44-3b3f40f7d4fa]

## Errata
- #382: upload action returns URL with `undefined` note_id when no note_id provided
- #383: from_template returns raw UUID parse error instead of helpful message

## Notes
- Create response returns only `{id}` — minimal but functional
- Bulk create returns array of `{id}` objects
- Semantic linking auto-creates links between related notes (0.77 similarity)
- AI revision generates titles even with revision_mode: "none" (title generation runs separately)

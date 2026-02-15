# UAT v9 Retest R3 Results

**Date**: 2026-02-15
**Trigger**: Server update (commit 22a3931) — major tool surface change
**Changes**:
- `describe_image` and `transcribe_audio` MCP tools REMOVED
- `manage_attachments` consolidated tool ADDED (actions: list, upload, get, download, delete)
- Tool count: 23 → 22
- Phase 9 rewritten from "Media Processing" (6 tests) to "Attachments" (8 tests)
- All user-facing URLs now use PUBLIC_URL

---

## Phase 9 (Attachments) — Full Re-execution

| Test ID | Tool | Result | Notes |
|---------|------|--------|-------|
| ATT-001 | get_system_info | PASS | vision=true (qwen3-vl:8b), audio=true (Whisper), video=true |
| ATT-002 | manage_attachments | PASS | Empty array for note with no attachments |
| ATT-003 | manage_attachments | PASS | Upload curl cmd with POST, PUBLIC_URL correct |
| ATT-004 | manage_attachments | PASS | 2 attachments listed with id, filename, content_type, size_bytes |
| ATT-005 | manage_attachments | PASS | Full metadata + `_api_urls` with download link |
| ATT-006 | manage_attachments | PASS | Download curl cmd with PUBLIC_URL correct |
| ATT-007 | manage_attachments | PASS | success=true, deleted matches attachment ID |
| ATT-008 | manage_attachments | PASS | Deleted attachment no longer in list |

**Phase 9 Result**: 8/8 PASS (100%)

---

## Issue Resolution

| Issue | Title | R2 Status | R3 Status | Resolution |
|-------|-------|-----------|-----------|------------|
| #389 | File provenance attachment_id gap | OPEN | CLOSED | Tools removed; manage_attachments provides IDs |
| #391 | describe_image localhost URLs | OPEN | CLOSED | describe_image removed entirely |
| #392 | describe_image file path validation | OPEN | CLOSED | describe_image removed entirely |
| #395 | Test phase reordering for sibling errors | OPEN | CLOSED | STOP markers added to phase docs |
| #386 | explore_graph depth=2 same as depth=1 | OPEN | OPEN | Cannot retest — no linked corpus after cleanup |

---

## Updated Pass Rate

### Core Run (141 tests)
- Phase 9 updated: 6 tests → 8 tests (net +2)
- Original failures in Phase 9: 2 (describe_image tests)
- New Phase 9: 8/8 PASS

### Cumulative (after R1 + R2 + R3)

| Metric | R2 | R3 |
|--------|----|----|
| Total tests | 141 | 143 (+2 from Phase 9 rewrite) |
| PASS | ~136 | ~140 |
| FAIL | 4 | 1 |
| PARTIAL | 0 | 0 |
| Pass rate | 96.4% | 97.9% |

### Remaining Open Issues

| Issue | Title | Status |
|-------|-------|--------|
| #386 | explore_graph depth=2 same as depth=1 | OPEN — needs linked corpus to retest |

---

## Summary

Commit 22a3931 resolved 4 of 5 remaining issues by removing the problematic `describe_image`/`transcribe_audio` tools and replacing them with the consolidated `manage_attachments` tool. The new tool surface (22 tools) is cleaner and all user-facing URLs correctly use `PUBLIC_URL`. Phase 9 passes at 100%.

Only #386 (graph depth traversal) remains open — it requires a corpus with multi-hop link chains to verify, which doesn't exist after Phase 13 cleanup.

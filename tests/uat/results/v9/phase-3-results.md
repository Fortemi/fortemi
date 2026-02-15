# Phase 3: Search — Results

**Date**: 2026-02-14
**Result**: 11 PASS, 1 FAIL (12 tests)

| Test ID | Tool | Focus | Result | Issue |
|---------|------|-------|--------|-------|
| SRCH-001 | search (text) | Basic text search | PASS | |
| SRCH-002 | search (text) | Tag filtering | PASS | |
| SRCH-003 | search (text) | Phrase search | PASS | |
| SRCH-004 | search (text) | OR operator | PASS | |
| SRCH-005 | search (text) | NOT operator | PASS | |
| SRCH-006 | search (spatial) | Spatial search | PASS | 0 results (no provenance, expected) |
| SRCH-007 | search (temporal) | Temporal search | PASS | 9 results via user_created_at fallback |
| SRCH-008 | search (spatial_temporal) | Combined search | PASS | 0 results (no location data, expected) |
| SRCH-009 | search (federated) | Cross-archive | PASS | |
| SRCH-010 | search (text) | Empty results for nonsense | FAIL | #384 |
| SRCH-011 | search | Invalid action (schema) | PASS | |
| SRCH-012 | search (text) | Missing query param | PASS | |

## Issues
- #384: Nonsense query "xyznonexistent99999" returns 5 scored results instead of empty array (semantic fallback)

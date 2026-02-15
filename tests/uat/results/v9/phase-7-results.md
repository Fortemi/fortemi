# Phase 7: Provenance — Results

**Date**: 2026-02-14
**Result**: 8 PASS, 1 FAIL, 1 PARTIAL (10 tests)

| Test ID | Tool | Focus | Result | Issue |
|---------|------|-------|--------|-------|
| PROV-001 | record_provenance (location) | NYC location | PASS | |
| PROV-002 | record_provenance (named_location) | London named location | PASS | |
| PROV-003 | record_provenance (device) | Device record | PASS | |
| PROV-004 | record_provenance (file) | File provenance | FAIL | #389 — requires attachment_id |
| PROV-005 | record_provenance (note) | Note + location | PASS | |
| PROV-006 | record_provenance (note) | Note + device | PARTIAL | #390 — time_source enum mismatch |
| PROV-007 | record_provenance (note) | Note + time | PASS | |
| PROV-008 | search (spatial) | Verify spatial query | PASS | |
| PROV-009 | record_provenance | Schema validation | PASS | |
| PROV-010 | record_provenance (location) | Missing required fields | PASS | |

## Stored IDs
- location_id (NYC): 019c5e78-6620-7120-ab8b-a3fff563c800
- location_id_london: 019c5e78-7030-736c-a874-32ee6e4e7800
- device_id: 019c5e78-785e-728e-a4a4-95add2f4ec00

## Issues
- #389: file provenance requires attachment_id, no MCP tool to create attachments
- #390: time_source "device_clock" accepted by MCP schema but rejected by DB constraint

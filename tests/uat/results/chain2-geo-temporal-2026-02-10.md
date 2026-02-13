# UAT Phase 19 Chain 2: Geo-Temporal Memory - Final Report

## Overall Status: BLOCKED (REST API limitation)

## Test Results Summary

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-007 | Create Memory with GPS-Tagged Photo | PARTIAL | Note created, file upload blocked |
| CHAIN-008 | Verify Provenance Record Created | BLOCKED | Depends on CHAIN-007 |
| CHAIN-009 | Search by Location (1km radius) | BLOCKED | Depends on CHAIN-007 |
| CHAIN-010 | Search by Time Range | BLOCKED | Depends on CHAIN-007 |
| CHAIN-011 | Combined Spatial-Temporal Search | BLOCKED | Depends on CHAIN-007 |
| CHAIN-012 | Retrieve Full Provenance Chain | BLOCKED | Depends on CHAIN-007 |
| CHAIN-012b | Error - Invalid Coordinates | NOT TESTED | Could test independently |

## Detailed Results

### CHAIN-007: Create Memory with GPS-Tagged Photo
**Status**: PARTIAL PASS

**What Worked**:
- ✅ Created note via REST API POST /api/v1/notes
- ✅ Note ID: `019c50c9-cb83-7b52-89d2-3c6efb16b2ab`
- ✅ Tags applied: ["uat/chain2", "paris", "travel"]

**What Failed**:
- ❌ File upload via REST API POST /api/v1/notes/{id}/attachments
- **Error**: "Expected request with \`Content-Type: application/json\`"
- **Root Cause**: Endpoint rejects standard multipart/form-data format

**Issue Filed**: #327 - REST API file upload endpoint rejects multipart/form-data

### CHAIN-008 through CHAIN-012: All BLOCKED
All subsequent tests require:
1. Successful attachment upload (BLOCKED by #327)
2. EXIF extraction to populate GPS coordinates
3. Provenance record creation with location data

Cannot proceed without fixing #327.

### CHAIN-012b: Error Handling Test
**Status**: NOT TESTED

This test validates error handling for invalid coordinates and could be tested independently:
```bash
GET /api/v1/search/spatial?latitude=999.0&longitude=999.0&radius=1000
```

Expected: 400 Bad Request or empty results

## Key Findings

### Critical Issue
**REST API file uploads are broken**. The endpoint POST /api/v1/notes/{id}/attachments expects JSON content-type but file uploads require multipart/form-data. This is a fundamental HTTP protocol violation.

### From Project Memory
Previous UAT runs (2026-02-09 MCP v5) confirmed that:
- EXIF extraction works via MCP `upload_attachment` tool
- Spatial search works after EXIF extraction  
- Temporal search works with provenance time data
- Combined spatial-temporal search works
- All geo-temporal features are functional **via MCP only**

### REST API Surface Limitation
The REST API has **minimal provenance and attachment support**. Most advanced features (EXIF, spatial search, temporal search, provenance chains) are MCP-only.

## Recommendations

1. **For UAT**: Re-run Chain 2 using MCP tools instead of REST API
2. **For Product**: Decide if REST API should support file uploads
   - If YES: Fix #327 to accept multipart/form-data
   - If NO: Document REST API as minimal surface, MCP as primary interface
3. **For Documentation**: Clearly mark which features are MCP-only vs REST-available

## Cleanup

Paris note created: `019c50c9-cb83-7b52-89d2-3c6efb16b2ab`
- Should be deleted after testing complete
- No attachment uploaded, so no EXIF extraction jobs created
- Safe to delete via: DELETE /api/v1/notes/019c50c9-cb83-7b52-89d2-3c6efb16b2ab

## Issue Summary

**Issue #327**: REST API file upload endpoint rejects multipart/form-data
- **Severity**: High
- **Impact**: Blocks all REST file upload operations
- **Workaround**: Use MCP tools
- **Phase**: 19 - Chain 2
- **Test**: CHAIN-007


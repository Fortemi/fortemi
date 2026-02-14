# UAT-3B-014: Get Provenance - Multiple Attachments

**Status**: BLOCKED - MCP Server Initialization Issue

## Test Objective
Verify that `get_memory_provenance` returns file provenance for multiple attachments on a single note.

## Blocking Issue

The test requires MCP server functionality for:
1. `create_file_provenance` - Only available via MCP
2. `get_memory_provenance` - Only available via MCP

**Current Problem**: MCP server returns "Server not initialized" error despite sending proper initialization request.

### MCP Initialization Attempt

```bash
curl -X POST "https://memory.integrolabs.net/mcp" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{
    "jsonrpc": "2.0",
    "id": 0,
    "method": "initialize",
    "params": {
      "protocolVersion": "2024-11-05",
      "capabilities": {},
      "clientInfo": {
        "name": "uat-client",
        "version": "1.0.0"
      }
    }
  }'
```

**Error**: Subsequent tool calls fail with "Bad Request: Server not initialized"

### REST API Limitations

REST API **does not** expose:
- Provenance creation endpoints (POST /api/v1/provenance/*)
- File provenance endpoints
- Attachment upload with provenance

REST API for attachments expects `Content-Type: application/json`, not multipart/form-data:
```
Expected request with `Content-Type: application/json`
```

## Partial Verification

Successfully verified prerequisite:
- ✅ Can create notes via REST API
- ❌ Cannot upload attachments via REST API (expects JSON, not files)
- ❌ Cannot create file provenance (MCP only)
- ❌ Cannot retrieve provenance (MCP only)

## Resolution Required

One of:
1. Fix MCP server initialization via curl/HTTP
2. Add REST API endpoints for file provenance creation
3. Use a proper MCP client library instead of curl
4. Document that these operations require integrated MCP client

## Test Steps (When Unblocked)

1. Create note: `create_note({ content: "# Multi-attachment test", tags: ["uat/multi-prov"], revision_mode: "none" })`
2. Upload 2 attachments via `upload_attachment`
3. Create provenance for each: `create_file_provenance({ attachment_id, capture_time, event_type })`
4. Get provenance: `get_memory_provenance({ note_id })`
5. Verify: files array contains 2 elements with distinct attachment_ids

## Expected Outcome

```json
{
  "files": [
    {
      "attachment_id": "uuid-1",
      "provenance_id": "uuid-a",
      "capture_time": "2026-02-13T10:00:00Z",
      "event_type": "test_event_1"
    },
    {
      "attachment_id": "uuid-2",
      "provenance_id": "uuid-b",
      "capture_time": "2026-02-13T11:00:00Z",
      "event_type": "test_event_2"
    }
  ]
}
```

## Recommendation

Mark test as **BLOCKED** pending MCP connectivity resolution. This is an infrastructure issue, not a test design issue.

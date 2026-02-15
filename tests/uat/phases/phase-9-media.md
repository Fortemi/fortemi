# UAT Phase 9: Attachments

## Purpose
Validate file attachment management via the `manage_attachments` consolidated MCP tool. Tests verify listing, uploading (curl command generation), metadata retrieval, download command generation, and deletion. Image/audio/video attachments are automatically processed by the extraction pipeline — there are no standalone media processing MCP tools.

## Duration
~5 minutes

## Prerequisites
- Phase 1 completed (notes exist for attaching files)
- At least one note UUID stored from previous phases (`NOTE_ID`)

## Tools Tested
- `manage_attachments`
- `get_system_info`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls through the matric-memory MCP server. Direct HTTP API calls are NOT permitted. Use `mcp.request({ method: "tools/call", params: { name: "tool_name", arguments: {...} }})`.

> **Upload Note**: The `upload` and `download` actions return curl commands rather than performing binary transfer directly. This is by design — MCP tools exchange JSON, not binary streams. Execute the returned curl commands in a shell to complete the transfer.

---

## Test Cases

### ATT-001: Check Media Capabilities
**MCP Tool**: `get_system_info`

Verify system reports media processing capabilities for the extraction pipeline.

```javascript
const systemInfo = await mcp.request({
  method: "tools/call",
  params: {
    name: "get_system_info",
    arguments: {}
  }
});

console.log("Vision enabled:", systemInfo.capabilities.vision_enabled);
console.log("Transcription enabled:", systemInfo.capabilities.transcription_enabled);
```

**Expected**:
- Returns capabilities object with boolean flags
- `vision_enabled` and `transcription_enabled` indicate backend availability
- These capabilities affect the extraction pipeline (not standalone tools)

**Pass Criteria**:
- Response contains `capabilities` object
- `vision_enabled` is boolean
- `transcription_enabled` is boolean

**Store**: `VISION_AVAILABLE`, `TRANSCRIPTION_AVAILABLE`

---

### ATT-002: List Attachments (Empty)
**MCP Tool**: `manage_attachments`

List attachments on a note that has none yet.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "list",
      note_id: NOTE_ID
    }
  }
});

console.log("Attachments:", result);
```

**Expected**:
- Returns empty array `[]`
- No error for notes with no attachments

**Pass Criteria**:
- Response is an array
- Array length is 0

---

### ATT-003: Upload Attachment (Curl Command)
**MCP Tool**: `manage_attachments`

Request an upload curl command for a test file.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "upload",
      note_id: NOTE_ID,
      filename: "test-image.jpg",
      content_type: "image/jpeg"
    }
  }
});

console.log("Upload command:", result.curl_command);
console.log("Upload URL:", result.upload_url);
```

**Expected**:
- Returns object with `curl_command` string
- Curl command includes correct endpoint URL
- Curl command includes authentication headers (if auth enabled)
- URL points to the note's attachment endpoint

**Pass Criteria**:
- `curl_command` is a non-empty string
- Command contains `POST` method
- Command targets `/api/v1/notes/{note_id}/attachments`

**Store**: Execute the curl command with a test file to create an attachment, then store the resulting `ATTACHMENT_ID`

---

### ATT-004: List Attachments (After Upload)
**MCP Tool**: `manage_attachments`

List attachments after uploading one.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "list",
      note_id: NOTE_ID
    }
  }
});

console.log("Attachment count:", result.length);
console.log("First attachment:", result[0]);
```

**Expected**:
- Returns array with at least 1 attachment
- Each attachment has `id`, `filename`, `content_type`, `size` fields

**Pass Criteria**:
- Array length >= 1
- First item has `id` (UUID format)
- First item has `filename` field

**Store**: `ATTACHMENT_ID` from first result

---

### ATT-005: Get Attachment Metadata
**MCP Tool**: `manage_attachments`

Retrieve metadata for a specific attachment.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "get",
      id: ATTACHMENT_ID
    }
  }
});

console.log("Attachment:", result);
console.log("API URLs:", result._api_urls);
```

**Expected**:
- Returns attachment metadata object
- Includes `id`, `filename`, `content_type`, `size`, `created_at`
- Includes `_api_urls` with download link

**Pass Criteria**:
- `id` matches `ATTACHMENT_ID`
- `filename` is present
- `_api_urls` object contains download URL

---

### ATT-006: Download Attachment (Curl Command)
**MCP Tool**: `manage_attachments`

Request a download curl command for an attachment.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "download",
      id: ATTACHMENT_ID
    }
  }
});

console.log("Download command:", result.curl_command);
```

**Expected**:
- Returns object with `curl_command` string
- Curl command targets the attachment download endpoint
- Command includes authentication headers (if auth enabled)

**Pass Criteria**:
- `curl_command` is a non-empty string
- Command contains `GET` method or no explicit method (GET is default)
- Command targets `/api/v1/attachments/{id}/download`

---

### ATT-007: Delete Attachment
**MCP Tool**: `manage_attachments`

**Isolation**: Required

> **STOP — ISOLATED CALL**: This test modifies state (deletes an attachment). Execute this MCP call ALONE in its own turn to avoid side effects on other calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

Delete the test attachment.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "delete",
      id: ATTACHMENT_ID
    }
  }
});

console.log("Delete result:", result);
```

**Expected**:
- Returns success confirmation
- Attachment is removed from the note

**Pass Criteria**:
- `result.success` is `true`
- `result.deleted` matches `ATTACHMENT_ID`

---

### ATT-008: List Attachments (After Delete)
**MCP Tool**: `manage_attachments`

Verify attachment was deleted.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "manage_attachments",
    arguments: {
      action: "list",
      note_id: NOTE_ID
    }
  }
});

console.log("Attachments after delete:", result.length);
```

**Expected**:
- Returns empty array (attachment was deleted)

**Pass Criteria**:
- Array length is 0
- Previously uploaded attachment no longer appears

---

## Phase Summary

| Test ID | Tool | Status | Notes |
|---------|------|--------|-------|
| ATT-001 | get_system_info | [ ] | Media capability detection |
| ATT-002 | manage_attachments | [ ] | List (empty) |
| ATT-003 | manage_attachments | [ ] | Upload curl command |
| ATT-004 | manage_attachments | [ ] | List (after upload) |
| ATT-005 | manage_attachments | [ ] | Get metadata |
| ATT-006 | manage_attachments | [ ] | Download curl command |
| ATT-007 | manage_attachments | [ ] | Delete attachment |
| ATT-008 | manage_attachments | [ ] | List (after delete) |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Upload/download actions return curl commands — binary transfer happens outside MCP
- Image/audio/video files are automatically processed by the extraction pipeline after upload
- Vision and transcription capabilities (ATT-001) affect pipeline processing, not standalone tools

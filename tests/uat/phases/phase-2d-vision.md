# UAT Phase 2D: Vision (Image Description)

**Purpose**: Verify ad-hoc image description via vision LLM (Ollama) through MCP tool calls
**Duration**: ~5 minutes
**Prerequisites**: Phase 0 preflight passed, Ollama vision model configured (`OLLAMA_VISION_MODEL`)
**Critical**: No (requires optional vision backend)
**Tools Tested**: `describe_image`, `get_system_info`

> **Vision Backend Requirement**: Tests in this phase require an Ollama-compatible vision model (e.g., `qwen3-vl:8b`, `llava`). If `OLLAMA_VISION_MODEL` is not configured, VIS-001 detects this and remaining tests are marked SKIPPED (not FAILED). This is expected — vision is an optional capability.

> **Curl-Command Pattern**: The `describe_image` tool returns a curl command for multipart file upload. The MCP client does NOT send image data directly — instead, it generates a curl command that the user executes to upload the file. This avoids base64 encoding overhead and shell argument limits.

> **Test Data**: This phase uses image files from `tests/uat/data/images/`:
> - `object-scene.jpg` — JPEG with objects/scene for general description
> - `png-transparent.png` — PNG image for format testing
> - `jpeg-with-exif.jpg` — JPEG with EXIF metadata

---

## Tests

### VIS-001: Check Vision Backend Availability

**MCP Tool**: `get_system_info`
**Parameters**: `{}`

**Pass Criteria**:
- Response includes `extraction.vision` object
- Record `extraction.vision.available` value
- If `available === true`: record model name, continue to VIS-002
- If `available === false`: mark VIS-002 through VIS-010 as SKIPPED, proceed to next phase

**Notes**: This is a gate test. If vision is not configured, remaining tests cannot execute.

---

### VIS-002: Describe JPEG Image (curl-command)

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "tests/uat/data/images/object-scene.jpg",
  mime_type: "image/jpeg"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- Response contains `upload_url` ending with `/api/v1/vision/describe`
- Response contains `method` === `"POST"`
- Response contains `content_type` === `"multipart/form-data"`
- `curl_command` includes the file path `tests/uat/data/images/object-scene.jpg`
- `curl_command` includes `-F` (multipart form flag)
- `curl_command` includes `type=image/jpeg`

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `description` (non-empty string, length > 10)
- API response contains `model` (matches configured vision model name)

---

### VIS-003: Describe PNG Image (curl-command)

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "tests/uat/data/images/png-transparent.png",
  mime_type: "image/png"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- `curl_command` includes `type=image/png`
- Response contains `upload_url` and `instructions`

---

### VIS-004: Custom Prompt for Image Analysis

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "tests/uat/data/images/jpeg-with-exif.jpg",
  mime_type: "image/jpeg",
  prompt: "List all colors visible in this image. Be concise."
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- `curl_command` includes `prompt=` (custom prompt passed as form field)
- Response contains `instructions`

**Manual Verification**: Execute the returned curl command and verify:
- API response `description` reflects the custom prompt (mentions colors)

---

### VIS-005: Default MIME Type (Omitted)

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "tests/uat/data/images/png-transparent.png"
})
```

**Pass Criteria**:
- Response contains `curl_command` (no error despite omitting mime_type)
- `curl_command` uses `file=@...` without `;type=` suffix (server infers type)
- Response contains `upload_url`

---

### VIS-006: Missing File Path

**Isolation**: Required (expects degraded response)

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({})
```

**Pass Criteria**:
- Response still returns `curl_command` (with placeholder `IMAGE_FILE_PATH`)
- Response contains `upload_url` and `instructions`
- The curl command is syntactically valid but requires user to substitute the file path

---

### VIS-007: Large Prompt with Image

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "tests/uat/data/images/object-scene.jpg",
  mime_type: "image/jpeg",
  prompt: "Describe this image in extreme detail. Include: 1) All objects visible. 2) Background elements. 3) Lighting conditions. 4) Any text or writing. 5) Estimated time of day. 6) Color palette. 7) Composition and framing."
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- `curl_command` includes `prompt=` with the long prompt text
- No error (tool handles long prompts)

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `description` (non-empty, length > 20)
- No timeout error (vision model should handle detailed prompts within 120s)

---

### VIS-008: Upload URL is Well-Formed

**MCP Tool**: `describe_image`
**Parameters**:
```javascript
describe_image({
  file_path: "/tmp/test.webp"
})
```

**Pass Criteria**:
- `upload_url` ends with `/api/v1/vision/describe`
- `method` === `"POST"`
- `content_type` === `"multipart/form-data"`
- `instructions` is a non-empty string describing how to execute the curl command

---

## Phase Summary

| Test ID | Test Name | Status | Notes |
|---------|-----------|--------|-------|
| VIS-001 | Check Vision Backend Availability | | Gate test |
| VIS-002 | Describe JPEG Image (curl-command) | | Curl-command + manual verify |
| VIS-003 | Describe PNG Image (curl-command) | | |
| VIS-004 | Custom Prompt for Image Analysis | | Curl-command + manual verify |
| VIS-005 | Default MIME Type (Omitted) | | |
| VIS-006 | Missing File Path | | Degraded but valid |
| VIS-007 | Large Prompt with Image | | Curl-command + manual verify |
| VIS-008 | Upload URL is Well-Formed | | Structure validation |

**Total Tests**: 8
**Curl-Command Tests**: 7 (VIS-002 through VIS-008)
**Manual Verification**: 3 (VIS-002, VIS-004, VIS-007 — requires executing curl)
**Conditional**: All tests after VIS-001 require vision backend availability

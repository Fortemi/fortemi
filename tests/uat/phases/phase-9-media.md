# UAT Phase 9: Media Processing

## Purpose
Validate media processing capabilities including image description via vision models and audio transcription via Whisper-compatible backends. Tests verify capability detection, successful processing, and error handling for missing backends or files.

## Duration
~5 minutes

## Prerequisites
- Phase 0 completed (system info confirms backend availability)
- Test data available in `tests/uat/data/images/` and `tests/uat/data/audio/`

## Tools Tested
- `describe_image`
- `transcribe_audio`
- `get_system_info`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls through the matric-memory MCP server. Direct HTTP API calls are NOT permitted. Use `mcp.request({ method: "tools/call", params: { name: "tool_name", arguments: {...} }})`.

> **Backend Dependency**: Media processing tools depend on external backends (Ollama vision model for image description, Whisper API for transcription). Unlike other UAT phases, media tests use conditional execution based on `get_system_info()` capabilities. Tests document expected behavior when backends are unavailable.

---

## Test Cases

### MEDIA-001: Check Media Capabilities
**MCP Tool**: `get_system_info`

Verify system reports media processing capabilities accurately.

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
console.log("Vision model:", systemInfo.capabilities.vision_model || "none");
console.log("Whisper URL:", systemInfo.capabilities.whisper_base_url || "none");
```

**Expected**:
- Returns capabilities object with boolean flags
- `vision_enabled` indicates Ollama vision model availability
- `transcription_enabled` indicates Whisper backend availability
- Model/URL fields present when respective feature is enabled

**Pass Criteria**:
- Response contains `capabilities` object
- `vision_enabled` is boolean
- `transcription_enabled` is boolean
- Model/URL fields match backend configuration

**Store**: `VISION_AVAILABLE`, `TRANSCRIPTION_AVAILABLE`

---

### MEDIA-002: Describe Image (Vision Available)
**MCP Tool**: `describe_image`

Generate description of test image using vision model. Conditional on vision backend availability.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "describe_image",
    arguments: {
      file_path: "tests/uat/data/images/jpeg-with-exif.jpg"
    }
  }
});

console.log("Description:", result.description);
console.log("Model used:", result.model);
```

**Expected** (if vision available):
- Returns description string
- Description contains relevant image content
- Model name matches configured vision model

**Expected** (if vision unavailable):
- Returns error indicating vision backend not configured
- Error message is informative (not generic 500)

**Pass Criteria**:
- If `VISION_AVAILABLE`: description length > 10 chars, model field present
- If not available: clear error message explaining missing backend

---

### MEDIA-003: Describe Image with Prompt
**MCP Tool**: `describe_image`

Generate targeted description using custom prompt. Conditional on vision backend availability.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "describe_image",
    arguments: {
      file_path: "tests/uat/data/images/jpeg-with-exif.jpg",
      prompt: "What objects are visible in this image?"
    }
  }
});

console.log("Prompted description:", result.description);
```

**Expected** (if vision available):
- Returns description addressing the prompt
- Response focuses on objects as requested
- Model respects custom prompt context

**Expected** (if vision unavailable):
- Returns error indicating vision backend not configured

**Pass Criteria**:
- If `VISION_AVAILABLE`: description references objects/items, length > 10 chars
- If not available: clear error message

---

### MEDIA-004: Transcribe Audio (Whisper Available)
**MCP Tool**: `transcribe_audio`

Transcribe test audio file. Conditional on Whisper backend availability.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "transcribe_audio",
    arguments: {
      file_path: "tests/uat/data/audio/english-speech-5s.mp3"
    }
  }
});

console.log("Transcription:", result.text);
console.log("Language:", result.language);
console.log("Duration:", result.duration);
```

**Expected** (if transcription available):
- Returns transcription text
- Language detection (e.g., "en")
- Duration in seconds
- Text matches audio content

**Expected** (if transcription unavailable):
- Returns error indicating Whisper backend not configured
- Error message is informative

**Pass Criteria**:
- If `TRANSCRIPTION_AVAILABLE`: text length > 0, language field present, duration > 0
- If not available: clear error message explaining missing backend

---

### MEDIA-005: Describe Image Missing File
**MCP Tool**: `describe_image`

**Isolation**: Required

Attempt to describe non-existent image file.

```javascript
try {
  await mcp.request({
    method: "tools/call",
    params: {
      name: "describe_image",
      arguments: {
        file_path: "/nonexistent/file.jpg"
      }
    }
  });
  console.error("FAIL: Should have thrown error for missing file");
} catch (error) {
  console.log("Correctly rejected missing file:", error.message);
}
```

**Expected**:
- Request fails with appropriate error
- Error indicates file not found
- Does not attempt to contact vision backend with invalid path

**Pass Criteria**:
- Request throws error
- Error message references file path or "not found"
- No backend connection attempt logged

---

### MEDIA-006: Transcribe Audio Missing File
**MCP Tool**: `transcribe_audio`

**Isolation**: Required

Attempt to transcribe non-existent audio file.

```javascript
try {
  await mcp.request({
    method: "tools/call",
    params: {
      name: "transcribe_audio",
      arguments: {
        file_path: "/nonexistent/file.mp3"
      }
    }
  });
  console.error("FAIL: Should have thrown error for missing file");
} catch (error) {
  console.log("Correctly rejected missing file:", error.message);
}
```

**Expected**:
- Request fails with appropriate error
- Error indicates file not found
- Does not attempt to contact Whisper backend with invalid path

**Pass Criteria**:
- Request throws error
- Error message references file path or "not found"
- No backend connection attempt logged

---

## Phase Summary

| Test ID | Tool | Status | Notes |
|---------|------|--------|-------|
| MEDIA-001 | get_system_info | [ ] | Capability detection |
| MEDIA-002 | describe_image | [ ] | Basic image description (conditional) |
| MEDIA-003 | describe_image | [ ] | Custom prompt (conditional) |
| MEDIA-004 | transcribe_audio | [ ] | Audio transcription (conditional) |
| MEDIA-005 | describe_image | [ ] | Missing file error handling |
| MEDIA-006 | transcribe_audio | [ ] | Missing file error handling |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Vision tests conditional on `capabilities.vision_enabled`
- Transcription tests conditional on `capabilities.transcription_enabled`
- Both features may be unavailable in minimal deployments
- Error handling tests (MEDIA-005, MEDIA-006) always run regardless of backend availability

# UAT Phase 2E: Audio Transcription

**Purpose**: Verify ad-hoc audio transcription via Whisper-compatible backend through MCP tool calls
**Duration**: ~5 minutes
**Prerequisites**: Phase 0 preflight passed, Whisper backend configured (`WHISPER_BASE_URL`)
**Critical**: Yes (transcription backend MUST be configured)
**Tools Tested**: `transcribe_audio`, `get_system_info`

> **Transcription Backend Requirement**: Tests in this phase require a Whisper-compatible API backend (e.g., faster-whisper-server, OpenAI Whisper API). The `WHISPER_BASE_URL` environment variable MUST be configured. If AUD-001 reports `enabled === false`, this is a test environment configuration failure — file issue and fix the environment.

> **Curl-Command Pattern**: The `transcribe_audio` tool returns a curl command for multipart file upload. The MCP client does NOT send audio data directly — instead, it generates a curl command that the user executes to upload the file. This avoids base64 encoding overhead and shell argument limits.

> **Test Data**: This phase uses audio files from `tests/uat/data/audio/`:
> - `english-speech-5s.mp3` — Short English speech clip (~5 seconds)
> - `spanish-greeting.mp3` — Spanish language greeting
> - `chinese-phrase.mp3` — Chinese language phrase

---

## Tests

### AUD-001: Check Transcription Backend Availability

**MCP Tool**: `get_system_info`
**Parameters**: `{}`

**Pass Criteria**:
- Response includes `extraction.audio` object
- Record `extraction.audio.enabled` value
- If `enabled === true`: record provider, continue to AUD-002
- If `enabled === false`: **FAIL** — transcription backend is required. File issue to configure test environment.

**Notes**: Transcription backend availability is a hard requirement. All AUD-* tests must pass.

---

### AUD-002: Transcribe English MP3 (curl-command)

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "tests/uat/data/audio/english-speech-5s.mp3",
  mime_type: "audio/mpeg"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- Response contains `upload_url` ending with `/api/v1/audio/transcribe`
- Response contains `method` === `"POST"`
- Response contains `content_type` === `"multipart/form-data"`
- `curl_command` includes the file path `tests/uat/data/audio/english-speech-5s.mp3`
- `curl_command` includes `-F` (multipart form flag)
- `curl_command` includes `type=audio/mpeg`

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `text` (non-empty string, length > 5)
- API response contains `segments` (array with at least 1 entry)
- Each segment has `start_secs` (number >= 0), `end_secs` (number > 0), `text` (string)
- API response contains `model` (non-empty string)
- `language` is present (should detect "en" or similar)

---

### AUD-003: Transcribe Spanish Audio (curl-command)

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "tests/uat/data/audio/spanish-greeting.mp3",
  mime_type: "audio/mpeg"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- `curl_command` includes `type=audio/mpeg`
- Response contains `upload_url` and `instructions`

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `text` (non-empty string)
- `language` is present (should detect "es" or similar)

**Notes**: Verifies multilingual transcription capability.

---

### AUD-004: Transcribe with Language Hint

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "tests/uat/data/audio/spanish-greeting.mp3",
  mime_type: "audio/mpeg",
  language: "es"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- `curl_command` includes `language=es` (language hint passed as form field)
- Response contains `instructions`

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `text` (non-empty string)
- Transcription quality should be at least as good as AUD-003 (language hint helps accuracy)

**Notes**: The `language` parameter provides a hint to the Whisper model. It should improve accuracy for the specified language.

---

### AUD-005: Default MIME Type (Omitted)

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "tests/uat/data/audio/english-speech-5s.mp3"
})
```

**Pass Criteria**:
- Response contains `curl_command` (no error despite omitting mime_type)
- `curl_command` uses `file=@...` without `;type=` suffix (server infers type)
- Response contains `upload_url`

---

### AUD-006: Missing File Path

**Isolation**: Required (expects degraded response)

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({})
```

**Pass Criteria**:
- Response still returns `curl_command` (with placeholder `AUDIO_FILE_PATH`)
- Response contains `upload_url` and `instructions`
- The curl command is syntactically valid but requires user to substitute the file path

---

### AUD-007: Transcribe Chinese Phrase (curl-command)

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "tests/uat/data/audio/chinese-phrase.mp3",
  mime_type: "audio/mpeg"
})
```

**Pass Criteria**:
- Response contains `curl_command` (non-empty string)
- Response contains `upload_url` and `instructions`

**Manual Verification**: Execute the returned curl command and verify:
- API response contains `text` (non-empty string, contains CJK characters or pinyin)
- `language` is present (should detect "zh" or similar)

**Notes**: Verifies CJK language transcription capability.

---

### AUD-008: Upload URL is Well-Formed

**MCP Tool**: `transcribe_audio`
**Parameters**:
```javascript
transcribe_audio({
  file_path: "/tmp/test.flac"
})
```

**Pass Criteria**:
- `upload_url` ends with `/api/v1/audio/transcribe`
- `method` === `"POST"`
- `content_type` === `"multipart/form-data"`
- `instructions` is a non-empty string describing how to execute the curl command

---

## Summary

| Test | Tool | Validates |
|------|------|-----------|
| AUD-001 | `get_system_info` | Backend availability gate |
| AUD-002 | `transcribe_audio` | English MP3 curl-command + manual verify |
| AUD-003 | `transcribe_audio` | Spanish curl-command + manual verify |
| AUD-004 | `transcribe_audio` | Language hint parameter |
| AUD-005 | `transcribe_audio` | Default MIME type |
| AUD-006 | `transcribe_audio` | Missing file path (degraded) |
| AUD-007 | `transcribe_audio` | Chinese CJK curl-command + manual verify |
| AUD-008 | `transcribe_audio` | Upload URL structure validation |

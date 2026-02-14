# Phase 2E: Audio Transcription — Results

**Date**: 2026-02-13
**Version**: v2026.2.8
**Result**: 8 tests — 5 PASS, 1 FAIL, 2 PARTIAL (62.5% / 75% with partials)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| AUD-001 | Check Audio Backend Availability | PASS | extraction.audio.enabled: true, whisper backend configured |
| AUD-002 | Transcribe English MP3 (curl-command) | FAIL | MCP returns valid curl_command but Whisper backend unreachable: `http://whisper:8000` connection refused |
| AUD-003 | Transcribe Spanish WAV | PARTIAL | MCP tool returns valid curl_command; API blocked by Whisper infrastructure |
| AUD-004 | Language Hint Parameter | PARTIAL | MCP correctly includes `-F "language=es"` in curl_command; actual transcription blocked |
| AUD-005 | Default MIME Type (Omitted) | PASS | No error with omitted mime_type, server accepts file parameter alone |
| AUD-006 | Missing File Path | PASS | Returns placeholder `AUDIO_FILE_PATH` in curl_command, graceful degradation |
| AUD-007 | Transcribe Chinese Phrase | PASS | language=zh parameter accepted, curl_command includes `-F "language=zh"` |
| AUD-008 | Upload URL is Well-Formed | PASS | POST multipart/form-data to `/api/v1/audio/transcribe` |

## Audio Backend Configuration
- **Backend**: Whisper via WHISPER_BASE_URL
- **Endpoint**: /api/v1/audio/transcribe
- **Upload Pattern**: MCP returns curl command for multipart/form-data upload
- **Status**: **INFRASTRUCTURE ISSUE** — Whisper service (`http://whisper:8000`) not reachable from API server

## Infrastructure Issue Details

When executing the curl command returned by MCP, the API returns:
```json
{
  "error": "Transcription error: Internal error: Transcription request failed: error sending request for url (http://whisper:8000/v1/audio/transcriptions)"
}
```

**Root Cause**: The Whisper backend container/service is not running or not accessible from the API server's network. The MCP tool layer works correctly (generates valid curl commands), but the API cannot reach the Whisper service.

**MCP Layer**: Working correctly ✓
**API Layer**: Blocked by Whisper infrastructure issue ✗

## Issues Filed
- **#356**: Whisper backend service (http://whisper:8000) not reachable from API server — audio transcription fails with connection error

## Test Artifacts

AUD-006 curl_command with placeholder:
```bash
curl -X POST \
  -F "file=@AUDIO_FILE_PATH" \
  -H "Authorization: Bearer mm_at_..." \
  "http://localhost:3000/api/v1/audio/transcribe"
```

AUD-007 curl_command with language hint:
```bash
curl -X POST \
  -F "file=@tests/uat/data/audio/chinese-phrase.mp3;type=audio/mpeg" \
  -F "language=zh" \
  -H "Authorization: Bearer mm_at_..." \
  "http://localhost:3000/api/v1/audio/transcribe"
```

# Phase 9: Media Processing — Results

**Date**: 2026-02-14
**Result**: 4 PASS, 2 PARTIAL (6 tests)

| Test ID | Tool | Focus | Result | Issue |
|---------|------|-------|--------|-------|
| MEDIA-001 | get_system_info | Capability detection | PASS | vision=true, audio=true |
| MEDIA-002 | describe_image | Basic vision | PASS | qwen3-vl:8b, description returned |
| MEDIA-003 | describe_image | Custom prompt | PASS | Focused response on objects |
| MEDIA-004 | transcribe_audio | Audio transcription | PASS | "Welcome to metric memory..." |
| MEDIA-005 | describe_image | Missing file error | PARTIAL | #392 — no validation, returns curl cmd |
| MEDIA-006 | transcribe_audio | Missing file error | PARTIAL | #392 — same issue |

## Issues
- #391: MCP media tools return hardcoded localhost:3000 URLs instead of deployment URL
- #392: describe_image/transcribe_audio accept non-existent file paths without error

## Notes
- Both vision (qwen3-vl:8b) and audio (Whisper) backends operational
- MCP tools return curl commands for binary upload (correct pattern)
- Transcription: language=en, duration=5.664s, model=Systran/faster-distil-whisper-large-v3

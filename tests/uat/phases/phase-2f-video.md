# UAT Phase 2F: Video Processing

**Purpose**: Verify video processing guidance tool and attachment pipeline extraction for video files
**Duration**: ~10 minutes
**Prerequisites**: Phase 0 preflight passed, Phase 2b attachment uploads working
**Critical**: No (requires optional ffmpeg + vision/transcription backends)
**Tools Tested**: `process_video`, `get_system_info`, `create_note`, `upload_attachment`, `get_attachment`, `list_jobs`

> **Attachment Pipeline**: Video files are processed through the standard attachment pipeline — NOT via base64 ad-hoc API. The `process_video` MCP tool is a **guidance tool** that returns workflow instructions for agents. Actual processing happens when a video file is uploaded as an attachment and the background job worker extracts content.

> **Backend Requirements**: Full extraction requires:
> - **ffmpeg** in PATH (for keyframe extraction)
> - **OLLAMA_VISION_MODEL** set (for keyframe description — optional)
> - **WHISPER_BASE_URL** set (for audio transcription — optional)
>
> If ffmpeg is not available, VID-002 detects this and attachment pipeline tests (VID-004+) are marked SKIPPED. Guidance tool tests (VID-001, VID-003) always execute.

> **Test Data**: This phase uses video files from supplementary test media. If `/mnt/global/test-media/video/` is not available, the executor should provide at least one MP4 file (3-10 seconds, <5MB) for attachment pipeline testing.

---

## Tests

### VID-001: Check Video Extraction Backend Availability

**MCP Tool**: `get_system_info`
**Parameters**: `{}`

**Pass Criteria**:
- Response includes `extraction.video` object
- Record `extraction.video.enabled` value
- Record `extraction.video.ffmpeg_available` (boolean)
- If `ffmpeg_available === true`: continue to VID-002
- If `ffmpeg_available === false`: mark VID-004 through VID-010 as SKIPPED (guidance tests VID-002, VID-003 still execute)

**Notes**: Gate test for attachment pipeline tests. Guidance tool tests always run regardless.

---

### VID-002: Guidance Tool — No Note ID

**MCP Tool**: `process_video`
**Parameters**:
```javascript
process_video({
  filename: "test-clip.mp4"
})
```

**Pass Criteria**:
- Response contains `workflow` field with value `"attachment_pipeline"`
- Response contains `message` (non-empty string mentioning "attachment pipeline")
- Response contains `steps` (array with 5 entries — includes note creation step)
- Response contains `supported_formats` (array including `"video/mp4"`)
- Response contains `requires` object with `ffmpeg` key
- Response contains `extraction_features` object with `keyframe_extraction` key
- Step 1 mentions `create_note`
- Step 2 mentions `upload_attachment`

**Notes**: Verifies the guidance tool returns proper workflow instructions when no note ID is provided.

---

### VID-003: Guidance Tool — With Note ID

**MCP Tool**: `process_video`
**Parameters**:
```javascript
process_video({
  note_id: "00000000-0000-0000-0000-000000000000",
  filename: "meeting-recording.webm"
})
```

**Pass Criteria**:
- Response contains `workflow` field with value `"attachment_pipeline"`
- Response contains `steps` (array with 4 entries — no note creation step)
- Step 1 mentions `upload_attachment` with the provided note_id
- Response contains `supported_formats` (array including `"video/webm"`)

**Notes**: Verifies the guidance tool adapts instructions when an existing note ID is provided.

---

### VID-004: Create Note for Video Upload

**MCP Tool**: `create_note`
**Parameters**:
```javascript
create_note({
  title: "UAT Video: Test Clip",
  body: "Video uploaded for extraction pipeline testing"
})
```

**Pass Criteria**:
- Response contains `id` (UUID string)
- Response contains `title` matching input
- Save `note_id` for VID-005

**Notes**: Creates the parent note for video attachment. Required by attachment pipeline.

---

### VID-005: Upload Video Attachment

**MCP Tool**: `upload_attachment`
**Parameters**:
```javascript
upload_attachment({
  note_id: "<note_id from VID-004>",
  filename: "test-clip.mp4",
  content_type: "video/mp4"
})
```

Then execute the returned curl command with an actual MP4 file.

**Pass Criteria**:
- `upload_attachment` returns a curl command template
- Executing the curl command returns 200/201 with attachment metadata
- Response contains `id` (attachment UUID)
- Response contains `extraction_strategy` — should be `"video_multimodal"` or equivalent
- Save `attachment_id` for VID-006

**Notes**: Binary upload via curl (approved exception per MCP-First policy). The upload triggers background extraction.

---

### VID-006: Check Extraction Job Created

**MCP Tool**: `list_jobs`
**Parameters**:
```javascript
list_jobs({
  limit: 5,
  status: "pending"
})
```

**Pass Criteria**:
- Response contains at least one job related to the uploaded attachment
- OR: If extraction is fast, job may already be `completed` — check with `status: "completed"` too

**Notes**: Verifies the attachment upload triggered a background extraction job.

---

### VID-007: Wait and Check Extraction Results

**MCP Tool**: `get_attachment`
**Parameters**:
```javascript
get_attachment({
  id: "<attachment_id from VID-005>"
})
```

**Pass Criteria**:
- Response contains attachment metadata
- `extraction_strategy` is `"video_multimodal"` or similar
- If extraction completed: `extraction_metadata` contains extracted content
- If extraction pending: retry after 10 seconds (max 3 retries)
- Extraction metadata (when present) should contain one or more of:
  - `keyframes` or `frames` (array of extracted frame descriptions)
  - `transcript` or `transcription` (audio transcription text)
  - `description` or `ai_description` (composite video description)
  - `metadata` with `duration`, `format`, or `resolution` info

**Notes**: Extraction may take 30-60 seconds depending on video length and backend speed. If backends (vision, whisper) are not configured, extraction metadata will be minimal (ffmpeg metadata only).

---

### VID-008: Video Content Searchable After Extraction

**MCP Tool**: `search_notes`
**Parameters**:
```javascript
search_notes({
  query: "UAT Video Test Clip",
  limit: 5
})
```

**Pass Criteria**:
- Results include the note created in VID-004
- Note appears in search results (title match at minimum)

**Notes**: After extraction completes, video content should be indexed and searchable.

---

### VID-009: Upload Video with No Note (Auto-Create)

**Isolation**: Recommended (may create note automatically or return error)

**MCP Tool**: `process_video`
**Parameters**:
```javascript
process_video({
  filename: "orphan-video.mp4"
})
```

**Pass Criteria**:
- Response contains workflow instructions with note creation as step 1
- Instructions correctly guide agent to create note first

**Notes**: Verifies guidance tool correctly instructs agents to create a note when none exists. The tool does NOT auto-create notes — it provides guidance.

---

### VID-010: Unsupported Video Format Guidance

**MCP Tool**: `process_video`
**Parameters**:
```javascript
process_video({
  filename: "video.xyz"
})
```

**Pass Criteria**:
- Response still returns workflow instructions (guidance tool doesn't validate formats)
- `supported_formats` list helps agent identify valid formats
- No error thrown (guidance tools are informational)

**Notes**: The guidance tool returns instructions regardless of filename. Format validation happens at upload time.

---

## Phase Summary

| Test ID | Test Name | Status | Notes |
|---------|-----------|--------|-------|
| VID-001 | Check Video Extraction Backend | | Gate test for pipeline tests |
| VID-002 | Guidance Tool — No Note ID | | Always executes |
| VID-003 | Guidance Tool — With Note ID | | Always executes |
| VID-004 | Create Note for Video Upload | | Requires ffmpeg |
| VID-005 | Upload Video Attachment | | Requires ffmpeg + test video |
| VID-006 | Check Extraction Job Created | | Requires ffmpeg |
| VID-007 | Wait and Check Extraction Results | | Requires ffmpeg + backends |
| VID-008 | Video Content Searchable | | After extraction |
| VID-009 | Upload Video with No Note | | Guidance validation |
| VID-010 | Unsupported Format Guidance | | Always executes |

**Total Tests**: 10
**Isolation Tests**: 1 (VID-009)
**Always-Execute Tests**: 4 (VID-001, VID-002, VID-003, VID-010)
**Conditional Tests**: 6 (VID-004 through VID-009, require ffmpeg)

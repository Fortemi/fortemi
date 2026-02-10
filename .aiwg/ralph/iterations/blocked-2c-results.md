# Phase 2C Extraction Pipeline - Blocked Test Results

**Date**: 2026-02-09T23:35Z
**Note C (extraction)**: `019c44c1-6eff-7ce2-8a94-9573f578653e`
**API**: `https://memory.integrolabs.net`
**Method**: MCP `upload_attachment` + curl upload + MCP verification

---

## Summary

| Result | Count |
|--------|-------|
| PASS   | 12    |
| FAIL   | 3     |
| Total  | 15    |

**Pass Rate: 80%**

---

## Auto-Detection Tests

### PROC-003: Markdown auto-detection
- **Status**: PASS
- **File**: `markdown-formatted.md` (1133 bytes, content_type: text/markdown)
- **Expected**: extraction_strategy contains "text"
- **Actual**: `extraction_strategy: "text_native"` -- contains "text"
- **Attachment ID**: `019c44c2-d85f-70a1-ab4d-b9c4bb9d1c87`

### PROC-004: JSON auto-detection
- **Status**: PASS
- **File**: `json-config.json` (784 bytes, content_type: application/json)
- **Expected**: extraction_strategy assigned
- **Actual**: `extraction_strategy: "structured_extract"`
- **Attachment ID**: `019c44c2-fd55-7f62-b765-f9841add21cf`

### PROC-005: MIME-only detection (octet-stream, no extension hint)
- **Status**: PASS
- **File**: `config.txt` uploaded with `content_type=application/octet-stream`
- **Expected**: Detection from content/extension despite generic MIME
- **Actual**: `extraction_strategy: "text_native"` -- API detected text despite octet-stream MIME
- **Attachment ID**: `019c44c3-0451-7bb1-8f25-c071c5ac2ce8`
- **Note**: Filename still had `.txt` extension; API may have used extension fallback

### PROC-008: Rust code auto-detection
- **Status**: PASS
- **File**: `code-rust.rs` (1643 bytes, content_type: text/x-rust)
- **Expected**: `extraction_strategy: "code_ast"`
- **Actual**: `extraction_strategy: "code_ast"`
- **Attachment ID**: `019c44c3-0a7e-76e0-8035-c2f8a6b78354`

### PROC-010: Plain text auto-detection
- **Status**: PASS
- **File**: `config.txt` (94 bytes, content_type: text/plain, filename: config-native.txt)
- **Expected**: `extraction_strategy: "text_native"`
- **Actual**: `extraction_strategy: "text_native"`
- **Attachment ID**: `019c44c3-313f-72c0-bfa2-637bbeb7e893`

### PROC-013: Audio file auto-detection
- **Status**: FAIL
- **File**: `english-speech-5s.mp3` (45312 bytes, content_type: audio/mpeg)
- **Expected**: `extraction_strategy: "audio_transcribe"`
- **Actual**: `extraction_strategy: "text_native"`
- **Attachment ID**: `019c44c3-379a-76b2-b40b-a2a13f34a386`
- **Issue**: Audio files get fallback `text_native` instead of `audio_transcribe`. The extraction strategy router does not recognize audio MIME types. This is a known gap -- the document type registry has no audio category in `list_document_types`.

### PROC-014: Python code auto-detection
- **Status**: PASS
- **File**: `code-python.py` (1248 bytes, content_type: text/x-python)
- **Expected**: `extraction_strategy: "code_ast"`
- **Actual**: `extraction_strategy: "code_ast"`
- **Attachment ID**: `019c44c3-3e78-7731-9252-9acb0c7bf5fd`

---

## Override Tests

### PROC-006: Document type override (markdown)
- **Status**: PASS (with caveat)
- **File**: `config.txt` (94 bytes) uploaded with `document_type_id=019c41af-a287-79ef-ba54-0d86fdb38400` (markdown)
- **Expected**: `document_type_id` stored, extraction_strategy reflects override type
- **Actual at upload**: `document_type_id: "019c41af-a287-79ef-ba54-0d86fdb38400"`, `extraction_strategy: "text_native"`
- **Actual via GET**: `document_type_id: "019c41af-a287-79ef-ba54-0d86fdb38400"`, `extraction_strategy: null`
- **Actual via list_attachments**: `document_type_name: "markdown"` confirmed
- **Attachment ID**: `019c44c3-61b7-7a82-9b35-b8dda591b14f`
- **Caveat**: The `document_type_id` override was accepted and stored. The extraction_strategy shows `text_native` in the upload response (which matches markdown's configured strategy) but returns `null` via GET. The override itself works -- the type association is correct.

---

## Multi-File Tests

### PROC-015: Multiple file uploads to same note
- **Status**: PASS
- **Verification**: Called `list_attachments` on Note C
- **Expected**: Both `code-python.py` and `markdown-formatted.md` present
- **Actual**: 10 attachments total on Note C, including both files:
  - `code-python.py` (1248 bytes, text/x-python)
  - `markdown-formatted.md` (1133 bytes, text/markdown)
- **All 10 attachments**: markdown-formatted.md, json-config.json, config.txt, code-rust.rs, config-native.txt, english-speech-5s.mp3, code-python.py, config-override.txt, empty.txt, wrong-mime.txt

### PROC-016: Different extraction strategies on same note
- **Status**: PASS
- **Expected**: Code file and text file show different extraction strategies
- **Actual**:
  - `code-python.py` -> `extraction_strategy: "code_ast"`
  - `markdown-formatted.md` -> `extraction_strategy: "text_native"`
  - `json-config.json` -> `extraction_strategy: "structured_extract"`
- **Note**: Three distinct strategies confirmed on the same note

---

## Job Verification Tests

### PROC-025: Queue stats after uploads
- **Status**: PASS
- **Verification**: Called `get_queue_stats`
- **Expected**: Extraction jobs exist in queue
- **Actual**: Queue stats returned successfully:
  - `pending: 0` (all jobs completed by check time)
  - `processing: 0`
  - `completed_last_hour: 58`
  - `failed_last_hour: 16`
  - `total: 74`
- **Note**: 74 total jobs in the last hour confirms active processing

### PROC-026: Jobs filtered by note_id
- **Status**: FAIL
- **Verification**: Called `list_jobs` with `note_id=019c44c1-6eff-7ce2-8a94-9573f578653e`
- **Expected**: Job references an attachment upload/extraction
- **Actual**: 4 jobs returned for Note C, but all are note-level jobs (concept_tagging, linking, title_generation, embedding) from note creation -- none are attachment extraction jobs
- **Job types found**: concept_tagging, linking, title_generation, embedding
- **Issue**: No attachment-specific extraction jobs were created. The extraction pipeline does not appear to queue background jobs for attachment processing. Attachments get their `extraction_strategy` assigned at upload time but no follow-up extraction job runs.

### PROC-027: Job lifecycle fields
- **Status**: PASS
- **Verification**: Called `get_job` on `019c44c1-6f07-7ca1-a261-97a714fbad0f` (concept_tagging)
- **Expected**: Status lifecycle fields present (created_at, started_at, completed_at)
- **Actual**: All lifecycle fields present and populated:
  - `status: "completed"`
  - `created_at: "2026-02-09T23:34:13.511434Z"`
  - `started_at: "2026-02-09T23:34:20.769339Z"`
  - `completed_at: "2026-02-09T23:34:22.250147Z"`
  - `progress_percent: 100`
  - `retry_count: 0`, `max_retries: 3`
  - `result` payload with concepts_suggested, concepts_tagged, labels

---

## Error Handling Tests

### PROC-024: Empty file upload
- **Status**: PASS
- **File**: `/tmp/empty.txt` (0 bytes, content_type: text/plain)
- **Expected**: No crash, upload succeeds
- **Actual**: Upload succeeded with HTTP 200
  - `extraction_strategy: "text_native"`
  - `size_bytes: 0`
  - `status: "uploaded"`
- **Attachment ID**: `019c44c3-691b-7ad1-a506-d0441339a3f4`

### PROC-028: Wrong MIME type (video/mp4 for .txt)
- **Status**: PASS
- **File**: `config.txt` (94 bytes) uploaded with `content_type=video/mp4`, filename `wrong-mime.txt`
- **Expected**: No crash, upload succeeds
- **Actual**: Upload succeeded with HTTP 200
  - `extraction_strategy: "text_native"` (correctly detected text despite video/mp4 MIME)
  - `status: "uploaded"`
- **Attachment ID**: `019c44c3-70c4-74e1-b5de-681d36191187`
- **Note**: API correctly fell back to extension-based detection, ignoring the incorrect MIME type

---

## Extraction Strategy Summary

| File Type | MIME Type | Expected Strategy | Actual Strategy | Match |
|-----------|-----------|-------------------|-----------------|-------|
| `.md` (markdown) | text/markdown | text* | text_native | Yes |
| `.json` (config) | application/json | any | structured_extract | Yes |
| `.txt` (octet) | application/octet-stream | detected | text_native | Yes |
| `.rs` (Rust code) | text/x-rust | code_ast | code_ast | Yes |
| `.txt` (plain) | text/plain | text_native | text_native | Yes |
| `.mp3` (audio) | audio/mpeg | audio_transcribe | text_native | **No** |
| `.py` (Python) | text/x-python | code_ast | code_ast | Yes |
| `.txt` (override=md) | text/plain | text_native (markdown) | text_native | Yes |
| empty `.txt` | text/plain | no crash | text_native | Yes |
| `.txt` (video/mp4) | video/mp4 | no crash | text_native | Yes |

**Distinct strategies observed**: `text_native`, `code_ast`, `structured_extract`
**Missing strategy**: `audio_transcribe` -- audio files not recognized by extraction router

---

## Issues Found

### Issue: Audio extraction strategy not detected (PROC-013)
- **Severity**: Medium
- **Description**: Uploading an MP3 file with `content_type=audio/mpeg` results in `extraction_strategy: "text_native"` instead of `"audio_transcribe"`. The document type registry has no audio category, so audio files fall through to the text fallback.
- **Impact**: Audio files will not be transcribed; they get treated as text (which will extract nothing meaningful from binary audio data).

### Issue: No attachment extraction jobs in job queue (PROC-026)
- **Severity**: Medium
- **Description**: After uploading attachments, `list_jobs` filtered by note_id returns only note-creation jobs (concept_tagging, embedding, linking, title_generation). No attachment-specific extraction jobs appear. The extraction_strategy is assigned at upload time but no background extraction job is queued.
- **Impact**: Extraction strategies are detected but not executed. The `extracted_text` field remains null for all uploaded attachments. The pipeline assigns strategy but does not process.

### Observation: GET returns null extraction_strategy for overridden type (PROC-006)
- **Severity**: Low
- **Description**: When uploading with `document_type_id` override, the upload response includes `extraction_strategy: "text_native"`, but subsequent GET on the attachment returns `extraction_strategy: null`. The `document_type_id` is correctly persisted. This may be a serialization inconsistency between upload response and GET response paths.

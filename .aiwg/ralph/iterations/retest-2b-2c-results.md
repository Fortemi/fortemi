# UAT Retest: Phases 2B & 2C — 2026-02-10

**System**: Matric Memory v2026.2.8 at https://memory.integrolabs.net
**Test Notes**: Note A (`019c44c1-5cee-7833-be0f-e7e6b16ae286`), Note B (`019c44c1-6353-7673-872f-c6c091ff08e0`), Note C (`019c44c1-6eff-7ce2-8a94-9573f578653e`)

## Results

| Test ID | Name | Result | Evidence |
|---------|------|--------|----------|
| UAT-2B-010 | EXIF GPS extraction | **FAIL** | Uploaded `exif-test-paris.jpg` (ID `019c453e-8191-7d03-a29d-8bf03530a862`) with known GPS EXIF (48.858N, 2.294E). After 18s wait, `get_attachment` shows `extracted_metadata: null`. Original paris-eiffel-tower.jpg (uploaded 2+ hours prior, ID `019c44c3-58fb-7c00-bce0-172729eaa814`) also has `extracted_metadata: null`. EXIF extraction is not implemented. |
| UAT-2B-011 | EXIF camera Make/Model | **FAIL** | Same attachment `019c453e-8191-7d03-a29d-8bf03530a862`. Source file contains `Make: Canon`, `Model: EOS R5` (verified via PIL). `extracted_metadata: null` -- no camera info extracted. |
| UAT-2B-012 | EXIF DateTimeOriginal | **FAIL** | Same attachment `019c453e-8191-7d03-a29d-8bf03530a862`. Source file contains `DateTimeOriginal: 2024:07:14 12:00:00` (verified via PIL). `extracted_metadata: null` -- no date info extracted. |
| UAT-2B-015a | Magic byte validation | **FAIL** | Created plain text file (`echo "This is just plain text"`) and uploaded as `fake.pdf` with `content_type=application/pdf`. API accepted it (HTTP 200) with `extraction_strategy: "pdf_text"`. Stored `content_type: "application/octet-stream"` (partial mitigation -- server didn't trust the claimed type for storage) but still assigned PDF extraction strategy based on filename extension. No magic byte validation. |
| UAT-2B-021a | Empty/missing content_type | **PASS** | Uploaded file without explicit content_type. API accepted (HTTP 200), auto-detected as `application/octet-stream`, `extraction_strategy: "text_native"`. No crash or error. |
| UAT-2B-021b | Special chars in content_type | **PASS** | Uploaded with `type=text/<script>alert(1)</script>`. API accepted (HTTP 200), stored as `content_type: "application/octet-stream"` (sanitized). No crash, XSS payload not stored. |
| UAT-2B-022 | Upload to non-existent note | **PASS** | Uploaded to `00000000-0000-0000-0000-000000000000`. API returned HTTP 400 with `{"error":"Referenced resource not found"}`. Proper error handling (400 is acceptable; 404 would be more precise). |
| PROC-013 | Audio extraction_strategy | **PASS** | Uploaded `test-audio.mp3` (720KB, content_type=audio/mpeg) to Note C. Response: `extraction_strategy: "audio_transcribe"`. This is a FIX -- the earlier upload (`english-speech-5s.mp3`, ID `019c44c3-379a-76b2-b40b-a2a13f34a386`) still shows `extraction_strategy: "text_native"` (the original bug). New uploads correctly route audio to transcription. |
| PROC-026 | Attachment extraction jobs | **FAIL** | Called `list_jobs(note_id=019c44c1-6eff-7ce2-8a94-9573f578653e)`. Found 4 jobs: `concept_tagging`, `linking`, `title_generation`, `embedding` -- all note-level. No attachment extraction jobs (e.g., `attachment_extraction`, `text_extraction`, `audio_extraction`). Note C has 11 attachments (including MP3, Markdown, JSON, Python, Rust, text) but none triggered extraction jobs. |
| PROC-027-retest | Extraction jobs in queue | **FAIL** | `get_queue_stats` returns: `pending: 0, processing: 0, completed_last_hour: 6, failed_last_hour: 6, total: 86`. Reviewed all 86 jobs -- job types are exclusively: `ai_revision`, `embedding`, `linking`, `title_generation`, `concept_tagging`. Zero extraction-type jobs exist anywhere in the system. `list_jobs(status=failed)` shows only note-not-found errors for deleted notes, not extraction failures. |

## Summary

| Result | Count |
|--------|-------|
| PASS   | 4     |
| FAIL   | 6     |
| **Total** | **10** |

## Analysis

### Fixed Since Last Run
- **PROC-013** (#279): Audio extraction strategy now correctly returns `audio_transcribe` for new MP3 uploads. Previously returned `text_native`. Confirmed fix.

### Still Failing
- **#278 EXIF Metadata** (UAT-2B-010/011/012): `extracted_metadata` remains null for all JPEG attachments regardless of age. EXIF parsing pipeline is not implemented. The `extraction_strategy` for JPEGs is `vision` but no actual extraction occurs.
- **#253 Magic Byte Validation** (UAT-2B-015a): Plain text files uploaded with PDF content_type are accepted without magic byte validation. The server does sanitize the stored content_type to `application/octet-stream`, but the extraction_strategy is derived from the filename extension (`.pdf` -> `pdf_text`), not from actual file content inspection.
- **#280 Extraction Jobs** (PROC-026/027): No attachment-level extraction jobs exist in the system. Attachments are stored with `status: "uploaded"` and `extraction_strategy` is assigned, but no background jobs are created to actually execute the extraction. The job worker only processes note-level jobs (ai_revision, embedding, linking, title_generation, concept_tagging).

### New Observations
- Content_type sanitization works: invalid/empty/special-character content types are normalized to `application/octet-stream`
- Non-existent note uploads properly rejected with HTTP 400
- Content-hash deduplication works (multiple uploads of same file share `blob_id: 019c453f-536b-7e12-91a1-2b7ffcb8ea5f`)

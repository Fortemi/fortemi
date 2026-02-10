# UAT Retest: Phase 2B/2C Attachment Processing

**Date**: 2026-02-10T04:04Z
**System**: https://memory.integrolabs.net (post-update)
**Tester**: Claude Agent (automated)

## Results Summary

| Test ID | Description | Result | Issue |
|---------|-------------|--------|-------|
| UAT-2B-010 | EXIF GPS extraction | **FAIL** | #278 |
| UAT-2B-011 | EXIF camera Make/Model extraction | **FAIL** | #278 |
| UAT-2B-012 | EXIF DateTimeOriginal extraction | **FAIL** | #278 |
| UAT-2B-015a | Magic byte validation | **FAIL** | #253 |
| PROC-026 | Attachment extraction jobs | **FAIL** | #280 |
| PROC-027 | Queue stats for extraction types | **FAIL** | #280 |

**Overall: 0/6 PASS, 6/6 FAIL**

---

## Detailed Evidence

### UAT-2B-010/011/012: EXIF Metadata Extraction (#278)

**Procedure:**
1. Checked existing JPEG attachments on Note A (`019c44c1-5cee-7833-be0f-e7e6b16ae286`)
2. Found two previously uploaded JPEGs (`paris-eiffel-tower.jpg`, `exif-test-paris.jpg`) -- both had `extracted_metadata: null`
3. Uploaded a fresh JPEG with known EXIF data (Canon EOS R5, GPS 48.8584/2.2945, DateTime 2024:07:14)
4. Waited 20 seconds for async processing
5. Re-checked -- still `extracted_metadata: null`

**Upload response** (new attachment `019c45b8-84e4-7f91-846e-1e390b60e3a5`):
```json
{
  "status": "uploaded",
  "extraction_strategy": "vision",
  "extracted_text": null,
  "extracted_metadata": null,
  "detected_document_type_id": null,
  "detection_method": null
}
```

**File verification** (local `file` command confirms EXIF data is present):
```
paris-eiffel-tower.jpg: JPEG image data, Exif standard: [TIFF image data, big-endian,
  manufacturer=Canon, model=EOS R5, datetime=2024:07:14 12:00:00, GPS-Data],
  baseline, precision 8, 3840x2160
```

**Root cause**: No EXIF extraction pipeline exists. The `extraction_strategy` is set to `vision` (correct for images) but no job processes the attachment binary to extract EXIF metadata. No attachment-level extraction jobs are created at upload time.

**Verdict**:
- **UAT-2B-010 (GPS)**: FAIL -- `extracted_metadata` is null, no GPS coordinates extracted
- **UAT-2B-011 (Camera)**: FAIL -- `extracted_metadata` is null, no Make/Model extracted
- **UAT-2B-012 (DateTime)**: FAIL -- `extracted_metadata` is null, no DateTimeOriginal extracted

---

### UAT-2B-015a: Magic Byte Validation (#253)

**Procedure:**
1. Created a plain text file: `echo "This is plain text, not a PDF" > /tmp/fake.pdf` (30 bytes, ASCII text)
2. Uploaded to Note B (`019c44c1-6353-7673-872f-c6c091ff08e0`) with `content_type=application/pdf`
3. Server accepted the upload (HTTP 200)

**Upload response** (attachment `019c45b8-9a35-7500-8c7e-c37aa762e565`):
```json
{
  "status": "uploaded",
  "extraction_strategy": "pdf_text",
  "extracted_text": null,
  "extracted_metadata": null,
  "content_type": null
}
```

**Analysis:**
- Server accepted a plain text file claimed to be `application/pdf` without rejection
- `extraction_strategy` was set to `pdf_text` based on the declared MIME type (not actual file content)
- No magic byte validation occurred -- the file's first bytes are `This is pl...`, not the PDF magic bytes `%PDF-`
- The list_attachments endpoint shows `content_type: "application/octet-stream"` (suggests some normalization but no content-based detection)
- `detected_document_type_id` and `detection_method` are both null -- no content-based detection ran

**Verdict**: FAIL -- Server accepted the upload and assigned `pdf_text` extraction strategy to a non-PDF file. No magic byte validation or content-type verification occurred.

---

### PROC-026: Attachment Extraction Jobs (#280)

**Procedure:**
1. Called `list_jobs` for Note C (`019c44c1-6eff-7ce2-8a94-9573f578653e`)
2. Examined all job types

**Jobs found for Note C:**
| Job Type | Status |
|----------|--------|
| `concept_tagging` | completed |
| `linking` | completed |
| `title_generation` | completed |
| `embedding` | completed |

**All 86 jobs in the system by type:**
| Job Type | Count |
|----------|-------|
| `ai_revision` | 7 |
| `concept_tagging` | 20 |
| `embedding` | 20 |
| `linking` | 19 |
| `re_embed_all` | 1 |
| `title_generation` | 19 |

**Verdict**: FAIL -- No attachment-level extraction jobs exist anywhere in the system. All jobs are note-level processing (embedding, linking, tagging, revision, title generation). There is no `attachment_extraction`, `exif_extraction`, `text_extraction`, or similar job type. Attachment uploads do not trigger extraction jobs.

---

### PROC-027: Queue Stats for Extraction Types

**Procedure:**
1. Called `get_queue_stats`

**Response:**
```json
{
  "pending": 0,
  "processing": 0,
  "completed_last_hour": 0,
  "failed_last_hour": 0,
  "total": 86
}
```

**Job type enumeration** (from `list_jobs` schema): The allowed `job_type` filter values are:
`ai_revision`, `embedding`, `linking`, `context_update`, `title_generation`

No extraction-related job types exist in the schema.

**Verdict**: FAIL -- No extraction job types exist in the queue system. The job type enum does not include any attachment extraction variant. The attachment processing pipeline does not create background jobs for content extraction.

---

## Summary

All 6 tests failed. The core issue is that the **attachment extraction pipeline is not implemented**:

1. **No EXIF extraction** (#278): Uploading images with EXIF data does not populate `extracted_metadata`. No background job processes attachment binaries for metadata extraction.

2. **No magic byte validation** (#253): The server trusts the declared `content_type` without verifying against the file's actual magic bytes. A plain text file uploaded as `application/pdf` is accepted and assigned `pdf_text` extraction strategy.

3. **No attachment extraction jobs** (#280): The job queue system only has note-level job types (embedding, linking, concept_tagging, title_generation, ai_revision). No attachment-level extraction job type exists in the schema or in practice.

These three issues are interconnected: without an attachment extraction job pipeline, neither EXIF extraction nor magic byte validation can run asynchronously after upload.

# UAT Phase 2B: File Attachments

**Purpose**: Verify file attachment system with deduplication, EXIF extraction, and safety validation
**Duration**: ~15 minutes
**Prerequisites**: Phase 1 seed data exists, test data generated
**Critical**: Yes (100% pass required)
**Tools Tested**: `create_note`, `upload_attachment`, `list_attachments`, `get_attachment`, `download_attachment`, `delete_attachment`

> **Attachment Workflow Architecture**: The attachment system uses a hybrid HTTP+MCP approach optimized for binary file transfer:
>
> **For Remote MCP Deployments** (the common case):
> - **Upload**: Use HTTP API with base64-encoded JSON body
> - **Manage**: Use MCP tools (`list_attachments`, `get_attachment`, `delete_attachment`) for metadata
> - **Download**: Use HTTP API to retrieve binary data
>
> **For Co-located MCP Deployments** (MCP server on same machine):
> - **Upload**: Use MCP `upload_attachment` tool with `file_path` parameter
> - **Manage**: Same MCP tools
> - **Download**: Use MCP `download_attachment` tool with `output_dir` parameter
>
> **Why This Architecture**:
> - Binary data transfer via HTTP API avoids passing large base64 payloads through MCP protocol
> - MCP tools excel at metadata operations (list, get details, delete)
> - Co-located deployments can use filesystem-based I/O for efficiency

> **HTTP Upload Pattern** (for remote deployments):
> ```bash
> # 1. Encode file and upload via HTTP API
> python3 -c "import base64,json; data=base64.b64encode(open('FILE_PATH','rb').read()).decode(); print(json.dumps({'filename':'NAME','content_type':'MIME','data':data}))" > /tmp/upload.json
> curl -s -X POST -H 'Content-Type: application/json' -d @/tmp/upload.json \
>   https://memory.integrolabs.net/api/v1/notes/{note_id}/attachments
> ```
>
> **HTTP Download Pattern**:
> ```bash
> curl -s -o OUTPUT_FILE https://memory.integrolabs.net/api/v1/attachments/{attachment_id}/download
> ```

> **Test Data**: This phase uses files from `tests/uat/data/`. Generate with:
> ```bash
> cd tests/uat/data/scripts && ./generate-test-data.sh
> ```
> Key files: `images/jpeg-with-exif.jpg` (EXIF/GPS), `provenance/paris-eiffel-tower.jpg` (GPS coords),
> `documents/code-python.py` (code), `edge-cases/binary-wrong-ext.jpg` (safety validation)

---

## File Upload - Basic

### UAT-2B-001: Upload Image File

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload a JPEG image and verify attachment record creation

**Prerequisites**:
- Test note exists from Phase 1
- JPEG image file available: `tests/uat/data/images/jpeg-with-exif.jpg` (or any <10MB JPEG)

**Steps**:
1. Create a test note: `create_note({ content: "# Photo Test", tags: ["uat/attachments"], revision_mode: "none" })`
2. Store the returned note ID as `attachment_test_note_id`
3. **Co-located Upload** (MCP): `upload_attachment({ note_id: attachment_test_note_id, file_path: "tests/uat/data/images/jpeg-with-exif.jpg", content_type: "image/jpeg" })`
4. **Remote Upload** (HTTP API):
   ```bash
   python3 -c "import base64,json; data=base64.b64encode(open('tests/uat/data/images/jpeg-with-exif.jpg','rb').read()).decode(); print(json.dumps({'filename':'jpeg-with-exif.jpg','content_type':'image/jpeg','data':data}))" > /tmp/upload.json
   curl -s -X POST -H 'Content-Type: application/json' -d @/tmp/upload.json \
     https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments
   ```

**Expected Results**:
- Returns `{ id: "<attachment-uuid>", note_id: "<note-uuid>", filename: "jpeg-with-exif.jpg", content_type: "image/jpeg", size_bytes: <size>, status: "uploaded" }`
- Attachment ID is UUIDv7 format
- `created_at` timestamp is present
- Filename defaults to basename of `file_path` (MCP) or `filename` field (HTTP)

**Verification**:
- `list_attachments({ note_id: attachment_test_note_id })` returns 1 attachment
- Attachment matches uploaded file metadata

**Store**: `attachment_image_id`

---

### UAT-2B-002: Upload PDF Document

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload a PDF file and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- PDF file available (test-document.pdf)

**Steps**:
1. **Co-located**: `upload_attachment({ note_id: attachment_test_note_id, file_path: "tests/uat/data/documents/test-document.pdf", content_type: "application/pdf" })`
2. **Remote**: Use HTTP upload pattern with PDF file

**Expected Results**:
- Returns attachment record with `content_type: "application/pdf"`
- Status is "uploaded"

**Verification**:
- `list_attachments({ note_id: attachment_test_note_id })` returns 2 attachments (image + PDF)

**Store**: `attachment_pdf_id`

---

### UAT-2B-003: Upload Audio File

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload an audio file (MP3) and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- Audio file available (test-audio.mp3)

**Steps**:
1. **Co-located**: `upload_attachment({ note_id: attachment_test_note_id, file_path: "tests/uat/data/audio/test-audio.mp3", content_type: "audio/mpeg" })`
2. **Remote**: Use HTTP upload pattern

**Expected Results**:
- Returns attachment record with `content_type: "audio/mpeg"`
- Status is "uploaded"

**Store**: `attachment_audio_id`

---

### UAT-2B-004: Upload Video File

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload a video file (MP4) and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- Video file available (test-video.mp4, preferably small <50MB)

**Steps**:
1. **Co-located**: `upload_attachment({ note_id: attachment_test_note_id, file_path: "tests/uat/data/video/test-video.mp4", content_type: "video/mp4" })`
2. **Remote**: Use HTTP upload pattern

**Expected Results**:
- Returns attachment record with `content_type: "video/mp4"`
- Status is "uploaded"

**Store**: `attachment_video_id`

---

### UAT-2B-005: Upload 3D Model File

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload a 3D model file (GLB/GLTF) and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- 3D model file available (test-model.glb)

**Steps**:
1. **Co-located**: `upload_attachment({ note_id: attachment_test_note_id, file_path: "tests/uat/data/models/test-model.glb", content_type: "model/gltf-binary" })`
2. **Remote**: Use HTTP upload pattern

**Expected Results**:
- Returns attachment record with `content_type: "model/gltf-binary"`
- Status is "uploaded"

**Store**: `attachment_3d_id`

---

## File Download and Integrity

### UAT-2B-006: Download File and Verify Integrity

**MCP Tool**: `download_attachment` (co-located) OR HTTP API (remote)

**Description**: Download uploaded image and verify content matches

**Prerequisites**:
- `attachment_image_id` from UAT-2B-001

**Steps**:
1. **Co-located**: `download_attachment({ id: attachment_image_id, output_dir: "/tmp/uat" })`
2. **Remote**:
   ```bash
   curl -s -o /tmp/uat/jpeg-with-exif.jpg https://memory.integrolabs.net/api/v1/attachments/{attachment_image_id}/download
   ```
3. Compute BLAKE3 hash of saved file
4. Compare with original file hash: `tests/uat/data/images/jpeg-with-exif.jpg`

**Expected Results**:
- MCP returns `{ saved_to: "/tmp/uat/jpeg-with-exif.jpg", filename: "jpeg-with-exif.jpg", size_bytes: <size>, content_type: "image/jpeg" }`
- HTTP returns binary data with `Content-Type: image/jpeg` and `Content-Disposition` headers
- `saved_to` path exists on disk
- File size matches original

**Verification**:
- Downloaded file is byte-for-byte identical to uploaded file
- BLAKE3 hash matches: `b3sum /tmp/uat/jpeg-with-exif.jpg` vs `b3sum tests/uat/data/images/jpeg-with-exif.jpg`

---

### UAT-2B-007: Download Non-Existent Attachment

**Isolation**: Required — negative test expects error response

**MCP Tool**: `download_attachment` (co-located) OR HTTP API (remote)

**Description**: Attempt to download non-existent attachment and verify error handling

**Prerequisites**: None

**Steps**:
1. **Co-located**: `download_attachment({ id: "00000000-0000-0000-0000-000000000000", output_dir: "/tmp/uat" })`
2. **Remote**: `curl -s -o /tmp/uat/test.bin https://memory.integrolabs.net/api/v1/attachments/00000000-0000-0000-0000-000000000000/download`

**Expected Results**:
- Returns error with status 404
- Error message: "Attachment not found"
- No crash or panic
- No file written to output_dir

---

## Content Deduplication

### UAT-2B-008: Upload Duplicate File

**MCP Tool**: `create_note`, `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Upload same file twice and verify deduplication

**Prerequisites**:
- `attachment_test_note_id` from UAT-2B-001
- Same JPEG file used in UAT-2B-001

**Steps**:
1. Create new note: `create_note({ content: "# Duplicate Test", tags: ["uat/dedup"], revision_mode: "none" })`
2. Store note ID as `dedup_note_id`
3. **Co-located**: `upload_attachment({ note_id: dedup_note_id, file_path: "tests/uat/data/images/jpeg-with-exif.jpg", content_type: "image/jpeg", filename: "duplicate-photo.jpg" })`
4. **Remote**: Use HTTP upload pattern with same file
5. Query attachment_blob table: `SELECT COUNT(*), content_hash FROM attachment_blob WHERE content_hash = '<hash>' GROUP BY content_hash`

**Expected Results**:
- New attachment record created with different attachment ID
- Same `blob_id` reused (deduplication)
- Only one blob record exists with this content hash
- Blob `reference_count` is 2

**Verification**:
- Two distinct attachment records point to same blob
- Storage space not duplicated

**Store**: `attachment_duplicate_id`

---

## EXIF Metadata Extraction

### UAT-2B-009: EXIF GPS Extraction

**MCP Tool**: `create_note`, `upload_attachment` (co-located) OR HTTP API (remote), `get_attachment`

**Description**: Upload photo with GPS EXIF data and verify extraction

**Prerequisites**:
- JPEG with GPS EXIF data (latitude, longitude)
- Test note exists

**Steps**:
1. Create note: `create_note({ content: "# EXIF Test", tags: ["uat/exif"], revision_mode: "none" })`
2. Upload JPEG with GPS (use MCP or HTTP as appropriate)
3. Wait 2 seconds for EXIF extraction job
4. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

**Expected Results**:
- Attachment `extracted_metadata` contains GPS coordinates
- `extracted_metadata.gps.latitude` is present
- `extracted_metadata.gps.longitude` is present
- `extracted_metadata.gps.altitude` may be present

**Verification**:
- Extracted GPS matches known EXIF data
- Coordinates are in decimal degrees format

**Store**: `attachment_gps_id`

---

### UAT-2B-010: EXIF Camera Metadata

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote), `get_attachment`

**Description**: Verify camera make/model extraction from EXIF

**Prerequisites**:
- JPEG with camera EXIF data (Make, Model)

**Steps**:
1. Upload JPEG with camera EXIF (use MCP or HTTP as appropriate)
2. Wait 2 seconds for EXIF extraction
3. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

**Expected Results**:
- `extracted_metadata.camera.make` present (e.g., "Apple", "Canon")
- `extracted_metadata.camera.model` present (e.g., "iPhone 15 Pro")
- `extracted_metadata.datetime_original` present (ISO 8601 format)

**Verification**:
- Camera metadata matches known EXIF data

---

### UAT-2B-011: EXIF Timestamp Extraction

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote), `get_attachment`

**Description**: Verify date/time extraction from EXIF

**Prerequisites**:
- JPEG with DateTimeOriginal EXIF tag

**Steps**:
1. Upload JPEG (use MCP or HTTP as appropriate)
2. Wait 2 seconds for EXIF extraction
3. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

**Expected Results**:
- `extracted_metadata.datetime_original` present
- Timestamp is valid ISO 8601 datetime
- Timestamp matches known EXIF DateTimeOriginal

---

## File Safety Validation

### UAT-2B-012: Block Executable Extension

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Attempt to upload executable and verify rejection

**Prerequisites**: None

**Steps**:
1. Attempt to upload (use MCP or HTTP as appropriate): `tests/uat/data/edge-cases/malware.exe`

**Expected Results**:
- Returns error with status 400
- Error message: "Blocked file extension: .exe"
- No file stored
- No database record created

**Verification**:
- System rejects dangerous extensions (.exe, .bat, .cmd, .sh, .ps1)

---

### UAT-2B-013: Block Script Extension

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Verify rejection of script files

**Prerequisites**: None

**Steps**:
1. Attempt to upload (use MCP or HTTP as appropriate): `tests/uat/data/edge-cases/script.sh`

**Expected Results**:
- Returns error with status 400
- Error message: "Blocked file extension: .sh"
- No file stored

---

### UAT-2B-014: Magic Bytes Validation

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Verify MIME type matches file content (magic bytes)

**Prerequisites**:
- File with mismatched extension and magic bytes (e.g., PNG magic bytes with .jpg extension)

**Steps**:
1. Attempt upload (use MCP or HTTP as appropriate): `tests/uat/data/edge-cases/binary-wrong-ext.jpg`

**Expected Results**:
- System detects mismatch (implementation-dependent)
- Either: Corrects content_type to actual MIME type OR returns error
- Logs warning about MIME type mismatch

**Verification**:
- Magic byte validation prevents content-type spoofing

---

## Attachment Listing and Filtering

### UAT-2B-015: List All Attachments for Note

**MCP Tool**: `list_attachments`

**Description**: List all attachments for a note with multiple files

**Prerequisites**:
- `attachment_test_note_id` with 5 attachments (UAT-2B-001 to UAT-2B-005)

**Steps**:
1. List attachments: `list_attachments({ note_id: attachment_test_note_id })`

**Expected Results**:
- Returns array of 5 `AttachmentSummary` objects
- Each contains: `id`, `note_id`, `filename`, `content_type`, `size_bytes`, `status`, `created_at`
- Results ordered by `display_order`, then `created_at`

**Verification**:
- All 5 uploaded files present
- Metadata matches upload records

---

### UAT-2B-016: List Attachments for Note with No Attachments

**MCP Tool**: `create_note`, `list_attachments`

**Description**: Verify empty result for note without attachments

**Prerequisites**:
- Note with no attachments

**Steps**:
1. Create note: `create_note({ content: "# No Attachments", tags: ["uat/empty"], revision_mode: "none" })`
2. List attachments: `list_attachments({ note_id: <note-id> })`

**Expected Results**:
- Returns empty array `[]`
- No error

---

## Attachment Deletion

### UAT-2B-017: Delete Attachment

**MCP Tool**: `delete_attachment`, `list_attachments`, `download_attachment` (co-located) OR HTTP API (remote)

**Description**: Delete an attachment and verify removal

**Prerequisites**:
- `attachment_audio_id` from UAT-2B-003

**Steps**:
1. Delete attachment: `delete_attachment({ attachment_id: attachment_audio_id })`
2. List attachments: `list_attachments({ note_id: attachment_test_note_id })`
3. Attempt to download (use MCP or HTTP as appropriate)

**Expected Results**:
- Delete succeeds (no error)
- List returns 4 attachments (audio removed)
- Download returns 404 error

**Verification**:
- Attachment record deleted
- Blob reference_count decremented (but blob retained)

---

### UAT-2B-018: Delete Attachment with Shared Blob

**MCP Tool**: `delete_attachment`, `download_attachment` (co-located) OR HTTP API (remote)

**Description**: Delete attachment that shares blob with another attachment (deduplication)

**Prerequisites**:
- `attachment_duplicate_id` from UAT-2B-008 (shares blob with `attachment_image_id`)

**Steps**:
1. Query blob reference count before: `SELECT reference_count FROM attachment_blob WHERE id = '<blob-id>'`
2. Delete duplicate: `delete_attachment({ attachment_id: attachment_duplicate_id })`
3. Query blob reference count after
4. Download original (use MCP or HTTP as appropriate)

**Expected Results**:
- Delete succeeds
- Blob `reference_count` decremented from 2 to 1
- Blob NOT deleted (still referenced)
- Original attachment download still works

**Verification**:
- Deduplication reference counting works correctly

---

## Error Handling

### UAT-2B-019: Upload Oversized File

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Attempt to upload file exceeding size limit

**Prerequisites**:
- File >2GB (or server-configured limit)
- Note: For testing, either use `/dev/urandom` via `dd` to generate a large file, or configure a lower limit for testing

**Steps**:
1. Generate large file: `dd if=/dev/zero of=/tmp/huge-file.bin bs=1M count=2100` (2.1GB)
2. Attempt upload (use MCP or HTTP as appropriate)

**Expected Results**:
- Returns error with status 413 (Payload Too Large)
- Error message: "File size exceeds maximum allowed"
- No file stored

**Verification**:
- Server rejects oversized uploads before processing

---

### UAT-2B-020: Upload with Invalid Content Type

**Isolation**: Recommended — dual-path test may return error

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Attempt upload with malformed MIME type

**Prerequisites**: None

**Steps**:
1. Attempt upload with content_type: "invalid/invalid/invalid"

**Expected Results**:
- Either: Server accepts and sanitizes OR returns 400 error
- If accepted: Content-type normalized or detected from magic bytes
- No crash

---

### UAT-2B-021: Upload to Non-Existent Note

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment` (co-located) OR HTTP API (remote)

**Description**: Attempt to attach file to non-existent note

**Prerequisites**: None

**Steps**:
1. Attempt upload with note_id: "00000000-0000-0000-0000-000000000000"

**Expected Results**:
- Returns error with status 404
- Error message: "Note not found"
- No attachment or blob created

**Verification**:
- Foreign key constraint prevents orphaned attachments

---

## Phase Summary

| Test ID | Name | Tools | Status |
|---------|------|-------|--------|
| UAT-2B-001 | Upload Image File | `create_note`, `upload_attachment` (MCP) OR HTTP API, `list_attachments` | |
| UAT-2B-002 | Upload PDF Document | `upload_attachment` (MCP) OR HTTP API, `list_attachments` | |
| UAT-2B-003 | Upload Audio File | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-004 | Upload Video File | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-005 | Upload 3D Model File | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-006 | Download File Integrity | `download_attachment` (MCP) OR HTTP API | |
| UAT-2B-007 | Download Non-Existent | `download_attachment` (MCP) OR HTTP API | |
| UAT-2B-008 | Upload Duplicate File | `create_note`, `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-009 | EXIF GPS Extraction | `create_note`, `upload_attachment` (MCP) OR HTTP API, `get_attachment` | |
| UAT-2B-010 | EXIF Camera Metadata | `upload_attachment` (MCP) OR HTTP API, `get_attachment` | |
| UAT-2B-011 | EXIF Timestamp Extraction | `upload_attachment` (MCP) OR HTTP API, `get_attachment` | |
| UAT-2B-012 | Block Executable Extension | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-013 | Block Script Extension | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-014 | Magic Bytes Validation | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-015 | List All Attachments | `list_attachments` | |
| UAT-2B-016 | List Empty Attachments | `create_note`, `list_attachments` | |
| UAT-2B-017 | Delete Attachment | `delete_attachment`, `list_attachments`, `download_attachment` (MCP) OR HTTP API | |
| UAT-2B-018 | Delete Shared Blob | `delete_attachment`, `download_attachment` (MCP) OR HTTP API | |
| UAT-2B-019 | Upload Oversized File | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-020 | Invalid Content Type | `upload_attachment` (MCP) OR HTTP API | |
| UAT-2B-021 | Upload to Non-Existent Note | `upload_attachment` (MCP) OR HTTP API | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:

---

## Tools and Architecture

This phase exercises the hybrid attachment system:

**MCP Tools** (for metadata operations):
- `list_attachments` - List attachments for a note
- `get_attachment` - Get attachment metadata
- `delete_attachment` - Delete attachments

**MCP Tools** (co-located file I/O):
- `upload_attachment` - Upload files from MCP server filesystem (reads from `file_path`)
- `download_attachment` - Download files to MCP server filesystem (saves to `output_dir`)

**HTTP API** (remote binary transfer):
- `POST /api/v1/notes/{id}/attachments` - Upload with base64-encoded JSON body
- `GET /api/v1/attachments/{id}/download` - Download binary data with Content-Type/Content-Disposition headers

> **Architecture Note**: Binary file operations use filesystem paths (MCP co-located) or HTTP API (remote deployments). Binary data never passes through the MCP protocol for remote deployments, ensuring efficient large file handling.

All tools are verified for correct behavior, error handling, and edge cases.

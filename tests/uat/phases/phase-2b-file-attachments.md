# UAT Phase 2B: File Attachments

**Purpose**: Verify file attachment system with deduplication, EXIF extraction, and safety validation
**Duration**: ~15 minutes
**Prerequisites**: Phase 1 seed data exists, test data generated
**Critical**: Yes (100% pass required)
**Tools Tested**: `create_note`, `upload_attachment`, `list_attachments`, `get_attachment`, `download_attachment`, `delete_attachment`

> **Attachment Workflow Architecture**: The attachment system uses a two-step MCP+HTTP approach optimized for binary file transfer:
>
> **Upload Flow** (all deployments):
> 1. Call MCP `upload_attachment` with `note_id` and `filename` — returns a curl command with auth token
> 2. Execute the curl command with the actual file path — multipart/form-data upload directly to API
> 3. API returns attachment metadata (id, filename, status, etc.)
>
> **Download Flow** (all deployments):
> 1. Call MCP `download_attachment` with attachment `id` — returns a curl command with download URL
> 2. Execute the curl command to save the file to disk
>
> **Metadata Operations** (MCP tools directly):
> - `list_attachments` — list attachments for a note
> - `get_attachment` — get full attachment metadata
> - `delete_attachment` — delete an attachment
>
> **Why This Architecture**:
> - Binary data never passes through the MCP protocol or LLM context window
> - Multipart/form-data supports files up to 50MB with content-hash deduplication
> - MCP tool provides the auth token so agents don't need to manage credentials
> - No base64 encoding needed — direct binary upload

> **Upload Pattern**:
> ```bash
> # 1. Get upload URL and curl command from MCP
> upload_attachment({ note_id: "<note-uuid>", filename: "photo.jpg", content_type: "image/jpeg" })
> # Returns: { upload_url, curl_command, max_size: "50MB" }
>
> # 2. Execute the curl command (replacing filename with actual path)
> curl -X POST \
>   -F "file=@/path/to/photo.jpg;type=image/jpeg" \
>   -H "Authorization: Bearer <token>" \
>   "https://memory.integrolabs.net/api/v1/notes/<note-id>/attachments/upload"
> ```
>
> **Download Pattern**:
> ```bash
> # 1. Get download URL from MCP
> download_attachment({ id: "<attachment-uuid>" })
> # Returns: { download_url, curl_command }
>
> # 2. Execute the curl command
> curl -s -o photo.jpg "<download-url>"
> ```
>
> **Important**: The curl command returned by MCP uses `localhost:3000` (internal).
> Replace with `https://memory.integrolabs.net` for external access.

> **Test Data**: This phase uses files from two locations:
> - `tests/uat/data/` - Generated test data (images, code, edge cases). Generate with:
>   ```bash
>   cd tests/uat/data/scripts && ./generate-test-data.sh
>   ```
> - `/mnt/global/test-media/` - Real CC-licensed media files (video, audio, PDFs)
>
> Key files: `tests/uat/data/images/jpeg-with-exif.jpg` (EXIF/GPS), `tests/uat/data/documents/code-python.py` (code),
> `tests/uat/data/edge-cases/binary-wrong-ext.jpg` (safety validation),
> `/mnt/global/test-media/video/01-big-buck-bunny.mp4` (real video),
> `/mnt/global/test-media/documents/11-arxiv-attention-paper.pdf` (real PDF)

---

## File Upload - Basic

### UAT-2B-001: Upload Image File

**MCP Tool**: `upload_attachment`

**Description**: Upload a JPEG image and verify attachment record creation

**Prerequisites**:
- Test note exists from Phase 1
- JPEG image file available: `tests/uat/data/images/jpeg-with-exif.jpg` (or any <10MB JPEG)

**Steps**:
1. Create a test note: `create_note({ content: "# Photo Test", tags: ["uat/attachments"], revision_mode: "none" })`
2. Store the returned note ID as `attachment_test_note_id`
3. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "jpeg-with-exif.jpg", content_type: "image/jpeg" })`
4. Execute the returned curl command with actual file path:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/images/jpeg-with-exif.jpg;type=image/jpeg" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
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

**MCP Tool**: `upload_attachment`

**Description**: Upload a PDF file and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- PDF file available: `tests/uat/data/documents/pdf-single-page.pdf`

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "pdf-single-page.pdf", content_type: "application/pdf" })`
2. Execute curl with actual file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/documents/pdf-single-page.pdf;type=application/pdf" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns attachment record with `content_type: "application/pdf"`
- Status is "uploaded"

**Verification**:
- `list_attachments({ note_id: attachment_test_note_id })` returns 2 attachments (image + PDF)

**Store**: `attachment_pdf_id`

---

### UAT-2B-003: Upload Audio File

**MCP Tool**: `upload_attachment`

**Description**: Upload an audio file (MP3) and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- Audio file available: `tests/uat/data/audio/english-speech-5s.mp3`

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "english-speech-5s.mp3", content_type: "audio/mpeg" })`
2. Execute curl with actual file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/audio/english-speech-5s.mp3;type=audio/mpeg" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns attachment record with `content_type: "audio/mpeg"`
- Status is "uploaded"

**Store**: `attachment_audio_id`

---

### UAT-2B-004: Upload Video File

**MCP Tool**: `upload_attachment`

**Description**: Upload a video file (MP4) and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- Real video test file: `/mnt/global/test-media/video/01-big-buck-bunny.mp4` (~3.1MB, CC-licensed)

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "01-big-buck-bunny.mp4", content_type: "video/mp4" })`
2. Execute curl with actual file:
   ```bash
   curl -s -X POST \
     -F "file=@/mnt/global/test-media/video/01-big-buck-bunny.mp4;type=video/mp4" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns attachment record with `content_type: "video/mp4"`
- Status is "uploaded"

**Store**: `attachment_video_id`

---

### UAT-2B-005: Upload Large PDF Document

**MCP Tool**: `upload_attachment`

**Description**: Upload a large PDF file and verify storage

**Prerequisites**:
- Test note exists from UAT-2B-001
- Real PDF test file: `/mnt/global/test-media/documents/11-arxiv-attention-paper.pdf` (CC-licensed research paper)

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "arxiv-attention-paper.pdf", content_type: "application/pdf" })`
2. Execute curl with actual file:
   ```bash
   curl -s -X POST \
     -F "file=@/mnt/global/test-media/documents/11-arxiv-attention-paper.pdf;type=application/pdf" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns attachment record with `content_type: "application/pdf"`
- Status is "uploaded"

**Store**: `attachment_largepdf_id`

---

## File Download and Integrity

### UAT-2B-006: Download File and Verify Integrity

**MCP Tool**: `download_attachment`

**Description**: Download uploaded image and verify content matches

**Prerequisites**:
- `attachment_image_id` from UAT-2B-001

**Steps**:
1. Get download command: `download_attachment({ id: attachment_image_id })`
2. Execute curl to save file:
   ```bash
   curl -s -o /tmp/uat/jpeg-with-exif.jpg "https://memory.integrolabs.net/api/v1/attachments/{attachment_image_id}/download"
   ```
3. Compute BLAKE3 hash of saved file
4. Compare with original file hash: `tests/uat/data/images/jpeg-with-exif.jpg`

**Expected Results**:
- MCP returns `{ download_url: "...", curl_command: "..." }`
- HTTP returns binary data with `Content-Type: image/jpeg` and `Content-Disposition` headers
- Downloaded file exists on disk
- File size matches original

**Verification**:
- Downloaded file is byte-for-byte identical to uploaded file
- BLAKE3 hash matches: `b3sum /tmp/uat/jpeg-with-exif.jpg` vs `b3sum tests/uat/data/images/jpeg-with-exif.jpg`

---

### UAT-2B-007: Download Non-Existent Attachment

**Isolation**: Required — negative test expects error response

**MCP Tool**: `download_attachment`

**Description**: Attempt to download non-existent attachment and verify error handling

**Prerequisites**: None

**Steps**:
1. Get download command: `download_attachment({ id: "00000000-0000-0000-0000-000000000000" })`
2. Execute curl: `curl -s -o /tmp/uat/test.bin "https://memory.integrolabs.net/api/v1/attachments/00000000-0000-0000-0000-000000000000/download"`

**Expected Results**:
- Returns error with status 404
- Error message: "Attachment not found"
- No crash or panic
- No file written to output location

---

## Content Deduplication

### UAT-2B-008: Upload Duplicate File

**MCP Tool**: `create_note`, `upload_attachment`

**Description**: Upload same file twice and verify deduplication

**Prerequisites**:
- `attachment_test_note_id` from UAT-2B-001
- Same JPEG file used in UAT-2B-001

**Steps**:
1. Create new note: `create_note({ content: "# Duplicate Test", tags: ["uat/dedup"], revision_mode: "none" })`
2. Store note ID as `dedup_note_id`
3. Get upload command: `upload_attachment({ note_id: dedup_note_id, filename: "duplicate-photo.jpg", content_type: "image/jpeg" })`
4. Execute curl with same file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/images/jpeg-with-exif.jpg;type=image/jpeg" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{dedup_note_id}/attachments/upload"
   ```
5. List attachments on dedup note: `list_attachments({ note_id: dedup_note_id })`
6. Get attachment metadata: `get_attachment({ id: <dedup_attachment_id> })`

**Expected Results**:
- New attachment record created with different attachment ID
- Same `blob_id` reused (deduplication) - visible in attachment metadata
- `size_bytes` matches original upload

**Verification**:
- `list_attachments` on both notes shows attachments with same `content_hash` or `blob_id`
- Both attachments are independently accessible via `get_attachment`
- Storage space not duplicated (same blob referenced twice)

**Store**: `attachment_duplicate_id`

---

## EXIF Metadata Extraction

### UAT-2B-009: EXIF GPS Extraction

**MCP Tool**: `create_note`, `upload_attachment`, `get_attachment`

**Description**: Upload photo with GPS EXIF data and verify extraction

**Prerequisites**:
- JPEG with GPS EXIF data (latitude, longitude)
- Test note exists

**Steps**:
1. Create note: `create_note({ content: "# EXIF Test", tags: ["uat/exif"], revision_mode: "none" })`
2. Get upload command: `upload_attachment({ note_id: <note-id>, filename: "jpeg-with-exif.jpg", content_type: "image/jpeg" })`
3. Execute curl with JPEG file
4. Wait 2 seconds for EXIF extraction job
5. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

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

**MCP Tool**: `upload_attachment`, `get_attachment`

**Description**: Verify camera make/model extraction from EXIF

**Prerequisites**:
- JPEG with camera EXIF data (Make, Model)

**Steps**:
1. Get upload command: `upload_attachment({ note_id: <note-id>, filename: "camera-photo.jpg", content_type: "image/jpeg" })`
2. Execute curl with JPEG file
3. Wait 2 seconds for EXIF extraction
4. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

**Expected Results**:
- `extracted_metadata.camera.make` present (e.g., "Apple", "Canon")
- `extracted_metadata.camera.model` present (e.g., "iPhone 15 Pro")
- `extracted_metadata.datetime_original` present (ISO 8601 format)

**Verification**:
- Camera metadata matches known EXIF data

---

### UAT-2B-011: EXIF Timestamp Extraction

**MCP Tool**: `upload_attachment`, `get_attachment`

**Description**: Verify date/time extraction from EXIF

**Prerequisites**:
- JPEG with DateTimeOriginal EXIF tag

**Steps**:
1. Get upload command: `upload_attachment({ note_id: <note-id>, filename: "timestamped-photo.jpg", content_type: "image/jpeg" })`
2. Execute curl with JPEG file
3. Wait 2 seconds for EXIF extraction
4. Get attachment: `get_attachment({ attachment_id: <attachment_id> })`

**Expected Results**:
- `extracted_metadata.datetime_original` present
- Timestamp is valid ISO 8601 datetime
- Timestamp matches known EXIF DateTimeOriginal

---

## File Safety Validation

### UAT-2B-012: Block Executable Extension

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment`

**Description**: Attempt to upload executable and verify rejection

**Prerequisites**: None

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "malware.exe", content_type: "application/x-msdownload" })`
2. Execute curl with executable file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/edge-cases/malware.exe;type=application/x-msdownload" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

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

**MCP Tool**: `upload_attachment`

**Description**: Verify rejection of script files

**Prerequisites**: None

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "script.sh", content_type: "application/x-sh" })`
2. Execute curl with script file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/edge-cases/script.sh;type=application/x-sh" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns error with status 400
- Error message: "Blocked file extension: .sh"
- No file stored

---

### UAT-2B-014: Magic Bytes Validation

**MCP Tool**: `upload_attachment`

**Description**: Verify MIME type matches file content (magic bytes)

**Prerequisites**:
- File with mismatched extension and magic bytes (e.g., PNG magic bytes with .jpg extension)

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "binary-wrong-ext.jpg", content_type: "image/jpeg" })`
2. Execute curl with mismatched file:
   ```bash
   curl -s -X POST \
     -F "file=@tests/uat/data/edge-cases/binary-wrong-ext.jpg;type=image/jpeg" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

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
- `attachment_test_note_id` with 5 attachments (UAT-2B-001 through UAT-2B-005: image, PDF, audio, video, large PDF)

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

**MCP Tool**: `delete_attachment`, `list_attachments`, `download_attachment`

**Description**: Delete an attachment and verify removal

**Prerequisites**:
- `attachment_audio_id` from UAT-2B-003

**Steps**:
1. Delete attachment: `delete_attachment({ attachment_id: attachment_audio_id })`
2. List attachments: `list_attachments({ note_id: attachment_test_note_id })`
3. Attempt to download: `download_attachment({ id: attachment_audio_id })`
4. Execute curl (if command returned)

**Expected Results**:
- Delete succeeds (no error)
- List returns 4 attachments (audio removed)
- Download returns 404 error

**Verification**:
- Attachment record deleted
- Blob reference_count decremented (but blob retained)

---

### UAT-2B-018: Delete Attachment with Shared Blob

**MCP Tool**: `delete_attachment`, `download_attachment`

**Description**: Delete attachment that shares blob with another attachment (deduplication)

**Prerequisites**:
- `attachment_duplicate_id` from UAT-2B-008 (shares blob with `attachment_image_id`)

**Steps**:
1. Query blob reference count before: `SELECT reference_count FROM attachment_blob WHERE id = '<blob-id>'`
2. Delete duplicate: `delete_attachment({ attachment_id: attachment_duplicate_id })`
3. Query blob reference count after
4. Download original: `download_attachment({ id: attachment_image_id })`
5. Execute curl to verify file still accessible

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

**MCP Tool**: `upload_attachment`

**Description**: Attempt to upload file exceeding size limit

**Prerequisites**:
- File >50MB (server-configured limit)
- Note: For testing, use `dd` to generate a large file

**Steps**:
1. Generate large file: `dd if=/dev/zero of=/tmp/huge-file.bin bs=1M count=55` (55MB)
2. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "huge-file.bin", content_type: "application/octet-stream" })`
3. Execute curl with oversized file:
   ```bash
   curl -s -X POST \
     -F "file=@/tmp/huge-file.bin;type=application/octet-stream" \
     -H "Authorization: Bearer <token>" \
     "https://memory.integrolabs.net/api/v1/notes/{attachment_test_note_id}/attachments/upload"
   ```

**Expected Results**:
- Returns error with status 400 or 413
- Error message indicates file size exceeds maximum allowed (50MB)
- No file stored

**Verification**:
- Server rejects oversized uploads before processing

---

### UAT-2B-020: Upload with Invalid Content Type

**Isolation**: Recommended — dual-path test may return error

**MCP Tool**: `upload_attachment`

**Description**: Attempt upload with malformed MIME type

**Prerequisites**: None

**Steps**:
1. Get upload command: `upload_attachment({ note_id: attachment_test_note_id, filename: "test.txt", content_type: "invalid/invalid/invalid" })`
2. Execute curl with invalid content type (if command returned)

**Expected Results**:
- Either: Server accepts and sanitizes OR returns 400 error
- If accepted: Content-type normalized or detected from magic bytes
- No crash

---

### UAT-2B-021: Upload to Non-Existent Note

**Isolation**: Required — negative test expects error response

**MCP Tool**: `upload_attachment`

**Description**: Attempt to attach file to non-existent note

**Prerequisites**: None

**Steps**:
1. Get upload command: `upload_attachment({ note_id: "00000000-0000-0000-0000-000000000000", filename: "test.txt", content_type: "text/plain" })`
2. Execute curl with any file (if command returned)

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
| UAT-2B-001 | Upload Image File | `create_note`, `upload_attachment` | |
| UAT-2B-002 | Upload PDF Document | `upload_attachment` | |
| UAT-2B-003 | Upload Audio File | `upload_attachment` | |
| UAT-2B-004 | Upload Video File | `upload_attachment` | |
| UAT-2B-005 | Upload Large PDF Document | `upload_attachment` | |
| UAT-2B-006 | Download File Integrity | `download_attachment` | |
| UAT-2B-007 | Download Non-Existent | `download_attachment` | |
| UAT-2B-008 | Upload Duplicate File | `create_note`, `upload_attachment` | |
| UAT-2B-009 | EXIF GPS Extraction | `create_note`, `upload_attachment`, `get_attachment` | |
| UAT-2B-010 | EXIF Camera Metadata | `upload_attachment`, `get_attachment` | |
| UAT-2B-011 | EXIF Timestamp Extraction | `upload_attachment`, `get_attachment` | |
| UAT-2B-012 | Block Executable Extension | `upload_attachment` | |
| UAT-2B-013 | Block Script Extension | `upload_attachment` | |
| UAT-2B-014 | Magic Bytes Validation | `upload_attachment` | |
| UAT-2B-015 | List All Attachments | `list_attachments` | |
| UAT-2B-016 | List Empty Attachments | `create_note`, `list_attachments` | |
| UAT-2B-017 | Delete Attachment | `delete_attachment`, `list_attachments`, `download_attachment` | |
| UAT-2B-018 | Delete Shared Blob | `delete_attachment`, `download_attachment` | |
| UAT-2B-019 | Upload Oversized File | `upload_attachment` | |
| UAT-2B-020 | Invalid Content Type | `upload_attachment` | |
| UAT-2B-021 | Upload to Non-Existent Note | `upload_attachment` | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:

---

## Tools and Architecture

This phase exercises the two-step attachment system:

**MCP Tools** (metadata operations):
- `list_attachments` - List attachments for a note
- `get_attachment` - Get attachment metadata
- `delete_attachment` - Delete attachments

**Two-Step Upload/Download Flow**:
- `upload_attachment({ note_id, filename, content_type, document_type_id? })` - Returns `{ upload_url, curl_command, max_size: "50MB" }`
- Execute curl with `-F "file=@<path>;type=<mime>"` to upload binary file (multipart/form-data)
- `download_attachment({ id })` - Returns `{ download_url, curl_command }`
- Execute curl to download binary file

> **Architecture Note**: Binary file operations use a two-step process:
> 1. MCP tool provides authenticated curl command
> 2. Direct HTTP multipart upload/download handles binary data
>
> Binary data NEVER passes through the MCP protocol, enabling efficient large file handling up to 50MB. The MCP tool manages authentication tokens so agents don't need credential management.

All tools are verified for correct behavior, error handling, and edge cases.

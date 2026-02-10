# Phase 2B Blocked Attachment Tests - Results

**Date**: 2026-02-09
**API**: https://memory.integrolabs.net
**Note A (uploads)**: `019c44c1-5cee-7833-be0f-e7e6b16ae286`
**Note B (edge cases)**: `019c44c1-6353-7673-872f-c6c091ff08e0`
**Method**: MCP `upload_attachment` to get curl command, then curl via Bash

## Summary

| # | Test | Result | Notes |
|---|------|--------|-------|
| 1 | UAT-2B-003 | PASS | MP3 upload succeeded |
| 2 | UAT-2B-004 | PASS | MP4 upload succeeded |
| 3 | UAT-2B-005 | PASS | GLB 3D model upload succeeded |
| 4 | UAT-2B-006 | PASS | PDF upload succeeded |
| 5 | UAT-2B-010 | FAIL | No GPS/EXIF metadata extracted |
| 6 | UAT-2B-011 | FAIL | No camera make/model in metadata |
| 7 | UAT-2B-012 | FAIL | No DateTimeOriginal in metadata |
| 8 | UAT-2B-009 | PASS | Blob dedup confirmed (same blob_id) |
| 9 | UAT-2B-013 | PASS | .exe rejected (400) |
| 10 | UAT-2B-014 | PASS | .sh rejected (400) |
| 11 | UAT-2B-015a | FAIL | .txt accepted as application/pdf (no magic byte validation) |
| 12 | UAT-2B-015b | PASS | .txt accepted with correct content_type |
| 13 | UAT-2B-018 | PASS* | Delete works via REST; MCP phantom delete issue |

**Totals: 8 PASS, 4 FAIL, 1 PASS* (with caveat)**
**Pass rate: 62% (8/13) strict, 69% (9/13) with PASS***

---

## Detailed Results

### UAT-2B-003: Audio Upload (MP3)
**Result: PASS**

- File: `/mnt/global/test-media/audio/01-radio-drama-suspense.mp3` (720,586 bytes)
- Content-type: `audio/mpeg`
- Response: HTTP 200
- Attachment ID: `019c44c2-769d-7703-ae30-b535db9a45b6`
- Blob ID: `019c44c2-7699-7663-902a-06951761294a`
- Extraction strategy: `audio_transcribe`
- File persisted and visible in `list_attachments` with correct size

### UAT-2B-004: Video Upload (MP4)
**Result: PASS**

- File: `/mnt/global/test-media/video/01-big-buck-bunny.mp4` (3,221,836 bytes)
- Content-type: `video/mp4`
- Response: HTTP 200
- Attachment ID: `019c44c2-9d31-7293-8dff-be0facd7f8a1`
- Blob ID: `019c431b-a971-7101-bf57-0542ef170441` (note: pre-existing blob, dedup from prior test run)
- Extraction strategy: `video_multimodal`
- File persisted correctly

### UAT-2B-005: 3D Model Upload (GLB)
**Result: PASS**

- File: `/mnt/global/test-media/3d-models/01-avocado.glb` (8,110,040 bytes)
- Content-type: `model/gltf-binary`
- Response: HTTP 200
- Attachment ID: `019c44c2-a446-7280-a44e-ca6295ce5c4b`
- Blob ID: `019c44c2-a441-7130-8797-c5543cafedb0`
- Extraction strategy: `vision`
- File persisted correctly. Largest upload in batch (7.7 MB), well within 50 MB limit.

### UAT-2B-006: Document Upload (PDF)
**Result: PASS**

- File: `/mnt/global/test-media/documents/11-arxiv-attention-paper.pdf` (2,215,244 bytes)
- Content-type: `application/pdf`
- Response: HTTP 200
- Attachment ID: `019c44c2-aa4e-7222-8a16-6234b044da28`
- Blob ID: `019c44c2-aa49-7901-8445-a50d0126bb51`
- Extraction strategy: `pdf_text`
- File persisted correctly

### UAT-2B-010: EXIF GPS Metadata
**Result: FAIL**

- File: `paris-eiffel-tower.jpg` uploaded to Note B
- Attachment ID: `019c44c2-cd76-7fd3-84a0-1b535fb2c680`
- Waited 5 seconds for async metadata extraction
- `extracted_metadata`: `null`
- Expected: GPS coordinates (48.858400N, 2.294500E at 35m altitude)
- Verified with Python PIL: JPEG contains full GPS EXIF data
- **Issue**: Server does not extract EXIF metadata from uploaded images. Related to existing issue #253 (no magic byte validation).

### UAT-2B-011: EXIF Camera Make/Model
**Result: FAIL**

- Same attachment as UAT-2B-010
- `extracted_metadata`: `null`
- Expected: `Make: Canon`, `Model: EOS R5`
- **Issue**: No EXIF extraction pipeline exists

### UAT-2B-012: EXIF DateTimeOriginal
**Result: FAIL**

- Same attachment as UAT-2B-010
- `extracted_metadata`: `null`
- Expected: `DateTimeOriginal: 2024:07:14 12:00:00`
- **Issue**: No EXIF extraction pipeline exists

### UAT-2B-009: Content Hash Deduplication
**Result: PASS**

- Uploaded `paris-eiffel-tower.jpg` to both Note A and Note B
- Note B attachment: `019c44c2-cd76-7fd3-84a0-1b535fb2c680`, blob_id: `019c44c2-cd73-73b3-b9df-75cd7ca4585f`
- Note A attachment: `019c44c3-58fb-7c00-bce0-172729eaa814`, blob_id: `019c44c2-cd73-73b3-b9df-75cd7ca4585f`
- **Blob IDs match** -- content-hash deduplication is working correctly
- Two separate attachment records point to the same underlying blob
- Both notes show the attachment in their respective `list_attachments` results

### UAT-2B-013: Executable File Upload (.exe)
**Result: PASS**

- Created `/tmp/test.exe` containing "MZ" (PE header magic bytes)
- Content-type: `application/x-msdownload`
- Response: HTTP 400 `{"error":"File extension .exe is not allowed"}`
- **Correctly rejected** -- extension-based blocklist works

### UAT-2B-014: Shell Script Upload (.sh)
**Result: PASS**

- Created `/tmp/test.sh` containing `#!/bin/bash`
- Content-type: `application/x-sh`
- Response: HTTP 400 `{"error":"File extension .sh is not allowed"}`
- **Correctly rejected** -- extension-based blocklist works

### UAT-2B-015a: Content-Type Mismatch (txt as PDF)
**Result: FAIL**

- Created `/tmp/test-mistype.txt` (plain text, 60 bytes)
- Uploaded with `content_type=application/pdf`
- Response: HTTP 200 -- **accepted without validation**
- Attachment ID: `019c44c3-d09d-7fd2-bdd3-e9143662da27`
- Stored `content_type`: `text/plain` (server used file extension, ignored declared type)
- Extraction strategy: `text_native` (correct for actual content, not the claimed PDF)
- **Issue**: No magic byte validation. A .txt file claiming to be a PDF is accepted. The server correctly ignores the bogus content_type for extraction strategy, but does not reject the mismatch. Related to existing issue #253.

### UAT-2B-015b: Correct Content-Type Upload
**Result: PASS**

- Same `/tmp/test-mistype.txt` file
- Uploaded with `content_type=text/plain`
- Response: HTTP 200
- Attachment ID: `019c44c3-d802-7c13-b3df-fec60d98b24f`
- Blob ID: `019c44c3-d09a-7bb3-bb98-bedc2de4c195` (same blob as 015a -- dedup works here too)
- Stored `content_type`: `text/plain`
- Extraction strategy: `text_native`
- All correct

### UAT-2B-018: Attachment Deletion
**Result: PASS* (with caveat)**

- Uploaded `/tmp/delete-test.txt` to Note B
- Attachment ID: `019c44c4-0dc7-73b2-ab1e-26a0d8b274b4`
- **MCP `delete_attachment` returned `{"success": true}` but attachment was NOT deleted** (phantom delete)
- Subsequent `get_attachment` and `list_attachments` both showed the attachment still present
- **REST API `DELETE /api/v1/attachments/{id}` worked correctly** -- returned `{"message":"Attachment deleted successfully","success":true}`
- After REST delete, attachment returns 404 and is absent from `list_attachments`
- **Verdict**: The delete functionality works at the API level, but the MCP tool has a phantom delete bug (returns success without actually calling the API). This mirrors the known issue #252 (phantom write on upload). Marking PASS* because the underlying feature works via REST.

---

## Issues Summary

### New Issues Found

| Issue | Severity | Description |
|-------|----------|-------------|
| EXIF extraction missing | Medium | No EXIF metadata (GPS, camera, timestamps) extracted from uploaded JPEG images. `extracted_metadata` always null. |
| Magic byte mismatch accepted | Low | Files with incorrect content_type declaration are accepted without validation (e.g., .txt uploaded as application/pdf). Server correctly uses extension for extraction strategy but does not warn/reject. Existing #253. |
| MCP delete_attachment phantom | High | MCP `delete_attachment` tool returns `success: true` but does not actually delete the attachment. REST API DELETE works correctly. Similar pattern to #252 phantom write. |

### Existing Issues Confirmed

| Issue | Status | Observation |
|-------|--------|-------------|
| #252 | Confirmed | MCP phantom operations pattern extends to delete_attachment |
| #253 | Confirmed | No magic byte validation on upload |

### Positive Findings

1. **Extension blocklist works** -- .exe and .sh correctly rejected with clear error messages
2. **Content-hash deduplication works** -- identical files across notes share the same blob
3. **Extraction strategy selection is smart** -- ignores bogus content_type, uses actual file extension
4. **Large file uploads work** -- up to 8.1 MB tested without issues
5. **Diverse content types supported** -- audio, video, 3D models, PDFs, text all accepted
6. **REST API attachment CRUD works correctly** -- upload, list, get, delete all functional via REST

# Phase 2B: File Attachments — Results

**Date**: 2026-02-13
**Version**: v2026.2.8
**Result**: 22 tests — 20 PASS, 1 PARTIAL, 1 FAIL (90.9%)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| UAT-2B-001 | Upload Image | PASS | note_id: 019c58ff-f1fd-7ad3-a49a-34ca288e53f0, image_id: 019c5900-3659-7323-8dbd-e65fbd3fe4ff |
| UAT-2B-002 | Upload PDF | PASS | pdf_id: 019c5900-f080-7d63-b3b8-fe968912b001 |
| UAT-2B-003 | Upload Audio | PASS | audio_id: 019c5900-f02e-7243-a480-e42e7ab567dc |
| UAT-2B-004 | Upload Video | PASS | video_id: 019c5900-f0f2-71e3-b5ca-2fdcfb225f35 |
| UAT-2B-005 | Upload GLB | PASS | glb_id: 019c5900-d23c-7c12-af98-897ee19e5811 |
| UAT-2B-006 | Large File Upload | PARTIAL | Upload + extraction succeeded but content_type and size_bytes null in metadata |
| UAT-2B-007 | Download Integrity | PASS | SHA256 checksums match for downloaded file |
| UAT-2B-008 | Download Non-Existent | PASS | 404 returned for non-existent attachment |
| UAT-2B-009 | Content Deduplication | PASS | Same file uploaded twice, different attachment IDs but same blob_id |
| UAT-2B-010 | EXIF GPS Extraction | PASS | lat 48.8584, lon 2.2945, alt 35m extracted |
| UAT-2B-011 | EXIF Camera Extraction | PASS | Apple iPhone 15 Pro, iOS 17.5 |
| UAT-2B-012 | EXIF Timestamp Extraction | PASS | datetime_original: 2024-06-15T14:30:00+00:00 |
| UAT-2B-013 | Block Dangerous Extension | PASS | .exe rejected: "File extension .exe is not allowed" |
| UAT-2B-014 | Allow Safe Extension | PASS | .sh uploaded successfully (not blocked) |
| UAT-2B-015a | Magic Bytes Accept | PASS | Text content uploaded as .jpg accepted; extraction_strategy correctly set to text_native |
| UAT-2B-016 | List All Attachments | PASS | 9 attachments returned (8 expected + 1 from size test) |
| UAT-2B-017 | List Empty Attachments | PASS | Empty array returned for note with no attachments |
| UAT-2B-018 | Delete Attachment | PASS | Audio attachment deleted, 404 on re-fetch, count decreased |
| UAT-2B-019 | Delete Shared Blob | FAIL | Deleting duplicate destroys shared blob, orphaning original — **#353 filed** |
| UAT-2B-020 | Upload Oversized File | PASS | 55MB file rejected (400), 49MB accepted, limit enforced at 50MB |
| UAT-2B-021a | Invalid Content Type | PASS | Malformed MIME type accepted, extraction_strategy correctly assigned |
| UAT-2B-022 | Upload Non-Existent Note | PASS | Returns 400 "Referenced resource not found" (rejects upload) |

## Issues Filed
- **#353**: delete_attachment destroys shared blob when other references exist (CRITICAL)

## Stored IDs
- attachment_test_note_id: 019c58ff-f1fd-7ad3-a49a-34ca288e53f0
- attachment_image_id: 019c5900-3659-7323-8dbd-e65fbd3fe4ff
- attachment_pdf_id: 019c5900-f080-7d63-b3b8-fe968912b001
- attachment_audio_id: 019c5900-f02e-7243-a480-e42e7ab567dc (deleted in 2B-018)
- attachment_video_id: 019c5900-f0f2-71e3-b5ca-2fdcfb225f35
- attachment_glb_id: 019c5900-d23c-7c12-af98-897ee19e5811
- attachment_largepdf_id: 019c5901-bca8-73e0-9297-22bc48fb891c
- dedup_note_id: 019c5901-f032-7042-ae5a-1b2a9bd4ade3
- attachment_duplicate_id: 019c5902-27b1-74b0-a640-00a290fed9fc (deleted in 2B-019)

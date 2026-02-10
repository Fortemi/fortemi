# Retest 3: UAT-2B / PROC Attachment & Extraction Tests

**Date**: 2026-02-10 00:40 EST
**Target**: https://memory.integrolabs.net
**Version**: v2026.2.x (post-system-update)
**Agent**: ralph

## Summary

| Test ID | Description | Result | Issue |
|---------|-------------|--------|-------|
| UAT-2B-010 | EXIF GPS coordinates extracted | **PASS** | #278 |
| UAT-2B-011 | EXIF camera Make/Model extracted | **PASS** | #278 |
| UAT-2B-012 | EXIF DateTimeOriginal extracted | **PASS** | #278 |
| UAT-2B-015a | Magic byte validation rejects fake PDF | **FAIL** | #253 |
| PROC-026 | Attachment extraction job types exist | **PASS** | #280 |
| PROC-027 | Queue stats include extraction types | **PASS** | #280 |

**Overall: 5/6 PASS, 1/6 FAIL**

---

## Seed Data

Original seed notes (IDs from task spec) were not found (404). Created fresh seed notes:

| Role | Original ID | New ID |
|------|-------------|--------|
| Note A (uploads) | `019c44c1-5cee-7833-be0f-e7e6b16ae286` | `019c4610-a3a7-7b21-839b-41abb1ff9445` |
| Note B (edge cases) | `019c44c1-6353-7673-872f-c6c091ff08e0` | `019c4610-a94e-7842-8741-9254188516c7` |
| Note C (extraction) | `019c44c1-6eff-7ce2-8a94-9573f578653e` | `019c4610-ada7-7483-99a6-edcfebe0ea10` |

---

## Detailed Results

### UAT-2B-010: EXIF GPS Coordinates Extracted -- PASS

**Issue**: #278 (EXIF Metadata Extraction)

**Procedure**: Uploaded `paris-eiffel-tower.jpg` (134KB, Canon EOS R5, GPS-tagged Paris photo) to Note A via curl multipart upload.

**Upload response** (HTTP 200):
```json
{
  "id": "019c4610-d976-71a1-9c6f-1c8dc823d86d",
  "note_id": "019c4610-a3a7-7b21-839b-41abb1ff9445",
  "filename": "paris-eiffel-tower.jpg",
  "status": "uploaded",
  "extraction_strategy": "vision"
}
```

After 20s async processing, `get_attachment` returned `extracted_metadata`:
```json
{
  "exif": {
    "gps": {
      "altitude": 35,
      "latitude": 48.8584,
      "longitude": 2.2945
    }
  }
}
```

**Verification**: GPS latitude 48.8584 (~48.86) and longitude 2.2945 (~2.29) match expected Eiffel Tower coordinates.

**Result**: **PASS**

---

### UAT-2B-011: EXIF Camera Make/Model Extracted -- PASS

**Issue**: #278

**Evidence** from same `extracted_metadata`:
```json
{
  "exif": {
    "camera": {
      "make": "Canon",
      "model": "EOS R5"
    }
  }
}
```

**Verification**: Make = "Canon", Model = "EOS R5" -- matches expected values exactly.

**Result**: **PASS**

---

### UAT-2B-012: EXIF DateTimeOriginal Extracted -- PASS

**Issue**: #278

**Evidence** from same `extracted_metadata`:
```json
{
  "exif": {
    "datetime": {
      "digitized": "2024:07:14 12:00:00",
      "original": "2024:07:14 12:00:00"
    }
  }
}
```

**Verification**: DateTimeOriginal = "2024:07:14 12:00:00" -- contains expected date 2024:07:14.

**Result**: **PASS**

---

### UAT-2B-015a: Magic Byte Validation Rejects Fake PDF -- FAIL

**Issue**: #253

**Procedure**: Created `/tmp/fake-retest3.pdf` containing plain text ("This is plain text not a PDF"), uploaded to Note B with `content_type=application/pdf`.

**Upload response** (HTTP 200):
```json
{
  "id": "019c4610-e108-7370-8854-5e333321a2fd",
  "note_id": "019c4610-a94e-7842-8741-9254188516c7",
  "filename": "fake-retest3.pdf",
  "status": "uploaded",
  "extraction_strategy": "pdf_text"
}
```

**Expected**: Server should reject the upload with HTTP 4xx (magic bytes don't match %PDF header).

**Actual**: Server accepted the upload (HTTP 200). The extraction job later failed during async processing:
```
error_message: "Extraction failed: Invalid input: File 'fake-retest3.pdf' is not a valid PDF (missing %PDF header)"
```

**Analysis**: The server performs magic byte validation at the *extraction* stage (async job), not at the *upload* stage (HTTP endpoint). The file is accepted and stored, then the extraction job correctly detects the invalid content and fails. While the extraction-time detection is good, the upload endpoint should reject files with mismatched magic bytes to prevent storing invalid data.

**Result**: **FAIL** -- Upload accepted (HTTP 200) instead of rejected (HTTP 4xx). Issue #253 remains open.

---

### PROC-026: Attachment Extraction Job Types Exist -- PASS

**Issue**: #280

**Procedure**: Called `list_jobs` for Note A (which had the JPEG upload) and inspected job types.

**Evidence**: Two attachment-level extraction job types found:

1. **`exif_extraction`** (completed successfully):
```json
{
  "job_type": "exif_extraction",
  "status": "completed",
  "result": {
    "exif_found": true,
    "has_gps": true,
    "has_device": true,
    "has_capture_time": true,
    "device_id": "019c4610-eeac-7255-8be1-4c471af33800",
    "location_id": "019c4610-eea9-7590-82a4-e67fd8d06000",
    "provenance_id": "019c4610-eeae-7245-bfb7-8d56243f3000"
  }
}
```

2. **`extraction`** (failed - no Vision adapter, but the job type exists):
```json
{
  "job_type": "extraction",
  "status": "failed",
  "error_message": "No adapter registered for strategy: Vision",
  "payload": {
    "attachment_id": "019c4610-d976-71a1-9c6f-1c8dc823d86d",
    "strategy": "vision"
  }
}
```

**Verification**: Both `exif_extraction` and `extraction` are attachment-level job types (they carry `attachment_id` in payload), distinct from note-level jobs (embedding, linking, title_generation, concept_tagging, ai_revision).

**Result**: **PASS**

---

### PROC-027: Queue Stats Include Extraction Types -- PASS

**Issue**: #280

**Procedure**: Called `list_jobs` (limit 50) and enumerated all unique `job_type` values across all jobs.

**Unique job types found**:
1. `ai_revision` -- note-level
2. `concept_tagging` -- note-level
3. `embedding` -- note-level
4. `extraction` -- **attachment-level** (text/vision extraction)
5. `exif_extraction` -- **attachment-level** (EXIF metadata parsing)
6. `linking` -- note-level
7. `title_generation` -- note-level

**Queue stats summary**:
```json
{
  "pending": 0,
  "processing": 0,
  "completed_last_hour": 27,
  "failed_last_hour": 6,
  "total": 33
}
```

**Verification**: Two extraction-related job types (`extraction`, `exif_extraction`) exist beyond the baseline note-level types. The job system supports attachment-level processing as a first-class capability.

**Note**: The `list_jobs` MCP tool's `job_type` enum filter only exposes `[ai_revision, embedding, linking, context_update, title_generation]` -- it does not include `extraction`, `exif_extraction`, or `concept_tagging` in the filter enum. This is an MCP schema gap but does not affect test outcome since unfiltered listing returns all types.

**Result**: **PASS**

---

## Notes

- The `exif_extraction` job automatically creates provenance records (location_id, device_id, provenance_id) -- full EXIF-to-provenance pipeline is working.
- Vision-based `extraction` jobs consistently fail with "No adapter registered for strategy: Vision" -- this is expected since no vision LLM adapter is configured on the production instance.
- The MCP `list_jobs` tool's `job_type` filter enum is incomplete -- it lacks `extraction`, `exif_extraction`, and `concept_tagging`. Jobs of these types are returned in unfiltered queries but cannot be filtered by type via MCP. Consider filing a separate issue for MCP schema completeness.

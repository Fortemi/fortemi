# Retest #282: EXIF DateTimeOriginal -> capture_time Fix Verification

**Date**: 2026-02-10
**Issue**: [#282](https://git.integrolabs.net/fortemi/fortemi/issues/282) - EXIF DateTimeOriginal should map to capture_time_start/capture_time_end
**Verdict**: **PASS** (9/9 tests passed)

## Seed Data

### Notes Created

| Label | Note ID | Title | Tags |
|-------|---------|-------|------|
| Paris | `019c4675-7068-7a23-9c1f-4416dddf3325` | Paris #282 Fix Test | test/282-fix |
| NYC | `019c4675-7aff-76a2-9493-6bc10675b054` | NYC #282 Fix Test | test/282-fix |
| Tokyo | `019c4675-8584-7381-a7a2-e1bd1c1fa86b` | Tokyo #282 Fix Test | test/282-fix |

### Attachments Uploaded

| Label | Attachment ID | Filename | EXIF DateTimeOriginal | EXIF GPS |
|-------|--------------|----------|----------------------|----------|
| Paris | `019c4676-3492-79d3-af51-bb88900b7eee` | paris-fix282.jpg | 2024:07:14 10:30:00 | 48.8584N, 2.2945E |
| NYC | `019c4676-a528-73e0-9b1d-46502c11ca75` | nyc-fix282.jpg | 2023:03:15 14:00:00 | 40.6892N, 74.0445W |
| Tokyo | `019c4676-c6e4-7733-835a-8c13dcb978b0` | tokyo-fix282.jpg | 2025:12:25 18:00:00 | 35.6595N, 139.7004E |

## Provenance Verification

All 3 provenance records populated correctly within 30 seconds of upload (no second wait needed).

| Label | Provenance ID | capture_time_start | capture_time_end | time_source | time_confidence | GPS source |
|-------|--------------|-------------------|-----------------|-------------|-----------------|------------|
| Paris | `019c4676-3dd1-742b-9831-724ca4bcf400` | **2024-07-14T10:30:00Z** | **2024-07-14T10:30:00Z** | exif | high | gps_exif |
| NYC | `019c4676-ae19-7f5f-8803-0274817ef800` | **2023-03-15T14:00:00Z** | **2023-03-15T14:00:00Z** | exif | high | gps_exif |
| Tokyo | `019c4676-d036-7968-82e0-25bb4e1d5c00` | **2025-12-25T18:00:00Z** | **2025-12-25T18:00:00Z** | exif | high | gps_exif |

**Key finding**: All `capture_time_start` and `capture_time_end` fields are **NON-NULL** and correctly match the EXIF DateTimeOriginal values. This confirms issue #282 is fixed.

## Test Results

### Spatial Search (3/3 PASS)

| # | Test | Query | Expected | Actual | Result |
|---|------|-------|----------|--------|--------|
| 1 | Paris spatial | `lat=48.8584, lon=2.2945, radius=10000` | Paris note found | Found (5 results total, paris-fix282.jpg included) | **PASS** |
| 2 | NYC spatial | `lat=40.6892, lon=-74.0445, radius=10000` | NYC note found | Found (3 results total, nyc-fix282.jpg included) | **PASS** |
| 3 | Tokyo spatial | `lat=35.6595, lon=139.7004, radius=10000` | Tokyo note found | Found (3 results total, tokyo-fix282.jpg included) | **PASS** |

### Temporal Search (3/3 PASS)

| # | Test | Query | Expected | Actual | Result |
|---|------|-------|----------|--------|--------|
| 4 | Paris temporal | `start=2024-07-01, end=2024-08-01` | Paris found, provenance_id non-null | Found (2 results), provenance_id=`019c4676-3dd1...` | **PASS** |
| 5 | NYC temporal | `start=2023-03-01, end=2023-04-01` | NYC found, provenance_id non-null | Found (1 result), provenance_id=`019c4676-ae19...` | **PASS** |
| 6 | Tokyo temporal | `start=2025-12-01, end=2026-01-01` | Tokyo found, provenance_id non-null | Found (1 result), provenance_id=`019c4676-d036...` | **PASS** |

### Combined Search (3/3 PASS)

| # | Test | Query | Expected | Actual | Result |
|---|------|-------|----------|--------|--------|
| 7 | Paris combined | `lat=48.8584, lon=2.2945, r=50000, 2024` | Paris found | Found (2 results, paris-fix282.jpg included) | **PASS** |
| 8 | Tokyo negative | `lat=35.6595, lon=139.7004, r=10000, 2024` | 0 results (Tokyo=Dec 2025) | 0 results | **PASS** |
| 9 | NYC combined | `lat=40.6892, lon=-74.0445, r=10000, 2023` | NYC found | Found (1 result, nyc-fix282.jpg) | **PASS** |

## Overall Score

| Category | Passed | Total | Rate |
|----------|--------|-------|------|
| Spatial | 3 | 3 | 100% |
| Temporal | 3 | 3 | 100% |
| Combined | 3 | 3 | 100% |
| **Total** | **9** | **9** | **100%** |

## Summary

Issue #282 is **confirmed fixed**. The EXIF `DateTimeOriginal` field now correctly populates `capture_time_start` and `capture_time_end` on `file_provenance` records during the automatic EXIF extraction pipeline. This enables:

1. **Temporal search** (`search_memories_by_time`) - correctly finds photos by their EXIF capture date
2. **Combined search** (`search_memories_combined`) - correctly intersects location AND time constraints
3. **Negative filtering** - combined search correctly excludes results outside the time range even when location matches

Previous uploads (from earlier test runs) still show `null` capture times, confirming the fix only applies to newly processed uploads. Existing records would need reprocessing to backfill capture times.

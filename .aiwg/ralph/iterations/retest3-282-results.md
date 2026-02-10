# Retest #282: EXIF DateTimeOriginal to capture_time Mapping

**Date**: 2026-02-10
**Issue**: [#282](https://git.integrolabs.net/fortemi/fortemi/issues/282) - EXIF DateTimeOriginal should map to capture_time_start/capture_time_end
**Deployed Version**: 2026.2.8
**Verdict**: FAIL -- Root cause identified (empty tstzrange from `[)` bound type with equal start/end)

---

## Seed Data

| City | Note ID | Attachment ID | EXIF DateTimeOriginal | GPS |
|------|---------|---------------|----------------------|-----|
| Paris | `019c4636-485e-7f31-856c-853ba5bd45cf` | `019c4638-536d-7280-b9f1-655ee8fa2b40` | 2024:07:14 10:30:00 | 48.8584N, 2.2945E |
| New York | `019c4636-53be-7c53-9bae-592295a5ce76` | `019c4638-7175-7863-98f0-ce1d6f83bdce` | 2023:03:15 14:00:00 | 40.6892N, 74.0445W |
| Tokyo | `019c4636-5ebb-75d2-9b86-2762496e2931` | `019c4638-8477-7c93-9c16-f465e1c62ce1` | 2025:12:25 18:00:00 | 35.6595N, 139.7004E |

Tags applied: `retest/282`, `travel/<city>`

---

## EXIF Extraction Verification

All 3 attachments reached `status: "completed"` with correct `extracted_metadata`:

| City | EXIF Parsed? | Camera | GPS Extracted? | datetime.original |
|------|-------------|--------|---------------|------------------|
| Paris | Yes | Canon EOS R5 | Yes (48.8584, 2.2945) | 2024:07:14 10:30:00 |
| New York | Yes | Sony A7IV | Yes (40.6892, -74.0445) | 2023:03:15 14:00:00 |
| Tokyo | Yes | Nikon Z9 | Yes (35.6595, 139.7004) | 2025:12:25 18:00:00 |

**Result**: EXIF extraction pipeline works correctly. Metadata is parsed and stored.

---

## Provenance Verification (CRITICAL CHECK for #282)

### File Provenance Records

All 3 notes have file provenance records created from EXIF data:

| City | Provenance ID | Location? | Device? | time_source | time_confidence | capture_time_start | capture_time_end |
|------|--------------|-----------|---------|-------------|-----------------|-------------------|-----------------|
| Paris | `019c4638-5d9e-7ab1-97eb-bceb2e3b2c00` | Yes | Yes (Canon EOS R5) | exif | high | **NULL** | **NULL** |
| New York | `019c4638-7bbe-7998-acb4-4a2e509c2400` | Yes | Yes (Sony A7IV) | exif | high | **NULL** | **NULL** |
| Tokyo | `019c4638-8dc5-71ca-b14f-b2987c5ff400` | Yes | Yes (Nikon Z9) | exif | high | **NULL** | **NULL** |

### Analysis

**`capture_time_start` and `capture_time_end` are NULL on all 3 provenance records despite EXIF DateTimeOriginal being correctly parsed.**

The Rust code correctly:
1. Parses EXIF `DateTimeOriginal` into a `chrono::DateTime<Utc>` (confirmed by `time_source: "exif"` and `time_confidence: "high"`, which are only set when `capture_time.is_some()`)
2. Passes both `capture_time_start` and `capture_time_end` set to the same timestamp value
3. The SQL constructs `tstzrange($2::timestamptz, $3::timestamptz, '[)')`

**Root Cause**: The `[)` range bound type creates an **empty tstzrange** when start == end.

In PostgreSQL, `tstzrange('2024-07-14 10:30:00+00', '2024-07-14 10:30:00+00', '[)')` means "from T (inclusive) to T (exclusive)" which is an empty range. PostgreSQL `lower()` and `upper()` on an empty range return NULL.

**Fix required**: Either:
- Change `'[)'` to `'[]'` (both inclusive) in `create_file_provenance` SQL
- Or set `capture_time_end = capture_time + 1 second` in the jobs handler when start == end

Code locations:
- `crates/matric-db/src/memory_search.rs` line 1374: `tstzrange($2::timestamptz, $3::timestamptz, '[)')`
- `crates/matric-db/src/memory_search.rs` line 1418: same in `_tx` variant
- `crates/matric-api/src/handlers/jobs.rs` lines 1874-1875: `capture_time_start: capture_time, capture_time_end: capture_time`

---

## Test Results

| Test # | Description | Expected | Actual | Result |
|--------|------------|----------|--------|--------|
| 1 | Temporal: July 2024 (Paris) | Paris found | 0 results | **FAIL** |
| 2 | Temporal: March 2023 (New York) | New York found | 0 results | **FAIL** |
| 3 | Temporal: Dec 2025 (Tokyo) | Tokyo found | 0 results | **FAIL** |
| 4 | Temporal: All 2024 (Paris only) | Paris found | 0 results | **FAIL** |
| 5 | Temporal: 2019-2020 (empty) | 0 results | 0 results | PASS (vacuous) |
| 6 | Spatial: Near Paris (10km) | Paris found | 3 results incl. Paris | **PASS** |
| 7 | Combined: Paris loc + 2024 time | Paris found | 0 results | **FAIL** |
| 8 | Combined: Tokyo loc + 2024 time (neg) | 0 results | 0 results | PASS (vacuous) |
| 9 | Combined: NYC loc + 2023 time | New York found | 0 results | **FAIL** |

### Notes on Test Results

- **Test 5**: Returns 0 results correctly, but this is a vacuous pass since ALL temporal searches return 0 due to the bug.
- **Test 6**: Spatial search works perfectly. GPS coordinates are correctly populated on provenance locations. Found our Paris note plus 2 prior test results at same coordinates.
- **Test 8**: Returns 0 results correctly, but again vacuous since the temporal component is broken.
- **Tests 1-4, 7, 9**: All fail because `capture_time` tstzrange is empty (null lower/upper), so `tstzrange && tstzrange` never matches.

---

## Summary

**Overall: 2/9 PASS, 7/9 FAIL** (genuine passes: 1/9 spatial, vacuous passes: 2/9)

| Component | Status |
|-----------|--------|
| EXIF metadata extraction | Working |
| GPS -> provenance location | Working |
| Camera -> provenance device | Working |
| DateTimeOriginal parsing | Working (Rust side) |
| capture_time tstzrange storage | **BROKEN** (empty range) |
| Temporal search (by_time) | **BROKEN** (no matching ranges) |
| Spatial search (by_location) | Working |
| Combined search (location+time) | **BROKEN** (temporal component fails) |

**Issue #282 is NOT FIXED.** The EXIF datetime is correctly parsed in Rust but creates an empty PostgreSQL tstzrange due to the `[)` bound type when start equals end. The fix is a one-line SQL change from `'[)'` to `'[]'` in `create_file_provenance` (and its `_tx` variant).

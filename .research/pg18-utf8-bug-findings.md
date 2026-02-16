# PostgreSQL 18 UTF-8 Bug Research Findings

## Bug #19406: SUBSTRING/LEFT Functions Fail on Toasted UTF-8 Values

### Summary
PostgreSQL versions 15.16, 16.12, 17.8, and 18.2 (released February 12, 2026) contain a critical regression where text functions like `SUBSTRING()` and `LEFT()` fail with "invalid byte sequence for encoding UTF8" errors when operating on TOAST-compressed text containing multi-byte UTF-8 characters.

### Symptoms
- **Error message**: `ERROR: invalid byte sequence for encoding "UTF8": 0xe2 0x80`
- **Affected functions**: `SUBSTRING()`, `LEFT()`, and other text slicing operations
- **Trigger condition**: Text values that are:
  - Stored in TOAST (typically >2KB)
  - Contain multi-byte UTF-8 characters (e.g., U+2011 non-breaking hyphen = 0xe2 0x80 0x91)
  - Accessed at positions that cause detoasting with byte boundary misalignment

### Root Cause
**Commit**: `1e7fe06c10c0a8da9dd6261a6be8d405dc17c728`  
**Title**: "Replace pg_mblen() with bounds-checked versions"  
**Author**: Thomas Munro  
**Date**: January 7, 2026 (merged February 8, 2026)

The commit changed `pg_mbstrlen_with_len()` to add bounds checking for security (preventing buffer overruns from corrupted strings). However, this introduced a regression in how TOAST slice operations interact with multi-byte character validation.

**Technical details**:
- The function chain `text_substr()` → `text_substring()` → `pg_mbcharcliplen_chars()` → `pg_mblen_with_len()`
- During TOAST decompression via `detoast_attr_slice()`, the new bounds checking incorrectly validates partial UTF-8 sequences
- Byte 0xe2 is the first byte of a 3-byte UTF-8 sequence (e.g., U+2011 = 0xe2 0x80 0x91)
- The slicing operation cuts at a position that exposes the incomplete sequence to validation

### Affected Versions
- PostgreSQL 15.16 (released ~February 12, 2026)
- PostgreSQL 16.12 (released ~February 12, 2026)
- PostgreSQL 17.8 (released ~February 12, 2026)
- PostgreSQL 18.2 (released February 12, 2026)

### Fix Status
**Fixed in**: An out-of-cycle emergency release scheduled for **February 26, 2026**

**Fix commits**:
- `toast-slice-mblen-v3.patch` (8.7 KB) - Main fix
- `mblen-valgrind-after-report-v1.patch` (1.7 KB) - Additional Valgrind instrumentation

**Fixed by**: Noah Misch (noah@leadboat.com)  
**Fix date**: February 14, 2026  
**Planned release**: February 26, 2026 (out-of-cycle)

### Workarounds
Until the emergency release:

1. **Cast to bytea and back**: 
   ```sql
   SELECT left(convert_from(content::bytea, 'UTF8'), 100) FROM notes;
   ```
   (This bypasses the TOAST slice validation issue)

2. **Disable TOAST compression**:
   ```sql
   ALTER TABLE notes ALTER COLUMN content SET STORAGE EXTERNAL;
   ```
   (Prevents TOAST but may impact performance for large text)

3. **Downgrade to previous minor version** (if available and regression was introduced recently)

### Verification
The content itself is valid UTF-8. You can verify with:
```sql
SELECT convert_from(content::bytea, 'UTF8') FROM notes WHERE id = 'problematic-id';
```

This should succeed, proving the data is valid UTF-8 and the issue is in the function processing, not data corruption.

### References
- Bug Report: https://www.postgresql.org/message-id/20260214053821.fa.noahmisch@microsoft.com
- Search Results: Bug #19406
- Release Info: PostgreSQL 18.2 released February 12, 2026
- Emergency Fix: Scheduled February 26, 2026

### Recommendations

1. **Immediate**: Use the bytea cast workaround for affected queries
2. **Monitor**: Watch for the February 26, 2026 emergency release
3. **Test**: After upgrading to the fixed version, verify all text functions work correctly
4. **Report**: If you encounter this bug, report to pgsql-bugs to help track impact

---

**Research Date**: February 15, 2026  
**Researcher**: Technical Researcher Agent

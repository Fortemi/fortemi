# Issue #362 Fix Summary

## Problem

When calling `update_note` with a non-existent note ID, the MCP server returned "Internal server error" instead of "Resource not found".

### Root Cause

The API had inconsistent behavior when updating non-existent notes:

1. **With content update**: Returned 500 Internal Server Error
   - `update_original()` tried to insert into `activity_log` with a foreign key to the non-existent note
   - This violated the FK constraint: `activity_log_note_id_fkey`
   - Database error leaked to the API response

2. **Without content update**: Returned 404 Not Found (correct)
   - Skipped `update_original()`
   - `fetch()` at the end properly detected missing note

3. **Status endpoint**: Returned 204 No Content (incorrect)
   - `update_status()` silently updated 0 rows without checking existence

## Solution

Added existence checks to both `update_original()` and `update_status()` in `/home/roctinam/dev/matric-memory/crates/matric-db/src/notes.rs`:

```rust
async fn update_original(&self, id: Uuid, content: &str) -> Result<()> {
    // Check if note exists first (issue #362)
    if !self.exists(id).await? {
        return Err(Error::NotFound(format!("Note {} not found", id)));
    }
    // ... rest of implementation
}

async fn update_status(&self, id: Uuid, req: UpdateNoteStatusRequest) -> Result<()> {
    // Check if note exists first (issue #362)
    if !self.exists(id).await? {
        return Err(Error::NotFound(format!("Note {} not found", id)));
    }
    // ... rest of implementation
}
```

## Results

### Before Fix
```
Test 1: Update with content    → 500 Internal Server Error
Test 2: Update without content → 404 Not Found
Test 3: Status endpoint        → 204 No Content (silent success)
```

### After Fix
```
Test 1: Update with content    → 404 Not Found
Test 2: Update without content → 404 Not Found
Test 3: Status endpoint        → 404 Not Found
```

### MCP Server Sanitization

The MCP server's `sanitizeError()` function correctly maps the error:
- API returns: `{"error":"Note 00000000-0000-0000-0000-000000000001 not found"}`
- Status code: 404
- MCP returns to Claude: "Resource not found"

## Test Coverage

1. **Unit tests**: `/home/roctinam/dev/matric-memory/crates/matric-api/tests/update_note_not_found_test.rs`
   - Documents expected behavior
   - Verifies consistency across code paths

2. **Integration tests**: Created multiple test scripts
   - `test-issue-362.js` - Initial investigation
   - `test-issue-362-detailed.js` - Detailed error analysis
   - `test-issue-362-mcp.js` - MCP error sanitization verification

3. **Regression test**: `test-update-note-returns-entity.js`
   - Verifies existing notes still update correctly
   - Confirms no functionality broken

## Verification

All tests pass:
```bash
cargo test --workspace              # ✓ 129 passed
cargo fmt --check                   # ✓ No issues
cargo clippy -- -D warnings         # ✓ No warnings
node test-issue-362-detailed.js     # ✓ All return 404
node test-update-note-returns-entity.js  # ✓ Updates work
```

## Files Changed

1. `/home/roctinam/dev/matric-memory/crates/matric-db/src/notes.rs`
   - Added existence check to `update_original()` (lines 641-644)
   - Added existence check to `update_status()` (lines 606-609)

2. `/home/roctinam/dev/matric-memory/crates/matric-api/tests/update_note_not_found_test.rs`
   - New test file documenting expected behavior

3. Test scripts (for manual verification)
   - `mcp-server/test-issue-362.js`
   - `mcp-server/test-issue-362-detailed.js`
   - `mcp-server/test-issue-362-mcp.js`

## Impact

- **User Experience**: Claude Code users now see "Resource not found" instead of "Internal server error"
- **API Consistency**: All update paths now behave the same for non-existent notes
- **Error Clarity**: Proper 404 status codes instead of 500
- **Security**: Database errors no longer leak to API responses
- **No Breaking Changes**: Existing functionality unchanged

## Follow-up

Consider adding similar existence checks to other update methods:
- `update_revised()`
- `update_title()`
- Other repository methods that modify data

This would provide consistent error handling across the entire API.

# UAT Test JOB-015: Verify Failed Jobs

**Date**: 2026-02-14
**Test**: List jobs with status="failed"
**Result**: ✅ **PASS**

## Test Execution

**MCP Call**:
```
mcp__fortemi__list_jobs(status="failed", limit=10)
```

## Results

**Status**: SUCCESS - API returned valid list of failed jobs

**Failed Jobs Found**: 10 (matching limit)

**Queue Statistics**:
- Total jobs: 1,526
- Pending: 78
- Processing: 1
- Completed (last hour): 53
- Failed (last hour): 0

## Error Information Verification

All 10 failed jobs include complete error information:

### Sample Failed Jobs:

1. **Job ID**: `019c5cd2-a3ef-7183-b9b1-bd77ab3db1a1`
   - Type: `embedding`
   - Note ID: `019c5cd2-7262-7153-a69d-59f864dcaebd`
   - Error: "Failed to fetch note: Not found: Note 019c5cd2-7262-7153-a69d-59f864dcaebd not found"
   - Retry count: 3/3 (max retries exhausted)
   - Created: 2026-02-14T15:43:54Z
   - Completed: 2026-02-14T15:43:57Z

2. **Job ID**: `019c5cd1-cbb2-7930-af9a-58066b389b48`
   - Type: `title_generation`
   - Note ID: `019c5cd1-cb75-7621-834c-746bb6ab6c05`
   - Error: "Note has no content"
   - Retry count: 3/3 (max retries exhausted)
   - Created: 2026-02-14T15:42:58Z
   - Completed: 2026-02-14T15:43:02Z

## Error Patterns Observed

### Error Type 1: Note Not Found (9/10 jobs)
- **Pattern**: "Failed to fetch note: Not found: Note {uuid} not found"
- **Affected job types**: embedding, linking, title_generation, concept_tagging
- **Cause**: Jobs queued for notes that were subsequently deleted

### Error Type 2: Missing Content (1/10 jobs)
- **Pattern**: "Note has no content"
- **Affected job types**: title_generation
- **Cause**: Empty note cannot generate title

## Job Metadata Completeness

Each failed job record includes:
- ✅ Job ID
- ✅ Job type (embedding, linking, title_generation, concept_tagging)
- ✅ Status (all "failed")
- ✅ Note ID (target note UUID)
- ✅ Error message (descriptive error text)
- ✅ Retry information (retry_count, max_retries)
- ✅ Timestamps (created_at, started_at, completed_at)
- ✅ Priority level (2-5)
- ✅ Progress information (progress_percent, progress_message - null for failed)

## Observations

1. **Error messages are informative**: Clearly indicate root cause (note not found vs. empty content)
2. **Retry exhaustion**: All failed jobs show retry_count = 3 = max_retries
3. **Job lifecycle tracked**: Created → Started → Completed timestamps present
4. **Graceful failure**: System properly marks jobs as failed rather than hanging indefinitely
5. **No recent failures**: failed_last_hour = 0 indicates stable recent operation

## Verdict

✅ **PASS** - Failed jobs endpoint returns valid list with complete error information.

**Key Success Criteria Met**:
- API accepts status filter "failed"
- Returns valid JSON list structure
- Failed jobs include error_message field
- Error messages are descriptive and actionable
- Job metadata is complete
- Empty list acceptable (no failures is valid state)

**Count**: 10 failed jobs found (out of 1,526 total)
**Error details present**: Yes, all jobs include error_message field with descriptive text

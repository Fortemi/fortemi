# UC-EXTSTORAGE-007: AI Agent Triggers Reindex via MCP Tool

**Workstream**: WS-8 (MCP Tool Surface)
**Source**: synthesis §4 WS-8, §3 Decision 6, §6 Q-4
**Status**: Draft
**Priority**: MEDIUM (agent-facing UX)

## Actor

**Primary**: AI agent (Claude or other MCP client) acting on user intent
**Secondary**: MCP server (Node.js, port 3001), Fortemi API, DirectoryScanHandler

## Goal

Let an MCP-connected AI agent trigger a Referenced-archive rescan in response to user intent like "I just added new files to my code archive, please re-index", without the user needing to know the REST API surface.

## Preconditions

- MCP server is running and configured with valid OAuth credentials (auto-managed per CLAUDE.md bundle entrypoint)
- Agent has invoked `manage_archives` tool or new `rescan_archive` tool
- Target archive exists and is Referenced mode
- MCP server has network access to API

## Main Success Scenario

1. Agent invokes MCP tool `rescan_archive` with params `{archive_name: "company-docs", full: false}`
2. MCP server validates params against tool schema
3. MCP server forwards request to API: `POST /api/v1/archives/company-docs/rescan` with `{full: false}` body
4. API runs UC-EXTSTORAGE-004 flow; returns HTTP 202 with `{job_id, status_url, archive_status_url}`
5. MCP server returns structured response to agent:

```json
{
  "ok": true,
  "archive_name": "company-docs",
  "job_id": "j-7f3a2b",
  "status_check": "Call get_job_status(job_id='j-7f3a2b') to poll progress, or use manage_archives with action='get_scan_status'",
  "estimated_completion": "depends on directory size; typical 1k-file repo completes in <60 seconds"
}
```

6. Agent reports to user: "Reindex started. I'll check status in a moment."
7. Agent polls via `get_job_status` MCP tool (existing) until `status='completed'`
8. Agent reports completion with summary from `last_scan_summary`

## Alternative Flows

### AF-1: Create Referenced archive via extended `manage_archives`

- Agent invokes `manage_archives` with `{action: 'create', name: 'notes-repo', storage_mode: 'referenced', source_path: '/home/user/notes'}`
- MCP server forwards to `POST /api/v1/archives/referenced` (UC-EXTSTORAGE-001)
- Backward-compat: `manage_archives` without `storage_mode` defaults to `managed` (existing behavior)

### AF-2: Agent queries scan status

- Agent invokes `manage_archives` with `{action: 'get_scan_status', name: 'company-docs'}`
- MCP server forwards to `GET /api/v1/archives/company-docs/scan-status` (UC-EXTSTORAGE-005)
- Returns the same structured payload

## Exception Flows

### EF-1: MCP OAuth credentials missing or expired

- MCP server cannot authenticate to API
- MCP tool returns `{ok: false, error: "mcp_auth_failed", remediation: "Restart MCP server to re-register OAuth credentials"}`
- Per CLAUDE.md, the bundle entrypoint auto-registers on restart

### EF-2: Archive does not exist

- API returns HTTP 404
- MCP tool returns `{ok: false, error: "archive_not_found", available_archives: [...]}` (helpful: lists existing archives)

### EF-3: Archive is Managed mode (rescan not applicable)

- API returns HTTP 400 (per UC-EXTSTORAGE-004 EF-3)
- MCP tool returns `{ok: false, error: "rescan_not_applicable", message: "Archive is Managed mode; reindex is automatic on attachment upload"}`

### EF-4: Scan already in progress

- API returns HTTP 409
- MCP tool returns `{ok: false, error: "scan_in_progress", current_job_id, started_at, suggestion: "Wait for current scan to complete, or call get_job_status to monitor"}`

## Postconditions

- Same as UC-EXTSTORAGE-004 (rescan job enqueued, status reflects progress)
- Agent has structured response usable for natural-language reporting

## Acceptance Criteria

- [ ] AC-1: `rescan_archive` MCP tool schema validates correctly in MCP Inspector
- [ ] AC-2: Agent can invoke `rescan_archive` and receives a valid job_id in <500ms (MCP overhead + API)
- [ ] AC-3: `manage_archives` with `storage_mode: 'referenced'` and `source_path` creates a Referenced archive (extends existing tool — backward compat preserved)
- [ ] AC-4: `manage_archives` without `storage_mode` continues to create Managed archives (no breakage for existing agents)
- [ ] AC-5: Tool descriptions are updated in `get_documentation` MCP tool output, surfacing the new Referenced-archive capability
- [ ] AC-6: Agent polling via `get_job_status` correctly reflects scan progress
- [ ] AC-7: All 4 exception flows produce structured `{ok: false, error, ...}` payloads (not raw HTTP errors)

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-009 (rescan API exposure via MCP)
- NFR-EXTSTORAGE-011 (no regression to existing 43 core MCP tools)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 6, §4 WS-8, §6 Q-4
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-096 (planned): MCP surface for Referenced archives

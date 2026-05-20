# ADR-100: MCP Tool Authorization Gate

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-071 (auth middleware), ADR-089 (authorization policy), ADR-091 (audit)

## Context

Fortemi exposes 43 MCP (Model Context Protocol) tools per the README. The current MCP routing layer:
- Authenticates the OAuth client (mm_at_* token)
- Validates the request structure
- Dispatches to the tool implementation

It does **not** enforce per-tool authorization. A client with any valid OAuth scope can invoke any tool. This is consistent with the broader missing-authorization gap (ADR-089), but MCP requires its own ADR because:

- MCP tools span low-risk reads (search) and high-risk admin operations (backup export, archive deletion, model configuration)
- MCP clients (LLM-driven agents in particular) compose tool calls in ways human-facing APIs do not — accidental misuse compounds
- Different OAuth clients may need different tool subsets (e.g., a "read-only research assistant" vs a "content editor")

## Decision

**Each MCP tool declares a required scope. The MCP dispatcher enforces scope via `AuthorizationPolicy` (ADR-089) before invoking the tool. Default scopes mirror current OAuth scope vocabulary; EE plugins may extend with finer-grained policy.**

### Tool scope declaration

Every tool registration declares:

```rust
McpTool {
    name: "search_notes",
    required_scope: "notes:read",
    required_action: Action("mcp:search_notes"),
    handler: ...,
    audit_severity: AuditSeverity::Info,
}
```

`required_scope` is a coarse-grained OAuth scope string (existing vocabulary).
`required_action` is the fine-grained action passed to `AuthorizationPolicy::authorize`.

### Initial scope map for the 43 tools

| Tool category | Scope | Example tools |
|---|---|---|
| Search & discovery | `notes:read` | `search_notes`, `find_related`, `get_note` |
| Note CRUD | `notes:write` | `create_note`, `update_note`, `delete_note` |
| Embedding / tagging | `notes:write` | `embed_note`, `tag_note`, `link_notes` |
| Archive read | `archives:read` | `list_archives`, `archive_stats` |
| Archive admin | `archives:admin` | `create_archive`, `delete_archive`, `set_default_archive` |
| Job control | `jobs:read` / `jobs:admin` | `list_jobs`, `cancel_job` |
| Model / inference config | `system:admin` | `set_model_config`, `register_provider` |
| Backup / export | `system:admin` | `export_archive`, `import_archive` |
| Health / metrics | `system:read` | `health_check`, `metrics_snapshot` |

The exact map is finalized as a subtask of this ADR with each tool inspected against current behavior. The list above is the planned starting point.

### Authorization flow per tool call

```
Client invokes MCP tool
  → MCP dispatcher resolves tool name → required_scope, required_action
  → Check OAuth scope contains required_scope (cheap; existing logic)
  → If yes, call AuthorizationPolicy::authorize(principal, required_action, resource, ctx)
  → If Allow: invoke tool, emit mcp.tool_invoked audit event
  → If Deny: return JSON-RPC error -32603 with reason, emit mcp.tool_denied event
```

### Tool-call rate limit

Independent of general API rate-limit (ADR-098), MCP tools have per-tool, per-client rate limits:

| Tier | Limit |
|---|---|
| Read tools | 100/min/client |
| Write tools | 30/min/client |
| Admin tools | 10/min/client |

Enforced via the same `QuotaPolicy` mechanism with a new dimension `McpToolCall { tool: String }`.

### Tool-level audit events

| Outcome | Event | Severity |
|---|---|---|
| Tool invoked successfully | `mcp.tool_invoked` | Info |
| Tool invocation denied (scope) | `mcp.tool_scope_denied` | Notice |
| Tool invocation denied (policy) | `mcp.tool_policy_denied` | Notice |
| Admin-tier tool invoked | `mcp.admin_tool_invoked` | Notice (escalated) |

### Plugin extension surface

EE plugins may install custom action handlers and policies:

- `fortemi-enterprise-mcp-gate-policies` — additional `AuthorizationPolicy` rules specifically for MCP context
- `fortemi-enterprise-mcp-tool-allowlist` — tenant-defined allowlist of MCP tools (default: all allowed; tenants can restrict)

### Tool deprecation and lifecycle

Tools added to the registry MUST declare a `stability_tier` (Stable / Beta / Experimental). Removals follow:
- 1 minor version of deprecation warning in tool responses
- Stable tools require a major version bump to remove

## Consequences

### Positive
- (+) MCP tool surface aligned with authorization plane
- (+) Per-tool granular control
- (+) Audit trail per tool invocation
- (+) Tenants can restrict tool subsets (defense in depth against compromised agent)
- (+) Existing OAuth scope vocabulary extended naturally

### Negative
- (-) Per-tool authorize call adds latency (sub-ms typical via in-memory cache)
- (-) Tenant tool-allowlist UX requires UI work (not in scope here)
- (-) Refactoring 43 tool registrations to declare scopes is one-time work

### Neutral
- (~) MCP `list_tools` response filtered to only the tools the requesting client has scope for (security-by-default; may surprise existing clients)

## Implementation

**Code location:**
- Tool registry refactor: `crates/matric-api/src/mcp/registry.rs`
- Authorization wire-in: `crates/matric-api/src/mcp/dispatcher.rs`

**Phases:**
1. Refactor tool registration to declare `required_scope` and `required_action`
2. Audit each of the 43 tools and finalize the scope map
3. Wire authorization check into dispatcher
4. Add tool-level audit events
5. Add per-tool rate limit
6. Filter `list_tools` by scope

**Testing:**
- For each tool: matrix of (scope-present + scope-absent + Anonymous + tenant-allowlist-yes/no)
- Property test: list_tools never returns a tool the caller cannot invoke

## References

- Model Context Protocol specification
- ADR-071, ADR-089, ADR-091
- OAuth 2.0 RFC 6749 §3.3 (scopes)

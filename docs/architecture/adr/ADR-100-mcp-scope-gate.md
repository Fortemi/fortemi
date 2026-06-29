# ADR-100: MCP Tool Authorization Gate

**Status:** Proposed (revised 2026-06-29 — Fortemi/fortemi#893)
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-071 (auth middleware), ADR-089 (authorization policy), ADR-091 (audit), ADR-098 (rate limits/quotas)
**Implementation tracker:** Fortemi/fortemi#718 (MCP tool authorization gate — per-tool scope + audit)
**Blocked by:** #710 (`AuthorizationPolicy` + decision types), #711 (`AuditSink` + mandatory audit taxonomy), #713/#714 (`UsageMeter`/`QuotaPolicy` quota dimensions). The Rust in-process MCP path (#853) MUST consume this same gate rather than create a second authorization model.

## Revision history

- **2026-06-29 (#893):** Aligned with the current MCP authorization model. (1) Removed the hard-coded `-32603 with reason` denial contract pending a public-error-shape security review. (2) Reframed per-tool rate limits as quota *dimensions* owned by #713/#714 rather than standalone fixed per-minute tiers. (3) Added that MCP tool annotations (`readOnlyHint`/`destructiveHint`/`idempotentHint`/`openWorldHint`) are client-facing risk/UX hints only and are never authorization input. (4) Added remote-MCP OAuth protected-resource requirements for Streamable HTTP, kept separate from stdio/local credential behavior. (5) Corrected the implementation location: tool metadata lives first in the active Node `mcp-server/` registry and is shared with / migrated to the Rust MCP path (#853).

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

### Three separate layers

Per-tool authorization is one of three independent layers; none substitutes for another:

1. **Transport authentication / authorization** — the MCP client is authenticated as an OAuth client. For remote (Streamable HTTP) the server is an OAuth *protected resource* (see "Remote MCP" below). This proves *who* is calling, not *what* they may invoke.
2. **MCP tool annotations** (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) — client-facing risk/UX hints only. They MAY guide client approval UX. They are **never** treated as authorization input (see "Annotations are hints, not authorization").
3. **Fortemi `AuthorizationPolicy` decision** — the server-side per-tool authorization made by `AuthorizationPolicy::authorize` (ADR-089). This is the authoritative gate.

### Authorization flow per tool call

```
Client invokes MCP tool
  → MCP dispatcher resolves tool name → required_scope, required_action, resource/tenant extraction
  → Check OAuth scope family contains required_scope (cheap; existing logic)
  → Call AuthorizationPolicy::authorize(principal, required_action, resource, ctx)
  → Allow:  invoke tool, emit mcp.tool_invoked audit event
  → Deny:   do NOT invoke handler; return a stable public denial (below); emit mcp.tool_*_denied event
```

### Denial contract (public error shape)

The denial response MUST be a **stable, public JSON-RPC error** that does not leak hidden tool metadata, tenant state, scope/policy internals, or the richer internal deny reason. The earlier draft hard-coded `-32603 (Internal error) with reason`; that is withdrawn because (a) `-32603` conflates internal failures with authorization denial, and (b) embedding a reason string risks disclosing policy/tenant detail.

Requirements (exact code/shape finalized in #718 under security review):

- Use a dedicated, documented application error code distinct from `-32603 Internal error` and from `-32600/-32601/-32602` (malformed/unknown-method/bad-params). A server-defined code in the JSON-RPC implementation-defined range (`-32000..-32099`) is the expected choice.
- The public `message`/`data` carries only a stable, generic denial indicator and (optionally) a correlation id. It MUST NOT carry the policy reason, the required scope/action, tenant identifiers, or whether the tool merely *exists*.
- The richer internal deny reason (scope-missing vs policy-denied vs unknown-tool) is recorded only in the audit event and server logs, never in the client-visible error.
- Unknown-tool and not-authorized SHOULD be indistinguishable to an unauthorized caller, consistent with `list_tools` filtering, so the gate does not become a tool-enumeration oracle.

### Tool-call quota (dimension, not fixed tiers)

MCP tool calls are a **quota dimension** owned by `UsageMeter`/`QuotaPolicy` (#713) and enforced by the quota middleware (#714) — not a standalone rate limiter with ADR-100-specific fixed limits. ADR-100 only defines the *dimension and tool-class metadata*; the concrete limits, windows, and per-tenant policy live in the quota layer.

- Each tool declares a **quota dimension class** (e.g., read / write / admin tool class) in its registry metadata.
- The dispatcher emits a `UsageEvent` for the call's dimension class; `QuotaPolicy` returns allow / soft-limit / hard-limit.
- Concrete numeric limits are policy/plan configuration in #713/#714, not hard-coded here. (The previous draft's fixed `100/30/10 per-minute` tiers were illustrative and are withdrawn as the contract.)
- Until #713/#714 land, the dispatcher MAY fall back to the general API rate limit (ADR-098); per-tool quota is emitted once the quota traits exist.

### Tool-level audit events

| Outcome | Event | Severity |
|---|---|---|
| Tool invoked successfully | `mcp.tool_invoked` | Info |
| Tool invocation denied (scope) | `mcp.tool_scope_denied` | Notice |
| Tool invocation denied (policy) | `mcp.tool_policy_denied` | Notice |
| Admin-tier tool invoked | `mcp.admin_tool_invoked` | Notice (escalated) |

### Annotations are hints, not authorization

MCP tool annotations — `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint` — are **client-facing risk/UX hints**. They MAY inform a client's human-approval prompt or an agent's caution. They are advisory metadata supplied alongside the tool and MUST NOT be used as a server-side authorization input. The server authorizes solely on `required_scope` + `AuthorizationPolicy::authorize(required_action, resource, ctx)`. A tool marked `readOnlyHint: true` still goes through the full policy check; an annotation can never widen or substitute for a policy decision. Plugin-provided tools likewise declare their capabilities at load time; runtime registration cannot silently introduce broader scopes.

### Remote MCP: OAuth protected resource (Streamable HTTP)

For remote MCP over Streamable HTTP, the MCP server acts as an **OAuth protected resource / resource server**, distinct from the per-tool authorization layer:

- The server exposes OAuth **protected-resource metadata** (RFC 9728) so clients can discover the authorization server, and validates that presented tokens carry the correct resource/audience (RFC 8707; aligns with #917).
- Transport authentication establishes the authenticated principal; **per-tool authorization remains an application decision** made by `AuthorizationPolicy`, never by the transport layer alone.
- Streamable HTTP authorization behavior (protected-resource metadata, token/audience validation, session/header handling) is documented and tested **separately** from stdio/local credential behavior, where the caller is trusted by process boundary and a local API key.
- `MCP_TOOL_MODE` (`core`/`full`) is a surface-size control, not an authorization control; it does not replace the per-tool gate.

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

Tracked by **#718**. Tool authorization metadata lives **first in the active Node MCP server** (`mcp-server/`), which is the runtime that ships today (stdio, SSE, Streamable HTTP). The metadata contract defined here is the shared source of truth: the Rust in-process MCP path (#853) MUST consume the **same** tool-metadata contract rather than diverge into a second authorization model. The earlier draft pointed implementation at `crates/matric-api/src/mcp/*`; that location does not match the active server and is superseded by this Node-first + shared-contract approach.

**Code location:**
- Active: Node MCP server tool registry + dispatcher (`mcp-server/`), e.g. `mcp-server/tools.js` registration metadata and the dispatch path in `mcp-server/index.js`.
- Future: Rust MCP path (#853) consumes the identical `required_scope` / `required_action` / resource-extraction / audit-severity / stability-tier / quota-dimension contract.

**Phases:**
1. Add per-tool authorization metadata (scope family, fine-grained `Action`, resource/tenant extraction, audit severity, stability tier, quota dimension class) to the Node tool registry.
2. Check the initial tool inventory against `.aiwg/architecture/roles-scopes-catalog.md` and `docs/content/mcp-permissions.md`.
3. Wire the dispatcher to call `AuthorizationPolicy::authorize` before invocation (consumes #710).
4. Emit tool-level audit events (consumes #711/#910): invoked, scope-denied, policy-denied, admin-invoked.
5. Emit the per-tool quota dimension once #713/#714 exist.
6. Filter `list_tools` by what the caller can actually invoke (unknown-tool ≈ not-authorized).
7. Ensure the Rust MCP path (#853) binds to the same contract.

**Testing:**
- Per tool: matrix of (scope present/absent, principal type, tenant allowlist yes/no, risk tier, plugin-provided tool).
- Property test: `list_tools` never returns a tool the caller cannot invoke.
- Denial: stable public error shape with no hidden tool metadata / tenant / policy-reason leakage.
- Streamable HTTP: protected-resource metadata + token/audience validation tested separately from stdio/local credentials.

## References

- Implementation tracker: Fortemi/fortemi#718; this refresh: #893
- Blockers: #710 (`AuthorizationPolicy`), #711/#910 (`AuditSink`/audit), #713/#714 (`UsageMeter`/`QuotaPolicy`); Rust MCP path: #853; resource/audience: #917
- ADR-071 (auth middleware), ADR-089 (authorization policy), ADR-091 (audit), ADR-098 (rate limits/quotas)
- Roles/scopes catalog: `.aiwg/architecture/roles-scopes-catalog.md`; MCP permissions: `docs/content/mcp-permissions.md`
- OAuth 2.0 RFC 6749 §3.3 (scopes); RFC 8707 (resource indicators); RFC 9728 (OAuth protected resource metadata)
- MCP authorization spec: https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- MCP Streamable HTTP transport: https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
- MCP tool annotations: https://blog.modelcontextprotocol.io/posts/2026-03-16-tool-annotations/

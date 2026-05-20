# Roles & Scopes Catalog

**Status:** Proposed (initial vocabulary)
**Date:** 2026-05-20
**Owner:** roctinam, security review TBD
**Related:** ADR-089 (Authorization Policy Trait), ADR-090 (Tenancy), ADR-100 (MCP Tool Gate), Fortemi/fortemi#28

## Purpose

Define the **concrete starter vocabulary** for roles, scopes, and the role→scope mapping. ADR-089 establishes the trait surface; this catalog establishes what gets passed to `AuthorizationPolicy::authorize()` and which scopes are issued on OAuth tokens.

This is the seed vocabulary. Once ratified, additions follow `.aiwg/process/action-vocabulary.md` governance (forthcoming via #28).

## Mental model

```
Principal --(belongs to)--> Tenant --(has)--> Roles
                                              │
                                              └──> grants Scopes
                                                            │
                                                            └──> permits Actions on Resources
```

- **Tenant**: isolation boundary (per ADR-090)
- **Role**: a named bundle of scopes within a tenant; assigned per-tenant per-principal
- **Scope**: a granular permission string in the format `<resource>:<verb>[:<sub>]`; tokens carry these
- **Action**: what the policy decision is about (`mcp:search_notes`, `notes:create`)
- **Resource**: what the action targets (a specific note, an archive, a tenant)

Scope is **what the token says you can do**. Action is **what you're trying to do right now**. The policy decides whether the scope + role + resource attributes permit the action.

## Roles

Six roles cover the access spectrum. Roles are **tenant-scoped** unless explicitly marked `system`.

| Role | Tenant-scoped? | Scopes granted | Typical user |
|---|---|---|---|
| `viewer` | Yes | `notes:read`, `archives:read`, `tags:read`, `templates:read`, `mcp:read` | Read-only researcher, shared link recipient, audit reviewer |
| `editor` | Yes | viewer + `notes:write`, `tags:write`, `attachments:write`, `embeddings:write`, `mcp:write` | Standard user / knowledge worker |
| `power_user` | Yes | editor + `archives:write`, `templates:write`, `jobs:read`, `collections:write` | Heavy user who manages their own archives and templates |
| `archive_admin` | Yes | power_user + `archives:admin`, `jobs:admin`, `provenance:read` | Tenant-side admin for archive lifecycle |
| `tenant_admin` | Yes | archive_admin + `tenant:admin`, `users:admin`, `audit:read`, `billing:read` | Customer-side admin: manage users, view audit, view billing |
| `system_admin` | **No** (cross-tenant) | All of the above + `system:*` + `tenant_admin:*` | Fortemi staff / EE operator |

### Role composition rules

1. A principal has **0 or 1 role per tenant**. Roles do not stack additively (use `power_user` rather than `viewer + editor`).
2. A principal can belong to multiple tenants with different roles in each.
3. `system_admin` is **not** a tenant role — it crosses tenants and is granted to a small set of operator identities (Fortemi staff, on-call engineers).
4. Every cross-tenant access by `system_admin` emits a `system.cross_tenant_access` audit event (per ADR-091).

### Implementation note

In the OAuth/JWT claim layer, the role is **derived** from the user's tenant membership at token-issue time:

```json
{
  "sub": "user_01HX...",
  "tenant_id": "tenant_acme",
  "role": "editor",
  "scope": "notes:read notes:write tags:read tags:write ...",
  "exp": 1716230000
}
```

The `scope` claim is the canonical permission list. The `role` claim is informational (used for UI, audit-readability). Authorization decisions read `scope`, not `role`.

## Scopes

Format: `<resource>:<verb>[:<qualifier>]`

### Content scopes

| Scope | Permits |
|---|---|
| `notes:read` | List, search, get notes |
| `notes:write` | Create, update notes (own + shared) |
| `notes:delete` | Soft-delete notes (own + shared) |
| `notes:admin` | Hard-delete, restore, transfer ownership |
| `attachments:read` | Download attachments |
| `attachments:write` | Upload attachments |
| `attachments:delete` | Delete attachments |
| `embeddings:read` | Read embedding metadata (not raw vectors typically) |
| `embeddings:write` | Trigger re-embedding, change embedding sets |
| `links:read` | Read note relationships |
| `links:write` | Create/delete note links |
| `tags:read` | Read SKOS tags and concept tree |
| `tags:write` | Apply/remove tags from notes |
| `tags:admin` | Edit the SKOS concept tree itself |
| `collections:read` | Read collections |
| `collections:write` | Create/edit collections |
| `templates:read` | Read note templates |
| `templates:write` | Create/edit note templates |

### Archive & infrastructure scopes

| Scope | Permits |
|---|---|
| `archives:read` | List archives, archive stats |
| `archives:write` | Create archives, set default |
| `archives:admin` | Delete archives, configure routing, schema migrations |
| `jobs:read` | List own jobs, check status |
| `jobs:write` | Submit jobs |
| `jobs:admin` | Cancel any job, requeue failed jobs |
| `provenance:read` | Read provenance graph and lineage |
| `provenance:write` | Annotate provenance |

### Inference / models

| Scope | Permits |
|---|---|
| `inference:use` | Call inference endpoints (chat, complete, embed) |
| `inference:configure` | Override model selection per-request |
| `inference:admin` | Configure providers, register models |

### Tenant administration

| Scope | Permits |
|---|---|
| `tenant:read` | Read tenant metadata, member list |
| `tenant:admin` | Update tenant settings, suspend/restore tenant |
| `users:read` | List users in tenant |
| `users:admin` | Invite, remove, role-change users in tenant |
| `audit:read` | Read audit events for the tenant |
| `billing:read` | Read usage and billing data |
| `billing:admin` | Change billing settings (payment method, plan) |

### System (cross-tenant; `system_admin` only)

| Scope | Permits |
|---|---|
| `system:read` | Read cross-tenant metrics, health |
| `system:tenant_admin` | Create/delete tenants, impersonate for support (with audit) |
| `system:admin` | Configure platform-level providers, registries, keys |
| `system:audit_read` | Read cross-tenant audit log |

### MCP scopes (per ADR-100)

MCP tools require both the underlying resource scope AND an MCP wrapper scope:

| Scope | Permits |
|---|---|
| `mcp:read` | Invoke read-tier MCP tools (search, get, list) |
| `mcp:write` | Invoke write-tier MCP tools (create, update, embed) |
| `mcp:admin` | Invoke admin-tier MCP tools (export, configure) |

A token must have `mcp:read` AND `notes:read` to invoke `mcp:search_notes`. This double-gating is intentional: it lets a customer issue a token that has note read access but not MCP access (e.g., for a direct REST integration that should not pretend to be an LLM agent).

### OAuth & identity scopes

| Scope | Permits |
|---|---|
| `oauth:client_register` | Register new OAuth clients |
| `oauth:token_introspect` | Introspect tokens (typically for resource servers) |
| `oauth:token_revoke` | Revoke tokens (self by default; with `users:admin`, revoke others) |
| `api_keys:read` | List own API keys |
| `api_keys:write` | Create/revoke own API keys |
| `api_keys:admin` | Manage API keys for other users (requires `users:admin`) |

### Privacy / DSAR (per ADR-099)

| Scope | Permits |
|---|---|
| `privacy:submit` | Submit a DSAR (own data) |
| `privacy:admin` | Process DSARs on behalf of others (compliance officer) |
| `privacy:audit` | Read DSAR audit trail |

## Role → scope matrix

`✓` = granted; `~` = own-resources only (enforced via resource attribute, not scope)

| Scope | viewer | editor | power_user | archive_admin | tenant_admin | system_admin |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Content** ||||||
| `notes:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `notes:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `notes:delete` | | ~ | ✓ | ✓ | ✓ | ✓ |
| `notes:admin` | | | | ✓ | ✓ | ✓ |
| `attachments:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `attachments:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `attachments:delete` | | ~ | ✓ | ✓ | ✓ | ✓ |
| `embeddings:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `embeddings:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `links:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `links:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `tags:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `tags:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `tags:admin` | | | ✓ | ✓ | ✓ | ✓ |
| `collections:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `collections:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `templates:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `templates:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Archives** ||||||
| `archives:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `archives:write` | | | ✓ | ✓ | ✓ | ✓ |
| `archives:admin` | | | | ✓ | ✓ | ✓ |
| `jobs:read` | | ✓ (own) | ✓ | ✓ | ✓ | ✓ |
| `jobs:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `jobs:admin` | | | | ✓ | ✓ | ✓ |
| `provenance:read` | | | | ✓ | ✓ | ✓ |
| **Inference** ||||||
| `inference:use` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `inference:configure` | | | ✓ | ✓ | ✓ | ✓ |
| `inference:admin` | | | | | ✓ | ✓ |
| **MCP** ||||||
| `mcp:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `mcp:write` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `mcp:admin` | | | | ✓ | ✓ | ✓ |
| **Tenant** ||||||
| `tenant:read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `tenant:admin` | | | | | ✓ | ✓ |
| `users:read` | | | | ✓ | ✓ | ✓ |
| `users:admin` | | | | | ✓ | ✓ |
| `audit:read` | | | | | ✓ | ✓ |
| `billing:read` | | | | | ✓ | ✓ |
| `billing:admin` | | | | | ✓ | ✓ |
| **Identity** ||||||
| `oauth:client_register` | | ✓ | ✓ | ✓ | ✓ | ✓ |
| `api_keys:read` (own) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `api_keys:write` (own) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `api_keys:admin` | | | | | ✓ | ✓ |
| **Privacy** ||||||
| `privacy:submit` (own) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `privacy:admin` | | | | | ✓ | ✓ |
| `privacy:audit` | | | | | ✓ | ✓ |
| **System** ||||||
| `system:read` | | | | | | ✓ |
| `system:tenant_admin` | | | | | | ✓ |
| `system:admin` | | | | | | ✓ |
| `system:audit_read` | | | | | | ✓ |

### Resource-level discrimination

A scope like `notes:write` permits the *capability*. Whether the principal can write to a *specific* note is the policy's call, using resource attributes:

```rust
authorize(
    principal: editor_alice,
    action: Action("notes:write"),
    resource: Resource {
        kind: ResourceKind::Note,
        id: Some("note_01HX..."),
        tenant_id: Some("tenant_acme"),
        attrs: { "created_by": "user_bob", "shared_with": ["user_alice"] }
    },
    ctx: ...,
)
```

The default policy (`RoleBasedPolicy`) allows when:
- Principal has the scope, AND
- Resource is in principal's tenant, AND
- Resource ownership matches (own-resource for `~` rows in the matrix; any resource in tenant for `✓`)

Casbin/OPA-based EE policies can express richer rules (e.g., "users in collection X may write to notes in collection X regardless of ownership").

## Custom roles (EE)

EE deployments may want custom roles beyond the six standard ones (e.g., "compliance_officer" who has `audit:read` + `privacy:admin` but not `tenant:admin`).

The EE plugin `fortemi-enterprise-rbac` reads role definitions from a tenant-scoped configuration table:

```sql
CREATE TABLE tenant_role_definitions (
    tenant_id text NOT NULL,
    role_name text NOT NULL,
    inherits text[] NOT NULL,   -- e.g., ['viewer']
    additional_scopes text[] NOT NULL,
    removed_scopes text[] NOT NULL,
    PRIMARY KEY (tenant_id, role_name)
);
```

Standard roles cannot be modified; only `additional_scopes` and `removed_scopes` overlays on top of inherited scopes.

## Service accounts (machine identities)

Service accounts are principals without an interactive user. They:
- Use API keys (`mm_key_*`) or client-credentials OAuth flow
- Are bound to a tenant
- Get a role at creation (typically `editor` or a custom role)
- Cannot self-elevate

`api_keys:admin` is required to create a service account; the creating user's tenant becomes the account's tenant.

## What's not covered (deferred)

- **Attribute-based fine-grained policy** (e.g., time-of-day, IP allowlist, device posture) — emergent via EE Casbin/OPA plugin (#15) and resource `attrs`
- **Custom scope vocabulary** — `plugin.<name>:<verb>` namespace is reserved (per #28); concrete pattern lives in plugin author docs
- **Cross-tenant collaboration** (e.g., user in tenant A reads a shared note from tenant B) — separate ADR if/when needed; today, cross-tenant is `system_admin` only
- **Delegation tokens** (user A grants user B a subset of their scopes for a limited time) — separate ADR; lower priority

## Open questions for review

1. Should `editor` have `notes:delete` for **own** notes only (current proposal) or all notes? Practitioner argument for "any" — collaboration feel. Counterargument — soft-delete-recoverable mitigates accidental loss.
2. Should `inference:use` be a free-tier scope (i.e., not a separate gate at all) or always required? Treating as required leaves the option to throttle/disable per plan.
3. Service accounts in `tenant_admin` role — yes or no? Operationally yes (automation needs it); security-wise risky.
4. Is `mcp:*` a useful wrapper, or does it add ceremony without benefit? Argument for: lets a customer disable LLM-agent access without disabling REST access.

## Sign-off (before #15 implementation lands)

- [ ] Six standard roles ratified
- [ ] Role→scope matrix reviewed
- [ ] Resource-level discrimination semantics agreed
- [ ] Custom-role mechanism agreed
- [ ] Open questions resolved

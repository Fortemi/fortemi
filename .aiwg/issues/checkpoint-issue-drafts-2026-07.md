# fortemi Issue Drafts - SDLC Checkpoint 2026-07

## 1. Critical: Implement ADR-090 RLS invariants before any hosted multi-tenant deployment

**Filed:** `Fortemi/fortemi#1016`

**Labels:** `security`, `architecture`, `tier/licensed-server`, `sdlc/checkpoint`, `P0`

### Problem

ADR-090 describes multi-tenancy through Postgres RLS, but the July checkpoint found no migrations enabling RLS or `app.current_tenant`. Shared-schema hosted deployment must not proceed with only application-side `tenant_id` filters.

### Acceptance Criteria

- Every tenant-scoped table has `ENABLE ROW LEVEL SECURITY` and `FORCE ROW LEVEL SECURITY`.
- Tenant policies bind rows to `current_setting('app.current_tenant', true)` or an approved equivalent.
- Application code sets tenant context transaction-locally before tenant-scoped queries.
- Hosted DB role is `NOSUPERUSER NOBYPASSRLS`.
- CI has a policy check that fails when tenant tables lack RLS.
- Tests prove cross-tenant access fails by default and succeeds only through audited/admin-approved paths.

## 2. High: Rebaseline ADR-088 through ADR-100 statuses against implementation reality

**Filed:** `Fortemi/fortemi#1017`

**Labels:** `architecture`, `planning`, `sdlc/checkpoint`, `needs-decision`

### Problem

Several enterprise launch ADRs are accepted or proposed without a clear implementation phase. ADR-090 and ADR-093 are especially risky because their accepted status can be read as implementation readiness.

### Acceptance Criteria

- ADR-090 and ADR-093 explicitly distinguish accepted target architecture from current implementation state, or link to completed implementation evidence.
- ADR-088, 089, 091, 092, 095, 097, 098, 099, and 100 have explicit phase owner and status.
- Roadmap/milestone 62 references the rebaselined ADR states.
- Any ADR that remains proposed has a blocking decision owner and date.

### Checkpoint Artifact

- `.aiwg/architecture/adr-rebaseline-checklist-2026-07.md` captures the July checkpoint matrix for ADR-088 through ADR-100, including current code evidence, owner/tracker, and allowed claims.
- `docs/architecture/adr/ADR-088-*` through `ADR-100-*` and `.aiwg/planning/roadmap.md` now carry July 2026 checkpoint notes that distinguish target architecture from implementation readiness.

## 3. High: Add API compatibility contract for HotM and hosted demos

**Filed:** `Fortemi/fortemi#1018`

**Labels:** `api`, `delivery`, `hotm-ux`, `sdlc/checkpoint`

### Problem

HotM consumes `/api/v1` without a strong version/compatibility guard. Pre-GA API changes can silently break the demo path and enterprise UX.

### Acceptance Criteria

- Fortemi exposes a stable version/capability endpoint or header.
- Response includes semantic capability flags relevant to HotM: auth mode, realtime support, premium/backoffice support, and MCP scope-gate support.
- HotM has enough information to block unsupported flows before invoking incompatible APIs.
- Compatibility behavior is documented and contract-tested.

### Checkpoint Artifact

- `.aiwg/architecture/api-compatibility-discovery-contract-2026-07.md` defines `GET /api/v1/system/compatibility` response shape, capability states, public-safety rules, reason codes, and Fortemi/HotM contract-test expectations.
- Initial endpoint implementation exists in `crates/matric-api/src/main.rs` with conservative enterprise/backoffice states and focused regression tests.

## 4. High: Establish KeyProvider/KMS construction gate

**Filed:** `Fortemi/fortemi#1019`

**Labels:** `security`, `architecture`, `tier/licensed-server`, `P0`

### Problem

ADR-093 describes a KeyProvider/KMS seam, but the codebase does not yet expose the trait or hosted fail-closed behavior. This blocks hosted secret lifecycle, encryption-at-rest, and mandatory audit chains.

### Acceptance Criteria

- `KeyProvider` contract is implemented or ADR-093 is re-scoped.
- Hosted mode has a fail-closed startup assertion when required KMS config is absent.
- Envelope metadata/AAD schema is documented and tested.
- KMS lifecycle events are audit-producing.

## 5. Medium: Add backoffice API contract discovery for enterprise UX

**Filed:** `Fortemi/fortemi#1020`

**Labels:** `backoffice`, `api`, `tier/licensed-server`, `hotm-ux`

### Problem

HotM now has a coarse compatibility contract and fixture-backed backoffice preview, but authenticated admin/backoffice APIs for tenant health, audit posture, quota, KMS, and premium component operations are not yet production contracts.

### Acceptance Criteria

- Draft admin/backoffice API contract exists.
- Contract identifies required scopes/actions and audit events.
- Capability discovery exposes whether the current deployment supports each surface.
- Stub/preview responses are gated from production unless explicitly enabled.
- Coarse compatibility states remain preview/unavailable until authenticated admin APIs and role/scope/audit gates are implemented.

# Progress: Fortemi CE/EE Architecture Audit + SDLC Documentation

## Task contract
- **Original request**: "leveraging the aiwg sdlc team complete all documentation needed including additional adrs, sdr audit docs, templates processes, etc. then file detailed issues for all planned work"
- **Subject**: Fortemi core (`/home/roctinam/dev/fortemi/fortemi`) + sibling repos (fortemi-auth, fortemi-react, HotM). Goal: CE on BSL-1.1 + EE plugin repos = full enterprise suite.
- **Completion criteria** (measurable):
  - Audit report committed at `.aiwg/architecture/ce-ee-audit-2026-05.md`
  - 12 new ADRs (ADR-088 through ADR-099) drafted in `docs/architecture/adr/`
  - Plugin contract spec at `.aiwg/architecture/plugin-contract-spec.md`
  - Tenancy threat model at `.aiwg/security/multi-tenant-threat-model.md`
  - EE plugin crate template at `.aiwg/templates/ee-plugin-crate/`
  - CE/EE contribution process doc at `.aiwg/process/ce-ee-contribution.md`
  - Plugin certification process at `.aiwg/process/plugin-certification.md`
  - Issues filed: 1 epic + ~20 implementation issues on `Fortemi/fortemi` (GitHub); 1 cross-repo coordination issue on each of fortemi-auth (Gitea), HotM, fortemi-react
  - All ADRs cross-link the audit report; all issues reference their ADR

- **Authorization scope**:
  - **In-scope**: write docs in `.aiwg/`, draft ADRs in `docs/architecture/adr/`, file GitHub/Gitea issues
  - **Requires human approval before**: pushing branches/opening PRs (per delivery policy), closing/merging anything, modifying production source code, touching license terms

## Findings catalog (from audit conversation 2026-05-20)
Carrying forward so post-compaction agents have full context:

### Critical security gaps
1. `REQUIRE_AUTH` defaults off — needs fail-closed inversion → ADR-094
2. No authorization layer (`AuthPrincipal` exists, no `can()` decision) → ADR-089
3. Tenancy ≠ archives; no `tenant_id` propagation through `AuthContext` → `SchemaContext` → ADR-090
4. `fortemi-auth` consumed via SSH git dep — supply-chain hazard → ADR-096
5. `agent-proxy` raw API keys (localhost-only) — must not be reused for multi-tenant
6. No CSP/CORS hardening visible in fortemi-react when plugin-loaded JS lands

### Missing traits (pluggability)
- `AuthorizationPolicy` (RBAC/ABAC plug-point) → ADR-089
- `AuditSink` → ADR-091
- `UsageMeter` (quota/billing/metering) → ADR-092
- `KeyProvider` (BYOK/HSM/KMS) → ADR-093
- `DataSubjectRequestHandler` (GDPR/CCPA) → ADR-099
- MCP tool authorization gate → ADR-100

### Strategic/structural decisions
- Plugin loading strategy (Cargo features primary + out-of-process for risk) → ADR-088
- CE/EE distribution shape → ADR-095
- Private Cargo registry / vendored mirror → ADR-096
- Stateless API verification → ADR-097
- Per-tenant rate limits/quotas → ADR-098

### Existing strengths (preserve)
- `matric-core/src/traits.rs` — 14 well-designed traits
- ADR-072 inference provider registry — model for other plug surfaces
- ADR-068 schema isolation (archives) — extends to tenancy
- ADR-071 auth middleware (Clerk + JWT) + fortemi-auth crate workspace
- ADR-048 extraction adapter pattern — 18 adapters working

## Failed approaches (do not retry)
- (none yet)

## Phases & status
- [x] Phase 0: Audit report drafted in chat (above)
- [ ] Phase 1: Write audit report doc + tenancy threat model (3 parallel agents)
- [ ] Phase 2: Write ADR-088 through ADR-093 (6 ADRs, 3 parallel agents x 2 waves)
- [ ] Phase 3: Write ADR-094 through ADR-099 (6 ADRs, 3 parallel agents x 2 waves)
- [ ] Phase 4: Write ADR-100 + plugin contract spec + templates + process docs (3 parallel agents)
- [ ] Phase 5: File epic + ~20 detailed issues on GitHub Fortemi/fortemi
- [ ] Phase 6: File coordination issues on fortemi-auth (Gitea), HotM, fortemi-react
- [ ] Phase 7: Commit all docs (no push without authorization)
- [ ] Phase 8: Summary report to user

## ADR numbering plan
- ADR-088: Plugin Architecture Strategy
- ADR-089: Authorization Policy Trait
- ADR-090: Multi-Tenancy Model — Schema-per-Tenant + Type-Enforced Scope
- ADR-091: Audit Sink Trait
- ADR-092: Usage Metering & Quota Trait
- ADR-093: Key Provider Trait (BYOK/HSM/KMS)
- ADR-094: Fail-Closed Authentication Default
- ADR-095: CE/EE Distribution Model
- ADR-096: Private Cargo Registry for EE Crates
- ADR-097: Stateless API Process Verification
- ADR-098: Per-Tenant Rate Limits & Quotas
- ADR-099: Data Subject Request Handler Trait (GDPR/CCPA)
- (ADR-100 reserved for MCP Tool Authorization Gate)

## State references
- Working dir: `/home/roctinam/dev/fortemi/fortemi` (this is the core repo, not the parent dir)
- Sibling repos: `../fortemi-auth` (Gitea, MIT), `../fortemi-react` (GitHub, AGPL-3.0), `../HotM` (GitHub, BSL-1.1)
- Existing ADR template: `docs/architecture/adr/ADR-TEMPLATE.md`
- Existing ADR count: 60 (ADR-001..ADR-087 with gaps)
- License: BSL-1.1 (matric-memory Change License at `LICENSE.txt`)
- GitHub remote: `github.com/Fortemi/fortemi`
- Delivery policy: not declared in fortemi's own `.aiwg/aiwg.config` (no `delivery` block) → assume `pr-required` per `delivery-policy` rule default

## Next action
Phase 1: dispatch 3 parallel agents — Architecture Designer (audit report), Security Architect (threat model), Architecture Documenter (consolidate findings catalog into reviewable form).

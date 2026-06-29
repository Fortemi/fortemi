# CE → Enterprise Disambiguation & Migration Plan

> **Status:** Plan (execution gated on the EE org/repos existing — `roctinam/devops` #33–#43).
> **Created:** 2026-06-29
> **Driver:** Operator decision to disambiguate enterprise security & tooling out of the CE repo (`Fortemi/fortemi`).
> **Authoritative inputs:** ADR-095 (CE/EE distribution), ADR-096 (private registry), `.aiwg/architecture/plugin-contract-spec.md`, `.aiwg/planning/enterprise-tooling-report.md`, `.aiwg/planning/roadmap.md`.

## 1. Core principle — what "migrate" actually means

The CE/EE seam is a **plugin contract** (ADR-088): the CE core (`Fortemi/fortemi`, BSL-1.1) defines the trait seams and ships permissive/no-op defaults; EE crates in the private `Fortemi-Enterprise/*` org implement the commercial behavior and are composed into a separate `fortemi-enterprise` binary.

Therefore this is **not a code-extraction**. Almost no code leaves CE:

- **Stays in CE (the seam + defaults + public contracts):** the trait definitions (`AuthorizationPolicy`, `AuditSink`, `UsageMeter`/`QuotaPolicy`, `KeyProvider`, MCP scope gate, `OAuthProvider`), the CE defaults (`AllowAllPolicy`, `TracingSink`, `NoOpMeter`, `EnvKeyProvider`, the core `AuditEvent`/`AuditBuffer`), and the **contract docs** (ADRs, `cryptographic-decisions.md`, `plugin-contract-spec.md`). These are the public interface EE builds on; removing them would break the contract.
- **Net-new in EE (additive, separate repos):** the commercial implementations (Casbin/OPA RBAC, Splunk/Elastic/S3-WORM audit sinks, Stripe/OpenMeter billing, AWS/Vault/GCP/YubiHSM KMS, SAML/Okta/AzureAD IdPs, the MCP gate enforcement, vector/job backends) and the `fortemi-enterprise` distribution crate.

So migration is primarily an **issue re-homing + doc-classification** exercise, plus standing up the EE org/repos. **Nothing is deleted from CE** by this plan.

## 2. Target repo map (ADR-095)

| Capability (plugin seam) | CE keeps | EE repo (private) | devops req |
|---|---|---|---|
| Authorization (`AuthorizationPolicy`) | trait + `AllowAllPolicy` + route inventory (#710) | `Fortemi-Enterprise/rbac` (rbac/-casbin/-opa) | #36 |
| Audit (`AuditSink`) | trait + `AuditEvent`/`TracingSink`/`AuditBuffer` + producers (#910 core, #711) | `Fortemi-Enterprise/audit-sinks` (splunk/elastic/s3worm/datadog) | #37 |
| Metering/quota (`UsageMeter`/`QuotaPolicy`) | traits + `NoOpMeter`/`UnlimitedQuota` (#713) | `Fortemi-Enterprise/billing` (stripe/openmeter/warehouse) | #38 |
| KMS (`KeyProvider`) | trait + `EnvKeyProvider` + contract (#897, `cryptographic-decisions.md`) | `Fortemi-Enterprise/kms` (aws/gcp/vault/yubihsm) | #41 |
| MCP scope gate | seam + ADR-100 (#893) | `Fortemi-Enterprise/mcp-gate` | #42 |
| External auth (`OAuthProvider`) | trait + Clerk default (`fortemi-auth`, MIT) | `Fortemi-Enterprise/auth-providers` (saml/okta/azuread) | #35 |
| Search backends (`SearchProvider`) | trait + hybrid default | `Fortemi-Enterprise/search-backends` (pinecone/weaviate/qdrant) | #39 |
| Job backends (`JobRepository`) | trait + `PgJobRepository` | `Fortemi-Enterprise/job-backends` (sqs/pubsub/temporal) | #40 |
| EE distribution | — | `Fortemi-Enterprise/distribution` (`fortemi-enterprise`) | #43 |
| Private Cargo registry | — | infra | #34 |
| EE org | — | `Fortemi-Enterprise` org | #33 |

CE-adjacent existing repos (not EE): `HotM` (desktop), `fortemi-react` (UI), `fortemi-auth` (auth core, MIT target), `licensing` (private).

## 3. Issue re-homing (candidate clusters → target)

Final per-issue classification is the operator's call; these are recommended clusters. **Hosted-auth milestone (#62)** is the main source.

| Cluster | Issues (anchors) | Recommended home | Notes |
|---|---|---|---|
| MCP authorization gate | **#718** | `mcp-gate` (#42) | CE keeps ADR-100 (#893, done) + `AuthorizationPolicy` seam (#710). |
| Audit sinks / hosted enforcement | **#910** (EE part), #711 (umbrella) | `audit-sinks` (#37) | CE keeps core audit (#910 core landed) + taxonomy (#711). |
| KMS / KeyProvider impl | **#734**, #730, #731, #911/#912 (gcp/vault) | `kms` (#41) | CE keeps contract (#897, done) + `EnvKeyProvider`. |
| RBAC / object-level authz | object authz #956–#963, admin-gating #945–#964, #710 EE impls | `rbac` (#36) | CE keeps `AuthorizationPolicy` trait + route inventory; EE holds Casbin/OPA + hosted enforcement. ⚠ many of these are hosted-enforcement = EE; confirm per-issue. |
| Metering / billing / quotas | #713, #714, #877–#880 | `billing` (#38) | CE keeps `UsageMeter`/`QuotaPolicy` traits + no-op defaults. |
| Advanced/enterprise OAuth & IdPs | enterprise IdP connectors; advanced-OAuth hosted chain (#943/#972/#944/#941/#917/#924/#1005/#1003) | `auth-providers` (#35) and/or `fortemi-auth` | ⚠ split: OAuth *protocol* hardening may stay in CE `fortemi-auth`; *enterprise IdP* connectors (SAML/Okta/AzureAD) are EE. Needs operator triage. |
| Tenant isolation / RLS | #726/#728/#729/#733 | **stays CE** (hosted mode of core) | Multi-tenant RLS is core hosted behavior, not a plugin; keep in `Fortemi/fortemi` under `tier/licensed-server`. |
| Search / job EE backends | (future) | `search-backends` (#39) / `job-backends` (#40) | Net-new EE; no current CE issues to move. |

**Decision needed (operator):** several #62 issues are *hosted-mode behavior of the CE core* (RLS, some admin-gating) vs *EE plugin implementations*. Recommend a per-issue triage pass labeling each `tier/licensed-server` + `home:<repo>` before any move.

## 4. Docs & data classification

| Artifact | Disposition |
|---|---|
| ADRs (088/089/090/091/092/093/095/096/098/099/100) | **Stay in CE.** They are the public seam/contract; EE references them. |
| `cryptographic-decisions.md`, `plugin-contract-spec.md` | **Stay in CE** (the contract EE implements). |
| `.aiwg/planning/enterprise-tooling-report.md`, this plan | **Stay in CE planning** (cross-repo planning record); optionally mirror into EE org wiki when it exists. |
| EE-specific *implementation* docs (per-backend setup, runbooks, Helm/values) | **Created in EE repos** as they're built — not retro-moved from CE (they don't exist in CE). |
| Operator/network KMS reality (OpenBao/Vault + tpm2/YubiHSM) | Lives in `roctinam/itops` (already). EE `kms` crate docs reference it. |
| Secrets/keys/data | **None move.** No secret material in CE; EE deployments hold their own per ADR-093/#897. |

## 5. Migration mechanics (Gitea)

Gitea has no atomic cross-repo issue move. Per relocated issue:

1. **Recreate** the issue in the target EE repo with original title/body + a header line `Migrated from Fortemi/fortemi#N`.
2. **Preserve thread** by appending a condensed history (or a link back) — full comment export only where the discussion is load-bearing.
3. **Cross-link**: comment on the source issue `Moved to <owner>/<repo>#M` and close the source as `not planned (relocated)` — only after the target exists and is linked.
4. **Labels/milestone**: apply the EE repo's milestone; drop `Fortemi/fortemi` milestone #62 membership on close.
5. **Update references**: fix roadmap anchors and any ADR "implementation tracker" lines to the new EE issue numbers.

Do **not** close source issues until the target repo exists and the migrated issue is linked (avoid limbo). Until then, source issues carry the §"Disposition" comments already posted (#718, #910).

## 6. Sequencing (dependency-ordered)

1. **Infra first (blocking):** devops#33 (EE org) → devops#34 (private Cargo registry, ADR-096) → devops#35–#43 (the nine EE repos). No migration is possible before this.
2. **Legal:** revise the BSL Additional Use Grant (ADR-095) before EE launch — gate, not blocker for repo creation.
3. **Per-capability triage:** operator labels #62 issues `tier/licensed-server` + `home:<repo>` (resolves the §3/§7 ⚠ items).
4. **Migrate by cluster** in dependency order: kms (#897 contract done) → audit-sinks → rbac → mcp-gate → billing → auth-providers → search/job backends → distribution.
5. **Update CE references** (roadmap, ADR trackers) after each cluster moves.
6. **Stand up `fortemi-enterprise` distribution** crate last; verify EE binary version-stamping (ADR-095).

## 7. Open decisions for the operator

1. **Per-issue CE-vs-EE line** for the hosted-mode issues (RLS/admin-gating/advanced-OAuth) — which are core-hosted (stay) vs plugin-impl (move)? Recommend a triage pass.
2. **Milestone #62** — keep as the CE "hosted launch gate" for the core-hosted residue, and create EE-repo milestones for moved work?
3. **ADR location** — confirm ADRs stay in CE (recommended) vs mirrored to EE.
4. **Who executes the migration** — this plan, or a dedicated `address-issues`/migration pass once the EE org exists.
5. **`fortemi-auth`** — confirm its move to MIT/public (ADR-095 target) vs current private state, since it's the seam the EE `auth-providers` build on.

## 8. Guardrails

- Nothing is deleted from CE by migration; CE retains every seam + default + contract doc.
- No source issue closes until its target exists and is cross-linked.
- BSL grant revision precedes EE distribution.
- Keep cross-repo references fully qualified (`<owner>/<repo>#N`) per the ops-cross-repo rule.

## References
- ADR-095 (`docs/architecture/adr/ADR-095-ce-ee-distribution-model.md`), ADR-096, ADR-088
- `.aiwg/architecture/plugin-contract-spec.md`, `.aiwg/planning/enterprise-tooling-report.md`, `.aiwg/planning/roadmap.md`
- `docs/architecture/cryptographic-decisions.md` (#897), ADR-100 (#893)
- `roctinam/devops` #33–#44; `roctinam/itops` (network KMS reality)

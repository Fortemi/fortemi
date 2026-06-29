# CE → Enterprise Migration — Issue Triage

> **Status:** Triage complete; **no moves executed yet** (operator chose triage-only this pass). This is the approved classification the execution pass will follow.
> **Created:** 2026-06-29
> **Companion:** `.aiwg/planning/ce-ee-migration-plan.md` (mechanics/sequencing), `.aiwg/planning/enterprise-tooling-report.md` (EE surface).
> **Targets verified:** all nine `Fortemi-Enterprise/*` repos exist, are private, have issues enabled, and the delivery actor (`roctibot`) has admin/push — ready when we execute.

## Classification rules (operator decisions, 2026-06-29)

1. **Plugin seams + defaults stay in CE.** `AuthorizationPolicy`/`AuditSink`/`UsageMeter`+`QuotaPolicy`/`KeyProvider`/MCP-gate seam + their CE defaults (`AllowAllPolicy`, `TracingSink`, `NoOp*`, `EnvKeyProvider`) and contract docs (ADRs, `cryptographic-decisions.md`, `plugin-contract-spec.md`) remain in `Fortemi/fortemi`.
2. **Enforcement wiring stays in CE/BSL, gated by tier.** Code that *consumes* the seams — per-route + object-level authorization wiring, quota enforcement, BYO-LLM secret endpoints/proxy — ships in CE (permissive in the open build via the no-op defaults; enforcing when EE plugins are linked). `Fortemi/fortemi` is the base for both the open and licensed-server builds.
3. **Advanced OAuth stays in CE** (`fortemi-auth`/core, OAuth protocol). Only enterprise **IdP connectors** (SAML/Okta/AzureAD) are EE.
4. **Only plugin *implementations* move to EE.** Umbrellas split: trait/wiring stay CE, backend impls move.
5. **Move mechanic** (execution pass): recreate in target with `Migrated from Fortemi/fortemi#N` + cross-link + close source as relocated. Fully-qualified refs.

## A. MOVE to Enterprise (the actionable set)

| Item | Target repo | Notes |
|---|---|---|
| KMS backend impls — EE portion of **#734** (AwsKms/VaultTransit/GcpKms providers), **#911** (Vault Transit), **#912** (GCP KMS) | `Fortemi-Enterprise/kms` | **#734 splits:** trait + `EnvKeyProvider` + wiring stay CE (re-scope #734 to the CE trait or keep as CE umbrella); file/move the *backend* impls to `kms`. Contract is locked (#897, `cryptographic-decisions.md`). |
| EE audit **sink** impls — EE remainder of **#711** (Splunk/Elastic/S3-WORM/Datadog, durability/WORM) | `Fortemi-Enterprise/audit-sinks` | **#711 splits:** trait + `TracingSink` + bounded buffer already landed in CE (#910). EE sinks become `audit-sinks` issues. |
| **#718** (MCP authorization gate impl) — already closed/dispositioned | `Fortemi-Enterprise/mcp-gate` | Re-create on execution. CE keeps ADR-100 seam (#893). |
| **#910** EE remainder (hosted enforcement, audit-health, concrete sinks) — already closed/dispositioned | `Fortemi-Enterprise/audit-sinks` | Re-create on execution. CE keeps audit core. |
| (future) RBAC engines, billing sinks, enterprise IdP connectors, vector/job backends | `rbac` / `billing` / `auth-providers` / `search-backends` / `job-backends` | **No current open CE issues** — these are net-new EE impl issues filed when that work starts. |

## B. STAY in CE (everything else — the bulk of #62)

Grouped; all remain in `Fortemi/fortemi` under `tier/licensed-server` where applicable.

- **Plugin seams + contracts:** #710 (`AuthorizationPolicy` + route inventory), #713 (`UsageMeter`/`QuotaPolicy` traits), #897/`cryptographic-decisions.md` (KeyProvider contract — done), ADR-100/#893 (MCP gate seam — done), `AuditEvent`/`AuditSink`/`TracingSink` core (#910 core — done).
- **Authorization enforcement wiring (consumes the seam — stays CE):** admin-gating #945/#946/#947/#949/#954/#955/#978; object-level policy #956/#957/#958/#959/#960/#961/#962/#963.
- **OAuth protocol (core + advanced):** #924/#941/#942/#943/#944/#972/#917/#1003/#1005.
- **Quota enforcement wiring:** #714 (consumes `QuotaPolicy`).
- **BYO-LLM secret endpoints/proxy (consume `KeyProvider`):** #730, #731.
- **Tenant isolation / RLS:** #726/#728/#729/#733.
- **Core hosted hardening:** attachments/upload #922/#950/#970/#994; jobs #971; backup #923/#927/#991/#978; embeddings/search #975/#979/#995; realtime/ingest #951/#953/#981/#996; inbound connectors #988; PKE #948; webhooks #925/#949/#950; observability/docs #968/#974/#964/#965/#966/#969/#1000/#1002/#940/#996.

## C. Flagged for the operator (handled, noted for the record)

- **#734 / #711 split:** confirmed — split rather than move whole (trait/wiring stay CE; impls move). On execution, re-scope #734 to the CE trait/`EnvKeyProvider` and file an EE `kms` issue for the backends; same for #711 → `audit-sinks`.
- **#730 / #731:** confirmed STAY CE (seam-consumers), overriding the coarser plan §3 KMS cluster.
- **Per-issue CE-hosted vs EE line:** resolved by rule 2 (wiring stays CE). No further per-issue triage needed for #62.

## D. Execution pass (deferred — needs operator go)

When you're ready to execute moves (targets are ready now):

1. KMS: re-scope #734 to CE trait; create `Fortemi-Enterprise/kms` issues for AWS/Vault/GCP backends (link #897 contract); relocate #911/#912; cross-link + close the EE-impl sources.
2. Audit: create `Fortemi-Enterprise/audit-sinks` issues for the EE sinks + hosted enforcement; re-create the closed #910/#718 dispositions in `audit-sinks`/`mcp-gate`.
3. Update CE references (roadmap anchors, ADR "implementation tracker" lines) to the new EE issue numbers.
4. Leave all STAY-CE issues untouched.

No tracker mutations were made in this triage pass beyond the prior #718/#910 closes.

## References
- `.aiwg/planning/ce-ee-migration-plan.md`, `.aiwg/planning/enterprise-tooling-report.md`, `.aiwg/planning/roadmap.md`
- ADR-093/#897 (`docs/architecture/cryptographic-decisions.md`), ADR-095, ADR-100/#893, plugin-contract-spec

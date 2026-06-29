# Fortemi Enterprise Tooling & Services — Planning Report

> **Status:** Reference synthesis (not a plan of record). Derived from the delivery roadmap, ADRs, the plugin contract spec, and the Gitea/devops issue trackers.
> **Created:** 2026-06-28
> **Scope:** What enterprise (EE) tooling and services are *planned* — the distribution model, the EE org/repos, the plugin extension surface, the per-capability services, and how they sequence through the roadmap.
> **Authoritative sources:** `.aiwg/planning/roadmap.md`, `docs/architecture/adr/ADR-088/089/090/091/092/093/095/096/099/100`, `.aiwg/architecture/plugin-contract-spec.md`, `.aiwg/architecture/ce-ee-audit-2026-05.md`, Gitea `Fortemi/fortemi` + `roctinam/devops` issues.

## Status legend
`[planned]` design only · `[design-locked]` ADR accepted, contract not yet in code · `[in-flight]` partial implementation in `crates/` · `[blocked]` waiting on a prerequisite · `[shipped]` in a released build

---

## 1. Executive summary

Fortemi's enterprise strategy is a **plugin-seam model**: the open core (BSL-1.1) defines a fixed set of Rust traits, and enterprise capabilities ship as **private commercial crates** that implement those traits, composed into a top-level `fortemi-enterprise` binary. The strategic pivot (#853) separates the **open BSL desktop build** (single-user, local/API-key auth) from the **licensed-server / hosted multi-tenant** product, so the two tiers develop in parallel.

As of this report, **no EE crate or EE trait implementation exists in `crates/` yet** — the work is at design/contract maturity. The EE plugin surface (auth-policy, audit, metering, key-provider, MCP-gate, DSAR) is specified in ADRs and the plugin contract spec, but the traits are **Beta/Experimental** and several are not yet present in code. The hosted product (Phase 2) is the near-term EE workstream and is gated on Phase 0 foundation contracts plus standing up the EE org, private Cargo registry, and nine private repos.

---

## 2. Distribution model (ADR-095, ADR-096)

A **three-tier** model separates open source, commercial extensions, and third-party plugins.

### Tier 1 — Community Edition (CE), open source
| Repo | License | Purpose |
|---|---|---|
| `Fortemi/fortemi` | BSL-1.1 | Rust core (this repo) |
| `Fortemi/fortemi-auth` | MIT (intended) | Auth trait + Clerk provider (permissive so anyone can link) |
| `Fortemi/fortemi-react` | AGPL-3.0 | Web UI |
| `Fortemi/HotM` | BSL-1.1 | Desktop ("Heart of the Machine") |
| `Fortemi/aiwg-fortemi-skills` (proposed → devops#44) | MIT | AI agent guidance & playbooks |

CE binaries (`fortemi`, `hotm`, `fortemi-react`) are built from public source only, with **no private dependencies**.

> **Note / discrepancy to reconcile:** ADR-095 designates `fortemi-auth` as MIT/public; the roadmap's current-state line lists `fortemi-auth` and `licensing` as **private** today. Treat MIT-public as the target end-state for `fortemi-auth`.

### Tier 2 — Enterprise Edition (EE), private commercial crates
Delivered via a **private Cargo registry** (ADR-096) and Helm charts / Docker images on a private container registry. The top-level `fortemi-enterprise` crate depends on `fortemi` (public) plus selected EE crates via Cargo features, and embeds version info so a deployment audit can distinguish CE from EE.

### Tier 3 — Community Plugins
Anyone may publish a plugin against the Fortemi plugin contract under their own license; unsupported unless certified (`.aiwg/process/plugin-certification.md`).

### Licensing mechanics
- **BSL Additional Use Grant** must be revised before EE launch to (1) let EE customers run CE in production as part of an EE deployment without a separate CE license, and (2) forbid third-party managed-service (competing SaaS that merely runs CE). *(Open legal item.)*
- CE→EE linking relies on the BSL additional-use grant; EE crates do not modify CE source.

---

## 3. Enterprise org, repos & infrastructure prerequisites (`roctinam/devops`)

Phases 2–3 require the EE org, private registry, and nine EE repos. Repo-creation requests were filed 2026-06-24.

| devops issue | Item | Status |
|---|---|---|
| devops#33 | Stand up `Fortemi-Enterprise` private Gitea org (gates all EE repos) | `[planned]` |
| devops#34 | Private Cargo registry (ADR-096); also migrates `fortemi-auth` off the SSH-git dep | `[planned]` |
| devops#35 | EE repo: `auth-providers` | `[planned]` |
| devops#36 | EE repo: `rbac` (→ #956–963) | `[planned]` |
| devops#37 | EE repo: `audit-sinks` (→ #910) | `[planned]` |
| devops#38 | EE repo: `billing` (→ #877+) | `[planned]` |
| devops#39 | EE repo: `search-backends` | `[planned]` |
| devops#40 | EE repo: `job-backends` | `[planned]` |
| devops#41 | EE repo: `kms` (→ #897) | `[planned]` |
| devops#42 | EE repo: `mcp-gate` | `[planned]` |
| devops#43 | EE repo: `distribution` (composes the EE binary) | `[planned]` |
| devops#44 | `Fortemi/aiwg-fortemi-skills` public/MIT (CE Tier-1) | `[planned]` |

**Near-term EE repos gating Phase 2:** `kms` (devops#41 → #897), `mcp-gate` (devops#42), `rbac` (devops#36 → #956–963), `audit-sinks` (devops#37 → #910), `billing` (devops#38 → #877+), `distribution` (devops#43).

**Existing CE repos:** `Fortemi/fortemi`, `HotM`, `fortemi-react`, `fortemi-auth` (private), `licensing` (private).

---

## 4. The EE plugin extension surface (ADR-088, plugin-contract-spec)

The plugin contract defines **23 trait seams**. Traits #1–17 are **Stable** core repositories/backends that already have CE implementations and *can* take EE implementations (e.g., sharded/cloud vector DBs, cloud job queues). Traits **#18–23 are the EE-defining seams** — Beta/Experimental, with CE no-op/permissive defaults and EE commercial implementations.

### EE-defining trait seams
| # | Trait | Crate | CE default | Example EE impls | Stability → target |
|---|---|---|---|---|---|
| 18 | `AuthorizationPolicy` | `matric-core` | `AllowAllPolicy` | Casbin, OPA, custom RBAC/ABAC | Beta (ADR-089) → Stable 2026.Q3 |
| 19 | `AuditSink` | `matric-core` | `TracingSink` | Splunk, Elastic SIEM, S3-WORM, Datadog | Beta (ADR-091) → Stable 2026.Q3 |
| 20 | `UsageMeter` + `QuotaPolicy` | `matric-core` | `NoOpMeter` | Stripe Metering, OpenMeter, warehouse | Beta (ADR-092) → Stable 2026.Q4 |
| 21 | `KeyProvider` | `matric-core` (re-export of `matric-crypto`) | `EnvKeyProvider` (single-tenant) / `KmsKeyProvider` (hosted) | YubiHSM2, BYOK wrapper | Beta (ADR-093) → Stable 2026.Q4 |
| 22 | `DataSubjectRequestHandler` | `matric-core` | `NotImplemented` | Privacy-automation vendors | Experimental (ADR-099) → Beta 2026.Q4 |
| 23 | MCP scope gate | `matric-api` | Default scope map | Custom scope policies | Beta (ADR-100) → Stable 2026.Q3 |

### Stable seams with EE-impl headroom (selected)
`EmbeddingRepository`/`SearchProvider` → Pinecone/Weaviate/Qdrant; `JobRepository`/`JobNotifier` → SQS/Cloud Tasks/Temporal/Pub-Sub; `OAuthProvider` (`fortemi-auth-core`) → SAML/Okta/Azure AD/Auth0; `ExtractionAdapter` → proprietary CAD/medical extractors.

### Contract guarantees that bound EE plugins
Single construction at startup (no hot-swap); `&AuthContext` on every user-data call (plugins may harden, never downgrade); **tenant scope enforced upstream** via `TenantScopedConn` + RLS (cross-tenant requires `system:tenant_admin` + audited `system.cross_tenant_access`); panic containment + per-plugin circuit breaker; namespaced `tracing` with auto-routing of `audit_relevant` events to the configured `AuditSink`; namespaced config (`config.plugins.<name>`); deployment classes **A** (in-process trait), **B** (gRPC sidecar over UDS/TLS), **C** (external service).

---

## 5. Enterprise services & tooling by capability

| Capability | EE repo / crates | Plugin seam | Owning issues / ADR | Status |
|---|---|---|---|---|
| **Tenant isolation (RLS)** | core (hosted mode) | `TenantScopedConn` + RLS policies | ADR-090; #726/#728/#729/#733 | `[planned]` (Phase 2 floor) |
| **Authorization / RBAC-ABAC** | `Fortemi-Enterprise/rbac` (rbac, -casbin, -opa) | `AuthorizationPolicy` (#18) | ADR-089; #956–963; #710 inventory | `[in-flight]` core contract (#710) partly built; CE `AllowAllPolicy` exists; EE impls `[planned]` |
| **Audit sinks** | `Fortemi-Enterprise/audit-sinks` (splunk, elastic, s3worm, datadog) | `AuditSink` (#19) | ADR-091; #910/#711 | `[in-flight]` CE `TracingSink` + many producers landed; hosted/KMS audit wiring `[blocked]` on #897; EE sinks `[planned]` |
| **Metering & billing** | `Fortemi-Enterprise/billing` (stripe, openmeter, warehouse) | `UsageMeter` + `QuotaPolicy` (#20) | ADR-092; #877+ | `[planned]` (Bridge cost/metering #877–#880 in Phase 3) |
| **KMS / key management** | `Fortemi-Enterprise/kms` (aws, gcp, vault, yubihsm) | `KeyProvider` (#21) | ADR-093; #897 (P0), #734/#730/#731 | `[design-locked]` trait designed; **not in `crates/` yet**; AWS-KMS-first contract (#897) `[blocked]`/open |
| **MCP scope gate** | `Fortemi-Enterprise/mcp-gate` | MCP scope policy (#23) | ADR-100; devops#42 | `[planned]` |
| **External auth providers** | `Fortemi-Enterprise/auth-providers` (saml, okta, azuread) | `OAuthProvider` (#17) | ADR-095; `fortemi-auth` (MIT base) | `[planned]`; CE `ClerkProvider` is the open default |
| **Vector / search backends** | `Fortemi-Enterprise/search-backends` (pinecone, weaviate, qdrant) | `SearchProvider`/`EmbeddingRepository` (#14/#2) | ADR-088 | `[planned]`; CE hybrid search is the default |
| **Job / queue backends** | `Fortemi-Enterprise/job-backends` (sqs, pubsub, temporal) | `JobRepository`/`JobNotifier` (#6/#7) | ADR-088 | `[planned]`; CE `PgJobRepository` is the default |
| **DSAR / privacy automation** | (CE seam + privacy-vendor plugins) | `DataSubjectRequestHandler` (#22) | ADR-099; #900/#969/#892/#961 | `[planned]` (Experimental; Phase 2 retention/DSAR) |
| **EE distribution** | `Fortemi-Enterprise/distribution` (`fortemi-enterprise`) | — (composition crate) | ADR-095; devops#43 | `[planned]` |
| **Licensing** | `Fortemi/licensing` (private) | — | ADR-095 grant revision; #901/#894 notices | `[planned]` |
| **Private Cargo registry** | infra | — | ADR-096; devops#34 | `[planned]` |

### Notes on the highest-leverage services

- **KeyProvider / KMS (#897, ADR-093).** Pluggable trait (`wrap_dek`/`unwrap_dek`/`generate_dek`/`sign`/`verify`/`rotate`/`health_check`) with `EnvKeyProvider` (single-tenant) and `KmsKeyProvider` (AWS/GCP/Vault Transit behind `kms-aws`/`kms-gcp`/`kms-vault` features); YubiHSM2/BYOK as EE crates. **AWS KMS is the locked first launch backend**; GCP/Vault are follow-on. The `WrappedKey`/`EncryptedBlob` + versioned AAD/encryption-context schema and the fail-closed degraded-mode matrix are still being finalized in #897 — the adaptability hinge for adding Vault/GCP without a major break.
- **Audit (#910, ADR-091).** CE `TracingSink` and a broad set of metadata-only `fortemi.audit` producers have landed; the **hosted mandatory-audit consumption + KMS lifecycle audit** remain blocked on #897 and hosted audit-health surfaces that do not yet exist in `crates/`.
- **Authorization (#710/#956–963, ADR-089).** The core `AuthorizationPolicy` contract, route/action inventory, and fail-closed hosted `RoleBasedPolicy` bridge are substantially built; **per-tool MCP authorization wiring is blocked** on #718/#893 tool/action metadata + ADR alignment. EE RBAC/Casbin/OPA impls are not started.
- **Metering/billing (ADR-092).** Surfaces in Phase 3 (Bridge) cost/metering (#877–#880) and per-consumer quota policy; the EE billing crates (Stripe/OpenMeter/warehouse) consume `UsageMeter`+`QuotaPolicy`.

---

## 6. Roadmap sequencing of EE delivery

- **Phase 0 — Foundation contracts** (cross-cutting, gates everything): error contract (#967 ✅), fail-closed startup (#926/#928/#933 ✅), AuthorizationPolicy inventory (#710, `[blocked]`), AuditEvent baseline (#910, `[blocked]`), secret inventory + telemetry redaction (#968/#974, in-flight).
- **Phase 2 — Licensed-server / hosted multi-tenant GA** (milestone #62; the core EE product): tenant RLS (#726/#728/#729/#733), object-level authz (#956–963), control-plane admin-gating (#945–964), advanced OAuth (#1003→#943→#972/#944→#941→#917→#924→#1005), KMS (#897 + #910; Vault/GCP #911/#912 deferred), PKE (#947/#948), privacy/DSAR (#900/#969/#892/#961), inbound connectors, realtime/Twilio. **Parallel to Phase 1 (open desktop).**
- **Phase 3 — Universal Model Gateway / Bridge** (milestones #60/#61): protocol-adapter framework (#873) → chat/models/route-plan; **metering/cost #877–#880**, per-consumer policy #870/#871, session logging #868; provider expansion (Anthropic/Gemini/vLLM/LiteLLM/Azure/Bedrock). *Benefits from Phase 2 auth/metering.*
- **Phase 4 — Referenced (BYO) storage** (milestones #58/#59): large workstream; design P0s first (#890/#902/#903/#904/#905), core impl, security regression suite (#746/#797), docs/ops.
- **Phase 5 — Streaming realtime + Native distribution** (#63/#64): streaming outbox/backpressure; **native distribution packaging/CI publish + supply-chain (#916/#888/#887/#886) + licensing notices (#901/#894)** — distribution gates final GA packaging and is where the EE `distribution` crate composes the EE binary.

---

## 7. Critical-path blockers for EE

| Blocker | Gates | Note |
|---|---|---|
| **#897** AWS KMS launch contract (P0) | `KeyProvider` impl (#734/#730/#731), hosted `#910` audit | Contract/AAD schema/fail-closed matrix not yet locked; AWS-first |
| **#943** OAuth consent/redirect (P0) | Advanced OAuth chain | Phase 2 |
| **#718 / #893** tool/action metadata + ADR | per-tool MCP authorization in #710 | Phase 0 authz completion |
| **devops#33 / #34** EE org + private Cargo registry | every EE crate, ADR-096 | infra prerequisite |
| **BSL additional-use grant revision** | EE launch (legal) | ADR-095 open legal item |
| **#890/#902/#903/#904/#905** referenced-storage design set + **#797** CI gate | Phase 4 | design-first |

---

## 8. Maturity snapshot

- **Designed & accepted (ADR):** distribution model (095), private registry (096), plugin contract (088), tenant RLS (090), authorization (089), audit (091), metering (092), key-provider (093), DSAR (099), MCP gate (100).
- **In code (`crates/`):** CE defaults for the stable seams (#1–17); partial `AuthorizationPolicy` contract (#710) and `AuditSink`/producers (#910); `matric-crypto` primitives only (no `KeyProvider`/`WrappedKey` yet).
- **Not yet started:** every EE repo/crate; the EE `distribution` binary; private Cargo registry; EE org.
- **Trait stability:** all EE-defining seams (#18–23) are Beta/Experimental, promoting to Stable across 2026.Q3–Q4 — signatures may still change before then.

---

## 9. Sources
- `.aiwg/planning/roadmap.md` (Phases 0–5, EE-repo gating, product-decision index)
- `.aiwg/architecture/plugin-contract-spec.md` (23-trait surface, contract guarantees, deployment classes)
- `docs/architecture/adr/ADR-095-ce-ee-distribution-model.md`, `ADR-093-key-provider-trait.md`, `ADR-091-audit-sink-trait.md`
- ADR-088 (plugin strategy), ADR-089 (authorization), ADR-090 (tenant RLS), ADR-092 (metering), ADR-096 (private registry), ADR-099 (DSAR), ADR-100 (MCP gate)
- Gitea `Fortemi/fortemi` issues #897, #910, #710, #956–963, #877+, #943; `roctinam/devops` #33–#44
- `.aiwg/architecture/ce-ee-audit-2026-05.md`, `.aiwg/security/multi-tenant-threat-model.md`

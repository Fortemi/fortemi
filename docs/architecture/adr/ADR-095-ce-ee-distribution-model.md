# ADR-095: CE / EE Distribution Model

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, legal/licensing review TBD
**Related:** ADR-088 (plugin strategy), ADR-096 (private Cargo registry)
**Related docs:** `.aiwg/architecture/ce-ee-audit-2026-05.md` §5

## July 2026 checkpoint rebaseline

This ADR remains proposed. The `Fortemi-Enterprise` organization and nine private repos are visible, but the repos are README-only at the checkpoint. The CE-in-EE grant and managed-service restriction text are not approved legal language.

- **Decision status:** Proposed; repository topology started, distribution not launch-ready.
- **Implementation phase:** Edition metadata, legal approval, and distribution construction.
- **Phase owner:** `Fortemi/fortemi#715`, with legal posture in `Fortemi/licensing#1` and private distribution in `Fortemi-Enterprise/distribution#1`.
- **Checkpoint decision date:** 2026-07-14.

## Context

Fortemi core is BSL-1.1 licensed (LICENSE.txt). `fortemi-auth` is MIT (intentionally permissive so anyone can link). `fortemi-react` is AGPL-3.0. HotM is BSL-1.1. The codebase is open and visible, but plugin-based commercial extensions need a clear, declared distribution model so:

- CE users know exactly what they get and under what terms
- EE customers know what enterprise crates exist, how they're delivered, and what their license permits
- Contributors know which repo their change belongs in
- Cloud providers know what BSL "production use" restrictions apply to them

The plugin contract (ADR-088, `.aiwg/architecture/plugin-contract-spec.md`) defines the technical seam. This ADR defines the **organizational and licensing** model around it.

## Decision

**Adopt a three-tier distribution model: CE (open, BSL-1.1+MIT+AGPL), EE (private commercial crates), and a third "Community Plugins" tier for third-party plugins of any license.**

### Tier 1: Community Edition (CE)

Public repositories with open-source licenses:

| Repo | License | Purpose |
|---|---|---|
| `Fortemi/fortemi` | BSL-1.1 | Rust core (this repo) |
| `Fortemi/fortemi-auth` | MIT | Auth trait + Clerk provider |
| `Fortemi/fortemi-react` | AGPL-3.0 | Browser-only client |
| `Fortemi/HotM` | BSL-1.1 | Desktop SPA |
| `Fortemi/aiwg-fortemi-skills` (proposed) | MIT | AI agent guidance & playbooks |

CE binaries: `fortemi`, `hotm`, `fortemi-react` distributions. Built from public source only. No private dependencies.

### Tier 2: Enterprise Edition (EE)

Private repositories under a commercial license. Distributed via:
- Private Cargo registry (ADR-096)
- Pre-built signed binaries on a customer portal
- Helm charts and Docker images on a private container registry

| Repo (private) | Crates | Plugin surfaces |
|---|---|---|
| `Fortemi-Enterprise/auth-providers` | fortemi-enterprise-auth-saml, -okta, -azuread | OAuthProvider |
| `Fortemi-Enterprise/rbac` | fortemi-enterprise-rbac, -rbac-casbin, -rbac-opa | AuthorizationPolicy |
| `Fortemi-Enterprise/audit-sinks` | fortemi-enterprise-audit-splunk, -elastic, -s3worm, -datadog | AuditSink |
| `Fortemi-Enterprise/billing` | fortemi-enterprise-billing-stripe, -openmeter, -warehouse | UsageMeter + QuotaPolicy |
| `Fortemi-Enterprise/search-backends` | fortemi-enterprise-search-pinecone, -weaviate, -qdrant | SearchProvider |
| `Fortemi-Enterprise/job-backends` | fortemi-enterprise-jobs-sqs, -pubsub, -temporal | JobRepository |
| `Fortemi-Enterprise/kms` | fortemi-enterprise-kms-aws, -gcp, -vault, -yubihsm | KeyProvider |
| `Fortemi-Enterprise/mcp-gate` | fortemi-enterprise-mcp-gate | MCP scope policy (ADR-100) |
| `Fortemi-Enterprise/distribution` | fortemi-enterprise (top-level) | Composes the EE binary |

The top-level `fortemi-enterprise` distribution crate depends on `fortemi` (public) plus selected EE crates via Cargo features. The EE binary embeds version info that identifies it as EE so a deployment audit can distinguish CE from EE.

### Tier 3: Community Plugins

Anyone may publish a plugin that targets the Fortemi plugin contract. Such plugins live in their authors' own repos with their authors' choice of license. They are not endorsed or supported by Fortemi unless certified (`.aiwg/process/plugin-certification.md`).

### License compatibility

| Direction | Notes |
|---|---|
| CE → CE | BSL/MIT/AGPL OK at their respective surfaces |
| CE → EE | EE can depend on CE BSL with the BSL "additional use grant" (next bullet) |
| EE → CE | Not applicable — EE crates do not modify CE source |
| Community plugin → CE | Plugin authors comply with BSL terms when they distribute compiled artifacts |

### BSL Additional Use Grant (required revision)

The current BSL grant terms have not been verified by this work. Before EE launch, the BSL "Additional Use Grant" MUST be updated to explicitly:
1. Permit Fortemi EE customers to run Fortemi CE in production as part of an EE deployment without needing a separate commercial license for the CE component
2. Forbid managed-service offerings by third parties (i.e., a competing SaaS that simply runs CE)
3. Specify the Change Date and Change License (typically Apache 2.0 after 4 years)

This is a separate workstream tracked by a dedicated issue.

### Binary identification

Both CE and EE builds embed:
- `package.version` (semver / CalVer)
- `package.edition` = `"community"` or `"enterprise"`
- `package.build_features` = list of compiled feature flags
- `package.git_sha`
- `package.build_timestamp`

Exposed via `/healthz` and `/api/v1/system/info` (the latter requires auth).

### Versioning

CE and EE share the same version number (CalVer). EE releases follow CE releases by at most 1 minor — never lead. EE binaries pinned to `matric-core = "=2026.X.Y"` (exact, not range) to avoid drift.

## Consequences

### Positive
- (+) Clear lines for contributors: "is this enterprise-only or community?"
- (+) Open-source community gets the full source-available core
- (+) Enterprise customers get supported, certified, vendor-integration plugins
- (+) BSL grant clearly delineates managed-service competitors from legitimate customers
- (+) Community plugin tier permits ecosystem growth without Fortemi having to support every plugin

### Negative
- (-) Operational complexity: three repos to release in sync, plus EE registry
- (-) EE customers must consume EE binaries, not vanilla CE; some friction during evaluation
- (-) Community plugin tier introduces security surface; mitigated by certification process (forthcoming)
- (-) BSL grant revision requires legal review and re-publication

### Neutral
- (~) AGPL-3.0 on fortemi-react keeps browser-side honest (forks must publish source) and doesn't infect server-side CE
- (~) MIT on fortemi-auth keeps the trait surface portable so any provider impl can be open-source

## Implementation

**Key changes (not all code):**
1. Stand up `Fortemi-Enterprise` GitHub/Gitea org for private EE repos
2. Stand up private Cargo registry (ADR-096)
3. Update `Cargo.toml` of `matric-core` to expose `EDITION` and `BUILD_FEATURES` constants
4. Update `/healthz` to include edition info
5. Draft revised BSL Additional Use Grant; submit for legal review
6. Update CONTRIBUTING.md to direct EE-feature contributions to appropriate private repo
7. Update README to describe CE/EE/Community-plugins distribution

## References

- ADR-088 — Plugin architecture strategy
- ADR-096 — Private Cargo registry
- `.aiwg/architecture/ce-ee-audit-2026-05.md` §5
- MariaDB BSL 1.1 reference text — `https://mariadb.com/bsl11/`
- BSL examples in industry: CockroachDB, MariaDB MaxScale, Sentry

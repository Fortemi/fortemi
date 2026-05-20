# CE / EE Contribution Process

**Status:** Draft
**Last updated:** 2026-05-20
**Owner:** roctinam
**Related:** ADR-095 (CE/EE distribution model), CONTRIBUTING.md

## Overview

Fortemi is distributed in two tiers:

- **Community Edition (CE)** — public, source-available under BSL-1.1 (or MIT/AGPL per repo), maintained on github.com/Fortemi
- **Enterprise Edition (EE)** — private, commercial license, maintained on the Fortemi-Enterprise org

This document explains where contributions belong and how to navigate the boundary.

## Decision tree: "Where does my change belong?"

### → Bug fix in existing CE code

Open a PR against the public repository (`github.com/Fortemi/fortemi`, `fortemi-react`, `HotM`, `fortemi-auth`). Follow the standard CONTRIBUTING.md.

### → New feature that benefits all users

CE. Open an issue first to confirm scope.

Examples:
- Improved search ranking algorithm
- Performance optimization
- New file-format extractor (added to existing `ExtractionAdapter`)
- Additional inference backend (e.g., new local model provider)
- UX improvements in HotM

### → New trait surface in `matric-core`

CE. Always lands in `matric-core` with a CE default implementation. Requires an ADR.

The default impl should be a "safe NoOp" or the simplest behavior that preserves existing UX. The EE-grade implementation lives in a private EE crate.

### → Integration with a commercial / enterprise-only platform

EE. Lives in the Fortemi-Enterprise org.

Examples:
- SAML / Okta / Azure AD identity provider
- Splunk / Datadog / Elastic SIEM audit sink
- AWS KMS / GCP KMS / Vault Transit key provider
- Stripe Metering / OpenMeter billing integration
- Pinecone / Weaviate vector backend
- Snowflake / BigQuery analytics warehouse

A user-facing CE feature MAY ship at the same time (e.g., the public trait + a basic CE impl) so the EE feature is just one of N implementations.

### → Integration with an open-source / freely-available tool

CE if the integration is non-commercial. The author chooses license.

Examples:
- LocalAI or Llama.cpp inference backend (CE)
- MinIO object store provider (CE)
- Kafka job notifier (CE — open-source)

### → Customer-specific (one customer's bespoke integration)

EE under a private feature flag, or a separate customer-specific crate that the customer maintains in their own private repo consuming the public plugin contract.

### → Documentation

CE for docs that describe the open product. EE for docs about commercial features.

### → Security fix

Follow `SECURITY.md`. Coordinated disclosure; landed in CE if the bug is in CE source, EE if it's in EE source. Both may land simultaneously if the issue spans both.

## Workflow

### CE contribution

1. Fork `github.com/Fortemi/fortemi` (or the appropriate public repo)
2. Branch from `main`
3. Implement; include tests; update CHANGELOG
4. Open PR; CI runs (build, test, lint, security audit)
5. Code review (one maintainer + one stakeholder for cross-cutting changes)
6. Merge; release happens on the next scheduled CalVer cut

### EE contribution

1. Coordinate with the Fortemi-Enterprise maintainer (typically via internal issue)
2. Branch in the relevant private repo
3. Implement; include integration test against the plugin contract
4. Open PR; CI runs + publishes to private registry on tag
5. Code review (one EE maintainer + one product stakeholder)
6. Merge; release coordinated with CE version (EE follows CE, never leads)

### Cross-cutting (CE trait + EE plugin)

When introducing a new plug-point:

1. Write the ADR in CE repo (`docs/architecture/adr/ADR-NNN-*.md`)
2. Land the trait + default impl in CE in PR `core/trait-NNN`
3. Land the EE plugin in the Fortemi-Enterprise org in PR `plugin-NNN`
4. CE PR may be merged first (with NoOp/default behavior). EE PR follows once registry is ready.

## What stays in CE vs moves to EE

A useful heuristic: **CE is everything required for a single-tenant, self-hosted, source-available product. EE is everything required for a commercial multi-tenant SaaS, vendor integrations, or hardware-backed security.**

| Concern | CE | EE |
|---|---|---|
| Core search / embedding / inference | ✓ | — |
| OAuth + JWT auth | ✓ | — |
| Schema-per-archive isolation | ✓ | — |
| Schema-per-tenant + TenantScopedDb | trait only | impls + admin |
| AuthorizationPolicy | AllowAll default | Casbin, OPA, RBAC engines |
| AuditSink | TracingSink default | SIEM, S3-WORM, Splunk |
| KeyProvider | EnvKeyProvider default | KMS, Vault, HSM |
| UsageMeter | NoOp default | Stripe, OpenMeter, warehouse |
| SAML / Okta / Azure AD | — | ✓ |
| Multi-region replication | — | ✓ |
| DSAR workflow | NotImplemented default | full impl + vendor integrations |
| Vendor-specific search backends | — | ✓ |
| Premium support / SLAs | N/A | ✓ |

## Licensing notes

- **BSL-1.1 (CE core, HotM)**: source-available; "additional use grant" permits non-production AND specific production uses; converts to a permissive license (typically Apache 2.0) after a stated change date. See `LICENSE.txt` for canonical terms and the planned grant revision tracked in a separate issue.
- **MIT (fortemi-auth)**: deliberately permissive so any provider impl (open or closed) can link
- **AGPL-3.0 (fortemi-react)**: forks that distribute must publish source; does not affect server-side
- **Commercial (EE)**: per-EE-crate license text; canonical text from legal team

## When in doubt

Default to CE if you're unsure. It's easier to extract something into EE later than to repatriate something stuck behind a private repo.

For sensitive decisions (license interpretation, customer-specific work, security disclosures), reach out to the project maintainers before opening a PR.

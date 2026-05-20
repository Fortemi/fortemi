# ADR-096: Private Cargo Registry for EE Crates

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, devops/security review TBD
**Related:** ADR-088 (plugin strategy), ADR-095 (CE/EE distribution)
**Related rules:** `.claude/rules/dependency-source-policy.md`, `.claude/rules/ci-action-pinning.md`

## Context

`fortemi-auth` is currently consumed via SSH git dependency:

```toml
fortemi-auth-axum = { git = "ssh://git@git.integrolabs.net:Fortemi/fortemi-auth.git", branch = "main" }
```

This works for one private dependency, but does not scale to the planned EE crate set (ADR-095 lists 8+ crates). Issues with SSH-git-deps at scale:

- **Deploy-key sprawl**: every CI runner needs SSH access; key rotation is manual
- **Version drift**: branch refs can shift; `Cargo.lock` pinning helps but does not guarantee reproducibility against a moving branch
- **No metadata layer**: Cargo cannot resolve "give me the latest patch of fortemi-enterprise-audit-splunk satisfying ^0.3"
- **Supply chain**: AIWG's own `dependency-source-policy.md` rule forbids exotic dep sources (`git+`, `github:`) — fortemi-auth's current pattern is an allowlist exception that will compound with each new EE crate
- **Cosign/SBOM**: no standard surface for plugin signing or SBOM publication

Industry pattern: private Cargo registry. Options:
- `crates.io` is public-only, not applicable
- **JFrog Artifactory** — paid, well-supported, full feature set
- **Cloudsmith** — paid SaaS, easy onboarding
- **GitHub Packages (with crates registry)** — recently announced but maturity unclear
- **Self-hosted `kellnr`** — open-source Rust registry, lightweight
- **Self-hosted `crates-io-mirror` / `panamax`** — read-through mirror, not full registry
- **`shipyard.rs`** — open-source, hosted option exists

## Decision

**Stand up a private Cargo registry for EE crates. Primary candidate: `kellnr` self-hosted. Secondary candidate: Cloudsmith SaaS for the first year while ops capacity is light. Decision deferred to a follow-up technical evaluation issue.**

This ADR locks in the **requirement** and the **policy**; the specific product selection lands in a follow-up issue.

### Requirements for the chosen registry

1. **Cargo-compatible alternate registry protocol** (sparse index preferred per RFC 2789)
2. **Token-based authentication** for both publish and consume
3. **Per-crate access control** (some EE crates may be Tier-3 access; most are Tier-2)
4. **CI publish hooks** from GitHub/Gitea Actions
5. **SBOM publication** alongside crate (CycloneDX format)
6. **Cosign signing** of published artifacts (planned, not blocking initial launch)
7. **Audit log** of who published / consumed which version
8. **Backup** of the registry contents (the registry is itself a single point of failure)
9. **`cargo audit` compatibility** — vulnerability advisory feed for EE crates if any are discovered post-publish

### Consumer configuration

EE customers configure their build environment with:

```toml
# .cargo/config.toml
[registries]
fortemi-ee = { index = "sparse+https://registry.fortemi.dev/api/v1/crates/" }

[net]
git-fetch-with-cli = true
```

And consume:

```toml
# Cargo.toml
[dependencies]
fortemi-enterprise-audit-splunk = { version = "^0.3", registry = "fortemi-ee" }
```

Authentication via `cargo login --registry fortemi-ee <token>` or `CARGO_REGISTRIES_FORTEMI_EE_TOKEN` environment variable.

### Migration path for `fortemi-auth`

Once the registry is operational:
1. Publish `fortemi-auth-core`, `fortemi-auth-clerk`, `fortemi-auth-axum` to the registry (initial version 0.1.0)
2. Update `fortemi`'s `Cargo.toml` to consume via registry, not git
3. Remove the SSH-git allowlist exception from `dependency-source-policy`
4. Keep the Gitea repo as the development home; CI publishes on tag

### What does NOT go in the private registry

- CE crates remain in source form on github.com/Fortemi (public)
- Internal-only utility crates (e.g., test helpers) stay as path or git deps inside their workspace
- Community Plugins (Tier 3 per ADR-095) publish wherever they choose; Fortemi does not host community plugin binaries

### Pinning discipline

Per `.claude/rules/dependency-source-policy.md`:
- Lockfile pinned by exact version (default Cargo behavior)
- EE customer deployments use `--locked` flag in CI
- Major version bumps gated by re-running the security audit (cargo audit + manual review)

## Consequences

### Positive
- (+) Scales cleanly to 10+ EE crates
- (+) Centralized auth and access control
- (+) Eliminates SSH key sprawl across CI runners
- (+) Enables SBOM and signing workflows
- (+) Removes the `dependency-source-policy` exception for fortemi-auth
- (+) Audit log of who downloaded what

### Negative
- (-) Operational responsibility (or vendor lock-in if SaaS)
- (-) Single point of failure for EE customer builds — mitigated by registry mirror or registry-vendoring (cargo-vendor + bundled artifacts)
- (-) Initial setup work (token issuance, CI configuration, docs)
- (-) If self-hosted: monitoring, backup, upgrade ops added

### Neutral
- (~) Open-source registry options (kellnr) reduce cost; SaaS options (Cloudsmith) reduce ops burden — tradeoff

## Implementation

**Phases:**
1. (separate issue) Evaluation of kellnr vs Cloudsmith vs Artifactory; recommendation in 2 weeks
2. Stand up chosen registry; configure auth + access policies
3. CI publish workflow for `fortemi-auth-*` crates (validates the pipeline)
4. Migrate `fortemi`'s `Cargo.toml` to consume via registry
5. Update `dependency-source-policy` allowlist (remove fortemi-auth exception, add registry policy)
6. Add SBOM publication to publish workflow
7. Document consumer setup in `docs/ee/registry-access.md`

## References

- ADR-088 — Plugin strategy
- ADR-095 — CE/EE distribution
- `.claude/rules/dependency-source-policy.md`
- `.claude/rules/ci-action-pinning.md`
- Cargo book — Alternate Registries
- `kellnr` — `https://kellnr.io`
- Cloudsmith Cargo docs

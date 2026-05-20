# Plugin Certification Process

**Status:** Draft
**Last updated:** 2026-05-20
**Owner:** roctinam
**Related:** ADR-088 (plugin strategy), ADR-095 (CE/EE distribution)

## Purpose

Define how third-party (Community Plugins, Tier 3 per ADR-095) and EE plugins (Tier 2) are evaluated, certified, and signed before being recommended for production use by Fortemi customers.

Certification is NOT mandatory for a plugin to function — the contract surface is open. Certification is a signal of trustworthiness and quality.

## Certification tiers

### Tier 0: Uncertified

Any plugin that implements the contract and compiles. No review by Fortemi.
- Suitable for: experimentation, internal use, open-source community plugins
- Risk: customer-borne; no Fortemi endorsement
- Signing: optional, by author

### Tier 1: Verified

Plugin author has registered with Fortemi, source has been reviewed by Fortemi for contract conformance, and the plugin appears in the public Fortemi plugin directory.
- Suitable for: community plugins customers want to evaluate
- Review: contract conformance, no malicious patterns, no obvious security gaps
- Signing: yes, with Fortemi's "Verified Plugin" key
- Listing: public plugin directory at `plugins.fortemi.dev` (forthcoming)

### Tier 2: Certified

Tier 1 + security audit by Fortemi (or designated reviewer), full test coverage of mandatory contract events, declared semver discipline, and active maintenance commitment.
- Suitable for: production use; default for first-party EE plugins
- Review: full plugin source + security audit + tested integration
- Signing: yes, with Fortemi's "Certified Plugin" key
- Listing: marked "Certified" in plugin directory

### Tier 3: Fortemi First-Party (EE)

Plugin maintained by Fortemi or a Fortemi-Enterprise subsidiary. Highest trust level.
- Suitable for: production EE deployments
- Review: maintained under the same standard as core
- Signing: yes, with Fortemi's first-party signing key
- Listing: "Fortemi" badge in plugin directory

## Certification review process

### Submission

Author submits a certification request via the plugin directory (or, until that exists, by emailing `plugins@fortemi.dev`). Required:

1. Plugin source (GitHub URL, or tarball for closed-source EE)
2. Crate metadata (name, version, license, target trait, stability tier)
3. Documentation describing config + operational considerations
4. Test results (CI badge or test report)
5. Author identity verification (organization, contact, code-signing key fingerprint)
6. Affirmation that the plugin contract is correctly implemented (signed checklist)

### Tier 1 review checklist

- [ ] Plugin source available for review (open-source OR closed-source provided to Fortemi under NDA)
- [ ] Compiles against current `matric-core`
- [ ] Implements declared trait(s); `cargo test` passes
- [ ] `PLUGIN_ABI_VERSION` checked at startup
- [ ] Does not write to filesystem outside declared paths
- [ ] Does not open inbound network ports (for in-process Class A plugins)
- [ ] Does not log secret material (grep for `password`, `token`, `key`, `secret` patterns in log statements)
- [ ] Honors `shutdown` grace
- [ ] CHANGELOG present and follows Keep-a-Changelog
- [ ] License compatible with downstream use (BSL or commercial; not GPLv3-or-later for Class A)

Review SLA: 2 weeks. Outcome: Verified / NeedsRevision / Rejected.

### Tier 2 review checklist (additional)

- [ ] Security audit by Fortemi or designated reviewer (rust-sec + manual review)
- [ ] All mandatory contract audit events emitted (verified via integration test)
- [ ] Idempotency-key handling validated (where applicable)
- [ ] Tenant-scope discipline validated (does not bypass `TenantScopedDb`)
- [ ] Trait semver compatibility test in CI
- [ ] Active maintenance commitment (response SLA documented)
- [ ] Optional: dependency security audit (`cargo audit` + manual review of unusual deps)

Review SLA: 4-6 weeks. Outcome: Certified / NeedsRevision / Rejected.

### Tier 3 (Fortemi first-party)

Reviewed under the same standards as core. Maintained by Fortemi engineering. Not a "submission" process per se — these are first-party crates.

## Signing

### Why signing

Cosign-signed binaries let customers verify they're running the plugin they think they're running. The signature is over the published `.crate` (Cargo registry artifact) and a SBOM.

### Signing keys

- `fortemi-verified-plugin-2026` — Tier 1
- `fortemi-certified-plugin-2026` — Tier 2
- `fortemi-firstparty-2026` — Tier 3

Keys are managed via the same KeyProvider plane that protects production secrets (ADR-093). Rotation: annually.

### Verification

Customers verify with:

```bash
cosign verify \
  --key https://fortemi.dev/keys/fortemi-certified-plugin-2026.pub \
  --certificate-identity-regexp '.*@fortemi.dev$' \
  fortemi-enterprise-audit-splunk-0.3.0.crate
```

## Revocation

Certifications can be revoked if:
- A security vulnerability is discovered and not remediated in a reasonable window
- Author abandons the plugin (no commits for 12+ months with no announced maintenance handover)
- Plugin violates the contract in a way that affects production users

Revocation is announced in:
- The plugin directory (banner on the plugin's page)
- Security advisory feed (`security.fortemi.dev/advisories/...`)
- `cargo audit` will surface the advisory via the AIWG-managed advisory feed (once registered)

## Plugin author responsibilities

- Maintain the plugin against the contract's stability tier
- Respond to security reports within 30 days
- Publish a CHANGELOG entry for every release
- Coordinate with Fortemi for trait-version compatibility updates

## Disputes

Author disputes about certification decisions go to a panel of two Fortemi maintainers + one community representative. Decision in 14 days.

## Open questions

1. Should community contributors get free Tier 1 review, or is there a cost? (Likely free for community; cost recovery for commercial Tier 2.)
2. Multi-year certification or annual re-cert? (Lean toward annual, lightweight.)
3. Reciprocal certification with vendor partners (e.g., a Splunk-issued cert that we recognize)?

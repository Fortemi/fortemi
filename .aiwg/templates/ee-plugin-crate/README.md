# EE Plugin Crate Template

Starter kit for building a Fortemi Enterprise Edition (EE) plugin.

This template targets **Class A** plugins — Rust crates linked into the EE distribution via Cargo feature flag. For Class B (gRPC sidecar) and Class C (external service) plugins, see `.aiwg/architecture/plugin-contract-spec.md` §7.

## Contents

```
ee-plugin-crate/
├── README.md                    ← this file
├── Cargo.toml                   ← template Cargo manifest
├── LICENSE                      ← commercial license stub
├── src/
│   └── lib.rs                   ← skeleton implementation
├── tests/
│   └── integration.rs           ← contract conformance tests
├── .github/workflows/
│   └── publish.yml              ← CI publish to private registry
└── CHANGELOG.md
```

## Quick start

1. Copy this directory into your new EE crate repo (under the `Fortemi-Enterprise` org).
2. Rename the crate and replace placeholders:
   - `{{PLUGIN_NAME}}` → e.g., `audit-splunk`
   - `{{PLUGIN_KIND}}` → e.g., `AuditSink` (the trait it implements)
   - `{{INITIAL_VERSION}}` → `0.1.0`
3. Implement the trait(s) from `matric-core`.
4. Run `cargo test` — the included integration test enforces the plugin contract.
5. Tag a release; CI publishes to the private registry per ADR-096.

## Required contract checklist

Before publishing, your plugin MUST satisfy:

- [ ] Implements `Plugin` trait (`name`, `version`, `health_check`, `shutdown`)
- [ ] Implements the target trait (e.g., `AuditSink`)
- [ ] Asserts `PLUGIN_ABI_VERSION` at construction
- [ ] Reads configuration via `Config::plugin_config(name)` not random env vars
- [ ] Constructed once at startup via `from_env()` or `from_config(&Config)`
- [ ] Emits mandatory audit events for its surface (per ADR-091)
- [ ] No blocking I/O on Tokio runtime threads
- [ ] No secret material in logs or error messages
- [ ] Honors idempotency keys for non-idempotent operations
- [ ] Errors map to `matric_core::Error`
- [ ] Includes integration test against the contract
- [ ] Includes CHANGELOG entry following Keep-a-Changelog format
- [ ] LICENSE file present (commercial license; see legal team for canonical text)
- [ ] CI workflow publishes to `fortemi-ee` registry on tag

## Stability tier declaration

In your plugin's main module:

```rust
/// This plugin targets the AuditSink Beta trait surface (ADR-091).
/// Breaking changes in the trait will be announced one minor version in advance.
pub const TARGETED_STABILITY_TIER: matric_core::StabilityTier = matric_core::StabilityTier::Beta;
```

## Versioning

- Patch: bug fixes that don't change observable behavior
- Minor: new functionality, additive
- Major: breaking changes to your plugin's API (independent of core's PLUGIN_ABI_VERSION)

Pin `matric-core = "^X.Y"` and add `assert_eq!(PLUGIN_ABI_VERSION, X)` at startup.

## Support

For questions about the plugin contract, see:
- `.aiwg/architecture/plugin-contract-spec.md`
- `.aiwg/architecture/ce-ee-audit-2026-05.md`
- The relevant ADR (e.g., ADR-091 for AuditSink)

# ADR-088: Plugin Architecture Strategy

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-001 (superseded by ADR-072 — trait abstraction), ADR-002 (Cargo feature flags), ADR-072 (inference provider registry)
**Related docs:** `.aiwg/architecture/ce-ee-audit-2026-05.md`, `.aiwg/architecture/plugin-contract-spec.md`

## July 2026 checkpoint rebaseline

This ADR remains a target architecture for the CE/EE plugin model. At the July 2026 enterprise/backoffice checkpoint, existing trait and registry patterns were present, but `Plugin` lifecycle, `PLUGIN_ABI_VERSION`, a stable plugin API crate, protobuf wire contracts, EE crate skeletons, and private package distribution proof were not complete.

- **Decision status:** Proposed; accepted for construction planning only.
- **Implementation phase:** Construction not started for the shared plugin lifecycle.
- **Phase owner:** `Fortemi/fortemi#712`, with distribution proof in `Fortemi-Enterprise/distribution#1`.
- **Checkpoint decision date:** 2026-07-14.

## Context

Fortemi today supports backend swapping via 14 `Send + Sync` traits in `crates/matric-core/src/traits.rs` and a provider registry pattern (ADR-072) for inference backends. All implementations are compiled in at build time via Cargo feature flags (ADR-002). There is no runtime plugin loader.

To enable the Community Edition (CE) + Enterprise Edition (EE) plugin model — where Fortemi ships open under BSL-1.1 and an enterprise distribution composes CE + private plugin crates — we need to formalize how plugins are packaged, distributed, and loaded.

Four candidate models were evaluated in the CE/EE audit (`.aiwg/architecture/ce-ee-audit-2026-05.md` §6):

| Option | Mechanism | Pros | Cons |
|---|---|---|---|
| A | Cargo feature flags + private crates | Zero runtime overhead, type-safe, Rust-native, matches current pattern | Build-time wiring; distinct binaries per edition |
| B | Dynamic loading (`libloading` + `abi_stable`) | True runtime plugins | ABI fragility, painful versioning, debugging difficulty in Rust ecosystem |
| C | Out-of-process plugins (gRPC/HTTP sidecars) | Language-agnostic, process isolation, independent scaling | Latency, ops complexity, requires plugin authentication infra |
| D | WASM plugins (`wasmtime`) | Sandboxed, portable | Async story still rough as of 2026; limited host API surface |

## Decision

**Adopt Option A as the primary plugin model. Adopt Option C as a secondary model for high-risk surfaces. Reject Option B. Park Option D for future evaluation.**

### Primary: Class A (Cargo features + private EE crates)

EE plugins are Rust crates that implement core traits and are linked via Cargo feature flags in a top-level `fortemi-enterprise` distribution crate. The enterprise build is a distinct binary from the CE build.

```
fortemi (BSL-1.1, public)         ← Cargo features select compiled impls
  ├── crates/matric-core          ← traits + default impls
  ├── crates/matric-db            ← PostgreSQL impls
  └── ...

fortemi-enterprise (commercial)   ← depends on fortemi + EE crates
  ├── fortemi-enterprise-auth     ← SAML, Okta, Azure AD OAuthProvider impls
  ├── fortemi-enterprise-rbac     ← AuthorizationPolicy impls
  ├── fortemi-enterprise-audit    ← AuditSink → SIEM/Splunk/S3-WORM
  ├── fortemi-enterprise-billing  ← UsageMeter for Stripe metering
  ├── fortemi-enterprise-search   ← SearchProvider for Pinecone/Weaviate
  ├── fortemi-enterprise-jobs     ← JobRepository over SQS/PubSub
  ├── fortemi-enterprise-kms      ← KeyProvider over AWS KMS / Vault / HSM
  └── fortemi-enterprise-mcp-gate ← MCP tool scope policy
```

### Secondary: Class B/C (gRPC sidecars or external services)

For plugin surfaces where in-process Rust is impractical, the core ships a thin trait shim that delegates to a sidecar (Class B) or external service (Class C). Used selectively for:

- **Authorization policy engines** (Casbin/OPA) where customers want language-flexible policy
- **Audit sinks** that need vendor SDKs (Splunk, Datadog) with deep transitive dependencies
- **Custom extractors** with proprietary or large native deps (medical imaging, CAD formats)

Class B/C plugins use mTLS-authenticated gRPC over Unix domain sockets (B) or TCP+TLS (C). Wire contracts are defined in `.aiwg/architecture/plugin-contract-spec.md` §7.

### Rejected: Class B-style dynamic loading (`libloading`)

Rust ABI is not stable across compiler versions; `abi_stable` works but introduces a large maintenance burden, painful debugging, and limited tooling support. The cost is not justified given Option A solves the same problem with stronger compile-time guarantees.

### Parked: Class D (WASM)

`wasmtime` async + WASI improvements continue. Revisit when async stories around WASI Preview 2 stabilize and when there's a concrete plugin surface where WASM's sandboxing materially helps (e.g., customer-supplied content processors).

## Consequences

### Positive
- (+) Reuses existing `#[cfg(feature = "...")]` pattern; no new infrastructure for the common case
- (+) Compile-time type checking — plugin contract violations fail the build
- (+) Zero runtime overhead for Class A plugins
- (+) Out-of-process option (Class B/C) available for the cases where it materially helps
- (+) Distinct CE and EE binaries make license boundaries explicit at the artifact level

### Negative
- (-) EE customers must consume `fortemi-enterprise` binaries, not vanilla `fortemi`
- (-) Class B/C plugins add deployment surface (sidecar processes, mTLS PKI)
- (-) Plugin authors cannot ship binary-only releases; source is required for Class A
- (-) Hot-swap of plugins requires process restart

### Neutral
- (~) Private Cargo registry is required for EE crate distribution (ADR-096)
- (~) Plugin author certification (signing, vendor verification) becomes a process question, not just a technical one (`.aiwg/process/plugin-certification.md`)

## Implementation

**Code location:** `crates/*` (existing), new top-level `fortemi-enterprise/` workspace (separate repo, private)

**Key changes:**
1. Add `PLUGIN_ABI_VERSION: u32` constant to `matric-core` per `.aiwg/architecture/plugin-contract-spec.md` §6
2. Define `trait Plugin { name, version, health_check, shutdown }` in `matric-core` as the common lifecycle interface
3. Update `BackendSelector` (ADR-001/072) pattern to use the new `Plugin` trait
4. Create `fortemi-enterprise` workspace as separate private repository
5. Stand up private Cargo registry per ADR-096
6. Write Class B/C wire contracts as protobuf files in `fortemi/protos/`

**Phasing:**
- Phase 1 (this ADR): Decision and `Plugin` trait
- Phase 2: First EE crate (`fortemi-enterprise-audit-jsonl`) as proof-of-contract
- Phase 3: ADRs 089-100 land each plugin surface
- Phase 4: Private registry + signing process

## References

- `.aiwg/architecture/ce-ee-audit-2026-05.md` — Full audit report
- `.aiwg/architecture/plugin-contract-spec.md` — Plugin contract surface
- ADR-072 — Inference provider registry (current reference pattern)
- ADR-002 — Feature flags for optional backends
- ADR-096 — Private Cargo registry (forthcoming)

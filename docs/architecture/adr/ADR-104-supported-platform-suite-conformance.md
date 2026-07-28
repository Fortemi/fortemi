# ADR-104: Supported-Platform Suite Conformance

**Status:** Accepted
**Date:** 2026-07-28
**Decision owners:** Fortemi contract authority maintainers
**Tracking:** [Fortemi #1095](https://git.integrolabs.net/Fortemi/fortemi/issues/1095),
[fortemi-react #399](https://git.integrolabs.net/Fortemi/fortemi-react/issues/399),
[HotM #284](https://git.integrolabs.net/Fortemi/HotM/issues/284)
**Parent gate:** [Fortemi #1081](https://git.integrolabs.net/Fortemi/fortemi/issues/1081)
**Extends:** ADR-102, ADR-103

## Context

The suite has immutable Linux receipts for individual Knowledge Shard, live
asset, API-consumer, authentication, recovery, and performance cells. It also
builds native macOS artifacts on `mutsu`. Those facts do not prove that the
same authority-to-consumer behavior executes on both platforms. A native build
is not an end-to-end behavioral receipt, and independent green repository
workflows do not prove that their revisions or contract inputs agree.

Contract ownership is also easy to blur when proving a cross-repository
journey. Fortemi owns and enforces the live REST, AsyncAPI, compatibility,
authentication-consumer, and Knowledge Shard contracts. `@fortemi/core`
implements browser-local PGlite and RecordStore conformance against pinned
Fortemi authority artifacts. HotM consumes the delivered server contracts. A
HotM fixture or a React type is not an independent authority.

The suite audit remains `NO-GO` for unqualified parity, complete backup, or
portability. The next useful claim is narrower: all declared contract
behaviors pass on the two platforms that are currently supported and operated.

## Decision

### Required platforms

The executable suite matrix has exactly two required platform cells:

| Platform ID | Operating system | Architecture | Execution authority |
|---|---|---|---|
| `linux-x86_64` | Linux | x86_64 | Current Fortemi Gitea contract runner |
| `macos-arm64` | macOS | arm64 | Native execution on `mutsu` through the established SSH coordinator |

Windows, macOS x86_64, Linux arm64, other operating systems, other
architectures, non-filesystem asset stores, and other filesystems are
deferred. They are neither passing nor failing cells and must not be included
in a supported-platform claim.

### Authority and consumer boundaries

Fortemi is the only authority in this matrix. It owns:

- generated and committed OpenAPI plus runtime equality;
- generated AsyncAPI and the event catalog;
- compatibility discovery and minimum-client policy;
- authentication consumer policy;
- Knowledge Shard schemas, profiles, migrations, fixtures, and matrix rules;
- live PostgreSQL and filesystem behavior; and
- the aggregate platform receipt and release decision.

`fortemi-react` consumes pinned Fortemi authority receipts. Its stable
conformance command covers PGlite, RecordStore, `core-v1`, `record-v1`, and
receipt-backed exact `2.0.0/full-v1` behavior. It reports reduced-profile
losses and must reject authority drift before its behavioral suite runs.

HotM consumes pinned generated API/event artifacts and a pinned native Fortemi
sidecar. Its platform receipt drives authenticated browser and production
Tauri command-core journeys against a real PostgreSQL/filesystem server. HotM
does not publish schema or API policy.

The AIWG static index remains an AIWG-owned contract. The AIWG-to-shard
converter remains an explicit bridge. Neither is a live persistence schema,
and this platform matrix does not merge those planes.

### Required automated surface

Each platform cell must execute the same declared contract surface:

1. Fortemi release build and workspace tests against PostgreSQL with required
   extensions.
2. Generated OpenAPI equality, AsyncAPI construction, compatibility discovery,
   auth-consumer policy, and the registered Knowledge Shard matrix.
3. The pinned `@fortemi/core` portable-contract command, including clean
   destinations, exact profiles, skew and malformed-input rejection, resource
   bounds, and zero mutation.
4. HotM delivered OpenAPI/event/compatibility guards.
5. HotM authenticated browser and production Tauri command-core lifecycle:
   local file upload, real TUS interruption/resume, server download and saved
   file, re-upload, signed `2.0.0/full-v1` export, source retirement,
   required-signature clean recovery, and exact bytes, digests, lengths,
   metadata, relationships, and redaction.

Platform-specific installation and process control may differ. Contract
assertions, participant revisions, authority inputs, and required child gates
must be identical.

### Receipt and release rules

The authority repository publishes a machine-readable manifest that pins:

- exact Fortemi authority/orchestrator, `fortemi-react`, and HotM commits;
- separate Knowledge Shard and server compatibility contract revisions;
- package and sidecar release identities;
- schema/profile and generated-contract digests;
- required platform cells and child gates;
- explicit deferred platforms; and
- prohibited broad claims.

Every platform run emits an immutable receipt with normalized OS,
architecture, filesystem, exact revisions, command identities, child receipt
hashes, and pass/fail results. An aggregate verifier fails closed when:

- either required platform receipt is absent;
- a platform or participant identity is unsupported or drifts;
- required coverage differs between platforms;
- a child verifier fails;
- a checkout used for a clean-revision claim is dirty; or
- any receipt claims universal portability, one universal schema, launched
  GUI/native dialogs, or complete backup without separate evidence.

The runner owns an isolated PostgreSQL container. Authority tests, React/core,
and HotM execute in separate database lifecycles with the same
image-provisioned extension baseline. The authority process owns SQLx
migrations and is stopped while database-backed authority tests run.

Passing both cells authorizes only the phrase:

> The declared Fortemi authority-to-React/core-to-HotM contract surface passes
> on Linux x86_64 and macOS arm64 on mutsu at the receipt-bound revisions.

It does not authorize universal portability, full product-feature parity,
complete backup, or a claim that all suite persistence planes share one
schema. Fortemi #1081 remains open until the independent final audit accepts
the resulting evidence.

## Consequences

- Contract changes cannot merge based on one operating system or a native
  build alone.
- macOS failures become authority/consumer conformance failures rather than
  release-packaging observations.
- HotM can focus on consumer behavior because schema and API policy remain
  upstream.
- React/core drift becomes visible before HotM integration.
- CI runtime and `mutsu` capacity increase because native behavior, not only
  compilation, is required.

## Alternatives Rejected

### Treat every repository's normal CI as the suite proof

Rejected because independent runs can use different revisions and do not bind
one end-to-end contract result.

### Let HotM own the cross-platform contract

Rejected because a consumer must not redefine the server contract it tests.

### Require every operating system before making any claim

Rejected because unsupported platforms would block useful evidence without
improving the accuracy of the two platforms that are actually operated.

### Count macOS artifact construction as behavioral evidence

Rejected because compilation and packaging do not execute authentication,
network, persistence, migration, recovery, or byte-preservation behavior.

## References

- @docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md
- @docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md
- @.aiwg/testing/bidirectional-asset-lifecycle-audit-plan-2026-07.md
- [fortemi-react ADR-010](https://git.integrolabs.net/Fortemi/fortemi-react/src/branch/main/.aiwg/adrs/ADR-010-portable-schema-topology-and-source-of-truth.md)
- [fortemi-react ADR-011](https://git.integrolabs.net/Fortemi/fortemi-react/src/branch/main/.aiwg/adrs/ADR-011-shard-server-conformance-and-version-negotiation.md)

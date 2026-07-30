# ADR-104: Supported-Platform Suite Conformance

**Status:** Accepted
**Date:** 2026-07-28
**Decision owners:** Fortemi contract authority maintainers
**Tracking:** [Fortemi #1095](https://git.integrolabs.net/Fortemi/fortemi/issues/1095),
[fortemi-react #399](https://git.integrolabs.net/Fortemi/fortemi-react/issues/399),
[HotM #284](https://git.integrolabs.net/Fortemi/HotM/issues/284)
**Deferred Windows story:** [Fortemi #1096](https://git.integrolabs.net/Fortemi/fortemi/issues/1096)
**Parent gate:** [Fortemi #1081](https://git.integrolabs.net/Fortemi/fortemi/issues/1081)
**Extends:** ADR-102, ADR-103

## Context

The suite has immutable Linux receipts for individual Knowledge Shard, live
asset, API-consumer, authentication, recovery, and performance cells. It also
builds native macOS artifacts on `mutsu`. Those facts do not prove that the
same authority-to-consumer behavior executes across the supported platforms.
A native build is not an end-to-end behavioral receipt, and independent green
repository workflows do not prove that their revisions or contract inputs
agree.

Contract ownership is also easy to blur when proving a cross-repository
journey. Fortemi owns and enforces the live REST, AsyncAPI, compatibility,
authentication-consumer, and Knowledge Shard contracts. `@fortemi/core`
implements browser-local PGlite and RecordStore conformance against pinned
Fortemi authority artifacts. HotM consumes the delivered server contracts. A
HotM fixture or a React type is not an independent authority.

The suite audit remains `NO-GO` for unqualified parity, complete backup, or
portability. The next useful claim is narrower: all declared contract
behaviors pass on the three platform cells that are currently supported and
operated.

## Decision

### Required platforms

The executable suite matrix has exactly three required platform cells:

| Platform ID | Operating system | Architecture | Execution authority |
|---|---|---|---|
| `linux-x86_64` | Linux | x86_64 | Current Fortemi Gitea contract runner |
| `linux-arm64` | Linux | arm64 | Native Linux arm64 Colima virtualization on `mutsu` through the established SSH coordinator |
| `macos-arm64` | macOS | arm64 | Native execution on `mutsu` through the established SSH coordinator |

Windows is the only deferred operating system. It is neither a passing nor a
failing cell and must not be included in a supported-platform claim.
Architectures outside the exact matrix, non-filesystem asset stores, and other
filesystems are outside this decision and are not separately claimed.
Fortemi #1096 owns native Windows x86_64 runner, sidecar, contract, and receipt
work without weakening or reopening the three required cells above.

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

- any required platform receipt is absent;
- a platform or participant identity is unsupported or drifts;
- required coverage differs between platforms;
- a child verifier fails;
- a checkout used for a clean-revision claim is dirty; or
- any receipt claims universal portability, one universal schema, launched
  GUI/native dialogs, or complete backup without separate evidence.

React/core package parity is evaluated over the exact decompressed npm tar
payload. The Linux cell additionally binds the published `.tgz` digest.
Platform gzip implementations may encode the same tar bytes differently, so a
raw compressed-stream mismatch alone is not package-content drift.

The runner owns an isolated PostgreSQL lifecycle: both Linux cells use the
pinned test database image and the macOS cell uses native Homebrew PostgreSQL
18. Authority tests, React/core, and HotM execute in separate database
lifecycles with the required extension baseline. The authority process owns
SQLx migrations and is stopped while database-backed authority tests run.

Passing all three cells authorizes only the phrase:

> The declared Fortemi authority-to-React/core-to-HotM contract surface passes
> on Linux x86_64, Linux arm64, and macOS arm64 on mutsu at the receipt-bound
> revisions.

It does not authorize universal portability, full product-feature parity,
complete backup, or a claim that all suite persistence planes share one
schema. Fortemi #1081 remains open until the independent final audit accepts
the resulting evidence.

### Historical delivered receipt

[Gitea run 6393](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/6393)
completed successfully at orchestrator commit
`5bfecfe8d55caced3652a225a60f5217b4c192e8`. Its Linux x86_64, Linux arm64,
macOS arm64, and required aggregate jobs all passed. The uploaded aggregate
binds exact `2.0.0/full-v1` and the participant revisions and package/sidecar
digests in that run's authority matrix, with false claims for launched desktop
GUI, interactive native dialogs, and suite-wide portability.

### Current release qualification

The next aggregate binds Fortemi runtime
`5ea08229c9f1565122df5f8e6906e89d98dc7e75` (`v2026.7.19`),
React/core `5cab4ea2d3d4bb985ea0d38f8bcb1ea790b32cf7`
(`@fortemi/core@2026.7.15`), HotM
`cdbf29aa5dbb924be4bcd4dac2494bfe714d50aa` (`2026.7.1`), and immutable
sidecar `sidecar-5ea08229c9f1`. These inputs do not become delivered evidence
until all three required platform jobs and the aggregate pass.

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

Rejected because Windows cannot currently be executed in the suite
infrastructure and would block useful evidence without improving the accuracy
of the three platform cells that are actually operated.

### Count macOS artifact construction as behavioral evidence

Rejected because compilation and packaging do not execute authentication,
network, persistence, migration, recovery, or byte-preservation behavior.

## References

- @docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md
- @docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md
- @.aiwg/testing/bidirectional-asset-lifecycle-audit-plan-2026-07.md
- [fortemi-react ADR-010](https://git.integrolabs.net/Fortemi/fortemi-react/src/branch/main/.aiwg/adrs/ADR-010-portable-schema-topology-and-source-of-truth.md)
- [fortemi-react ADR-011](https://git.integrolabs.net/Fortemi/fortemi-react/src/branch/main/.aiwg/adrs/ADR-011-shard-server-conformance-and-version-negotiation.md)

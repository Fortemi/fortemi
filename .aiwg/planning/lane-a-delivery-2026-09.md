# Lane A delivery ledger — September 2026

Status: partial delivery. Receipts recorded on September 7 UTC, during the September 6 America/New_York session. This ledger supports [the roadmap](roadmap.md) and [release cadence](release-cadence-2026-09.md); it does not close the remaining issue gates.

## Delivered components and exact identities

| Component | Source and immutable artifact | Verified outcome | Remaining gate |
|---|---|---|---|
| React 2026.9.2 | Commit `3133964953d4d9207dbf2e77456f220cead5b5aa`; signed tag object `b1d79fa236bc6d594a9f4c9cf0c3733421416186` | Core/Graph/React package bytes match both registries, release assets, integrity metadata and manifests. Public provenance source/tag/workflow fields match. [#415 closure](https://git.integrolabs.net/Fortemi/fortemi-react/issues/415#issuecomment-125298). | No independent Sigstore cryptographic verification is claimed. Cadence observations remain separate. |
| HotM 2026.9.1 | Commit `ef574ca389cd222ed2b13ec000441469c69c758e`; signed tag object `0ee3169701d99feba501e3ed905bb57c2c84cd1e`; UI digest `sha256:12933557ef6e071ade4bc21c15071367dc48397eb8e356e135cba20d9a3771ac` | All pre-tag and release CI passed. All six release assets match across Gitea/GitHub; three native installers match checksum manifests. Anonymous GHCR and Gitea UI pulls agree. Ten staged and ten installed desktop/mobile browser checks pass. [Installed receipt](https://git.integrolabs.net/Fortemi/HotM/issues/304#issuecomment-125356). | Actual hosted-auth sign-in/reconnection remains open in #304. Browser authorization checks use injected responses. UI dist tarball is not covered by the native checksum manifests. Native GUI launch is not claimed. |
| Server 2026.9.6 | Commit `14f4562e88cc6852e049a91c4e6ae1615f837f67`; signed tag object `d82682edb6512882e19cf2ba7de8e3f532204cf1`; Gitea bundle digest `sha256:f463db4ed2ba641a4fdbb12b0d886b088d1ccf0ea4e1246e8bbf8130b296ed39` | Candidate/main CI and comprehensive run 54569 passed. Published Gitea bundle passed delayed readiness and recovery create/reuse/fresh restore. | Tag pipeline 54582 is still publishing GHCR; public image verification, native assets and finalization remain pending. #1132/#1133 stay open until applicable delivery gates finish. |

## Customer-path qualification and recovery

The published server bundle was qualified on Linux amd64 with a synthetic 75-second API start delay. PID 1 and the API launcher remained stable through 62 sampled seconds; API and MCP became healthy at 82 seconds, with five progress diagnostics and no restart. This does not reproduce the reported 90-minute Windows migration. See [#1132](https://git.integrolabs.net/Fortemi/fortemi/issues/1132#issuecomment-125349).

For recovery, the harness applied 136 of 137 exact image migrations and created synthetic user data. Two delayed starts created then reused one verified artifact with SHA256 `4f36d3a7ed60fe132490cd715b2d7f7feabca3fa0195f7a09bfd4faccdb2122f`; the complete checksum-bearing ledger remained unchanged. The gzip-wrapped custom dump restored into a fresh same-image PostgreSQL destination, where the fixture count was one. This is recovery-path evidence, not an unqualified complete-backup claim. See [#1133](https://git.integrolabs.net/Fortemi/fortemi/issues/1133#issuecomment-125352).

Separately, #1041's realistically seeded February upgrade exercised 102,000 notes and verified backup restoration with recorded counts and invariants. The large run predates a checksum-prevalidation-only refactor, which passed four real PostgreSQL regressions and a smaller end-to-end upgrade. Field-report reconciliation, the observation window and explicit hypercare sign-off remain open; a successful fixture run does not establish their completion.

HotM 9.1 replaced the installed UI on port 4180, preserving its backend network, read-only nginx configuration, memory/restart settings and runtime overrides. Installed container `0bd82736f498640b480214dbcfe8ffd8b2c836c1864fd0d96e5abf8d11c62291` uses the exact UI digest above. Previous 9.0 container `7e2671af592ae130d3dbfedb040a30d8841cc5e94b192dcdf9490fac4189d3cd` is stopped and retained as `hotm-ui-manual-rollback-202690-20260907`; the older rollback is also retained. The backend was not replaced. Update automation verifies expected identities before mutation and restores the old container if replacement startup fails; controlled simulations verified that path.

## Open coordination gates

- **HotM #304:** a configured hosted-auth test environment is required to prove actual sign-in/reconnection. The installed backend is anonymous local. Keep this gate distinct from the passing mocked-response browser checks.
- **HotM #301 / [itops #658](https://git.integrolabs.net/roctinam/itops/issues/658):** exhausted Gitea inventory found no eligible package-version deletion candidates at the recorded snapshot. GHCR tag aliases do not establish deletion-capable version mapping or authority. Exact cleanup scope and explicit approval remain required; no cleanup was executed. Preserve newly published CalVer releases as well as earlier releases and reachable manifests.
- **Server #1041:** reconcile outstanding field failures, complete the observation window and record sign-off. Do not infer these from the published synthetic checks.
- **Server #1143:** named maintainer acceptance, first scheduled cycle receipts and two-cycle measurement remain open. Expedited qualified corrections do not silently change the September 15–18 window or count as two observed cycles.

The suite audit remains **NO-GO** for unqualified full parity, complete backup or portability. Knowledge Shard claims require named `core-v1`, `full-v1` or `record-v1` cells and their executable evidence. AIWG static indexes, the explicit shard bridge and live persistence remain separate.

Detailed receipts are retained in the suite workspace under `.aiwg/working/lane-a-2026-09-06/evidence/`; the linked issue comments carry the durable artifact identities and qualification limits.

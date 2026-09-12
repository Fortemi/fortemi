# Remote Adapter Capture

`remote-adapter.json` contains actual HTTP request/response pairs captured by
`scripts/ci/capture-remote-adapter-fixture.mjs` for producer issue #1146 and
React consumer issues #417 through #421. Runtime identity is the immutable
2026.9.9 bundle, source `e91c595a896275f835cb7ed1aef173cb26056206`.

The capture requires a loopback, explicitly lane-owned disposable container
with zero native note rows, including tombstones. It creates synthetic notes,
an explicit directional link and seeded provenance. Seeding demonstrates
serialization, not actual AI execution. All fixture notes are soft-deleted
in a finally block; removing the owned tmpfs container completes cleanup.

The Rust `remote_adapter_fixture_tests` deserialize the captured list/detail,
links, search and provenance through producer models and reject malformed
required fields. The fixture does not replace the generated OpenAPI authority.

This first capture covers 25 cases. It is not complete acceptance for #1146:
auth denial/rate-limit/internal error fixtures, explicit degraded semantic
search, full advertised mutation coverage, consumer source/release pins and
published-package live execution remain pending. No native shard-restore,
general API parity or suite portability claim follows; suite NO-GO remains.

## Search and Mutation Extension

`remote-operations.json` is a second clean-destination capture from the same
released runtime. Its 37 cases retain the first capture's operations and add
bounded search limits, an AND-tag no-match, semantic/hybrid FTS degradation,
content/tag update, archive/unarchive, delete, absent-after-delete and restore.
The first fixture's bytes remain unchanged. Both notes are soft-deleted again
after restoration. The capture script now emits this expanded case set.

The producer tests validate search degradation and mutation inputs/outputs
through the actual Rust types, plus body-free deletion and restore identity.
This extends source evidence for React #419/#420; it is not successful vector
retrieval, auth denial/rate-limit/internal-error qualification or live published
React acceptance. Those remaining #1146 gates and suite NO-GO still apply.

## Authenticated Native Read Capture

`native-remote-auth.json` is a supplemental, byte-preserving capture produced by
`scripts/ci/capture-native-remote-fixture.mjs`. Its adjacent receipt binds the
capture script, raw response digests, exact published native Linux AMD64 server
2026.9.9 and clean-installed published `@fortemi/core` 2026.9.4. It records
13 checks over86 real HTTP calls. The older25/37-case files are unchanged.

This fixture uses required local authentication in **personal mode**. It covers
empty/nonempty lists, UTC timestamps, tags, directional links, composed revised
content, and SQL-seeded nonempty provenance. The latter proves serialization,
not inference. Missing/invalid identities produce401, missing notes produce404,
a reversible private-database fault produces500, and a bounded serial probe
produces429. Both public `getNote` and `getNoteFull` are exercised live.

The403 response is from `/api/v1/operator/openapi.yaml`, not a note route. Personal
mode selects `AllowAllPolicy` for general note access, while operator inventory
always enforces `RoleBasedPolicy`. Therefore this capture does not prove hosted
denied-note behavior, tenant isolation, or read-only mutation enforcement. The
public binary is built without the internal `hosted-auth` feature and cannot be
turned into that hosted fixture by merely setting `FORTEMI_MULTI_TENANT=true`.
Do not remap the operator response and call the result live note authorization.

Reproduction requires the suite's reviewed Titan local-test runner and ephemeral
PostgreSQL helper, the exact pinned artifacts, and a populated npm cache. Run
from this producer checkout, with absolute paths and a **new** output directory:

```sh
node "$SUITE/.aiwg/testing/scripts/local-test-runner.mjs" start --timeout-seconds 300 -- \
  node "$SUITE/.aiwg/testing/scripts/with-ephemeral-postgres.mjs" -- \
  node scripts/ci/capture-native-remote-fixture.mjs \
  "$RELEASED_API" "$PUBLISHED_CORE_TARBALL" "$NEW_OUTPUT_DIRECTORY" "$NPM_CACHE"
```

The capture refuses to run outside the verified2-CPU/8-GiB/no-swap offline
cgroup with private network, devices and temporary storage. PostgreSQL listens
on a private Unix socket; the API uses private loopback. No Docker daemon,
GPU, host inference endpoint, production identities or persistent database is
used. Temporary identities are never written into the capture. API cleanup is
recorded inside it; publication additionally requires the terminal unit and
postmaster/cgroup cleanup receipt, because the outer database helper exits last.
Historical receipts attest the observed cleanup, not the current host state.

```sh
node scripts/ci/verify-native-remote-fixture.mjs
node --test scripts/ci/verify-native-remote-fixture.test.mjs
cargo test -p matric-api --bin matric-api remote_adapter_fixture_tests
```

The integrity/negative tests run in the server-independent CI gate; Rust tests
compare captures with producer note/link/provenance models and the actual
problem serializer. These replay checks do not launch services. The native
capture remains a Linux AMD64 personal-mode receipt, not a general CI sandbox.

Producer [#1146](https://git.integrolabs.net/Fortemi/fortemi/issues/1146) remains
linked to consumers [#417](https://git.integrolabs.net/Fortemi/fortemi-react/issues/417),
[#418](https://git.integrolabs.net/Fortemi/fortemi-react/issues/418), and
[#421](https://git.integrolabs.net/Fortemi/fortemi-react/issues/421). Consumer pins,
producer-owned malformed-success/transport negative controls, hosted denial and
the remaining advertised-operation acceptance are separate pending gates.
No OpenAPI/schema/profile change, release replacement, or suite parity claim is
authorized by this fixture; suite NO-GO remains.

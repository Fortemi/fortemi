# Fortemi AsyncAPI Contract

`asyncapi.yaml` is the deterministic AsyncAPI 3.0 consumer contract generated
from the same `ServerEvent` metadata and schemas used by the runtime operator
endpoint. `asyncapi.sha256` authenticates its exact bytes.

Regenerate and verify the committed artifact:

```bash
scripts/ci/asyncapi-contract.sh generate
scripts/ci/asyncapi-contract.sh check
scripts/ci/asyncapi-event-fixtures.sh generate
scripts/ci/asyncapi-event-fixtures.sh check
```

Consumers pin the Fortemi commit and checksum. A copied event list, a HotM
fixture, or a successful route connection is not an independent contract
authority.

Positive producer-owned `EventEnvelope` fixtures live under
`fixtures/events/`. `fixtures/manifest.json` binds every fixture to its
dot-namespaced event name, Rust payload variant, `ServerEvent.oneOf[...]`
payload schema, payload revision, envelope contract revision, the committed
AsyncAPI SHA-256, and the aggregate fixture corpus SHA-256. Consumers should
pin the Fortemi commit they integrated with plus
`producer-event-fixture-receipt.json` fields:

- `manifest_sha256`
- `asyncapi_sha256`
- `corpus_sha256`
- `event_count`

The in-repository receipt uses `external-delivery-pin` for commit fields so the
byte drift check remains stable after commit; release and issue handoff notes
bind those receipt digests to the exact delivered Fortemi Git commit.

## Application Release Versions

`info.version` identifies the generating application release, independently of
the envelope and payload revisions. Every package-version bump must refresh the
AsyncAPI artifact, checksum, fixture manifest and producer receipt together, even
when all event payload bytes remain unchanged. The core serialized-contract test
compares this field with the compiled package version before checking exact YAML
bytes, so a stale artifact fails before the runtime container gate.

A metadata-only update does not invalidate an older consumer's immutable pin.
Prove full wire-document equivalence and unchanged payload corpus before recording
a no-change consumer disposition; advancing a consumer pin still requires its
normal exact-source and released-artifact qualification. Neither unit tests nor a
metadata comparison replaces the runtime endpoint gate or broadens suite claims.

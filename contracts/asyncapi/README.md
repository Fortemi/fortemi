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

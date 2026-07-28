# Fortemi AsyncAPI Contract

`asyncapi.yaml` is the deterministic AsyncAPI 3.0 consumer contract generated
from the same `ServerEvent` metadata and schemas used by the runtime operator
endpoint. `asyncapi.sha256` authenticates its exact bytes.

Regenerate and verify the committed artifact:

```bash
scripts/ci/asyncapi-contract.sh generate
scripts/ci/asyncapi-contract.sh check
```

Consumers pin the Fortemi commit and checksum. A copied event list, a HotM
fixture, or a successful route connection is not an independent contract
authority.

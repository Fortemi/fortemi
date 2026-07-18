# Fortemi OpenAPI contract

`openapi.yaml` is the deterministic OpenAPI 3.1 consumer contract generated
from the same `ApiDoc` registration used by Fortemi's runtime
`/openapi.yaml` endpoint. `openapi.sha256` authenticates its exact bytes.

Regenerate and verify the committed artifact:

```sh
scripts/ci/openapi-contract.sh generate
scripts/ci/openapi-contract.sh check
```

Consumers pin the producer Git commit and fetch
`contracts/openapi/openapi.yaml` from that immutable revision. CI publishes
the same file with `openapi-contract-receipt.json`, which records the producer
commit, contract revision/version, stable path, and SHA-256 digest.

Every documented operation includes the global middleware's `429`
`application/problem+json` response using the shared `ProblemDetails` schema.
This gives consumers a schema-bearing response boundary for every operation
without inventing success payload schemas for handlers that have not declared
one. Existing typed success responses and body-free statuses remain unchanged.

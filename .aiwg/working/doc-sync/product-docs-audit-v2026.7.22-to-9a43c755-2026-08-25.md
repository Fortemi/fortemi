# Product documentation dry-run audit

- Direction: `code-to-docs`
- Range: `v2026.7.22..9a43c755`
- Mode: dry run; no product or documentation files edited
- Scope: public API, authentication, configuration, operations, hosted inference/credentials, and container build behavior
- Result: `DRIFT_FOUND` (9 findings)

## Findings

1. **High — credential-revealing diagnostics remain published.** `docs/content/mcp-deployment.md:459-460` and `docs/content/troubleshooting.md:940` tell operators to run `printenv | grep ...MCP_CLIENT`, which prints `MCP_CLIENT_SECRET`. Replace with checks that report variable presence only, or inspect the non-secret client ID explicitly without expanding the secret.

2. **Medium — MCP startup examples contradict the new masking behavior.** `docs/content/mcp-deployment.md:311-350` still shows the secret being printed and a failed registration response being logged. `docker/bundle-entrypoint.sh:579-588` now masks the secret and omits the response. Update expected transcripts and troubleshooting guidance; do not direct operators to look for a response body the service intentionally suppresses.

3. **High — the hosted OIDC authentication contract is undocumented.** `crates/matric-api/src/hosted_auth.rs:28-42` requires `ISSUER_URL` and `FORTEMI_AUTH_AUDIENCE`, defaults `FORTEMI_AUTH_TENANT_CLAIM`, and bounds clock skew, JWKS capacity, and HTTP timeout. None of the five `FORTEMI_AUTH_*` settings appear in `docs/content/authentication.md` or the claimed single source of truth, `docs/content/configuration.md:161-171`. Add the internal `hosted-auth` profile and clearly distinguish external OIDC bearer verification from the self-hosted OAuth compatibility profile described at `docs/content/authentication.md:206-218`.

4. **High — the hosted startup configuration is fragmented and incomplete.** Runtime code additionally requires distinct `MIGRATION_DATABASE_URL`/`DATABASE_URL` (`main.rs:3293-3304`) and `FORTEMI_AWS_KMS_KEY_ID` plus KMS health (`main.rs:2695-2722`). The PostgreSQL runbook only lists the database pair, while `docs/content/configuration.md` omits both `MIGRATION_DATABASE_URL` and the KMS key ID. Add one fail-closed hosted startup checklist that covers build features, OIDC, database roles, audit, KMS, quota, attachment scanning, and destination policy.

5. **High — `.env.example` cannot serve as a hosted bootstrap template.** It includes quota and destination settings but omits the required audience, migration URL, AWS KMS key ID, and all new breaker/rewrap controls. Add commented, non-secret hosted variables with secret-manager guidance; never include credential values.

6. **Medium — new hosted lifecycle controls are absent from the central configuration reference.** The breaker variables (`main.rs:2761-2791`) and rewrap worker variables (`main.rs:2794-2831`) exist only in specialized runbooks. Add their defaults, ranges, pairing rule, hosted-only scope, restart implications, and links from `docs/content/configuration.md`/operator guidance.

7. **Medium — the public API page contradicts the implemented hosted quota behavior.** `docs/content/api.md:3512-3530` says a single process-local limiter applies and calls hosted quota fields future behavior, despite the hosted response documented earlier at `api.md:3408-3428` and the live Redis gate in `main.rs:2726-2758`. Split CE and hosted behavior consistently, including hosted `429` fields and fail-closed `503` semantics.

8. **Medium — the Streamable HTTP session recovery contract is undocumented.** `mcp-server/index.js:5878-5891` and `:5965-5972` now return `404` for an unknown `Mcp-Session-Id` so clients reinitialize. Add this to MCP deployment/troubleshooting docs, distinguishing it from the no-session `400` response and warning custom clients to retry initialization without the stale session ID.

9. **Medium — container release evidence lists an obsolete build-argument allowlist.** `docs/content/container-release-evidence.md:30-33` names only `VERSION`, `GIT_SHA`, and `BUILD_DATE`, while both Dockerfiles now expose non-secret `RUST_MIN_STACK=16777216` (`Dockerfile:12-19`, `Dockerfile.bundle:13-18`) and use it for release compilation. Document the deterministic stack guard and keep it classified as non-secret build configuration; also acknowledge the Rust 1.92 builder pin.

## Coverage notes

- The hosted credential API, destination policy, circuit breaker, key rotation, PostgreSQL role, and durable audit topics have substantive new runbooks, but the scoped published product docs do not provide a single index linking those internal runbooks. Address discoverability while fixing findings 3–6.
- The UTF-8/CRLF chunk-offset correction preserves the documented chunking contract and needs a changelog entry, not a reference-doc contract change.

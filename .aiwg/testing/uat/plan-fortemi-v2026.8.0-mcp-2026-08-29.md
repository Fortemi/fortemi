# Fortemi Server v2026.8.0 Live MCP UAT Plan

## Execution target

- Release: `v2026.8.0`
- Release commit: `c76e5ef72dcf039acfe78b1e7b254cba30a79b8d`
- Image: `ghcr.io/fortemi/fortemi:bundle-2026.8.0`
- OCI index digest: `sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b`
- API: `http://127.0.0.1:3000`
- MCP: `http://127.0.0.1:3001`
- Transport: Streamable HTTP with OAuth client-credentials bearer authentication
- Protocol negotiated during discovery: `2025-03-26`
- Live discovered surface: 43 tools; every tool has `name`, `description`, and `inputSchema`

The signed release is the authoritative target. A local `fortemi:uat-fix`
candidate may be used for issue remediation and regression evidence, but its
results must be reported separately and must not be described as a release.

## Scope and sequence

Run the release-aligned repository suite at `mcp-server/tests/run-all.sh`. The suite
executes 31 test files sequentially in this order:

1. Static schema, branding, and error-response contracts.
2. Live initialization, session persistence, seed state, CRUD, attachments, and vision.
3. Search and memory-search behavior.
4. Tags, collections, links, embeddings, document types, and edge cases.
5. Templates, versions, archives, SKOS, PKE, jobs, observability, and memories.
6. OAuth, API-management, and the consolidated 43-tool surface.
7. Cross-feature chains, data export, and annotations.
8. Cleanup of UAT-created state.

Each test file runs in its own Node test process, providing negative-test isolation.
The suite is deliberately sequential because live sessions share PostgreSQL state.

## Acceptance criteria

- API `livez`, `readyz`, and `health` return HTTP 200.
- MCP initialization and `tools/list` succeed with OAuth authentication.
- All discovered schemas contain their required top-level fields.
- All 31 files complete and all assertions pass.
- Cleanup completes successfully.
- Fortemi remains healthy and ready after the run, with no new fatal startup or
  request-processing errors in the deployment logs.

Any failed assertion makes the UAT disposition `FAIL`; environment limitations are
reported separately and are not silently converted into passes.

Security-sensitive findings involving credentials, cryptographic validation, or
secret-adjacent diagnostics require issue-specific authorization before filing or
implementation. An authorization hold does not convert the associated assertion
to a pass.

## Evidence outputs

- Result: `.aiwg/testing/uat/results/fortemi-v2026.8.0-mcp-uat-2026-08-29.md`
- Deployment audit: `.aiwg/ops/audit/fortemi-server-v2026.8.0-deployment-uat-2026-08-29.md`

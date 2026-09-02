# Fortemi Server v2026.9.0 Live MCP UAT Plan

## Execution target

- Release: `v2026.9.0`
- Release commit: `e4fd622adc5b6759d69483d56eefeae7b0c462ca`
- Image: `ghcr.io/fortemi/fortemi:bundle-2026.9.0`
- OCI index digest:
  `sha256:1c6298790ac30dbddfb9160289dcdd68f6302e8186cd30b6ea62ac73786f1231`
- API: `http://127.0.0.1:3000`
- MCP: `http://127.0.0.1:3001`
- Transport: Streamable HTTP with OAuth client-credentials bearer authentication
- Suite: `mcp-server/tests/run-all.sh`, 31 files executed sequentially

The signed release image is the runtime authority. Post-release test-only
corrections may be applied when a UAT assertion contradicts the released
contract, but the original result must be retained and the release tag must not
be moved.

## Safety profile

1. Verify ports 3000 and 3001 are unused and no Fortemi service or optional
   media/inference sidecar is running.
2. Pull the numbered release image without building or publishing an image.
3. Render `docker-compose.bundle.yml` with
   `.aiwg/working/compose.host-stability.yml` and an isolated
   `10.202.0.0/24` network.
4. Start exactly Redis and Fortemi with `--no-build`, `--pull always`, and
   `--no-deps`.
5. Require restart policy `no`, `NVIDIA_VISIBLE_DEVICES=void`, CPU Open3D, and
   no Docker GPU device request.
6. Generate the database secret in a mode-0600 temporary environment file.
   Retrieve the bundle-generated mode-0600 OAuth client without echoing it,
   acquire a bearer token through `client_secret_basic`, and keep all secret
   values process-local.

Whisper, GLiNER, Pyannote, Ollama, autoheal, and other optional support services
are outside this CPU-safe run and must remain stopped.

## Scope and sequence

1. Verify release identity, labels, digest, API liveness/readiness, MCP health,
   restart state, OOM state, and absence of GPU device requests.
2. Execute the canonical 31-file MCP suite sequentially.
3. If the suite exposes a defect, file a GitHub issue authorized for this run,
   preserve the failing receipt, implement the bounded correction, run focused
   tests, and rerun the entire suite.
4. Exercise `manual-note-link-v1` live create and exact replay using two
   MCP-created notes and the same process-local OAuth bearer. Verify persisted
   identity and graph visibility.
5. Recheck health and logs for panic, `handler_panicked`, fatal, traceback,
   out-of-memory, OOM-kill, and semantic-chunk panic signatures.
6. Tear down the exact Compose project with volumes and orphans, remove
   temporary credentials, and verify zero project containers, networks,
   volumes, listeners, and deployed Fortemi/support services remain.

## Acceptance criteria

- Exact numbered release image, OCI digest, version label, and revision match.
- API `/livez`, `/readyz`, and MCP `/health` return success before and after UAT.
- All 31 files complete with no failed assertion or missing file; the single
  documented branding skip is reported, not silently converted into a pass.
- Canonical tag characters are accepted. Unsupported tag characters are
  rejected with the generic grammar message and do not echo the submitted tag
  or note content.
- `manual-note-link-v1` returns `201`/`created: true` on first insert,
  `200`/`created: false` on exact replay, and the same persisted ID and
  timestamp; the outgoing graph surface observes the link.
- Fortemi and Redis remain healthy with zero restarts, zero OOM kills, no GPU
  device requests, and no panic/fatal/OOM log match.
- Teardown leaves no UAT resource, credential file, port listener, Fortemi
  service, or optional support sidecar running.

Any failed assertion makes that run a failure. A later corrected run is recorded
separately and does not rewrite the initial evidence.

## Evidence and claim boundary

- Result:
  `.aiwg/testing/uat/results/fortemi-v2026.9.0-mcp-uat-2026-09-02.md`
- Deployment audit:
  `.aiwg/ops/audit/fortemi-server-v2026.9.0-deployment-uat-2026-09-02.md`
- Tracking: GitHub `Fortemi/fortemi#61`, `#62`, `#67`, `#68`, and
  `Fortemi/HotM#10`

This plan validates the live Fortemi persistence plane and MCP/HTTP release
surface. The local bundle is not hosted-mode evidence; hosted write-scope and
tenant-transaction enforcement remain producer integration-test evidence. The
AIWG static index, Knowledge Shard state transfer, and Fortemi persistence
planes remain separate. No `core-v1`, `full-v1`, or `record-v1` claim and no
unqualified parity, complete-backup, or portability claim is authorized while
the suite audit remains `NO-GO`.

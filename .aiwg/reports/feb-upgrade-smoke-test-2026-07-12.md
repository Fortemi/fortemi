# Feb-Baseline Upgrade Smoke Test — fortemi-docs Archive

**Date**: 2026-07-12
**Scope**: Validate the Feb (2026.2.x) → current (2026.7.0) in-place Docker bundle upgrade using the fortemi-docs support archive as the dataset (default-seeded in Feb, opt-in today). Related: #1027, #1036, #1037, #1035.

## Method

1. Started `ghcr.io/fortemi/fortemi:bundle-2026.2.12` (latest Feb bundle actually published to ghcr — 2026.2.13 has a git tag but no image) as isolated compose project `fortemi-febtest` on ports 13000/13001, fresh volumes, host Ollama (`nomic-embed-text`, `qwen3.5:*`), sidecars disabled.
2. Docs dataset: the `fortemi-docs` archive auto-seeded on first boot (Feb default) — 180 notes. Extraction pipeline ran 900 jobs, 0 failures. Embeddings + semantic links generated via `POST /api/v1/notes/reprocess` (`steps: ["embedding"]` then `["linking"]`) since shard seeding is FTS-only by design.
3. Baseline captured: API snapshot (counts, FTS rankings, semantic results, graph topology) + DB fingerprint (md5 over note IDs, content, link edges).
4. Upgraded in place: stopped containers (volumes kept), started `bundle-2026.7.0` with the current compose on the same volumes, modeling a real Feb user (`REQUIRE_AUTH=false` + `I_UNDERSTAND_NO_AUTH=true`).
5. Re-validated against the baseline and exercised live features.

## Result: upgrade succeeds — but only after fixing 2 blockers a real user would hit

Once unblocked: verified pre-migration backup created (54MB gz, sha256-verified), all 20 pending migrations applied cleanly (96 → 116), and validation passed everywhere.

| Check | Feb 2026.2.12 | Post 2026.7.0 | Status |
|---|---|---|---|
| Notes (fortemi-docs) | 180 | 180 | ✅ identical |
| Note-ID md5 | ad3ed0a3… | ad3ed0a3… | ✅ identical |
| Content md5 | 121af3ec… | 121af3ec… | ✅ identical |
| Tags / note_tags | 14 / 334 | 14 / 334 | ✅ identical |
| Embedding rows / embedded notes | 13002 / 180 | 13002 / 180 | ✅ identical |
| Links / link-edge md5 | 156 / 9e3047cc… | 156 / 9e3047cc… | ✅ identical |
| FTS result counts (3 queries) | 50/50/50 | 50/50/50 | ✅ identical |
| FTS top-5 ranking ("hybrid search") | 5 note IDs | same IDs, same order | ✅ identical |
| Semantic search results | 10 | 10 | ✅ identical |
| Graph topology (links/components) | 156 / 12 | 156 / 12 | ✅ identical |
| Graph maintenance (normalize→snn→pfnet→snapshot) | n/a | runs, snapshot recorded | ✅ |
| New-note pipeline (create→NLP→embed→semantic hit) | n/a | works; new note is top semantic result | ✅ |
| Job failures across entire test | 0 | 0 | ✅ |

## Findings

### F1 — BLOCKER: compose default `POSTGRES_PASSWORD` changed, breaks all existing volumes
Current `docker-compose.bundle.yml` defaults `POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-fortemi-local-dev}`; the Feb compose hardcoded `matric`, which is what existing pgdata volumes were initialized with. On upgrade, pg_dump (pre-migration backup) and the API cannot authenticate → the backup gate fails → container restart-loops forever. **Workaround**: set `POSTGRES_PASSWORD=matric` in `.env`. **Fix options**: entrypoint should detect auth failure against existing data and fall back/alert explicitly; or migration docs must call this out; or entrypoint ALTERs the password to match env on startup.

### F2 — BLOCKER (large data): pre-migration backup stages dump in `/dev/shm` (64MB Docker default)
`bundle-entrypoint.sh` defaults `BACKUP_TEMP_DIR=/dev/shm/fortemi-pre-migration-backup`; `backup.sh` *requires* tmpfs/ramfs (unless `BACKUP_TEMP_TRUSTED_ENCRYPTED=true`). Docker's default `/dev/shm` is 64MB and `docker-compose.bundle.yml` sets no `shm_size`. Any database whose raw `pg_dump --format=custom --compress=0` exceeds 64MB — our modest 180-note docs archive already does — fails with ENOSPC → gate fails → restart loop. This is precisely the #1037 "large data" scenario; it fails at *any* realistic size. **Fix options**: add `shm_size` to the compose service (still fails beyond that size); or compress in-stream to reduce staging size; or stage on disk with the encrypted-ack documented; or pre-check dump size vs shm and emit an actionable error.

### F3 — Diagnosability: backup failures are silent
`backup.sh` pipes `pg_dump --verbose 2>&1` into a `while read` that only logs under `--verbose`; with `set -euo pipefail` the script dies with **no error output at all**. The entrypoint then prints only "verified pre-migration backup failed". F1 (auth) and F2 (ENOSPC) present identically — nothing distinguishes them without manually re-running the script. **Fix**: always surface pg_dump stderr on failure (e.g., capture to a file and tail it in `error_exit`).

### F4 — `FORTEMI_ALLOW_LOCAL_ISSUER` not plumbed through compose
2026.7.0 refuses to start with an http `ISSUER_URL` unless `FORTEMI_ALLOW_LOCAL_ISSUER=true`, but that variable is not in the compose env list, so bundle users (localhost/LAN deployments upgraded from Feb) cannot set it via `.env` — they must edit the compose file. **Fix**: add `- FORTEMI_ALLOW_LOCAL_ISSUER=${FORTEMI_ALLOW_LOCAL_ISSUER:-false}` to the `fortemi` service.

### F5 — Minor: `POST /api/v1/notes/reprocess` without `note_ids` only processes 100 notes
The handler defaults `limit` to 500, but the note listing it delegates to caps at 100, so "reprocess all" silently covers only the first 100 notes (observed on both 2026.2.12 and the current handler code path). Workaround: pass explicit `note_ids`. Affects the documented seed-script hint for enabling semantic search on the docs archive (180 notes).

### Notes (expected behavior, no action)
- ADR-094 fail-closed auth: anonymous continuation via `REQUIRE_AUTH=false` + `I_UNDERSTAND_NO_AUTH=true` worked as documented.
- Shard seeding is FTS-only by design; embeddings/links require the documented reprocess step.
- Feb `concept_tagging` jobs no-op (~7ms) with GLiNER disabled — consistent pre/post upgrade, not an upgrade regression.

## Test environment / artifacts

- Compose project `fortemi-febtest` (ports 13000/13001) — left running post-upgrade for inspection.
  Teardown: `docker compose -p fortemi-febtest -f <scratch>/febtest/docker-compose-current.yml down` (add `-v` to drop data).
- Snapshots + DB fingerprints + patched compose/env in the session scratchpad `febtest/` directory.
- Verified pre-migration backup on volume `fortemi-febtest_matric-backups`: `pre-migration-20260222220000-20260614150000-20260712T192628Z.sql.gz` (54MB, sha256-verified).

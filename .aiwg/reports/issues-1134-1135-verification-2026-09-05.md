# Issues #1134 and #1135 verification

Base: `faa8048b` on `main`. Delivery: repository `.aiwg/aiwg.config` mode `direct`.
The API fix is commit `663164ed`; the operator recipe accompanies this report.

## #1134 acceptance evidence

- `PgNoteRepository::live_ids_tx` resolves one bounded UUID array with the live
  predicate `deleted_at IS NULL`. `bulk_reprocess_notes` invokes it through
  `with_request_schema`, the same tenant/archive mechanism as the implicit list.
- Truncation precedes filtering; no backfilling. Default 500 and maximum 5000
  remain; nonpositive limits now consistently mean no work on both paths.
- Missing, deleted, and invisible IDs have identical skip behavior. Existing
  response fields remain: eligible input entries in `notes_count`, newly queued
  pipeline work in `jobs_queued`; no per-ID existence reasons or skipped field.
- The live API regression in `crates/matric-api/src/bulk_reprocess_tests.rs`
  invokes the actual route with an RLS-subject role against a migrated,
  non-default archive. It covers mixed IDs, deleted-only, missing, other archive,
  another tenant, empty IDs, zero/negative limits, truncation without backfill,
  the 5000 cap, duplicate entries, existing-job deduplication, and 101-note
  implicit pagination. It inspects persisted jobs to prove invalid IDs enqueue
  nothing. A hosted `TenantScopedConn` also verifies inverse tenant visibility.
- API documentation defines counts, limit behavior, and the deletion race.
  Preflight does not lock notes or cancel queued jobs. The test deletes an
  accepted note after enqueue and verifies its job still exists; normal worker
  missing/deleted-note handling is still required.

## #1135 acceptance evidence

- `scripts/sql/read-only-notes.sql` is the actual tested psql artifact, linked
  from operations docs. It uses `BEGIN READ ONLY`, transaction-local tenant
  binding, a one-row scoped archive-registry lookup, quoted schema selection,
  context checks, and the canonical live-note query.
- Operations docs distinguish the reserved local UUID from an authorized hosted
  tenant, explain that SQL tenant binding is not authentication, and prohibit
  weakening runtime RLS. Missing/wrong scope, zero results, connection reuse,
  and transaction lifetime are described. Backup/maintenance remain separate,
  with references to #1115 and #727. The deployment guide includes an upgrade note.
- `operator_read_only_psql_recipe_enforces_scope` executes the shipped file with
  psql against fully migrated PostgreSQL 18.6. A non-superuser/NOBYPASSRLS role
  reads a non-default archive containing local live, archived, deleted, and
  second-tenant notes: only the two local live notes are counted. Wrong tenant,
  missing archive, malformed UUID, unscoped reads, writes, and privileged roles
  fail. A subsequent query on the same connection verifies scope reset at commit.

## Commands and results

Executed with `DATABASE_URL` pointing only to an isolated PostgreSQL cluster,
`FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS=1`, and `CARGO_BUILD_JOBS=4`:

```sh
cargo test -p matric-api --bin matric-api bulk_reprocess_filters_live_ids -- --nocapture
cargo test -p matric-db --features migrations --test tenant_isolation_test -- --nocapture
cargo clippy -p matric-api -p matric-db --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
node scripts/ci/docs-contract.cjs --self-test --profile=hosted_strict
DOCS_CONTRACT_MODE=blocking bash scripts/ci/docs-contract.sh . --profile=hosted_strict
npm run docs:build
npm run docs:check-assets
```

Results: API regression passed; all 7 tenant-isolation tests passed; strict
Clippy, formatting, whitespace, documentation contract (zero findings), docsite
build, and asset-link checks passed. No production data was used or changed.
CI and final tracker closure are recorded in the issue threads after push.

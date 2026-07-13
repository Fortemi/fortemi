# Feb 3 Baseline Migration Audit - 2026-07-12

## Baseline

- Release tag: `v2026.2.0`
- Tag date: `2026-02-03 13:49:39 -0500`
- Tag object: `e16f13fe`
- Workspace version at tag: `2026.2.0`

This is a distinct baseline from the first seeded large-data gate, which starts
from migration `20260215000000`.

## Finding

An exact `v2026.2.0` database state required a compatibility repair before
current `sqlx::migrate!()` validation could pass. The tag's migration table can
contain checksums for migration versions whose current HEAD file bytes now
differ, and the tag also includes duplicate version numbers that were later
renumbered.

Duplicate migration versions in `v2026.2.0`:

```text
20260202100000
20260205000000
```

The effective stored checksum for those duplicate versions is the later
lexicographic file in the tag fixture:

```text
20260202100000 embedding_config_api
20260205000000 fix_embedding_pipeline
```

Current HEAD maps those version numbers to different files:

```text
20260202100000 document_type_agentic_config
20260205000000 colbert_embeddings
```

## Initial Probe Result

A small exact-tag fixture applied the `v2026.2.0` migration files, stamped the
tag-file SHA-384 checksums into `_sqlx_migrations`, seeded one `note_original`
row, and then ran the current migration gate before the compatibility repair:

```bash
DATABASE_URL=<DATABASE_URL> \
FORTEMI_RUN_LARGE_MIGRATION_GATE=true \
FORTEMI_MIN_SEEDED_NOTES=1 \
cargo test -p matric-db --features migrations \
  --test feb_to_current_migration_gate -- --ignored --nocapture
```

Result:

```text
test feb_2026_to_current_seeded_upgrade_completes_and_is_resumable ... FAILED
2026.2.x to current migration: Database { message_len: 69, message_class: "text" }
```

The panic output is redacted by the current error formatting, but the migration
table mismatch is visible before the current migrator runs.

## Repair

The active branch now normalizes known legacy migration history before
`sqlx::migrate!()` validation:

- updates only exact known old SHA-384 checksums to the current canonical
  migration bytes;
- marks extracted seed migrations as already applied only when the source
  `_sqlx_migrations` row still has the known old combined checksum; and
- preserves the existing `20260215000000_native_uuidv7.sql` checksum repair.

This repair runs inside `Database::migrate()` before `sqlx::migrate!()`.

The branch also applies a schema-only restore compatibility repair before the
automatic pre-migration backup. The literal Feb 3 schema had a SKOS trigger
condition using `OLD.embedding IS DISTINCT FROM NEW.embedding`; `pg_restore`
could not recreate that trigger because pgvector does not provide a vector
equality operator for the restored DDL. The entrypoint repair rewrites that
known trigger to compare `OLD.embedding::text` and `NEW.embedding::text`, making
the recovery dump restorable before any data migrations run. The same trigger
rewrite is also represented as a new forward migration,
`20260614150000_restore_safe_skos_embedding_trigger.sql`, so shipped migration
files remain immutable.

## Mismatch Inventory

The following `v2026.2.0` effective migration rows have SHA-384 checksums that
do not match current HEAD migration bytes for the same version:

```text
20260117000000 tag=20260117000000_embedding_sets.sql head=20260117000000_embedding_sets.sql
20260117000001 tag=20260117000001_fix_embedding_set_stats.sql head=20260117000001_seed_default_embedding_set.sql
20260118000000 tag=20260118000000_skos_tags.sql head=20260118000000_skos_tags.sql
20260201100000 tag=20260201100000_multilingual_fts_phase1.sql head=20260201100000_multilingual_fts_phase1.sql
20260201200000 tag=20260201200000_multilingual_fts_phase2.sql head=20260201200000_multilingual_fts_phase2.sql
20260201300000 tag=20260201300000_multilingual_fts_phase3.sql head=20260201300000_multilingual_fts_phase3.sql
20260201500000 tag=20260201500000_full_embedding_sets.sql head=20260201500000_full_embedding_sets.sql
20260202000000 tag=20260202000000_document_types.sql head=20260202000000_document_types.sql
20260202100000 tag=20260202100000_embedding_config_api.sql head=20260202100000_document_type_agentic_config.sql
20260203400000 tag=20260203400000_doctype_extraction_strategy.sql head=20260203400000_doctype_extraction_strategy.sql
20260204100000 tag=20260204100000_temporal_spatial_provenance.sql head=20260204100000_temporal_spatial_provenance.sql
20260204300000 tag=20260204300000_specialized_media_metadata.sql head=20260204300000_specialized_media_metadata.sql
20260204400000 tag=20260204400000_temporal_positional_doctypes.sql head=20260204400000_temporal_positional_doctypes.sql
20260205000000 tag=20260205000000_fix_embedding_pipeline.sql head=20260205000000_colbert_embeddings.sql
```

## Release Impact

The original Feb-to-current evidence remains useful for the later
`20260215000000` baseline, but it was insufficient for a literal Feb 3 user.
The repaired exact-tag fixture now proves that `v2026.2.0` can reach current
HEAD.

## Verified Exact-Tag Gate With Restore Drill

Command:

```bash
FORTEMI_FEB_BASELINE_TAG=v2026.2.0 \
FORTEMI_SEED_NOTES=100000 \
FORTEMI_MIN_SEEDED_NOTES=100000 \
FORTEMI_TESTDB_CONTAINER=fortemi-v2026-2-0-100k-restore-v2 \
FORTEMI_TESTDB_PORT=55454 \
scripts/ci/run-feb-to-current-fixture.sh
```

Result:

```text
2026.2.x seeded upgrade gate passed: before=20260205000000 after=20260614150000 notes=100000 elapsed_seconds=6
feb-to-current fixture completed
seed_notes=100000
baseline=v2026.2.0
baseline_sql_version=20260205000000
target=20260614150000
pre_migration_backup=pre-migration-20260712T054004Z-v2026.2.0-fixture.sql.gz
pre_migration_backup_sha256=4877a78743a79a65aa16b7fff4e8993b414965a974f4898b1e0baacfea1cffd5
restore_drill_note_original_count=100000
duration_seconds=7
wal_start_lsn=0/44845C68
wal_end_lsn=0/4C3484A8
wal_bytes=128985152
longest_migration=20260215200000	fts qualify config names	6799854692
max_ungranted_locks_sampled=1
```

The fixture restored the generated backup into a fresh database before running
current migrations and confirmed all `100000` seeded `note_original` rows were
present. The gate also ran a second `db.migrate()` pass successfully, proving
the final state is idempotent/resumable at current HEAD.

Until then, the Fortemi publish path should remain held even though the HotM
container publish path is complete.

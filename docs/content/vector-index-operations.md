# Vector Index Operations

Fortemi stores semantic embeddings in PostgreSQL with pgvector HNSW indexes.
Shape-keyed indexes are shared by embedding dimension and vector storage type,
then searched with per-set filters. Operators normally manage these indexes
through embedding-set APIs and background jobs rather than direct SQL.

## Weekly Cadence

Run the database maintenance window weekly, or after unusually large embedding
imports:

```bash
VACUUM ANALYZE embedding;
```

The `embedding` table is configured with lower autovacuum thresholds than the
database default, so routine churn should be picked up automatically. The weekly
manual pass is still useful after sustained imports, deletes, or profile-vector
refreshes because it updates planner statistics before the next high-traffic
search window.

Check each large set after the window:

```bash
curl -H "Authorization: Bearer $FORTEMI_ADMIN_TOKEN" \
  "https://fortemi.example/api/v1/embedding-sets/my-set/index-health?probe=50"
```

Use `probe=0` or omit `probe` for a cheap metadata-only check. Probe values are
capped at 200 because recall probing runs exact comparison queries as well as
HNSW queries.

## Bulk-load Procedure

For large external profile-vector loads, defer the index build until the import
has finished:

1. Create or update the target embedding set with `defer_index_build=true`.
2. Import the run folder or NDJSON upload.
3. Build the shape index through `POST /api/v1/embedding-sets/{slug}/build-index`.
4. Inspect `GET /api/v1/embedding-sets/{slug}/index-health`.

This keeps ingestion from repeatedly rebuilding the same shape index while the
table is changing. Imports that change more than 10,000 rows schedule a
deduplicated `AnalyzeEmbedding` job so PostgreSQL can refresh planner statistics
after the batch without requesting `VACUUM FULL`.

## Memory And Workers

HNSW builds are memory-sensitive. Set PostgreSQL `maintenance_work_mem` high
enough for the largest embedding shape you build during a maintenance window,
then lower it if the database shares the host with latency-sensitive services.
For dedicated maintenance windows, start with 1-4 GiB and adjust from observed
resident memory and build duration.

Parallel workers help when PostgreSQL can use them for index builds and table
scans, but they compete with inference and API traffic. On a dedicated database
host, reserve at least one CPU core for normal query handling and background
workers. On a single-node Docker bundle, schedule large builds when the API and
embedding backend are otherwise quiet.

## `ef_search`

Each embedding set can store an optional `ef_search` between 10 and 1000. When
unset, searches use Fortemi's default HNSW tuning value. Semantic search,
embedding queries, entity-profile similarity, and hybrid search apply the value
inside the same transaction as the vector query with `SET LOCAL hnsw.ef_search`.

Raise `ef_search` when recall is more important than latency. Lower it only
after measuring search quality for that set. The value is exposed on REST set
create/update and the matching MCP embedding-set tools.

## Index Health

`GET /api/v1/embedding-sets/{slug}/index-health` requires admin scope and
returns:

- the shape index name, validity, and size;
- live/dead tuple counts plus last autovacuum and analyze timestamps for
  `embedding`;
- rows in the set and changes since the last index build when tracked;
- effective `ef_search` for the set;
- optional recall@10 when `probe=N` is provided.

Use recall probes for release verification, after changing HNSW tuning, and
after large imports. A probe compares HNSW results with exact vector search by
disabling index scans inside the exact-query transaction.

## Recall Acceptance Test

The default database test suite includes a deterministic 10,000-vector recall
acceptance test for 1024-dimensional clustered unit vectors. It inserts through
normal storage, builds the HNSW index through the background job path, then
requires recall@10 of at least 0.95 over 200 queries at the default `ef_search`.

Run the default gate:

```bash
cargo test -p matric-db --features migrations \
  --test vector_recall_acceptance_test \
  hnsw_recall_acceptance_for_1024_dimensional_vectors \
  -- --test-threads=1 --nocapture
```

Run the larger 100,000-vector variant before release or after HNSW tuning
changes:

```bash
cargo test -p matric-db --features "migrations recall-100k" \
  --test vector_recall_acceptance_test \
  hnsw_recall_acceptance_for_1024_dimensional_vectors \
  -- --test-threads=1 --nocapture
```

On the #1181 R1 disposable test database, the 10,000-vector run measured
recall@10 of 1.0000 in 27.5 seconds. The 100,000-vector feature run measured
recall@10 of 0.9830 in 1,349.8 seconds with parallel maintenance workers
disabled for the fixture to fit the disposable database's shared-memory limit.
Record release verification results for the target hardware when PostgreSQL
memory and worker settings differ.

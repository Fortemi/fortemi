# Managed PostgreSQL compatibility

This page lists what Fortemi needs from PostgreSQL so you can decide whether a
managed offering (Amazon RDS, Aurora, CloudNativePG) or a self-managed server
will run it. Everything here was checked against the files in `migrations/`
and the search code in `crates/`. Where a statement depends on a cloud
provider's current catalog, it says so: check the provider's documentation for
the exact engine version you deploy.

The Docker bundle already satisfies every requirement below. This page is for
deployments that bring their own database.

## Minimum version: PostgreSQL 18

PostgreSQL 18 is a hard requirement. PostgreSQL 17 is not supported and is not
qualified.

Why:

- `migrations/20260215000000_native_uuidv7.sql` (lines 14-57) sets column
  defaults to `uuidv7()`, and later migrations create tables with
  `DEFAULT uuidv7()` (for example
  `migrations/20260523000000_realtime_call_sessions.sql:4`). `uuidv7()` is a
  built-in function added in PostgreSQL 18. The same migration drops the old
  PL/pgSQL fallback `gen_uuid_v7()` (line 65), so there is nothing to fall back
  to on older servers.
- Runtime SQL also calls `uuidv7()` directly
  (`crates/matric-db/src/jobs/scoped.rs:359`).
- `matric-api` refuses to run migrations when `server_version_num < 180000`
  (`require_postgres_18` in `crates/matric-db/src/lib.rs`).

On a stock PostgreSQL 17.11 server, `SELECT uuidv7();` fails with
`function uuidv7() does not exist`, so the migration chain cannot complete.

No other PostgreSQL 18-only feature was found in `migrations/` (no virtual
generated columns; the `GENERATED ALWAYS AS` uses are identity columns).
Backporting to 17 would still require replacing every `uuidv7()` default, and
that work has not been done or tested.

CI and the bundle run on the `pgvector/pgvector:pg18` image with PostGIS 3
added (`build/Dockerfile.testdb`).

## Extensions

| Extension | Status | Created by | Trusted? | Used for |
|-----------|--------|------------|----------|----------|
| `vector` (pgvector) | Required | `migrations/20260102000000_initial_schema.sql:16` | No (superuser or provider admin role) | Embeddings and similarity search |
| `postgis` | Required, created **before** migrations | Not created by any migration; `geography(Point, 4326)` columns start in `migrations/20260204100000_temporal_spatial_provenance.sql:38` | No (superuser or provider admin role) | Location and spatial provenance |
| `pg_trgm` | Required | `migrations/20260201200000_multilingual_fts_phase2.sql:19` | Yes | Emoji, symbol and fallback CJK search |
| `unaccent` | Required | `migrations/20260131000000_fts_unicode_normalization.sql:12` | Yes | Accent-insensitive text search configurations |
| `uuid-ossp` | Required during a fresh migration run only | `migrations/20260102000000_initial_schema.sql:19`, dropped again by `migrations/20260215000000_native_uuidv7.sql:72` | Yes | Nothing at runtime |
| `pg_bigm` | **Optional** | `migrations/20260201300000_multilingual_fts_phase3.sql:53-63`, inside a `DO` block that catches the error | Depends on build | Faster, more precise CJK search |

"Trusted" means the extension's control file sets `trusted = true`, so a
non-superuser with `CREATE` on the database can install it. This was checked
on the PostgreSQL 18 images used here: `pg_trgm`, `unaccent` and `uuid-ossp`
are trusted; `vector` (0.8.6) and `postgis` are not.

No listed extension needs an entry in `shared_preload_libraries`.

### Machine-readable list

CI runs `scripts/ci/verify-postgres-extension-matrix.py`, which parses every
`CREATE EXTENSION` in `migrations/` and fails when this list no longer matches.
A bare `CREATE EXTENSION` must be listed as `required`. One inside a `DO` block
that traps errors (`EXCEPTION`) or checks `pg_available_extensions` may be
`optional`. `prerequisite` marks extensions the schema needs that migrations do
not create. If you add an extension to a migration, add it here together with
its provider support in the matrix below.

<!-- extension-matrix:begin -->
```text
required      vector
required      pg_trgm
required      unaccent
required      uuid-ossp
prerequisite  postgis
optional      pg_bigm
```
<!-- extension-matrix:end -->

## pg_bigm is optional

`pg_bigm` is not required. The phase 3 migration tries to create it and, if
that fails for any reason (not installed, not permitted), logs a notice and
continues. Bigram indexes are created only when the extension exists at
migration time (`migrations/20260201300000_multilingual_fts_phase3.sql`,
"Phase 3C").

At query time the search code checks `pg_extension` for `pg_bigm` on each CJK
query and falls back to `pg_trgm` when it is absent
(`crates/matric-db/src/search.rs`, `search_bigram` and `search_cjk`;
`crates/matric-db/src/search_candidates.rs`, `lexical_on_connection`). The
`FTS_BIGRAM_CJK` flag (default `true`, `crates/matric-search/src/fts_flags.rs`)
only selects the bigram strategy; it does not require the extension.

What changes without `pg_bigm`:

- CJK queries use `pg_trgm` `similarity()` and the `%` operator, plus a
  case-insensitive substring (`ILIKE`) match. Results are still returned.
- Trigram similarity is a weak signal for one- and two-character CJK queries,
  which are common. Ranking for those queries is coarser, and the substring
  match is what finds them.
- Latin-script, emoji and other search paths do not use `pg_bigm` and are
  unaffected.

If you install `pg_bigm` after the phase 3 migration has already run, searches
switch to bigram matching, but the `gin_bigm_ops` indexes are not created
retroactively. Create them by hand if you need them; the definitions are in the
phase 3 migration.

## Privileges and who creates extensions

Migrations run `CREATE EXTENSION IF NOT EXISTS`. If the extension already
exists, that statement succeeds without any privilege check. If it does not,
the migrating role needs superuser (or the provider's admin role) for `vector`
and `postgis`, and `CREATE` on the database for the trusted ones.

The reliable sequence on any platform:

1. As the database administrator (superuser, `rds_superuser` member, or the
   CloudNativePG bootstrap), create the extensions the migration role cannot:

   ```sql
   CREATE EXTENSION IF NOT EXISTS vector;
   CREATE EXTENSION IF NOT EXISTS postgis;
   CREATE EXTENSION IF NOT EXISTS pg_trgm;
   CREATE EXTENSION IF NOT EXISTS unaccent;
   -- Optional, where the platform offers it:
   CREATE EXTENSION IF NOT EXISTS pg_bigm;
   ```

2. Do **not** pre-create `uuid-ossp`. Migration
   `20260215000000_native_uuidv7.sql` drops it, and `DROP EXTENSION` must run
   as the extension's owner. If an administrator created it, the migration
   fails with `must be owner of extension uuid-ossp`. Let the migration role
   create and drop it, which requires `CREATE` on the database for the first
   migration run (or make the migration role the database owner).

3. Run migrations with the migration role, then start `matric-api` with the
   runtime role.

For the hosted multi-tenant build, the migration and runtime roles are already
defined in [Hosted PostgreSQL roles and tenant scope](hosted-postgresql-role.md).
That runbook does not grant `CREATE ON DATABASE` to `fortemi_migrator`; either
grant it for the first migration run or let the migrator own the database.
The runtime role must never receive `CREATE` or `BYPASSRLS`.

## Parameters

- `max_locks_per_transaction`: memory archives are separate schemas, and
  creating or dropping one locks roughly 41 tables plus their indexes. The
  test image raises the setting from 64 to 256 (`build/Dockerfile.testdb`) to
  avoid lock exhaustion when archives are created in parallel. Consider the
  same value if you create or drop archives concurrently. It needs a server
  restart (a parameter group change plus reboot on RDS and Aurora).
- No `shared_preload_libraries` entries are required.
- Hosted deployments set `app.current_tenant` per transaction with
  `set_config(..., true)`. Custom `app.*` settings need no server
  configuration.

## Platform matrix

Provider catalogs change between engine versions. Treat the cloud columns as a
checklist, and confirm each item in the provider's extension list for the
PostgreSQL 18 minor version you deploy.

| | Amazon RDS for PostgreSQL | Amazon Aurora PostgreSQL | CloudNativePG | Self-managed |
|---|---|---|---|---|
| PostgreSQL 18 | Verify availability in your region | Verify availability in your region | Yes (operand images for 18 are published; tested here with an 18.6 `standard` image) | Yes (PGDG packages) |
| `vector` | Supported; verify version for PG18 | Supported; verify version for PG18 | Included in the `standard` image | `postgresql-18-pgvector` (PGDG) or the `pgvector/pgvector:pg18` image |
| `postgis` | Supported; verify version for PG18 | Supported; verify version for PG18 | **Not** in the `standard` image; use a PostGIS-enabled image | `postgresql-18-postgis-3` (PGDG), as in `build/Dockerfile.testdb` |
| `pg_trgm`, `unaccent`, `uuid-ossp` | Supported (contrib) | Supported (contrib) | Included in the `standard` image | Included with the server (contrib) |
| `pg_bigm` (optional) | Listed by AWS for RDS; verify for your version | Verify against provider docs for your version | Not in the `standard` image; build a custom image if you want it | Build from source or a distribution package if available; not in Debian's main repos |
| Who runs `CREATE EXTENSION` | The master user (member of `rds_superuser`); there is no true superuser | The master user (member of `rds_superuser`); there is no true superuser | Superuser access is disabled by default; use `bootstrap.initdb.postInitApplicationSQL` or the operator's declarative extension support (verify for your operator version) | Superuser |
| `BYPASSRLS` for the hosted migration role | Verify whether your admin role can grant it | Verify whether your admin role can grant it | Granted by the superuser at bootstrap | Granted by superuser |
| Known limits | No superuser; extensions limited to the AWS-supported list | Same as RDS | Extensions must be in the container image | None beyond packaging |

Community and personal-server deployments do not use the hosted role split, so
a single owner role that can create the trusted extensions is enough once the
administrator has created `vector` and `postgis`.

## Verifying a database

After migrations, confirm the server version and installed extensions:

```sql
SHOW server_version_num;          -- expect 180000 or higher
SELECT extname, extversion FROM pg_extension ORDER BY extname;
```

Expected: `vector`, `postgis`, `pg_trgm`, `unaccent`, `plpgsql`, and
`pg_bigm` if you installed it. `uuid-ossp` should be absent.

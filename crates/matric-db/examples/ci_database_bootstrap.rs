//! Initialize a fresh, explicitly designated disposable CI database.

use matric_db::Database;
use serde_json::Value;
use sqlx::PgPool;

async fn snapshot(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('table', 'ledger', 'row', to_jsonb(m))
         FROM public._sqlx_migrations m
         UNION ALL
         SELECT jsonb_build_object('table', 'embedding_bootstrap', 'row', to_jsonb(b))
         FROM public.shard_embedding_set_bootstrap b
         UNION ALL
         SELECT jsonb_build_object('table', 'skos_bootstrap', 'row', to_jsonb(b))
         FROM public.shard_skos_scheme_bootstrap b
         ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .expect("read CI migration and bootstrap snapshot")
}

#[tokio::main]
async fn main() {
    assert_eq!(
        std::env::var("FORTEMI_CI_DISPOSABLE_DATABASE").as_deref(),
        Ok("1"),
        "explicit disposable CI database designation is required"
    );
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let db = Database::connect(&url)
        .await
        .expect("connect to disposable CI database");
    let pool = db.pool();
    // Extension-owned tables such as spatial_ref_sys are valid preconditions.
    let application_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname <> 'information_schema' AND n.nspname !~ '^pg_'
           AND c.relkind IN ('r', 'p', 'f')
           AND NOT EXISTS (SELECT 1 FROM pg_depend d
               WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid
                 AND d.deptype = 'e')",
    )
    .fetch_one(pool)
    .await
    .expect("verify fresh CI database");
    assert_eq!(
        application_tables, 0,
        "CI bootstrap requires a fresh database"
    );

    db.migrate()
        .await
        .expect("initialize CI schema with migration runner");
    let expected: Vec<(i64, Vec<u8>, bool)> = sqlx::migrate!("../../migrations")
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| (migration.version, migration.checksum.to_vec(), true))
        .collect();
    let actual: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
        "SELECT version, checksum, success FROM public._sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .expect("read CI migration ledger");
    assert_eq!(
        actual, expected,
        "CI ledger must bind every applied migration"
    );
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM public.note),
                (SELECT count(*) FROM public.shard_embedding_set_bootstrap),
                (SELECT count(*) FROM public.shard_skos_scheme_bootstrap)",
    )
    .fetch_one(pool)
    .await
    .expect("verify fresh seed custody");
    assert_eq!(
        counts,
        (0, 1, 1),
        "initializer must record both fresh defaults"
    );
    let first = snapshot(pool).await;
    db.migrate().await.expect("repeat CI migration runner");
    assert_eq!(
        snapshot(pool).await,
        first,
        "repeat must preserve ledger and custody"
    );
    println!(
        "CI bootstrap passed: {} migrations, repeated ledger and seed custody unchanged",
        actual.len()
    );
    pool.close().await;
}

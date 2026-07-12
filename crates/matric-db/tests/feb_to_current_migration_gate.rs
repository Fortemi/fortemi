use std::time::Instant;

use matric_db::Database;
use sqlx::PgPool;

async fn scalar_i64(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .expect("gate query should succeed")
}

#[tokio::test]
#[ignore = "requires a staged 2026.2.x database with representative production-scale data"]
async fn feb_2026_to_current_seeded_upgrade_completes_and_is_resumable() {
    if std::env::var("FORTEMI_RUN_LARGE_MIGRATION_GATE").as_deref() != Ok("true") {
        eprintln!("set FORTEMI_RUN_LARGE_MIGRATION_GATE=true to run the release gate");
        return;
    }

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a staged 2026.2.x seeded database");
    let min_seeded_notes: i64 = std::env::var("FORTEMI_MIN_SEEDED_NOTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000);

    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect to staged database");
    let seeded_notes = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'note_original'",
    )
    .await;
    assert_eq!(
        seeded_notes, 1,
        "staged database must contain note_original"
    );
    let note_count = scalar_i64(&pool, "SELECT COUNT(*) FROM note_original").await;
    assert!(
        note_count >= min_seeded_notes,
        "seeded database has {note_count} notes, expected at least {min_seeded_notes}"
    );
    let before = scalar_i64(
        &pool,
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    drop(pool);

    let db = Database::connect(&database_url)
        .await
        .expect("connect through Database");
    let started = Instant::now();
    db.migrate().await.expect("2026.2.x to current migration");
    let first_elapsed = started.elapsed();
    db.migrate()
        .await
        .expect("second migrate run should be idempotent/resumable");

    let pool = PgPool::connect(&database_url)
        .await
        .expect("reconnect after migration");
    let after = scalar_i64(
        &pool,
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    assert!(
        after > before,
        "migration gate expected pending migrations from 2026.2.x; before={before} after={after}"
    );

    eprintln!(
        "2026.2.x seeded upgrade gate passed: before={before} after={after} notes={note_count} elapsed_seconds={}",
        first_elapsed.as_secs()
    );
}

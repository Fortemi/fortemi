use std::{error::Error as StdError, time::Instant};

use matric_db::Database;
use sqlx::PgPool;

fn parse_seeded_table_counts(note_count: i64) -> Vec<(String, i64)> {
    match std::env::var("FORTEMI_SEEDED_TABLE_COUNTS") {
        Ok(value) if !value.trim().is_empty() => value
            .split(',')
            .map(|entry| {
                let (table, count) = entry.split_once('=').unwrap_or_else(|| {
                    panic!("invalid FORTEMI_SEEDED_TABLE_COUNTS entry: {entry}")
                });
                assert!(
                    table.chars().all(|ch| ch.is_ascii_lowercase()
                        || ch.is_ascii_digit()
                        || ch == '_'
                        || ch == '.'),
                    "invalid table name in FORTEMI_SEEDED_TABLE_COUNTS: {table}"
                );
                assert!(
                    table.split('.').all(|part| !part.is_empty())
                        && table.matches('.').count() <= 1,
                    "invalid schema-qualified table name in FORTEMI_SEEDED_TABLE_COUNTS: {table}"
                );
                let count = count
                    .parse::<i64>()
                    .unwrap_or_else(|_| panic!("invalid seeded count for {table}: {count}"));
                (table.to_owned(), count)
            })
            .collect(),
        _ => vec![("note_original".to_owned(), note_count)],
    }
}

async fn scalar_i64(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .expect("gate query should succeed")
}

fn seeded_table_count_sql(table: &str) -> String {
    match table.split_once('.') {
        Some((schema, table)) => format!("SELECT COUNT(*) FROM {schema}.{table}"),
        None => format!("SELECT COUNT(*) FROM public.{table}"),
    }
}

async fn seeded_table_count(pool: &PgPool, table: &str) -> i64 {
    scalar_i64(pool, &seeded_table_count_sql(table)).await
}

async fn migrate_or_panic(db: &Database, label: &str) {
    if let Err(error) = db.migrate().await {
        eprintln!("{label}: {error}");
        let mut source = error.source();
        while let Some(error) = source {
            eprintln!("caused by: {error}");
            source = error.source();
        }
        panic!("{label} failed");
    }
}

#[test]
fn seeded_table_count_sql_defaults_unqualified_tables_to_public() {
    assert_eq!(
        seeded_table_count_sql("note_original"),
        "SELECT COUNT(*) FROM public.note_original"
    );
}

#[test]
fn seeded_table_count_sql_preserves_schema_qualified_tables() {
    assert_eq!(
        seeded_table_count_sql("archive_fixture_research.note"),
        "SELECT COUNT(*) FROM archive_fixture_research.note"
    );
    assert_eq!(
        seeded_table_count_sql("public.note_original"),
        "SELECT COUNT(*) FROM public.note_original"
    );
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
        .map(|value| {
            value
                .parse()
                .expect("FORTEMI_MIN_SEEDED_NOTES must be an integer")
        })
        .unwrap_or(100_000);
    let allow_small_fixture =
        std::env::var("FORTEMI_ALLOW_SMALL_FEB_FIXTURE").as_deref() == Ok("true");
    if allow_small_fixture {
        assert!(
            min_seeded_notes >= 1_000,
            "small February fixtures require at least 1000 notes"
        );
    } else {
        assert!(
            min_seeded_notes >= 100_000,
            "the large-data gate requires at least 100000 notes"
        );
    }

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
    let seeded_table_counts = parse_seeded_table_counts(note_count);
    for (table, expected) in &seeded_table_counts {
        let actual = seeded_table_count(&pool, table).await;
        assert_eq!(
            actual, *expected,
            "staged database seeded count mismatch before migration for {table}"
        );
    }
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
    migrate_or_panic(&db, "2026.2.x to current migration").await;
    let first_elapsed = started.elapsed();
    drop(db);

    let pool = PgPool::connect(&database_url)
        .await
        .expect("reconnect after first migration pass");
    let after = scalar_i64(
        &pool,
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    let after_migration_rows = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    for (table, expected) in &seeded_table_counts {
        let actual = seeded_table_count(&pool, table).await;
        assert_eq!(
            actual, *expected,
            "migration must preserve seeded table {table}"
        );
    }
    drop(pool);

    let db = Database::connect(&database_url)
        .await
        .expect("reconnect through Database for resume pass");
    migrate_or_panic(&db, "second migrate run should be idempotent/resumable").await;
    drop(db);

    let pool = PgPool::connect(&database_url)
        .await
        .expect("reconnect after resume pass");
    let after_resume = scalar_i64(
        &pool,
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    let after_resume_migration_rows = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = true",
    )
    .await;
    let failed_migrations = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = false",
    )
    .await;
    assert_eq!(
        after_resume, after,
        "resume pass must not advance migration version"
    );
    assert_eq!(
        after_resume_migration_rows, after_migration_rows,
        "resume pass must not add migration rows"
    );
    assert_eq!(
        failed_migrations, 0,
        "migration ledger must not contain failed rows"
    );
    for (table, expected) in &seeded_table_counts {
        let actual = seeded_table_count(&pool, table).await;
        assert_eq!(
            actual, *expected,
            "resume pass must preserve seeded table {table}"
        );
    }
    assert!(
        after > before,
        "migration gate expected pending migrations from 2026.2.x; before={before} after={after}"
    );

    eprintln!(
        "2026.2.x seeded upgrade gate passed: before={before} after={after} notes={note_count} seeded_tables={} elapsed_seconds={}",
        seeded_table_counts
            .iter()
            .map(|(table, count)| format!("{table}={count}"))
            .collect::<Vec<_>>()
            .join(","),
        first_elapsed.as_secs()
    );
}

use std::{error::Error as StdError, str::FromStr};

use crate::{create_pool, Database};
use sqlx::migrate::MigrateDatabase;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Row};
use uuid::Uuid;

static MIGRATION_REPAIR_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct DisposableDatabase {
    url: String,
}

impl DisposableDatabase {
    async fn create(label: &str) -> Option<Self> {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping migration repair test: DATABASE_URL unavailable");
            return None;
        };

        let options = PgConnectOptions::from_str(&database_url)
            .expect("DATABASE_URL must be a valid PostgreSQL URL when configured");
        let database_name = format!("fortemi_{label}_{}", Uuid::now_v7().simple());
        let url = options.database(&database_name).to_url_lossy().to_string();

        <sqlx::Postgres as MigrateDatabase>::create_database(&url)
            .await
            .expect("create disposable migration test database");

        Some(Self { url })
    }
}

impl Drop for DisposableDatabase {
    fn drop(&mut self) {
        let url = self.url.clone();
        std::thread::spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(async move {
                    let _ = <sqlx::Postgres as MigrateDatabase>::force_drop_database(&url).await;
                });
            }
        })
        .join()
        .expect("join disposable database cleanup thread");
    }
}

#[derive(Clone, Copy)]
enum InvalidLedger {
    Dirty,
    UnknownVersion,
    ChecksumMismatch,
}

async fn setup_repair_candidate(pool: &sqlx::PgPool, archive_schema: &str) {
    sqlx::query(
        r#"
        CREATE TABLE public.tenant_registry (
            id UUID PRIMARY KEY,
            slug TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            status TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create tenant registry fixture");
    sqlx::query(
        r#"
        CREATE TABLE public.archive_registry (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            schema_name TEXT NOT NULL UNIQUE,
            description TEXT,
            note_count INTEGER DEFAULT 0,
            size_bytes BIGINT DEFAULT 0,
            is_default BOOLEAN DEFAULT FALSE,
            tenant_id UUID NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create archive registry fixture");
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");
    sqlx::query(&format!("CREATE SCHEMA {archive_schema}"))
        .execute(pool)
        .await
        .expect("create archive schema fixture");
    sqlx::query(&format!(
        "CREATE TABLE {archive_schema}.note (id UUID PRIMARY KEY, tenant_id UUID NOT NULL)"
    ))
    .execute(pool)
    .await
    .expect("create archive note fixture");
    sqlx::query("INSERT INTO public.tenant_registry (id, slug, display_name, status) VALUES ($1, 'local', 'Local personal server', 'active')")
        .bind(Uuid::nil())
        .execute(pool)
        .await
        .expect("seed tenant fixture");
    sqlx::query("INSERT INTO public.archive_registry (id, name, schema_name, tenant_id) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(format!("repair-{}", Uuid::now_v7().simple()))
        .bind(archive_schema)
        .bind(Uuid::nil())
        .execute(pool)
        .await
        .expect("seed archive registry fixture");
}

async fn seed_current_migration_ledger_with_early_pending_and_late_bad_checksum(
    pool: &sqlx::PgPool,
) {
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");

    let migrations: Vec<_> = sqlx::migrate!("../../migrations")
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .collect();
    let pending_version = migrations
        .first()
        .expect("current migration source must not be empty")
        .version;
    let late_bad_version = migrations
        .iter()
        .rev()
        .find(|migration| migration.version > pending_version)
        .expect("test requires a later applied migration after the pending version")
        .version;

    for migration in migrations {
        if migration.version == pending_version {
            continue;
        }
        let checksum: &[u8] = if migration.version == late_bad_version {
            &[0]
        } else {
            migration.checksum.as_ref()
        };
        sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)")
            .bind(migration.version)
            .bind(migration.description.to_string())
            .bind(checksum)
            .execute(pool)
            .await
            .expect("seed migration ledger row with late bad checksum");
    }
}

async fn seed_current_migration_ledger(pool: &sqlx::PgPool) {
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");

    for migration in sqlx::migrate!("../../migrations").iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)")
            .bind(migration.version)
            .bind(migration.description.to_string())
            .bind(migration.checksum.as_ref())
            .execute(pool)
            .await
            .expect("seed current migration ledger row");
    }
}

async fn insert_invalid_ledger(pool: &sqlx::PgPool, kind: InvalidLedger) {
    let (version, description, success) = match kind {
        InvalidLedger::Dirty => (20260903010000_i64, "dirty", false),
        InvalidLedger::UnknownVersion => (99999999999999_i64, "unknown", true),
        InvalidLedger::ChecksumMismatch => {
            let version = sqlx::migrate!("../../migrations")
                .iter()
                .find(|migration| !migration.migration_type.is_down_migration())
                .expect("current migration source must not be empty")
                .version;
            (version, "bad_checksum", true)
        }
    };

    sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, $3, decode('00', 'hex'), 0)")
        .bind(version)
        .bind(description)
        .bind(success)
        .execute(pool)
        .await
        .expect("seed invalid migration ledger");
}

async fn archive_note_repair_index_exists(pool: &sqlx::PgPool, archive_schema: &str) -> bool {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
              FROM pg_index i
              JOIN pg_class idx ON idx.oid = i.indexrelid
              JOIN pg_class tbl ON tbl.oid = i.indrelid
              JOIN pg_namespace n ON n.oid = tbl.relnamespace
             WHERE n.nspname = $1
               AND tbl.relname = 'note'
               AND idx.relname = 'uq_archive_note_tenant_id_id'
               AND i.indisunique
        )
        "#,
    )
    .bind(archive_schema)
    .fetch_one(pool)
    .await
    .expect("inspect repair index")
}

async fn advisory_lock_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)::bigint
          FROM pg_locks
         WHERE locktype = 'advisory'
           AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
           AND granted
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("inspect advisory locks")
}

fn error_chain_contains(error: &matric_core::Error, needle: &str) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(error) = current {
        if error.to_string().contains(needle) {
            return true;
        }
        current = error.source();
    }
    false
}

#[tokio::test]
async fn migration_runner_rejects_invalid_ledger_before_legacy_archive_repair() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("invalid_ledger").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");

    for (kind, expected) in [
        (InvalidLedger::Dirty, "partially applied"),
        (
            InvalidLedger::UnknownVersion,
            "previously applied but is missing",
        ),
        (
            InvalidLedger::ChecksumMismatch,
            "previously applied but has been modified",
        ),
    ] {
        let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
        setup_repair_candidate(&pool, &archive_schema).await;
        insert_invalid_ledger(&pool, kind).await;

        let error = Database::new(pool.clone())
            .migrate()
            .await
            .expect_err("invalid migration ledger must reject before repair");
        assert!(
            error_chain_contains(&error, expected),
            "expected error chain to contain {expected:?}, got {error:?}"
        );
        assert!(
            !archive_note_repair_index_exists(&pool, &archive_schema).await,
            "legacy archive repair must not mutate before ledger validation"
        );
        assert_eq!(
            advisory_lock_count(&pool).await,
            0,
            "migration runner must release advisory lock after rejection"
        );

        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(&pool)
            .await
            .expect("drop invalid ledger fixture schema");
        sqlx::query("CREATE SCHEMA public")
            .execute(&pool)
            .await
            .expect("recreate invalid ledger fixture schema");
    }

    pool.close().await;
}

#[tokio::test]
async fn migration_runner_prevalidates_late_checksum_before_pending_apply_or_repair() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("late_checksum").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
    setup_repair_candidate(&pool, &archive_schema).await;
    seed_current_migration_ledger_with_early_pending_and_late_bad_checksum(&pool).await;

    let error = Database::new(pool.clone())
        .migrate()
        .await
        .expect_err("late checksum mismatch must reject before pending apply or repair");
    assert!(
        error_chain_contains(&error, "previously applied but has been modified"),
        "expected checksum mismatch error, got {error:?}"
    );
    assert!(
        !archive_note_repair_index_exists(&pool, &archive_schema).await,
        "late checksum mismatch must be validated before pending apply or legacy archive repair mutates"
    );
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "migration runner must release advisory lock after late checksum rejection"
    );

    pool.close().await;
}

#[tokio::test]
async fn migration_runner_second_pass_is_idempotent_and_releases_lock() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("idempotent").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    seed_current_migration_ledger(&pool).await;
    let db = Database::new(pool.clone());

    db.migrate().await.expect("first idempotent migration pass");
    let after_first: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(max(version), 0), count(*) FROM public._sqlx_migrations WHERE success = true",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect first migration ledger");
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "first migration pass must release advisory lock"
    );

    db.migrate().await.expect("second migration pass");
    let after_second: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(max(version), 0), count(*) FROM public._sqlx_migrations WHERE success = true",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect second migration ledger");
    assert_eq!(after_second, after_first);
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "second migration pass must release advisory lock"
    );

    pool.close().await;
}

#[tokio::test]
async fn legacy_archive_note_repair_creates_tenant_qualified_unique_index() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("repair_helper").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
    setup_repair_candidate(&pool, &archive_schema).await;

    let mut conn = pool.acquire().await.expect("acquire repair connection");
    Database::repair_legacy_archive_tenant_note_indexes(&mut conn)
        .await
        .expect("repair legacy archive note index");
    drop(conn);

    let rows = sqlx::query(
        r#"
        SELECT a.attname
          FROM pg_index i
          JOIN pg_class idx ON idx.oid = i.indexrelid
          JOIN pg_class tbl ON tbl.oid = i.indrelid
          JOIN pg_namespace n ON n.oid = tbl.relnamespace
          JOIN unnest(i.indkey) WITH ORDINALITY key(attnum, ordinality) ON true
          JOIN pg_attribute a ON a.attrelid = tbl.oid AND a.attnum = key.attnum
         WHERE n.nspname = $1
           AND tbl.relname = 'note'
           AND idx.relname = 'uq_archive_note_tenant_id_id'
           AND i.indisunique
         ORDER BY key.ordinality
        "#,
    )
    .bind(&archive_schema)
    .fetch_all(&pool)
    .await
    .expect("inspect repair index");
    let columns: Vec<String> = rows
        .into_iter()
        .map(|row| row.get::<String, _>("attname"))
        .collect();
    assert_eq!(columns, ["tenant_id", "id"]);

    pool.close().await;
}

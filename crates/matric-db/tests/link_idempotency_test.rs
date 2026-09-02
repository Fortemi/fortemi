use std::sync::Arc;

use matric_db::{create_pool, PgLinkRepository};
use serde_json::json;
use tokio::sync::Barrier;
use uuid::Uuid;

async fn setup() -> Option<sqlx::PgPool> {
    let database_url = std::env::var("DATABASE_URL").ok()?;
    let pool = create_pool(&database_url)
        .await
        .unwrap_or_else(|_| panic!("connect link idempotency test database"));
    let schema_ready: bool = sqlx::query_scalar(
        r#"SELECT to_regclass('public.note') IS NOT NULL
               AND to_regclass('public.link') IS NOT NULL
               AND to_regclass('public.ux_link_note_identity') IS NOT NULL"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| panic!("inspect link idempotency test database schema"));
    assert!(
        schema_ready,
        "link idempotency test database schema is unavailable"
    );
    Some(pool)
}

async fn insert_note(pool: &sqlx::PgPool, id: Uuid) {
    sqlx::query(
        r#"INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
           VALUES ($1, 'markdown', 'link-idempotency-test', now(), now())"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("insert note fixture");
}

async fn cleanup(pool: &sqlx::PgPool, note_ids: &[Uuid]) {
    sqlx::query("DELETE FROM note WHERE id = ANY($1)")
        .bind(note_ids)
        .execute(pool)
        .await
        .expect("clean note fixtures");
}

#[tokio::test]
async fn concurrent_writers_return_one_authoritative_link() {
    let Some(pool) = setup().await else {
        eprintln!("skipping link idempotency test: DATABASE_URL unavailable");
        return;
    };
    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    insert_note(&pool, source).await;
    insert_note(&pool, target).await;

    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let pool = pool.clone();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            let repository = PgLinkRepository::new(pool.clone());
            let mut tx = pool.begin().await.expect("begin writer transaction");
            barrier.wait().await;
            let result = repository
                .create_idempotent_tx(&mut tx, source, target, "explicit", 0.75, None)
                .await
                .expect("idempotent create");
            tx.commit().await.expect("commit writer transaction");
            result
        }));
    }
    barrier.wait().await;
    let first = tasks.remove(0).await.expect("first writer task");
    let second = tasks.remove(0).await.expect("second writer task");

    assert_eq!(first.id, second.id);
    assert_ne!(first.created, second.created);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM link WHERE from_note_id = $1 AND to_note_id = $2 AND kind = 'explicit'",
    )
    .bind(source)
    .bind(target)
    .fetch_one(&pool)
    .await
    .expect("count persisted links");
    assert_eq!(count, 1);

    cleanup(&pool, &[source, target]).await;
}

#[tokio::test]
async fn repeat_conflict_and_rollback_preserve_the_authoritative_row() {
    let Some(pool) = setup().await else {
        eprintln!("skipping link idempotency test: DATABASE_URL unavailable");
        return;
    };
    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    let rollback_target = Uuid::new_v4();
    for id in [source, target, rollback_target] {
        insert_note(&pool, id).await;
    }
    let repository = PgLinkRepository::new(pool.clone());

    let mut first_tx = pool.begin().await.expect("begin initial transaction");
    let first = repository
        .create_idempotent_tx(&mut first_tx, source, target, "explicit", 0.8, None)
        .await
        .expect("initial create");
    first_tx.commit().await.expect("commit initial transaction");
    assert!(first.created);

    let mut repeat_tx = pool.begin().await.expect("begin replay transaction");
    let repeat = repository
        .create_idempotent_tx(&mut repeat_tx, source, target, "explicit", 0.8, None)
        .await
        .expect("exact replay");
    repeat_tx.commit().await.expect("commit replay transaction");
    assert!(!repeat.created);
    assert_eq!(repeat.id, first.id);

    let secret_metadata = json!({"api_key": "sk-live-link-idempotency-secret"});
    let mut conflict_tx = pool.begin().await.expect("begin conflict transaction");
    let conflict = repository
        .create_idempotent_tx(
            &mut conflict_tx,
            source,
            target,
            "explicit",
            0.2,
            Some(secret_metadata),
        )
        .await
        .expect("conflicting replay returns persisted authority");
    conflict_tx.commit().await.expect("commit no-op conflict");
    assert!(!conflict.created);
    assert_eq!(conflict.id, first.id);
    assert_eq!(conflict.score, 0.8);
    assert_eq!(conflict.metadata, None);
    let debug = format!("{conflict:?}");
    assert!(!debug.contains(&source.to_string()));
    assert!(!debug.contains(&target.to_string()));
    assert!(!debug.contains("sk-live-link-idempotency-secret"));

    let mut rollback_tx = pool.begin().await.expect("begin rollback transaction");
    let rolled_back = repository
        .create_idempotent_tx(
            &mut rollback_tx,
            source,
            rollback_target,
            "explicit",
            0.5,
            None,
        )
        .await
        .expect("create rollback fixture");
    assert!(rolled_back.created);
    rollback_tx.rollback().await.expect("rollback transaction");
    let rollback_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM link WHERE from_note_id = $1 AND to_note_id = $2 AND kind = 'explicit'",
    )
    .bind(source)
    .bind(rollback_target)
    .fetch_one(&pool)
    .await
    .expect("count rolled-back links");
    assert_eq!(rollback_count, 0);

    cleanup(&pool, &[source, target, rollback_target]).await;
}

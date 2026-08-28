use chrono::{Duration, Utc};
use matric_core::defaults::INTERNAL_FETCH_LIMIT;
use matric_db::PgNoteRepository;
use sqlx::{postgres::PgPoolOptions, Executor};

async fn isolated_pool() -> Option<sqlx::PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .ok()?;
    pool.execute(
        "CREATE TEMP TABLE note (
            id uuid PRIMARY KEY,
            updated_at_utc timestamptz NOT NULL,
            deleted_at timestamptz
        );
        CREATE TEMP TABLE link (
            from_note_id uuid NOT NULL,
            to_note_id uuid
        );
        CREATE TEMP TABLE tag (name text PRIMARY KEY);
        CREATE TEMP TABLE note_tag (
            note_id uuid NOT NULL,
            tag_name text NOT NULL
        );",
    )
    .await
    .ok()?;
    Some(pool)
}

async fn replace_notes(pool: &sqlx::PgPool, count: i64) {
    pool.execute("TRUNCATE note_tag, link, tag, note")
        .await
        .expect("reset health fixtures");
    sqlx::query(
        "INSERT INTO note (id, updated_at_utc)
         SELECT md5(i::text)::uuid,
                CASE WHEN i % 2 = 0 THEN now() - interval '90 days' ELSE now() END
           FROM generate_series(1, $1) AS i",
    )
    .bind(count)
    .execute(pool)
    .await
    .expect("insert health fixture notes");
}

async fn counts(pool: &sqlx::PgPool) -> matric_db::KnowledgeHealthCounts {
    let repository = PgNoteRepository::new(pool.clone());
    let mut transaction = pool.begin().await.expect("begin health transaction");
    let counts = repository
        .knowledge_health_counts_tx(&mut transaction, Utc::now() - Duration::days(30))
        .await
        .expect("aggregate health counts");
    transaction
        .rollback()
        .await
        .expect("rollback read transaction");
    counts
}

#[tokio::test]
async fn aggregate_counts_are_complete_across_internal_fetch_boundaries() {
    let Some(pool) = isolated_pool().await else {
        eprintln!("skipping PostgreSQL health test: DATABASE_URL unavailable");
        return;
    };

    for corpus_size in [0, 7, INTERNAL_FETCH_LIMIT, INTERNAL_FETCH_LIMIT + 1] {
        replace_notes(&pool, corpus_size).await;
        let result = counts(&pool).await;
        assert_eq!(result.total_notes, corpus_size);
        assert_eq!(result.unlinked_notes, corpus_size);
        assert_eq!(result.notes_without_tags, corpus_size);
        assert_eq!(result.total_links, 0);
        assert_eq!(result.total_tags, 0);
        assert_eq!(result.orphan_tags, 0);
        assert_eq!(result.stale_notes, corpus_size / 2);
    }
}

#[tokio::test]
async fn aggregate_counts_share_one_archive_scope_for_links_and_tags() {
    let Some(pool) = isolated_pool().await else {
        eprintln!("skipping PostgreSQL health test: DATABASE_URL unavailable");
        return;
    };
    replace_notes(&pool, 3).await;
    pool.execute(
        "INSERT INTO link (from_note_id, to_note_id)
         VALUES (md5('1')::uuid, md5('2')::uuid);
         INSERT INTO tag (name) VALUES ('used'), ('orphan');
         INSERT INTO note_tag (note_id, tag_name) VALUES (md5('1')::uuid, 'used');",
    )
    .await
    .expect("insert scoped health relationships");

    let result = counts(&pool).await;
    assert_eq!(result.total_notes, 3);
    assert_eq!(result.total_links, 1);
    assert_eq!(result.unlinked_notes, 1);
    assert_eq!(result.total_tags, 2);
    assert_eq!(result.orphan_tags, 1);
    assert_eq!(result.notes_without_tags, 2);
}

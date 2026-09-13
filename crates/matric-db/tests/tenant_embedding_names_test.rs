//! Forward migration coverage for old public/archive catalogs and future clones.
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const MIGRATION: &str =
    include_str!("../../../migrations/20260912000100_tenant_embedding_names.sql");

async fn insert_set(
    pool: &PgPool,
    schema: &str,
    tenant: Uuid,
    name: &str,
    slug: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(&format!("INSERT INTO {schema}.embedding_set(id,tenant_id,name,slug) VALUES($1,$2,$3,$4) RETURNING id"))
        .bind(Uuid::new_v4()).bind(tenant).bind(name).bind(slug).fetch_one(pool).await
}

fn unique_violation(error: sqlx::Error) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23505")
    );
}

#[sqlx::test(migrations = false)]
async fn embedding_names_upgrade_existing_archives_and_preserve_tenant_uniqueness(pool: PgPool) {
    sqlx::raw_sql("CREATE TABLE archive_registry(schema_name text NOT NULL);
        CREATE TABLE embedding_config(id uuid PRIMARY KEY, tenant_id uuid NOT NULL, name text NOT NULL UNIQUE);
        CREATE TABLE embedding_set(id uuid PRIMARY KEY, tenant_id uuid NOT NULL, name text NOT NULL UNIQUE, slug text NOT NULL UNIQUE);
        CREATE SCHEMA archive_old;
        CREATE TABLE archive_old.embedding_set (LIKE public.embedding_set INCLUDING ALL);
        INSERT INTO archive_registry VALUES ('archive_old');")
        .execute(&pool).await.unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    for schema in ["public", "archive_old"] {
        insert_set(&pool, schema, a, "common", "common")
            .await
            .unwrap();
        unique_violation(
            insert_set(&pool, schema, b, "common", "common")
                .await
                .unwrap_err(),
        );
    }
    let config_id = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_config VALUES($1,$2,'common')")
        .bind(config_id)
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    let before: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM embedding_set e UNION ALL SELECT to_jsonb(e) FROM archive_old.embedding_set e")
        .fetch_all(&pool).await.unwrap();
    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();
    let after: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM embedding_set e UNION ALL SELECT to_jsonb(e) FROM archive_old.embedding_set e")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(before, after, "migration must preserve rows and IDs");
    sqlx::raw_sql("CREATE SCHEMA archive_new; CREATE TABLE archive_new.embedding_set (LIKE public.embedding_set INCLUDING ALL)")
        .execute(&pool).await.unwrap();
    insert_set(&pool, "archive_new", a, "common", "common")
        .await
        .unwrap();
    for schema in ["public", "archive_old", "archive_new"] {
        insert_set(&pool, schema, b, "common", "common")
            .await
            .unwrap();
        unique_violation(
            insert_set(&pool, schema, a, "common", "different")
                .await
                .unwrap_err(),
        );
        unique_violation(
            insert_set(&pool, schema, a, "different", "common")
                .await
                .unwrap_err(),
        );
    }
    sqlx::query("INSERT INTO embedding_config VALUES($1,$2,'common')")
        .bind(Uuid::new_v4())
        .bind(b)
        .execute(&pool)
        .await
        .unwrap();
    unique_violation(
        sqlx::query("INSERT INTO embedding_config VALUES($1,$2,'common')")
            .bind(Uuid::new_v4())
            .bind(a)
            .execute(&pool)
            .await
            .unwrap_err(),
    );
    let retained: Uuid = sqlx::query_scalar("SELECT id FROM embedding_config WHERE tenant_id=$1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(retained, config_id);
}

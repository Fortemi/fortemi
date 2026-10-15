//! Archive authors reference the shared deployment identity registry.
use matric_core::{ArchiveRepository, CreateNoteRequest};
use matric_db::{Database, PoolConfig};
use sqlx::{ConnectOptions, PgPool};
use uuid::Uuid;

#[sqlx::test(migrations = false)]
async fn existing_archive_user_references_are_repaired_without_changing_rows(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let url = admin.connect_options().to_url_lossy().to_string();
    let tenant = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Synthetic','active')")
        .bind(tenant).bind(format!("archive-user-{tenant}")).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,tenant_id,iss,sub,kind) VALUES($1,$2,'https://issuer.example','synthetic','user')")
        .bind(user).bind(tenant).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO embedding_config (id,tenant_id,name,model,dimension,chunk_size,chunk_overlap,is_default) VALUES($1,$2,'default','nomic-embed-text',768,1500,200,true)")
        .bind(Uuid::new_v4()).bind(tenant).execute(&admin).await.unwrap();
    let runtime = matric_db::create_pool_with_config(&url, PoolConfig::new().single_tenant(tenant))
        .await
        .unwrap();
    let db = Database::new(runtime.clone());
    let archive = db
        .archives
        .create_archive_schema("identity_repair", None)
        .await
        .unwrap();
    let schema = archive.schema_name;
    for table in ["app_user", "personal_access_token"] {
        let cloned: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.{table}"))
            .fetch_one(&admin)
            .await
            .unwrap();
        assert!(!cloned, "deployment identities must not be cloned");
    }
    let context = db.for_schema(&schema).unwrap();
    let notes = matric_db::PgNoteRepository::new(runtime.clone());
    let note = context
        .execute(move |tx| {
            Box::pin(async move {
                let id = notes
                    .insert_tx(
                        tx,
                        CreateNoteRequest {
                            content: "Synthetic archive identity".into(),
                            format: "markdown".into(),
                            source: "test".into(),
                            collection_id: None,
                            tags: None,
                            metadata: None,
                            document_type_id: None,
                            title: None,
                        },
                    )
                    .await?;
                sqlx::query(
                    "UPDATE note SET created_by_user_id=$2,updated_by_user_id=$2 WHERE id=$1",
                )
                .bind(id)
                .bind(user)
                .execute(&mut **tx)
                .await?;
                Ok(id)
            })
        })
        .await
        .unwrap();
    // Reproduce a populated legacy archive whose FK was redirected by cloning.
    sqlx::query(&format!(
        "CREATE TABLE {schema}.app_user (LIKE public.app_user INCLUDING ALL)"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "INSERT INTO {schema}.app_user SELECT * FROM public.app_user WHERE id=$1"
    ))
    .bind(user)
    .execute(&admin)
    .await
    .unwrap();
    for (constraint, column) in [
        ("note_created_by_user_fk", "created_by_user_id"),
        ("note_updated_by_user_fk", "updated_by_user_id"),
    ] {
        sqlx::query(&format!("ALTER TABLE {schema}.note DROP CONSTRAINT {constraint}, ADD CONSTRAINT {constraint} FOREIGN KEY(tenant_id,{column}) REFERENCES {schema}.app_user(tenant_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT"))
            .execute(&admin).await.unwrap();
    }
    let before: serde_json::Value = sqlx::query_scalar(&format!(
        "SELECT to_jsonb(n) FROM {schema}.note n WHERE id=$1"
    ))
    .bind(note)
    .fetch_one(&admin)
    .await
    .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../migrations/20261014020000_archive_user_principals.sql"
        ))
        .execute(&admin)
        .await
        .unwrap();
    }
    let after: serde_json::Value = sqlx::query_scalar(&format!(
        "SELECT to_jsonb(n) FROM {schema}.note n WHERE id=$1"
    ))
    .bind(note)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(before, after);
    let references: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE conrelid=$1::regclass AND confrelid='public.app_user'::regclass")
        .bind(format!("{schema}.note")).fetch_one(&admin).await.unwrap();
    assert_eq!(references, 2);
    let retained: Uuid = sqlx::query_scalar(&format!("SELECT id FROM {schema}.app_user"))
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(retained, user);
    runtime.close().await;
}

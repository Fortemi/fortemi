use super::*;
use matric_core::ArchiveRepository;
use query_template_tests::response;
use serde_json::{json, Value};

async fn snapshot(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
    new_column: &str,
) -> Vec<(Uuid, Value, String)> {
    sqlx::query_as(&format!(
        "SELECT id,to_jsonb(t)-'{new_column}',xmin::text FROM {schema}.{table} t ORDER BY id"
    ))
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn populated_prechange_public_and_archives_upgrade_without_rewriting_rows(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let pool = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let mut previous = sqlx::migrate!("../../migrations");
    previous.migrations = std::borrow::Cow::Owned(
        previous
            .iter()
            .filter(|migration| migration.version < 20261015020000)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    let db = Database::new(pool.clone());
    let archive = db
        .archives
        .create_archive_schema("prechange-dimensions", None)
        .await
        .unwrap();
    // Current archives share configs; also exercise older archives with a local config table.
    sqlx::query(&format!(
        "CREATE TABLE {}.embedding_config (LIKE public.embedding_config INCLUDING ALL)",
        archive.schema_name
    ))
    .execute(&pool)
    .await
    .unwrap();
    let mut snapshots = Vec::new();
    for schema in ["public", archive.schema_name.as_str()] {
        let config = Uuid::new_v4();
        sqlx::query(&format!(
            "INSERT INTO {schema}.embedding_config(id,name,model,dimension)
            VALUES($1,$2,'synthetic',512)"
        ))
        .bind(config)
        .bind(format!("prechange-{schema}"))
        .execute(&pool)
        .await
        .unwrap();
        let context = db.for_schema(schema).unwrap();
        let mut tx = context.begin_tx().await.unwrap();
        let note = db
            .notes
            .insert_tx(
                &mut tx,
                matric_core::CreateNoteRequest {
                    content: "Synthetic prechange document".into(),
                    format: "markdown".into(),
                    source: "test".into(),
                    collection_id: None,
                    document_type_id: None,
                    title: None,
                    tags: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();
        let set: Uuid = sqlx::query_scalar("SELECT id FROM embedding_set WHERE slug='default'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO embedding(id,note_id,embedding_set_id,chunk_index,text,vector,model)
            VALUES($1,$2,$3,0,'prechange',$4,'synthetic')",
        )
        .bind(Uuid::new_v4())
        .bind(note)
        .bind(set)
        .bind(matric_core::Vector::from(vec![1.; 768]))
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        for (table, column) in [
            ("embedding_set", "query_template"),
            ("embedding_config", "provider_dimension"),
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM information_schema.columns
                WHERE table_schema=$1 AND table_name=$2 AND column_name=$3)",
            )
            .bind(schema)
            .bind(table)
            .bind(column)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(
                !exists,
                "{schema}.{table}.{column} must be absent before upgrade"
            );
        }
        snapshots.push((
            schema.to_string(),
            config,
            snapshot(&pool, schema, "embedding_config", "provider_dimension").await,
            snapshot(&pool, schema, "embedding_set", "query_template").await,
            snapshot(&pool, schema, "embedding", "unused_column").await,
            snapshot(&pool, schema, "note", "unused_column").await,
        ));
    }

    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    for (schema, config, configs, sets, embeddings, notes) in snapshots {
        assert_eq!(
            snapshot(&pool, &schema, "embedding_config", "provider_dimension").await,
            configs
        );
        assert_eq!(
            snapshot(&pool, &schema, "embedding_set", "query_template").await,
            sets
        );
        assert_eq!(
            snapshot(&pool, &schema, "embedding", "unused_column").await,
            embeddings
        );
        assert_eq!(
            snapshot(&pool, &schema, "note", "unused_column").await,
            notes
        );
        for (table, column) in [
            ("embedding_set", "query_template"),
            ("embedding_config", "provider_dimension"),
        ] {
            let nullable: String = sqlx::query_scalar(
                "SELECT is_nullable FROM information_schema.columns
                WHERE table_schema=$1 AND table_name=$2 AND column_name=$3",
            )
            .bind(&schema)
            .bind(table)
            .bind(column)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(nullable, "YES");
            let nulls: bool = sqlx::query_scalar(&format!(
                "SELECT bool_and({column} IS NULL) FROM {schema}.{table}"
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(nulls);
        }
        let validated: bool = sqlx::query_scalar(
            "SELECT convalidated FROM pg_constraint
            WHERE conrelid=$1::regclass AND conname='embedding_config_provider_dimension_check'",
        )
        .bind(format!("{schema}.embedding_config"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(validated);
        for raw in [511, 16385] {
            let error = sqlx::query(&format!(
                "UPDATE {schema}.embedding_config SET provider_dimension=$1 WHERE id=$2"
            ))
            .bind(raw)
            .bind(config)
            .execute(&pool)
            .await
            .unwrap_err();
            let error = error.as_database_error().unwrap();
            assert_eq!(error.code().as_deref(), Some("23514"));
            assert_eq!(
                error.constraint(),
                Some("embedding_config_provider_dimension_check")
            );
        }
        sqlx::query(&format!(
            "UPDATE {schema}.embedding_config SET provider_dimension=1024 WHERE id=$1"
        ))
        .bind(config)
        .execute(&pool)
        .await
        .unwrap();
        let error = sqlx::query(&format!(
            "UPDATE {schema}.embedding_config SET dimension=1536 WHERE id=$1"
        ))
        .bind(config)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("embedding_config_provider_dimension_check")
        );
    }
    let new = db
        .archives
        .create_archive_schema("postchange-dimensions", None)
        .await
        .unwrap();
    let ctx = db.for_schema(&new.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let template: Option<String> =
        sqlx::query_scalar("SELECT query_template FROM embedding_set WHERE slug='default'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(template.is_none());
    // Configs resolve through public in new archives and retain the provider constraint.
    let raw: Option<i32> =
        sqlx::query_scalar("SELECT provider_dimension FROM embedding_config WHERE is_default")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(raw.is_none());
    tx.rollback().await.unwrap();
    pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn dimension_only_api_update_cannot_exceed_existing_raw_dimension(admin: sqlx::PgPool) {
    let db = embedding_dimension_tests::database(admin).await;
    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    let app = Router::new()
        .route("/api/v1/embedding-configs", post(create_embedding_config))
        .route(
            "/api/v1/embedding-configs/{id}",
            patch(update_embedding_config),
        )
        .with_state(state);
    let (status,created) = response(&app,Method::POST,"/api/v1/embedding-configs",json!({
        "name":"raw-dimension-update","model":"synthetic","dimension":512,"provider_dimension":1024
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id: Uuid = serde_json::from_value(created["id"].clone()).unwrap();
    let (status, body) = response(
        &app,
        Method::PATCH,
        &format!("/api/v1/embedding-configs/{id}"),
        json!({"dimension":1536}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["status"], 400);
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .contains("provider_dimension"));
    let dimensions: (i32, Option<i32>) =
        sqlx::query_as("SELECT dimension,provider_dimension FROM embedding_config WHERE id=$1")
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(dimensions, (512, Some(1024)));
    db.pool.close().await;
}

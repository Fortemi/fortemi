//! Community startup and HTTP cloning of the shipped archive layout.
use super::*;
use sqlx::ConnectOptions;
use tower::ServiceExt;

use super::legacy_archive;

#[sqlx::test(migrations = false)]
async fn community_startup_clones_release_archives_without_copying_shared_identity_tables(
    admin: sqlx::PgPool,
) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let release = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261014000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    release.run(&admin).await.unwrap();
    let url = admin.connect_options().to_url_lossy();
    let local = matric_db::create_pool(url.as_str()).await.unwrap();
    let user = Uuid::new_v4();
    let pat = Uuid::new_v4();
    let mut sources = Vec::new();
    for populated in [false, true] {
        let name = if populated {
            "community_populated_legacy"
        } else {
            "community_empty_legacy"
        };
        let source = legacy_archive::PgArchiveRepository::new(local.clone())
            .create_archive_schema(name, None)
            .await
            .unwrap();
        if populated {
            sqlx::query(&format!("INSERT INTO {}.app_user(id,iss,sub,kind,status) VALUES($1,'https://legacy.example','community-clone','user','disabled')", source.schema_name))
            .bind(user).execute(&local).await.unwrap();
            sqlx::query(&format!("INSERT INTO {}.personal_access_token(id,user_id,token_hash,token_prefix,token_last4,name,status,expires_at) VALUES($1,$2,repeat('b',64),'mm_pat_fixture','1234','Community legacy','suspended','2030-01-01')", source.schema_name))
            .bind(pat).bind(user).execute(&local).await.unwrap();
        }
        let notes = matric_db::PgNoteRepository::new(local.clone());
        let note = Database::new(local.clone())
            .for_schema(&source.schema_name)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    notes
                        .insert_tx(
                            tx,
                            CreateNoteRequest {
                                content: "Community legacy content survives cloning".into(),
                                format: "markdown".into(),
                                source: "release-fixture".into(),
                                collection_id: None,
                                tags: None,
                                metadata: None,
                                document_type_id: None,
                                title: None,
                            },
                        )
                        .await
                })
            })
            .await
            .unwrap();
        sources.push((source, note, populated));
    }
    let db = startup::database(url.as_str(), None, startup::Tenancy::Community)
        .await
        .unwrap();
    let state =
        super::super::tests::build_call_api_test_state(db.clone(), "community-upgrade").await;
    let app = super::production_router(state, None);
    for (source, note, populated) in &sources {
        let target = if *populated {
            "community_populated_clone"
        } else {
            "community_empty_clone"
        };
        let response = app
            .clone()
            .oneshot(http_request(
                Method::POST,
                &format!("/api/v1/archives/{}/clone", source.name),
                serde_json::json!({"new_name": target}),
                false,
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = json_body(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let schema = body["schema_name"].as_str().unwrap();
        let copied: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.note WHERE id=$1)"
        ))
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert!(copied, "legacy note must be copied with its UUID");
        for table in ["app_user", "personal_access_token"] {
            let source_exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
                .bind(format!("{}.{table}", source.schema_name))
                .fetch_one(&admin)
                .await
                .unwrap();
            assert!(source_exists, "community must preserve legacy {table}");
            let target_absent: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NULL")
                .bind(format!("{schema}.{table}"))
                .fetch_one(&admin)
                .await
                .unwrap();
            assert!(target_absent, "shared {table} must not be cloned");
        }
    }
    let populated_schema = &sources[1].0.schema_name;
    let preserved: bool = sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {populated_schema}.app_user WHERE id=$1 AND status='disabled') AND EXISTS(SELECT 1 FROM {populated_schema}.personal_access_token WHERE id=$2 AND user_id=$1 AND status='suspended')"))
        .bind(user).bind(pat).fetch_one(&admin).await.unwrap();
    assert!(
        preserved,
        "cloning must leave legacy identities and credentials intact"
    );
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM external_oidc_ownership")
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(receipts, 0, "community startup must not adopt content");
    drop(app);
    db.pool.close().await;
    local.close().await;
}

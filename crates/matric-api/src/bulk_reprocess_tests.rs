// Live PostgreSQL regression coverage for bulk reprocessing eligibility (#1134).

#[tokio::test]
async fn bulk_reprocess_filters_live_ids_in_selected_tenant_and_archive() {
    let _guard = MANUAL_LINK_HOSTED_TEST_LOCK.lock().await;
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            assert_ne!(
                std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
                Ok("1")
            );
            eprintln!("skipping bulk reprocess PostgreSQL test: DATABASE_URL unavailable");
            return;
        }
    };
    let (admin, runtime) = hosted_manual_link_test_pools().await.unwrap();
    let db = Database::new(admin.clone());
    let archive = db
        .archives
        .create_archive_schema(&format!("bulk-{}", Uuid::new_v4().simple()), None)
        .await
        .unwrap();
    let schema = &archive.schema_name;
    sqlx::raw_sql(&format!(
        "GRANT USAGE ON SCHEMA {schema} TO fortemi_manual_link_hosted_test;
         GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA {schema} TO fortemi_manual_link_hosted_test;"
    )).execute(&admin).await.unwrap();
    // This is a real non-superuser, non-BYPASSRLS connection pool. Personal
    // mode's local tenant is bound by create_pool on every connection.
    let options = database_url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .username("fortemi_manual_link_hosted_test")
        .password("fortemi-manual-link-hosted-test-only");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET app.current_tenant = '00000000-0000-0000-0000-000000000000'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .unwrap();
    matric_db::assert_hosted_runtime_role(&pool).await.unwrap();
    let other_tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')")
        .bind(other_tenant).bind(format!("bulk-{other_tenant}"))
        .execute(&admin).await.unwrap();
    let mut live = Vec::new();
    for i in 0..102 {
        live.push(create_asset_lifecycle_note(&db, schema, &format!("bulk {i}")).await);
    }
    let deleted = create_asset_lifecycle_note(&db, schema, "deleted").await;
    let foreign = create_asset_lifecycle_note(&db, "public", "other archive").await;
    let hidden = Uuid::new_v4();
    sqlx::query(&format!(
        "UPDATE {schema}.note SET deleted_at = now() WHERE id = $1"
    ))
    .bind(deleted)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "UPDATE {schema}.note SET archived = true WHERE id = $1"
    ))
    .bind(live[0])
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("INSERT INTO {schema}.note (id, tenant_id, format, source, created_at_utc, updated_at_utc) VALUES ($1, $2, 'markdown', 'bulk-test', now(), now())"))
        .bind(hidden).bind(other_tenant).execute(&admin).await.unwrap();

    // Also verify the repository using an explicit hosted tenant transaction.
    let notes = matric_db::PgNoteRepository::new(runtime.clone());
    let mut scoped = matric_db::TenantScopedConn::begin(&runtime, other_tenant)
        .await
        .unwrap();
    sqlx::query(&format!("SET LOCAL search_path TO {schema}, public"))
        .execute(scoped.executor())
        .await
        .unwrap();
    assert_eq!(
        notes
            .live_ids_tx(scoped.executor(), &[live[0], hidden, deleted, foreign])
            .await
            .unwrap(),
        vec![hidden]
    );
    scoped.rollback().await.unwrap();

    let state = build_call_api_test_state(Database::new(pool.clone()), &database_url).await;
    let router = Router::new()
        .route("/api/v1/notes/reprocess", post(bulk_reprocess_notes))
        .layer(Extension(ArchiveContext {
            schema: schema.clone(),
            is_default: false,
            name: Some(archive.name.clone()),
        }))
        .with_state(state);
    use tower::ServiceExt;
    let capped_ids: Vec<Uuid> = std::iter::repeat_n(deleted, 5000)
        .chain([live[1]])
        .collect();
    let cases = [
        (
            serde_json::json!({"note_ids": [live[0], deleted, Uuid::new_v4(), foreign, hidden], "steps": ["embedding"]}),
            1,
            1,
        ),
        (serde_json::json!({"note_ids": [deleted]}), 0, 0),
        (
            serde_json::json!({"note_ids": [Uuid::new_v4(), foreign, hidden]}),
            0,
            0,
        ),
        (
            serde_json::json!({"note_ids": [deleted, live[1]], "limit": 1}),
            0,
            0,
        ),
        (serde_json::json!({"note_ids": [live[1]], "limit": 0}), 0, 0),
        (
            serde_json::json!({"note_ids": [live[1]], "limit": -1}),
            0,
            0,
        ),
        (
            serde_json::json!({"note_ids": [], "steps": ["embedding"]}),
            0,
            0,
        ),
        (
            serde_json::json!({"note_ids": [live[0]], "steps": ["embedding"]}),
            1,
            0,
        ),
        (
            serde_json::json!({"note_ids": [live[0], live[0]], "steps": ["embedding"]}),
            2,
            0,
        ),
        (
            serde_json::json!({"note_ids": capped_ids, "limit": 5001}),
            0,
            0,
        ),
        (
            serde_json::json!({"steps": ["embedding"], "limit": 101}),
            101,
            101,
        ),
    ];
    for (body, expected_notes, expected_jobs) in cases {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/notes/reprocess")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "request: {body}");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["notes_count"], expected_notes, "request: {body}");
        assert_eq!(result["jobs_queued"], expected_jobs, "request: {body}");
    }
    let invalid_jobs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE note_id = ANY($1)")
            .bind(vec![deleted, hidden, foreign])
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(invalid_jobs, 0);
    let accepted_jobs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE note_id = ANY($1)")
            .bind(&live)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(accepted_jobs, 102);
    // A deletion after preflight is still possible; eligibility is a snapshot,
    // and the normal worker's missing/deleted-note handling remains necessary.
    sqlx::query(&format!(
        "UPDATE {schema}.note SET deleted_at = now() WHERE id = $1"
    ))
    .bind(live[0])
    .execute(&admin)
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM job_queue WHERE note_id = $1")
            .bind(live[0])
            .fetch_one(&admin)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DELETE FROM job_queue WHERE note_id = ANY($1)")
        .bind(&live)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM archive_registry WHERE schema_name = $1")
        .bind(schema)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM note WHERE id = $1")
        .bind(foreign)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenant_registry WHERE id = $1")
        .bind(other_tenant)
        .execute(&admin)
        .await
        .unwrap();
    pool.close().await;
}

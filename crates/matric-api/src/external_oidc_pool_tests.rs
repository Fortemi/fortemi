//! Bounded production regressions for admission and streaming with one connection.

use super::*;
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(12);

async fn one_connection_router(admin: &sqlx::PgPool) -> (Router, sqlx::PgPool, Uuid) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let tenant = Uuid::new_v4();
    let url = runtime_url(admin, tenant).await;
    let initial = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    initial.pool.close().await;
    let runtime = matric_db::create_pool_with_config(
        &url,
        PoolConfig::new()
            .single_tenant(tenant)
            .max_connections(1)
            .connect_timeout(Duration::from_secs(3)),
    )
    .await
    .unwrap();
    let db = Database::new(runtime.clone())
        .with_administrative_pool(admin.clone())
        .await
        .unwrap();
    let (app, runtime) = production_router_with_database(db, tenant).await;
    (app, runtime, tenant)
}

async fn released(runtime: &sqlx::PgPool, tenant: Uuid) {
    let admitted: String = tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query_scalar("SELECT current_setting('app.current_tenant')").fetch_one(runtime),
    )
    .await
    .expect("response must release the only connection before body consumption")
    .unwrap();
    assert_eq!(admitted, tenant.to_string());
}

#[sqlx::test(migrations = false)]
async fn external_oidc_one_connection_concurrent_existing_note_gets(admin: sqlx::PgPool) {
    let (app, runtime, tenant) = one_connection_router(&admin).await;
    let (status, archive) = call(
        &app,
        Method::POST,
        "/api/v1/archives",
        serde_json::json!({"name":"pool_admission"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{archive}");
    let schema = archive["schema_name"].as_str().unwrap();
    let (status, note) = call(
        &app,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"Existing pool regression note","pipeline":[]}),
        Some("pool_admission"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{note}");
    let id = note["id"].as_str().unwrap();
    for outdated in [false, true] {
        if outdated {
            sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='pool_admission'")
                .execute(&admin)
                .await
                .unwrap();
            sqlx::query(&format!("DROP TABLE {schema}.note_template"))
                .execute(&admin)
                .await
                .unwrap();
        }
        let barrier = Arc::new(tokio::sync::Barrier::new(9));
        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let app = app.clone();
            let barrier = barrier.clone();
            // Sign before the barrier so signing time cannot serialize admission.
            let request = oidc_request(
                Method::GET,
                &format!("/api/v1/notes/{id}"),
                serde_json::Value::Null,
                Some("pool_admission"),
            );
            requests.spawn(async move {
                barrier.wait().await;
                let response = app.oneshot(request).await.unwrap();
                (response.status(), json_body(response).await)
            });
        }
        tokio::time::timeout(BOUND, async {
            barrier.wait().await;
            while let Some(result) = requests.join_next().await {
                let (status, body) = result.unwrap();
                assert_eq!(status, StatusCode::OK, "outdated={outdated}: {body}");
            }
        })
        .await
        .expect("synchronized GET requests must not exhaust their own pool");
        released(&runtime, tenant).await;
        let present: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.note_template"))
            .fetch_one(&admin)
            .await
            .unwrap();
        assert!(present, "outdated archive must be repaired");
    }
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event WHERE tenant_id=$1 AND resource_id=$2 AND outcome='Success'",
    ).bind(tenant).bind(id).fetch_one(&admin).await.unwrap();
    assert!(
        audited >= 16,
        "each admitted GET must have a durable authorization audit"
    );

    // Maintenance can remove a resource after admission. Revalidation must deny
    // on the handler connection and release it before auditing that denial.
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION public.remove_pool_note() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN DELETE FROM {schema}.note WHERE id='{id}'; RETURN NEW; END $$;
         CREATE TRIGGER remove_pool_note AFTER UPDATE OF schema_version ON public.archive_registry
         FOR EACH ROW WHEN (NEW.name='pool_admission' AND NEW.schema_version>OLD.schema_version)
         EXECUTE FUNCTION public.remove_pool_note();"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='pool_admission'")
        .execute(&admin)
        .await
        .unwrap();
    let (status, body) = tokio::time::timeout(
        BOUND,
        call(
            &app,
            Method::GET,
            &format!("/api/v1/notes/{id}"),
            serde_json::Value::Null,
            Some("pool_admission"),
        ),
    )
    .await
    .expect("revalidation denial must not nest an audit checkout");
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let denied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM audit_event WHERE tenant_id=$1 AND resource_id=$2 AND outcome='Denied')",
    ).bind(tenant).bind(id).fetch_one(&admin).await.unwrap();
    assert!(denied, "revalidation denial must remain durably audited");
    released(&runtime, tenant).await;
    drop(app);
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_one_connection_streaming_and_long_responses(admin: sqlx::PgPool) {
    use futures::StreamExt;
    let (app, runtime, tenant) = one_connection_router(&admin).await;
    let response = tokio::time::timeout(BOUND, app.clone().oneshot(oidc_request(
        Method::POST, "/api/v1/inference/stream",
        serde_json::json!({"provider_id":"ollama","model":"synthetic","messages":[{"role":"user","content":"Stream this"}]}), None,
    ))).await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    released(&runtime, tenant).await;
    let bytes = tokio::time::timeout(BOUND, axum::body::to_bytes(response.into_body(), 64 * 1024))
        .await
        .unwrap()
        .unwrap();
    let events = String::from_utf8(bytes.to_vec()).unwrap();
    assert_eq!(events.matches("event: delta").count(), 2, "{events}");
    assert!(
        events.contains("streamed-first") && events.contains("streamed-second"),
        "{events}"
    );
    assert!(events.contains("event: done"), "{events}");

    // /events is the only other SSE route on the reused qualification list.
    // A missing replay cursor produces an immediate event while leaving the
    // stream open; database access must remain available during that stream.
    let mut request = oidc_request(
        Method::GET,
        "/api/v1/events?memory=public&types=note.created",
        serde_json::Value::Null,
        None,
    );
    request
        .headers_mut()
        .insert("last-event-id", Uuid::new_v4().to_string().parse().unwrap());
    let response = tokio::time::timeout(BOUND, app.clone().oneshot(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    released(&runtime, tenant).await;
    let mut body = response.into_body().into_data_stream();
    let frame = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(std::str::from_utf8(&frame)
        .unwrap()
        .contains("event: resync_required"));
    released(&runtime, tenant).await;

    // The qualification list's two export routes return fully materialized
    // bodies. Exercise actual large responses to verify their transaction ends
    // before a slow client consumes the bytes.
    let (status, collection) = call(
        &app,
        Method::POST,
        "/api/v1/collections",
        serde_json::json!({"name":"long response"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{collection}");
    let collection = collection["id"].as_str().unwrap();
    let content = "long-export-marker ".repeat(65_000);
    for _ in 0..2 {
        let (status, note) = call(
            &app,
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":content,"collection_id":collection,"pipeline":[]}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{note}");
    }
    let frame = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(std::str::from_utf8(&frame)
        .unwrap()
        .contains("event: note.created"));
    released(&runtime, tenant).await;
    drop(body);
    for (method, path, request) in [
        (
            Method::POST,
            "/api/v1/memory/export".to_owned(),
            serde_json::json!({"entity_types":["note","note_original"]}),
        ),
        (
            Method::GET,
            format!("/api/v1/collections/{collection}/export?content=original"),
            serde_json::Value::Null,
        ),
    ] {
        let response = tokio::time::timeout(
            BOUND,
            app.clone()
                .oneshot(oidc_request(method, &path, request, None)),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        released(&runtime, tenant).await;
        let bytes = tokio::time::timeout(
            BOUND,
            axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            bytes.len() > 2 * 1024 * 1024,
            "{path} must exercise a large body"
        );
        assert!(std::str::from_utf8(&bytes)
            .unwrap()
            .contains("long-export-marker"));
    }
    drop(app);
    runtime.close().await;
}

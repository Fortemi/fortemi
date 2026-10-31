//! #1280: actual middleware, backing normalization, handler and FORCE RLS.
use super::*;

pub(super) struct GraphIdentity(pub(super) Uuid, pub(super) Uuid);
#[async_trait::async_trait]
impl HostedAuthenticator for GraphIdentity {
    async fn authenticate(
        &self,
        token: &str,
    ) -> Result<fortemi_auth_core::AuthContext, fortemi_auth_core::AuthError> {
        let mut context = FixtureIdentity(self.0, self.1)
            .authenticate(match token {
                "service-read" | "service-write" | "service-none" => "fixture-a",
                other => other,
            })
            .await?;
        if token.starts_with("service-") {
            context.principal_kind = fortemi_auth_core::PrincipalKind::Service;
            context.scopes = match token {
                "service-read" => vec!["read".into()],
                "service-write" => vec!["write".into()],
                _ => vec![],
            };
        }
        Ok(context)
    }
}

pub(super) fn app(state: AppState, runtime: PgPool) -> Router {
    Router::new()
        .route("/api/v1/graph/{id}", get(explore_graph))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            runtime,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

pub(super) async fn stale_graph(
    State(state): State<AppState>,
    scope: Option<Extension<TenantRequestScope>>,
    Extension(archive): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<GraphQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // This handler runs after real normalization/authorization. Simulate a root
    // deleted between the preliminary lookup and the handler's transaction.
    with_request_schema(&state, scope.clone().map(|Extension(s)| s), archive.schema.clone(), move |c| Box::pin(async move {
        sqlx::query("UPDATE note SET deleted_at=now() WHERE id=$1 AND tenant_id=current_setting('app.current_tenant')::uuid")
            .bind(id).execute(c).await.map_err(matric_core::Error::Database)?;
        Ok(())
    })).await?;
    explore_graph(
        State(state),
        scope,
        Extension(archive),
        Path(id),
        Query(query),
    )
    .await
}

pub(super) async fn request_raw(
    app: &Router,
    token: &str,
    memory: &str,
    id: &str,
    method: Method,
    query: &str,
) -> (StatusCode, Vec<u8>) {
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.clone().oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(format!("/api/v1/graph/{id}?{query}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header("X-Fortemi-Memory", memory)
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("graph must use its single request connection")
    .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1048576)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

pub(super) async fn request(
    app: &Router,
    token: &str,
    memory: &str,
    id: &str,
    method: Method,
    query: &str,
) -> (StatusCode, Value) {
    let (status, bytes) = request_raw(app, token, memory, id, method, query).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub(super) async fn seed_graph(db: &Database, schema: &str, tenant: Uuid) -> [Uuid; 4] {
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema}, public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    let mut ids = [Uuid::nil(); 4];
    for (index, id) in ids.iter_mut().enumerate() {
        *id = db
            .notes
            .insert_tx(
                &mut tx,
                CreateNoteRequest {
                    content: "synthetic graph".into(),
                    format: "markdown".into(),
                    source: "fixture".into(),
                    collection_id: None,
                    document_type_id: None,
                    title: Some(format!("graph-{index}")),
                    tags: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();
    }
    // Root -> live neighbor; root -> deleted bridge -> unreachable live note.
    for (from, to) in [(ids[0], ids[1]), (ids[0], ids[2]), (ids[2], ids[3])] {
        sqlx::query(
            "INSERT INTO link(id,from_note_id,to_note_id,score,kind,created_at_utc) VALUES($1,$2,$3,0.9,'semantic',now())",
        )
        .bind(Uuid::new_v4())
        .bind(from)
        .bind(to)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE note SET deleted_at=now() WHERE id=$1")
        .bind(ids[2])
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    ids
}

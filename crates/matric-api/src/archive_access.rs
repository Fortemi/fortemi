//! Trusted archive selection and visibility shared by HTTP and SSE (#1201).
use super::*;
use matric_core::{AuthzContext, Resource};

#[derive(Clone)]
pub(crate) struct Caller {
    pub principal: AuthPrincipal,
    pub tenant_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

impl Caller {
    pub fn from_extensions(extensions: &axum::http::Extensions) -> Self {
        Self {
            principal: extensions
                .get::<Auth>()
                .map(|auth| auth.principal.clone())
                .unwrap_or(AuthPrincipal::Anonymous),
            tenant_id: extensions
                .get::<VerifiedRequestTenant>()
                .map(|tenant| tenant.tenant_id()),
            user_id: extensions
                .get::<ValidatedBearerIdentity>()
                .and_then(sse::stable_user_id),
        }
    }

    pub async fn allows(&self, policy: &dyn AuthorizationPolicy, name: &str, schema: &str) -> bool {
        let (archive, context) = policy_input(self.tenant_id, self.user_id, name, schema);
        policy
            .may_select_archive(&self.principal, &archive, &context)
            .await
            .unwrap_or(false)
    }

    pub async fn require(
        &self,
        state: &AppState,
        archive: &matric_core::ArchiveInfo,
    ) -> Result<(), ApiError> {
        if self
            .allows(
                state.authorization_policy.as_ref(),
                &archive.name,
                &archive.schema_name,
            )
            .await
        {
            Ok(())
        } else {
            Err(not_found())
        }
    }
}

/// A complete catalog includes the built-in public archive even before it is
/// registered. Errors deny access, including when used for event delivery.
pub(crate) async fn complete_catalog(
    db: &Database,
    caller: &Caller,
    policy: &dyn AuthorizationPolicy,
) -> bool {
    if policy.permits_complete_archive_catalog() {
        return true;
    }
    let Ok(mut tx) = db.pool.begin().await else {
        return false;
    };
    if let Some(tenant) = caller.tenant_id {
        if sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
            .bind(tenant.to_string())
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return false;
        }
    }
    let archives = sqlx::query_as::<_, (String, String)>(
        "SELECT name,schema_name FROM public.archive_registry WHERE tenant_id=current_setting('app.current_tenant')::uuid ORDER BY created_at DESC"
    ).fetch_all(&mut *tx).await;
    if tx.rollback().await.is_err() {
        return false;
    }
    let Ok(archives) = archives else {
        return false;
    };
    let public_registered = archives.iter().any(|(_, schema)| schema == "public");
    for (name, schema) in archives {
        if !caller.allows(policy, &name, &schema).await {
            return false;
        }
    }
    public_registered || caller.allows(policy, "public", "public").await
}

impl Caller {
    pub async fn require_complete_catalog(&self, state: &AppState) -> Result<(), ApiError> {
        if complete_catalog(&state.db, self, state.authorization_policy.as_ref()).await {
            Ok(())
        } else {
            Err(ApiError::Forbidden(
                "Complete-catalog access is required for this operation.".into(),
            ))
        }
    }

    /// Resolve persisted metadata, never constructing authority from a raw schema.
    pub async fn resolve_schema(
        &self,
        state: &AppState,
        schema: &str,
    ) -> Result<ArchiveContext, ApiError> {
        let archives = catalog(state, None).await?;
        let context = if let Some(archive) = archives
            .iter()
            .find(|archive| archive.schema_name == schema)
        {
            ArchiveContext::from(archive)
        } else if schema == "public" {
            ArchiveContext::default()
        } else {
            return Err(not_found());
        };
        if self
            .allows(
                state.authorization_policy.as_ref(),
                context.name.as_deref().unwrap_or("public"),
                &context.schema,
            )
            .await
        {
            Ok(context)
        } else {
            Err(not_found())
        }
    }
}

/// Body-selected federated search targets use the same visibility decision.
pub(crate) async fn search_targets(
    state: &AppState,
    caller: &Caller,
    names: &[String],
) -> Result<Vec<(String, String)>, ApiError> {
    let all = names == ["all"];
    let mut catalog: Vec<_> = catalog(state, None)
        .await?
        .into_iter()
        .map(|archive| (archive.name, archive.schema_name))
        .collect();
    if !catalog.iter().any(|(name, _)| name == "public") {
        catalog.push(("public".into(), "public".into()));
    }
    let candidates = if all {
        let mut candidates = vec![("public".into(), "public".into())];
        candidates.extend(catalog);
        candidates
    } else {
        names
            .iter()
            .map(|name| {
                catalog
                    .iter()
                    .find(|(known, _)| known == name)
                    .cloned()
                    .ok_or_else(not_found)
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, schema) in candidates {
        if caller
            .allows(state.authorization_policy.as_ref(), &name, &schema)
            .await
        {
            if seen.insert(schema.clone()) {
                selected.push((name, schema));
            }
        } else if !all {
            return Err(not_found());
        }
    }
    Ok(selected)
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Caller {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_extensions(&parts.extensions))
    }
}

pub(crate) fn policy_input(
    tenant: Option<Uuid>,
    user: Option<Uuid>,
    name: &str,
    schema: &str,
) -> (Resource, AuthzContext) {
    let mut archive = Resource::new(ResourceKind::Archive).with_id(name);
    archive.tenant_id = tenant.map(|id| id.to_string());
    archive
        .attrs
        .insert("archive_name".into(), serde_json::json!(name));
    archive
        .attrs
        .insert("archive_schema".into(), serde_json::json!(schema));
    let mut context = tenant.map_or_else(AuthzContext::personal, |id| {
        AuthzContext::hosted(id.to_string())
    });
    context
        .environment
        .insert("archive_name".into(), serde_json::json!(name));
    context
        .environment
        .insert("archive_schema".into(), serde_json::json!(schema));
    if let Some(user) = user {
        context
            .environment
            .insert("app_user_id".into(), serde_json::json!(user));
    }
    (archive, context)
}

pub(crate) fn not_found() -> ApiError {
    ApiError::NotFound("Requested memory is not present or not visible to the caller.".into())
}

pub(crate) async fn enforce(
    state: &AppState,
    extensions: &axum::http::Extensions,
    archive: &ArchiveContext,
) -> Result<(), ApiError> {
    if let Some(scope) = extensions.get::<TenantRequestScope>() {
        scope.release_preliminary().await?;
    }
    let caller = Caller::from_extensions(extensions);
    if caller
        .allows(
            state.authorization_policy.as_ref(),
            archive.name.as_deref().unwrap_or("public"),
            &archive.schema,
        )
        .await
    {
        Ok(())
    } else {
        Err(not_found())
    }
}

pub(crate) fn is_inventory_or_named_path(path: &str) -> bool {
    route_policy::route_policy_for_path(path).is_some_and(|policy| {
        matches!(
            policy.path,
            "/api/v1/search/federated"
                | "/api/v1/jobs/pause/{archive}"
                | "/api/v1/jobs/resume/{archive}"
                | "/api/v1/backup/memory/{name}"
                | "/api/v1/backup/database/restore"
        ) || (policy.action_family == "memory_management"
            && (policy.path.contains("{name}")
                || matches!(
                    policy.path,
                    "/api/v1/archives"
                        | "/api/v1/memories"
                        | "/api/v1/memories/overview"
                        | "/api/v1/memories/available"
                )))
    })
}

/// Resolve path targets before route policy evaluation; neither denied nor
/// missing targets reach normalization, maintenance, audit, or the handler.
pub(crate) async fn path_target(
    state: &AppState,
    extensions: &axum::http::Extensions,
    input: &route_policy::RoutePolicyInput,
) -> Result<Option<ArchiveContext>, ApiError> {
    if !(input.policy.action_family == "memory_management" && input.policy.path.contains("{name}"))
        && input.policy.path != "/api/v1/backup/memory/{name}"
    {
        return Ok(None);
    }
    let name = urlencoding::decode(input.resource.id.as_deref().ok_or_else(not_found)?)
        .map_err(|_| not_found())?
        .into_owned();
    let archive = if let Some(scope) = extensions.get::<TenantRequestScope>() {
        scope.with_connection(move |connection| Box::pin(async move {
            sqlx::query_as::<_, matric_core::ArchiveInfo>("SELECT id,name,schema_name,description,created_at,last_accessed,note_count,size_bytes,is_default,schema_version FROM public.archive_registry WHERE name=$1 AND tenant_id=current_setting('app.current_tenant')::uuid")
                .bind(name).fetch_optional(connection).await.map_err(matric_core::Error::Database)
        })).await?
    } else {
        state.db.archives.get_archive_by_name(&name).await?
    }.ok_or_else(not_found)?;
    if let Some(scope) = extensions.get::<TenantRequestScope>() {
        scope.release_preliminary().await?;
    }
    let context = ArchiveContext::from(&archive);
    enforce(state, extensions, &context).await?;
    Ok(Some(context))
}

/// Read metadata before statistics or schema maintenance. The tenant resolver's
/// connection is released before enterprise policies may access their store.
pub(crate) async fn catalog(
    state: &AppState,
    scope: Option<&TenantRequestScope>,
) -> Result<Vec<matric_core::ArchiveInfo>, ApiError> {
    const SQL: &str = "SELECT id,name,schema_name,description,created_at,last_accessed,note_count,size_bytes,is_default,schema_version FROM public.archive_registry WHERE tenant_id=current_setting('app.current_tenant')::uuid ORDER BY created_at DESC";
    if let Some(scope) = scope {
        let archives = scope
            .with_connection(|connection| {
                Box::pin(async move {
                    sqlx::query_as(SQL)
                        .fetch_all(connection)
                        .await
                        .map_err(matric_core::Error::Database)
                })
            })
            .await?;
        scope.release_preliminary().await?;
        Ok(archives)
    } else {
        sqlx::query_as(SQL)
            .fetch_all(&state.db.pool)
            .await
            .map_err(matric_core::Error::Database)
            .map_err(ApiError::from)
    }
}

pub(crate) async fn visible(
    state: &AppState,
    caller: &Caller,
    scope: Option<&TenantRequestScope>,
) -> Result<Vec<matric_core::ArchiveInfo>, ApiError> {
    let mut visible = Vec::new();
    for archive in catalog(state, scope).await? {
        if caller
            .allows(
                state.authorization_policy.as_ref(),
                &archive.name,
                &archive.schema_name,
            )
            .await
        {
            visible.push(archive);
        }
    }
    Ok(visible)
}

/// Preserve inventory's live statistics without changing registry timestamps.
/// Call only after filtering, so denied schemas are never touched for statistics.
pub(crate) async fn load_stats(
    state: &AppState,
    archives: &mut [matric_core::ArchiveInfo],
) -> Result<(), ApiError> {
    for archive in archives {
        let schema = &archive.schema_name;
        matric_db::validate_schema_name(schema)?;
        if let Ok(count) = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM {schema}.note WHERE tenant_id=current_setting('app.current_tenant')::uuid AND deleted_at IS NULL"
        )).fetch_one(&state.db.pool).await {
            archive.note_count = Some(count as i32);
        }
        if let Ok(size) = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT pg_total_relation_size('{schema}.note'::regclass) + pg_total_relation_size('{schema}.embedding'::regclass)"
        )).fetch_one(&state.db.pool).await {
            archive.size_bytes = Some(size);
        }
    }
    Ok(())
}

/// Statistics are collected only after each archive passes the visibility hook.
pub(crate) async fn overview(
    state: AppState,
    caller: Caller,
    scope: Option<TenantRequestScope>,
) -> Result<Json<MemoriesOverviewResponse>, ApiError> {
    let catalog = archive_access::catalog(&state, scope.as_ref()).await?;
    let has_default = catalog.iter().any(|archive| archive.is_default);
    let public_registered = catalog
        .iter()
        .any(|archive| archive.schema_name == "public");
    let catalog_count = catalog.len();
    let mut archives = Vec::new();
    for archive in catalog {
        if caller
            .allows(
                state.authorization_policy.as_ref(),
                &archive.name,
                &archive.schema_name,
            )
            .await
        {
            archives.push(archive);
        }
    }

    load_stats(&state, &mut archives).await?;

    let public_visible = !public_registered
        && caller
            .allows(state.authorization_policy.as_ref(), "public", "public")
            .await;

    // Count notes in public schema
    let public_notes: i64 = if public_visible {
        sqlx::query_scalar("SELECT COUNT(*) FROM public.note WHERE deleted_at IS NULL")
            .fetch_one(state.db.pool())
            .await
            .map_err(matric_core::Error::Database)?
    } else {
        0
    };

    let public_size: i64 = if public_visible {
        sqlx::query_scalar(
        "SELECT pg_total_relation_size('public.note'::regclass) + pg_total_relation_size('public.embedding'::regclass)",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0)
    } else {
        0
    };

    let mut total_notes: i64 = public_notes;
    let mut total_size: i64 = public_size;
    let mut breakdowns = if public_visible {
        vec![MemoryBreakdown {
            name: "public".to_string(),
            note_count: public_notes as i32,
            size_bytes: public_size,
            is_default: !has_default,
            created_at: chrono::Utc::now(), // public has no creation time
        }]
    } else {
        Vec::new()
    };

    for archive in &archives {
        let notes = archive.note_count.unwrap_or(0);
        let size = archive.size_bytes.unwrap_or(0);
        total_notes += notes as i64;
        total_size += size;
        breakdowns.push(MemoryBreakdown {
            name: archive.name.clone(),
            note_count: notes,
            size_bytes: size,
            is_default: archive.is_default,
            created_at: archive.created_at,
        });
    }

    // Sort by note_count descending
    breakdowns.sort_by_key(|breakdown| std::cmp::Reverse(breakdown.note_count));

    let memory_count = archives.len() as i64;
    let remaining = (state.max_memories - memory_count).max(0);

    // Get total database size on disk
    let db_size: i64 = if (public_registered || public_visible) && archives.len() == catalog_count {
        sqlx::query_scalar("SELECT pg_database_size(current_database())")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0)
    } else {
        total_size
    };

    Ok(Json(MemoriesOverviewResponse {
        memory_count,
        max_memories: state.max_memories,
        remaining_slots: remaining,
        total_notes,
        total_size_bytes: total_size,
        total_size_human: format_size(total_size as u64),
        database_size_bytes: db_size,
        database_size_human: format_size(db_size as u64),
        memories: breakdowns,
    }))
}

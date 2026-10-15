//! Event ownership follows admitted request scope or durable job/archive rows.

use axum::{extract::FromRequestParts, http::request::Parts};
use matric_core::{EventBus, EventContext, JobType, ServerEvent};
use matric_db::Database;
use std::sync::Arc;
use uuid::Uuid;

use crate::{AppState, ArchiveContext, TenantRequestScope, VerifiedRequestTenant};

/// Captures extensions before a handler moves its database scope. Scoped
/// publications are discarded on rollback and enter live delivery and replay
/// only after the middleware has successfully committed the request.
pub struct RequestEvents {
    scope: Option<TenantRequestScope>,
    tenant: Uuid,
    archive: ArchiveContext,
    schema_names: bool,
}

impl FromRequestParts<AppState> for RequestEvents {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let scope = parts.extensions.get::<TenantRequestScope>().cloned();
        let tenant = parts
            .extensions
            .get::<VerifiedRequestTenant>()
            .copied()
            .map(VerifiedRequestTenant::tenant_id)
            .or_else(|| scope.as_ref().map(|scope| scope.tenant().tenant_id()));
        // Accepted legacy external credentials address the startup-reconciled
        // deployment owner, even though they have no canonical bearer identity.
        #[cfg(feature = "hosted-auth")]
        let tenant = tenant.or_else(|| {
            state
                .external_oidc
                .as_ref()
                .filter(|_| state.auth_mode.external_oidc())
                .map(|config| config.default_tenant_id)
        });
        Ok(Self {
            scope,
            tenant: tenant.unwrap_or_else(Uuid::nil),
            archive: parts
                .extensions
                .get::<ArchiveContext>()
                .cloned()
                .unwrap_or_default(),
            schema_names: state.multi_tenant,
        })
    }
}

pub fn event_context_for(
    archive: &ArchiveContext,
    tenant: Uuid,
    schema_names: bool,
) -> EventContext {
    EventContext {
        tenant_id: Some(tenant.to_string()),
        memory: Some(if schema_names {
            archive.schema.clone()
        } else {
            archive.name.clone().unwrap_or_else(|| "public".into())
        }),
        archive_name: Some(archive.name.clone().unwrap_or_else(|| "public".into())),
        archive_schema: Some(archive.schema.clone()),
        ..Default::default()
    }
}

impl RequestEvents {
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    pub fn after_commit(&self, action: impl FnOnce() + Send + 'static) {
        if let Some(scope) = &self.scope {
            scope.after_commit(action);
        } else {
            action();
        }
    }

    pub fn publish(&self, bus: &Arc<EventBus>, event: ServerEvent) {
        self.publish_in(bus, event, &self.archive);
    }

    pub async fn publish_job(
        &self,
        db: &Database,
        bus: &Arc<EventBus>,
        job_id: Uuid,
        event: ServerEvent,
    ) {
        let ownership = if let Some(scope) = &self.scope {
            scope
                .with_connection(move |connection| {
                    Box::pin(async move { job_context_on(connection, job_id).await })
                })
                .await
        } else {
            job_context(db, job_id).await
        };
        if let Ok(Some(mut ownership)) = ownership {
            if self.schema_names {
                ownership.context.memory = Some(ownership.schema.clone());
            }
            let bus = bus.clone();
            self.after_commit(move || bus.emit_with_context(event, ownership.context));
        } else {
            tracing::warn!("Event publication skipped: durable job ownership unavailable");
        }
    }

    /// Archive lifecycle/queue controls target an archive other than the one
    /// selected by the request header. Callers pass its persisted metadata.
    pub fn publish_in(&self, bus: &Arc<EventBus>, event: ServerEvent, archive: &ArchiveContext) {
        let context = event_context_for(archive, self.tenant, self.schema_names);
        let bus = bus.clone();
        if let Some(scope) = &self.scope {
            scope.after_commit(move || bus.emit_with_context(event, context));
        } else {
            bus.emit_with_context(event, context);
        }
    }

    /// Deployment-wide configuration changes affect each owned memory. Read
    /// the registry on the request connection when one is already checked out.
    pub async fn publish_all(&self, db: &Database, bus: &Arc<EventBus>, event: ServerEvent) {
        let tenant = self.tenant;
        let archives = if let Some(scope) = &self.scope {
            scope
                .with_connection(move |connection| {
                    Box::pin(async move { owned_archives(connection, tenant).await })
                })
                .await
        } else {
            match db.pool.acquire().await {
                Ok(mut connection) => owned_archives(&mut connection, tenant).await,
                Err(error) => Err(error.into()),
            }
        };
        if let Ok(archives) = archives {
            for archive in archives {
                self.publish_in(bus, event.clone(), &archive);
            }
        }
    }
}

async fn owned_archives(
    connection: &mut sqlx::PgConnection,
    tenant: Uuid,
) -> matric_core::Result<Vec<ArchiveContext>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT name,schema_name FROM public.archive_registry WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_all(connection)
            .await?;
    let mut archives = vec![ArchiveContext::default()];
    for (name, schema) in rows {
        matric_db::validate_schema_name(&schema)?;
        if name != "public" || schema != "public" {
            archives.push(ArchiveContext {
                name: Some(name),
                schema,
                is_default: false,
            });
        }
    }
    Ok(archives)
}

/// Legacy background tasks run on the deployment-bound pool. An unbound
/// hosted pool cannot mint a tenant event; hosted claim events have their own
/// verified publisher. Never broadcast tenant data as an unowned system event.
async fn deployment_owner(db: &Database) -> matric_core::Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM public.tenant_registry
         WHERE id=NULLIF(current_setting('app.current_tenant',true),'')::uuid AND status='active'",
    )
    .fetch_optional(&db.pool)
    .await?)
}

pub async fn publish_system(db: &Database, bus: &EventBus, event: ServerEvent) {
    let Ok(Some(tenant)) = deployment_owner(db).await else {
        return;
    };
    let Ok(mut connection) = db.pool.acquire().await else {
        return;
    };
    let Ok(archives) = owned_archives(&mut connection, tenant).await else {
        return;
    };
    for archive in archives {
        bus.emit_with_context(event.clone(), event_context_for(&archive, tenant, false));
    }
}

pub async fn publish_queue_status(db: &Database, bus: &EventBus) {
    let Ok(Some(tenant)) = deployment_owner(db).await else {
        return;
    };
    let Ok(mut connection) = db.pool.acquire().await else {
        return;
    };
    let Ok(archives) = owned_archives(&mut connection, tenant).await else {
        return;
    };
    // Community monitors retain their global queue summary in every owned
    // memory's context. External streams receive statistics for the selected
    // memory, with the same tenant fence and queue_stats() eligibility.
    let community = tenant.is_nil();
    let supported_types: Vec<&str> = JobType::ALL.into_iter().map(JobType::as_str).collect();
    for archive in archives {
        let counts: Result<(i64, i64, i64), _> = sqlx::query_as(
            "SELECT count(*),
                    count(*) FILTER (WHERE status='running' AND job_type::text=ANY($4)),
                    count(*) FILTER (WHERE status='pending' AND job_type::text=ANY($4)
                                      AND (next_attempt_at IS NULL OR next_attempt_at<=NOW()))
             FROM public.job_queue WHERE tenant_id=$1
               AND ($3 OR COALESCE(payload->>'schema','public')=$2)",
        )
        .bind(tenant)
        .bind(&archive.schema)
        .bind(community)
        .bind(&supported_types)
        .fetch_one(&mut *connection)
        .await;
        if let Ok((total_jobs, running, pending)) = counts {
            bus.emit_with_context(
                ServerEvent::QueueStatus {
                    total_jobs,
                    running,
                    pending,
                },
                event_context_for(&archive, tenant, false),
            );
        }
    }
}

/// Resolve a legacy worker/queued job's ownership from its committed row.
/// Payload tenant hints never grant ownership; an archive must belong to the
/// row's tenant. Hosted workers already carry a fenced, committed claim.
pub struct PersistedJobContext {
    pub context: EventContext,
    pub tenant: Uuid,
    pub schema: String,
}

pub async fn job_context(
    db: &Database,
    job_id: Uuid,
) -> matric_core::Result<Option<PersistedJobContext>> {
    job_context_on(&mut *db.pool.acquire().await?, job_id).await
}

async fn job_context_on(
    connection: &mut sqlx::PgConnection,
    job_id: Uuid,
) -> matric_core::Result<Option<PersistedJobContext>> {
    let row: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT j.tenant_id, COALESCE(j.payload->>'schema','public'), a.name
         FROM public.job_queue j
         LEFT JOIN public.archive_registry a
           ON a.schema_name=COALESCE(j.payload->>'schema','public') AND a.tenant_id=j.tenant_id
         WHERE j.id=$1",
    )
    .bind(job_id)
    .fetch_optional(connection)
    .await?;
    let Some((tenant, schema, name)) = row else {
        return Ok(None);
    };
    matric_db::validate_schema_name(&schema)?;
    if schema != "public" && name.is_none() {
        return Err(matric_core::Error::InvalidInput(
            "Job archive ownership is unavailable".into(),
        ));
    }
    Ok(Some(PersistedJobContext {
        tenant,
        schema: schema.clone(),
        context: EventContext {
            tenant_id: Some(tenant.to_string()),
            memory: Some(name.clone().unwrap_or_else(|| "public".into())),
            archive_name: Some(name.unwrap_or_else(|| "public".into())),
            archive_schema: Some(schema),
            correlation_id: Some(job_id),
            ..Default::default()
        },
    }))
}

pub async fn publish_job(db: &Database, bus: &EventBus, job_id: Uuid, event: ServerEvent) {
    match job_context(db, job_id).await {
        Ok(Some(ownership)) => bus.emit_with_context(event, ownership.context),
        _ => tracing::warn!("Event publication skipped: durable job ownership unavailable"),
    }
}

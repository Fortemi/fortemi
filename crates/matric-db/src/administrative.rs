//! Narrow DDL capability. Callers must authorize requests or claim jobs first.
//! The owner pool is never exposed to repositories doing ordinary runtime DML.
use matric_core::{ArchiveInfo, ArchiveRepository, Error, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::{vector_index, Database, PgArchiveRepository};

#[derive(Clone)]
pub struct AdministrativeExecutor {
    pool: PgPool,
    runtime_role: String,
}

impl AdministrativeExecutor {
    pub async fn new(pool: PgPool, runtime: &PgPool) -> Result<Self> {
        let runtime_role = sqlx::query_scalar("SELECT current_user::text")
            .fetch_one(runtime)
            .await?;
        let capable: bool = sqlx::query_scalar(
            "SELECT (r.rolsuper OR r.rolbypassrls)
               AND has_database_privilege(current_database(),'CREATE')
               AND NOT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
                   WHERE (n.nspname='public' OR n.nspname LIKE 'archive\\_%' ESCAPE '\\')
                     AND c.relkind IN ('r','p')
                     AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.objid=c.oid AND d.deptype='e')
                     AND NOT r.rolsuper AND c.relowner<>r.oid)
             FROM pg_roles r WHERE r.rolname=current_user",
        )
        .fetch_one(&pool)
        .await?;
        if !capable {
            return Err(Error::Config("MIGRATION_DATABASE_URL must own application tables, have BYPASSRLS and database CREATE (or be superuser) for archive/index DDL".into()));
        }
        Ok(Self { pool, runtime_role })
    }

    // A separate short-lived pool prevents a session tenant from leaking between
    // concurrent administrative operations. Every connection binds the owner.
    async fn for_tenant(&self, tenant: Uuid) -> Result<PgPool> {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM public.tenant_registry WHERE id=$1 AND status='active')",
        )
        .bind(tenant)
        .fetch_one(&self.pool)
        .await?;
        if !active {
            return Err(Error::InvalidInput(
                "Administrative tenant is not active".into(),
            ));
        }
        Ok(PgPoolOptions::new().max_connections(2).after_connect(move |conn, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant',$1,false), set_config('search_path','public',false)")
                    .bind(tenant.to_string()).execute(conn).await?;
                Ok(())
            })
        }).connect_with(self.pool.connect_options().as_ref().clone()).await?)
    }

    async fn grant_runtime(&self, pool: &PgPool, schema: &str) -> Result<()> {
        let mut connection = pool.acquire().await?;
        self.grant_runtime_on(&mut connection, schema).await
    }

    async fn grant_runtime_on(
        &self,
        connection: &mut sqlx::PgConnection,
        schema: &str,
    ) -> Result<()> {
        crate::validate_schema_name(schema)?;
        let role = self.runtime_role.replace('"', "\"\"");
        let grants = format!("GRANT USAGE ON SCHEMA \"{schema}\" TO \"{role}\";
            GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA \"{schema}\" TO \"{role}\";
            GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA \"{schema}\" TO \"{role}\";
            GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA \"{schema}\" TO \"{role}\";
            ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO \"{role}\";
            ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" GRANT USAGE,SELECT ON SEQUENCES TO \"{role}\";");
        sqlx::Executor::execute(connection, grants.as_str()).await?;
        Ok(())
    }

    pub async fn create_archive(
        &self,
        tenant: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<ArchiveInfo> {
        let pool = self.for_tenant(tenant).await?;
        let repo = PgArchiveRepository::new(pool.clone());
        let result = async {
            let archive = repo.create_archive_schema(name, description).await?;
            if let Err(error) = self.grant_runtime(&pool, &archive.schema_name).await {
                repo.drop_archive_schema(name).await?;
                return Err(error);
            }
            Ok(archive)
        }
        .await;
        pool.close().await;
        result
    }

    pub async fn clone_archive(
        &self,
        tenant: Uuid,
        source: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<ArchiveInfo> {
        let pool = self.for_tenant(tenant).await?;
        let repo = PgArchiveRepository::new(pool.clone());
        let result = async {
            let archive = repo.clone_archive_schema(source, name, description).await?;
            if let Err(error) = self.grant_runtime(&pool, &archive.schema_name).await {
                repo.drop_archive_schema(name).await?;
                return Err(error);
            }
            Ok(archive)
        }
        .await;
        pool.close().await;
        result
    }

    pub async fn drop_archive(&self, tenant: Uuid, name: &str) -> Result<()> {
        let pool = self.for_tenant(tenant).await?;
        let result = PgArchiveRepository::new(pool.clone())
            .drop_archive_schema(name)
            .await;
        pool.close().await;
        result
    }

    pub async fn sync_archive(&self, tenant: Uuid, name: &str) -> Result<()> {
        let pool = self.for_tenant(tenant).await?;
        let result = async {
            let mut transaction = pool.begin().await?;
            PgArchiveRepository::sync_archive_schema_on(&mut transaction, name).await?;
            let schema: String = sqlx::query_scalar(
                "SELECT schema_name FROM public.archive_registry WHERE name=$1 AND tenant_id=$2",
            )
            .bind(name)
            .bind(tenant)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| Error::NotFound("Archive unavailable".into()))?;
            self.grant_runtime_on(&mut transaction, &schema).await?;
            transaction.commit().await?;
            Ok(())
        }
        .await;
        pool.close().await;
        result
    }

    pub async fn analyze_embedding(&self, tenant: Uuid, schema: &str) -> Result<()> {
        crate::validate_schema_name(schema)?;
        let pool = self.for_tenant(tenant).await?;
        let result = async {
            let visible: bool = schema == "public" || sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.archive_registry WHERE tenant_id=$1 AND schema_name=$2)")
                .bind(tenant).bind(schema).fetch_one(&pool).await?;
            if !visible {
                return Err(Error::InvalidInput("Analyze job archive does not belong to its tenant".into()));
            }
            sqlx::query(&format!("ANALYZE {schema}.embedding")).execute(&pool).await?;
            Ok(())
        }.await;
        pool.close().await;
        result
    }

    /// Called with a verified job claim's tenant, never a payload tenant field.
    pub async fn maintain_vector_indexes(
        &self,
        tenant: Uuid,
        payload: vector_index::VectorIndexJobPayload,
    ) -> Result<()> {
        crate::validate_schema_name(&payload.schema)?;
        let pool = self.for_tenant(tenant).await?;
        let result = async {
            let visible: bool = payload.schema == "public" || sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.archive_registry WHERE tenant_id=$1 AND schema_name=$2)")
                .bind(tenant).bind(&payload.schema).fetch_one(&pool).await?;
            if !visible {
                return Err(Error::InvalidInput("Index job archive does not belong to its tenant".into()));
            }
            vector_index::maintain_vector_indexes(&pool, payload).await
        }.await;
        pool.close().await;
        result
    }
}

impl Database {
    pub async fn with_administrative_pool(mut self, pool: PgPool) -> Result<Self> {
        self.administrative = Some(AdministrativeExecutor::new(pool, &self.pool).await?);
        Ok(self)
    }
    pub fn administrative(&self) -> Option<&AdministrativeExecutor> {
        self.administrative.as_ref()
    }

    pub async fn analyze_embedding_job(&self, job: Uuid, schema: &str) -> Result<()> {
        crate::validate_schema_name(schema)?;
        if let Some(admin) = &self.administrative {
            let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM public.job_queue WHERE id=$1 AND job_type='analyze_embedding' AND status='running' AND payload->>'schema'=$2")
                .bind(job).bind(schema).fetch_optional(&self.pool).await?
                .ok_or_else(|| Error::InvalidInput("Analyze job is not an authorized runtime claim".into()))?;
            admin.analyze_embedding(tenant, schema).await
        } else {
            sqlx::query(&format!("ANALYZE {schema}.embedding"))
                .execute(&self.pool)
                .await?;
            Ok(())
        }
    }

    /// Re-read the claimed row through runtime RLS. Payloads cannot nominate an
    /// owner or turn a job for another archive into an administrative capability.
    pub async fn build_vector_index_job(
        &self,
        job: Uuid,
        payload: vector_index::VectorIndexJobPayload,
    ) -> Result<()> {
        let Some(admin) = &self.administrative else {
            return vector_index::build_vector_indexes(&self.pool, payload).await;
        };
        let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM public.job_queue WHERE id=$1 AND job_type='build_set_index' AND status='running' AND payload->>'schema'=$2")
            .bind(job).bind(&payload.schema).fetch_optional(&self.pool).await?
            .ok_or_else(|| Error::InvalidInput("Vector index job is not an authorized runtime claim".into()))?;
        let shape = vector_index::VectorIndexShape::from_payload(payload.clone())?;
        // Validate scope before changing operational metadata.
        let visible: bool = payload.schema == "public" || sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.archive_registry WHERE tenant_id=$1 AND schema_name=$2)")
            .bind(tenant).bind(&payload.schema).fetch_one(&self.pool).await?;
        if !visible {
            return Err(Error::InvalidInput(
                "Index job archive does not belong to its tenant".into(),
            ));
        }
        vector_index::set_shape_status(&self.pool, &shape, "building").await?;
        let result = admin.maintain_vector_indexes(tenant, payload).await;
        vector_index::set_shape_status(
            &self.pool,
            &shape,
            if result.is_ok() { "ready" } else { "stale" },
        )
        .await?;
        result
    }
}

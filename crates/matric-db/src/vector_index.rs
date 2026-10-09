use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{validate_embedding_dimension, EmbeddingVectorType, Error, JobType, Result};

/// Validate a schema name the way PostgreSQL resolves it. Archive schema names
/// can exceed the 63-byte identifier limit; PostgreSQL truncates them (in DDL
/// and in queries alike), so validate the truncated form it actually stores.
fn validate_schema_name(name: &str) -> Result<()> {
    const PG_IDENTIFIER_MAX: usize = 63;
    let mut end = name.len().min(PG_IDENTIFIER_MAX);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    crate::schema_validation::validate_schema_name(&name[..end])
}

const DEFAULT_HNSW_M: i32 = 16;
const DEFAULT_HNSW_EF_CONSTRUCTION: i32 = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndexJobPayload {
    pub schema: String,
    pub dimension: i32,
    pub vector_type: String,
    pub hnsw_m: i32,
    pub hnsw_ef_construction: i32,
    #[serde(default)]
    pub force: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VectorIndexShape {
    pub schema: String,
    pub dimension: i32,
    pub vector_type: EmbeddingVectorType,
    pub hnsw_m: i32,
    pub hnsw_ef_construction: i32,
}

impl VectorIndexShape {
    pub fn from_payload(payload: VectorIndexJobPayload) -> Result<Self> {
        let vector_type = payload
            .vector_type
            .parse::<EmbeddingVectorType>()
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        validate_shape(payload.dimension, vector_type)?;
        validate_schema_name(&payload.schema)?;
        Ok(Self {
            schema: payload.schema,
            dimension: payload.dimension,
            vector_type,
            hnsw_m: payload.hnsw_m,
            hnsw_ef_construction: payload.hnsw_ef_construction,
        })
    }

    pub fn embedding_index_name(&self) -> String {
        format!("idx_embedding_hnsw_{}_{}", self.vector_type, self.dimension)
    }

    pub fn attachment_index_name(&self) -> String {
        format!(
            "idx_attach_emb_hnsw_{}_{}",
            self.vector_type, self.dimension
        )
    }

    fn vector_cast(&self) -> String {
        format!("{}({})", self.vector_type, self.dimension)
    }

    fn operator_class(&self) -> &'static str {
        match self.vector_type {
            EmbeddingVectorType::Vector => "vector_cosine_ops",
            EmbeddingVectorType::Halfvec => "halfvec_cosine_ops",
        }
    }

    fn payload(&self, force: bool, reason: &str) -> Result<serde_json::Value> {
        serde_json::to_value(VectorIndexJobPayload {
            schema: self.schema.clone(),
            dimension: self.dimension,
            vector_type: self.vector_type.to_string(),
            hnsw_m: self.hnsw_m,
            hnsw_ef_construction: self.hnsw_ef_construction,
            force,
            reason: Some(reason.to_string()),
        })
        .map_err(|error| Error::Internal(error.to_string()))
    }
}

fn validate_shape(dimension: i32, vector_type: EmbeddingVectorType) -> Result<()> {
    let dimension = usize::try_from(dimension)
        .map_err(|_| Error::InvalidInput("embedding dimension must be positive".to_string()))?;
    validate_embedding_dimension(dimension, vector_type)
        .map_err(|error| Error::InvalidInput(error.to_string()))
}

pub async fn enqueue_build_for_config(
    pool: &Pool<Postgres>,
    schema: &str,
    config_id: Uuid,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    validate_schema_name(schema)?;
    let Some(shape) = shape_for_config(pool, schema, config_id).await? else {
        return Ok(None);
    };
    enqueue_shape(pool, &shape, Some(config_id), force, reason).await
}

pub async fn enqueue_build_for_config_tx(
    tx: &mut Transaction<'_, Postgres>,
    schema: &str,
    config_id: Uuid,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    validate_schema_name(schema)?;
    let Some(shape) = shape_for_config_tx(tx, schema, config_id).await? else {
        return Ok(None);
    };
    enqueue_shape_tx(tx, &shape, Some(config_id), force, reason).await
}

/// Queue a background index build for a config's shape without touching any
/// embedding_set row. Shard import uses this so that applying a shard, which
/// may leave native sets unchanged, never rewrites their operational status;
/// the worker reports status when it builds.
pub async fn enqueue_build_job_only_for_config_tx(
    tx: &mut Transaction<'_, Postgres>,
    schema: &str,
    config_id: Uuid,
    reason: &str,
) -> Result<Option<Uuid>> {
    validate_schema_name(schema)?;
    let Some(shape) = shape_for_config_tx(tx, schema, config_id).await? else {
        return Ok(None);
    };
    if valid_index_exists_tx(tx, &shape, &shape.embedding_index_name()).await?
        || pending_job_exists_tx(tx, &shape).await?
    {
        return Ok(None);
    }
    insert_job_tx(tx, &shape, false, reason).await
}

pub async fn clear_defer_and_enqueue_for_set_tx(
    tx: &mut Transaction<'_, Postgres>,
    schema: &str,
    set_id: Uuid,
) -> Result<Option<Uuid>> {
    validate_schema_name(schema)?;
    let update = format!(
        "UPDATE {schema}.embedding_set
         SET defer_index_build = FALSE, index_status = 'pending'::embedding_index_status, updated_at = NOW()
         WHERE id = $1
         RETURNING embedding_config_id"
    );
    let config_id: Option<Uuid> = sqlx::query_scalar(&update)
        .bind(set_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?
        .flatten();
    let Some(config_id) = config_id else {
        return Ok(None);
    };
    enqueue_build_for_config_tx(tx, schema, config_id, true, "manual_build_index").await
}

async fn shape_for_config(
    pool: &Pool<Postgres>,
    schema: &str,
    config_id: Uuid,
) -> Result<Option<VectorIndexShape>> {
    let query = format!(
        "SELECT ec.dimension, ec.vector_type, ec.hnsw_m, ec.hnsw_ef_construction
         FROM public.embedding_config ec
         WHERE ec.id = $1
           AND EXISTS (
             SELECT 1 FROM {schema}.embedding_set es
             WHERE es.embedding_config_id = ec.id
               AND COALESCE(es.defer_index_build, FALSE) IS FALSE
           )"
    );
    let Some(row) = sqlx::query(&query)
        .bind(config_id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)?
    else {
        return Ok(None);
    };
    shape_from_row(schema, row)
}

async fn shape_for_config_tx(
    tx: &mut Transaction<'_, Postgres>,
    schema: &str,
    config_id: Uuid,
) -> Result<Option<VectorIndexShape>> {
    let query = format!(
        "SELECT ec.dimension, ec.vector_type, ec.hnsw_m, ec.hnsw_ef_construction
         FROM public.embedding_config ec
         WHERE ec.id = $1
           AND EXISTS (
             SELECT 1 FROM {schema}.embedding_set es
             WHERE es.embedding_config_id = ec.id
               AND COALESCE(es.defer_index_build, FALSE) IS FALSE
           )"
    );
    let Some(row) = sqlx::query(&query)
        .bind(config_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?
    else {
        return Ok(None);
    };
    shape_from_row(schema, row)
}

fn shape_from_row(schema: &str, row: sqlx::postgres::PgRow) -> Result<Option<VectorIndexShape>> {
    let dimension: i32 = row.get("dimension");
    let vector_type = row
        .get::<String, _>("vector_type")
        .parse::<EmbeddingVectorType>()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    validate_shape(dimension, vector_type)?;
    Ok(Some(VectorIndexShape {
        schema: schema.to_string(),
        dimension,
        vector_type,
        hnsw_m: row
            .get::<Option<i32>, _>("hnsw_m")
            .unwrap_or(DEFAULT_HNSW_M),
        hnsw_ef_construction: row
            .get::<Option<i32>, _>("hnsw_ef_construction")
            .unwrap_or(DEFAULT_HNSW_EF_CONSTRUCTION),
    }))
}

async fn enqueue_shape(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    if !force && valid_index_exists(pool, shape, &shape.embedding_index_name()).await? {
        mark_sets_ready(pool, shape, config_id).await?;
        return Ok(None);
    }
    if !force && pending_job_exists(pool, shape).await? {
        mark_sets_pending(pool, shape, config_id).await?;
        return Ok(None);
    }
    mark_sets_pending(pool, shape, config_id).await?;
    insert_job(pool, shape, force, reason).await
}

async fn enqueue_shape_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    if !force && valid_index_exists_tx(tx, shape, &shape.embedding_index_name()).await? {
        mark_sets_ready_tx(tx, shape, config_id).await?;
        return Ok(None);
    }
    if !force && pending_job_exists_tx(tx, shape).await? {
        mark_sets_pending_tx(tx, shape, config_id).await?;
        return Ok(None);
    }
    mark_sets_pending_tx(tx, shape, config_id).await?;
    insert_job_tx(tx, shape, force, reason).await
}

async fn valid_index_exists(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    index_name: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_index i ON i.indexrelid = c.oid
            WHERE n.nspname = $1 AND c.relname = $2 AND i.indisvalid
        )",
    )
    .bind(&shape.schema)
    .bind(index_name)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

async fn valid_index_exists_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    index_name: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_index i ON i.indexrelid = c.oid
            WHERE n.nspname = $1 AND c.relname = $2 AND i.indisvalid
        )",
    )
    .bind(&shape.schema)
    .bind(index_name)
    .fetch_one(&mut **tx)
    .await
    .map_err(Error::Database)
}

async fn pending_job_exists(pool: &Pool<Postgres>, shape: &VectorIndexShape) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM public.job_queue
            WHERE job_type = $1::job_type
              AND status IN ('pending'::job_status, 'running'::job_status)
              AND payload->>'schema' = $2
              AND (payload->>'dimension')::int = $3
              AND payload->>'vector_type' = $4
        )",
    )
    .bind(JobType::BuildSetIndex.as_str())
    .bind(&shape.schema)
    .bind(shape.dimension)
    .bind(shape.vector_type.to_string())
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

async fn pending_job_exists_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM public.job_queue
            WHERE job_type = $1::job_type
              AND status IN ('pending'::job_status, 'running'::job_status)
              AND payload->>'schema' = $2
              AND (payload->>'dimension')::int = $3
              AND payload->>'vector_type' = $4
        )",
    )
    .bind(JobType::BuildSetIndex.as_str())
    .bind(&shape.schema)
    .bind(shape.dimension)
    .bind(shape.vector_type.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(Error::Database)
}

async fn insert_job(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    let payload = shape.payload(force, reason)?;
    let id = matric_core::new_v7();
    sqlx::query_scalar(
        "INSERT INTO public.job_queue (id, job_type, status, priority, payload, created_at)
         VALUES ($1, $2::job_type, 'pending'::job_status, $3, $4, NOW())
         RETURNING id",
    )
    .bind(id)
    .bind(JobType::BuildSetIndex.as_str())
    .bind(JobType::BuildSetIndex.default_priority())
    .bind(payload)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

async fn insert_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    force: bool,
    reason: &str,
) -> Result<Option<Uuid>> {
    let payload = shape.payload(force, reason)?;
    let id = matric_core::new_v7();
    sqlx::query_scalar(
        "INSERT INTO public.job_queue (id, job_type, status, priority, payload, created_at)
         VALUES ($1, $2::job_type, 'pending'::job_status, $3, $4, NOW())
         RETURNING id",
    )
    .bind(id)
    .bind(JobType::BuildSetIndex.as_str())
    .bind(JobType::BuildSetIndex.default_priority())
    .bind(payload)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Error::Database)
}

async fn mark_sets_pending(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
) -> Result<()> {
    update_sets_status(pool, shape, config_id, "pending").await
}

async fn mark_sets_ready(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
) -> Result<()> {
    update_sets_status(pool, shape, config_id, "ready").await
}

async fn mark_sets_pending_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
) -> Result<()> {
    update_sets_status_tx(tx, shape, config_id, "pending").await
}

async fn mark_sets_ready_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
) -> Result<()> {
    update_sets_status_tx(tx, shape, config_id, "ready").await
}

async fn update_sets_status(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
    status: &str,
) -> Result<()> {
    let query = format!(
        "UPDATE {schema}.embedding_set es
         SET index_status = $1::embedding_index_status,
             last_indexed_at = CASE WHEN $1 = 'ready' THEN NOW() ELSE last_indexed_at END
         FROM public.embedding_config ec
         WHERE ec.id = es.embedding_config_id
           AND ec.dimension = $2
           AND ec.vector_type = $3
           AND COALESCE(es.defer_index_build, FALSE) IS FALSE
           AND ($4::uuid IS NULL OR ec.id = $4)
           -- Operational status only: no-op when unchanged, and never a content
           -- edit (updated_at untouched), so repeat imports leave sets unchanged.
           AND es.index_status IS DISTINCT FROM $1::embedding_index_status",
        schema = shape.schema
    );
    sqlx::query(&query)
        .bind(status)
        .bind(shape.dimension)
        .bind(shape.vector_type.to_string())
        .bind(config_id)
        .execute(pool)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

async fn update_sets_status_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: &VectorIndexShape,
    config_id: Option<Uuid>,
    status: &str,
) -> Result<()> {
    let query = format!(
        "UPDATE {schema}.embedding_set es
         SET index_status = $1::embedding_index_status,
             last_indexed_at = CASE WHEN $1 = 'ready' THEN NOW() ELSE last_indexed_at END
         FROM public.embedding_config ec
         WHERE ec.id = es.embedding_config_id
           AND ec.dimension = $2
           AND ec.vector_type = $3
           AND COALESCE(es.defer_index_build, FALSE) IS FALSE
           AND ($4::uuid IS NULL OR ec.id = $4)
           -- Operational status only: no-op when unchanged, and never a content
           -- edit (updated_at untouched), so repeat imports leave sets unchanged.
           AND es.index_status IS DISTINCT FROM $1::embedding_index_status",
        schema = shape.schema
    );
    sqlx::query(&query)
        .bind(status)
        .bind(shape.dimension)
        .bind(shape.vector_type.to_string())
        .bind(config_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

pub async fn build_vector_indexes(
    pool: &Pool<Postgres>,
    payload: VectorIndexJobPayload,
) -> Result<()> {
    let shape = VectorIndexShape::from_payload(payload)?;
    set_shape_status(pool, &shape, "building").await?;
    let result = build_vector_indexes_once(pool, &shape).await;
    let result = match result {
        Ok(()) => Ok(()),
        Err(error) => {
            drop_invalid_index(pool, &shape, &shape.embedding_index_name()).await?;
            drop_invalid_index(pool, &shape, &shape.attachment_index_name()).await?;
            drop_invalid_swap_indexes(pool, &shape).await?;
            tracing::warn!(
                error_len = error.to_string().chars().count(),
                "Retrying vector index build after dropping invalid index"
            );
            build_vector_indexes_once(pool, &shape).await
        }
    };
    match result {
        Ok(()) => {
            set_shape_status(pool, &shape, "ready").await?;
            Ok(())
        }
        Err(error) => {
            set_shape_status(pool, &shape, "stale").await?;
            Err(error)
        }
    }
}

async fn build_vector_indexes_once(pool: &Pool<Postgres>, shape: &VectorIndexShape) -> Result<()> {
    build_table_index(pool, shape, "embedding", &shape.embedding_index_name()).await?;
    build_table_index(
        pool,
        shape,
        "attachment_embedding",
        &shape.attachment_index_name(),
    )
    .await
}

async fn build_table_index(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    table: &str,
    index_name: &str,
) -> Result<()> {
    if !table_exists(pool, &shape.schema, table).await? {
        return Ok(());
    }
    drop_invalid_index(pool, shape, index_name).await?;
    let existing = index_definition(pool, &shape.schema, index_name).await?;
    let expected_m = format!("m='{}'", shape.hnsw_m);
    let expected_ef = format!("ef_construction='{}'", shape.hnsw_ef_construction);
    if existing.as_ref().is_some_and(|definition| {
        definition.contains(&expected_m) && definition.contains(&expected_ef)
    }) {
        return Ok(());
    }

    let target_name = if existing.is_some() {
        format!(
            "{}_swap_{}",
            index_name.chars().take(24).collect::<String>(),
            Uuid::new_v4().simple()
        )
    } else {
        index_name.to_string()
    };
    create_index(pool, shape, table, &target_name).await?;
    if existing.is_some() {
        let drop_sql = format!(
            "DROP INDEX CONCURRENTLY IF EXISTS {}.{}",
            shape.schema, index_name
        );
        sqlx::query(&drop_sql)
            .execute(pool)
            .await
            .map_err(Error::Database)?;
        let rename_sql = format!(
            "ALTER INDEX {}.{} RENAME TO {}",
            shape.schema, target_name, index_name
        );
        sqlx::query(&rename_sql)
            .execute(pool)
            .await
            .map_err(Error::Database)?;
    }
    Ok(())
}

async fn create_index(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    table: &str,
    index_name: &str,
) -> Result<()> {
    let sql = format!(
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS {index_name}
         ON {schema}.{table} USING hnsw ((vector::{cast}) {ops})
         WITH (m = {m}, ef_construction = {ef})
         WHERE vector IS NOT NULL AND vector_dims(vector) = {dimension}",
        index_name = index_name,
        schema = shape.schema,
        table = table,
        cast = shape.vector_cast(),
        ops = shape.operator_class(),
        m = shape.hnsw_m,
        ef = shape.hnsw_ef_construction,
        dimension = shape.dimension,
    );
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

async fn table_exists(pool: &Pool<Postgres>, schema: &str, table: &str) -> Result<bool> {
    sqlx::query_scalar("SELECT to_regclass(format('%I.%I', $1, $2)) IS NOT NULL")
        .bind(schema)
        .bind(table)
        .fetch_one(pool)
        .await
        .map_err(Error::Database)
}

async fn index_definition(
    pool: &Pool<Postgres>,
    schema: &str,
    index_name: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT pg_get_indexdef(c.oid)
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_index i ON i.indexrelid = c.oid
         WHERE n.nspname = $1 AND c.relname = $2 AND i.indisvalid",
    )
    .bind(schema)
    .bind(index_name)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

async fn drop_invalid_index(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    index_name: &str,
) -> Result<()> {
    let invalid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_index i ON i.indexrelid = c.oid
            WHERE n.nspname = $1 AND c.relname = $2 AND NOT i.indisvalid
        )",
    )
    .bind(&shape.schema)
    .bind(index_name)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)?;
    if invalid {
        let sql = format!(
            "DROP INDEX CONCURRENTLY IF EXISTS {}.{}",
            shape.schema, index_name
        );
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(Error::Database)?;
    }
    Ok(())
}

async fn drop_invalid_swap_indexes(pool: &Pool<Postgres>, shape: &VectorIndexShape) -> Result<()> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_index i ON i.indexrelid = c.oid
         WHERE n.nspname = $1
           AND c.relname LIKE $2
           AND NOT i.indisvalid",
    )
    .bind(&shape.schema)
    .bind(format!("idx_%_hnsw_{}_%_swap_%", shape.vector_type))
    .fetch_all(pool)
    .await
    .map_err(Error::Database)?;
    for name in names {
        let sql = format!(
            "DROP INDEX CONCURRENTLY IF EXISTS {}.{}",
            shape.schema, name
        );
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(Error::Database)?;
    }
    Ok(())
}

async fn set_shape_status(
    pool: &Pool<Postgres>,
    shape: &VectorIndexShape,
    status: &str,
) -> Result<()> {
    let query = format!(
        "UPDATE {schema}.embedding_set es
         SET index_status = $1::embedding_index_status,
             last_indexed_at = CASE WHEN $1 = 'ready' THEN NOW() ELSE last_indexed_at END
         FROM public.embedding_config ec
         WHERE ec.id = es.embedding_config_id
           AND ec.dimension = $2
           AND ec.vector_type = $3
           AND COALESCE(es.defer_index_build, FALSE) IS FALSE
           AND es.index_status IS DISTINCT FROM $1::embedding_index_status",
        schema = shape.schema
    );
    sqlx::query(&query)
        .bind(status)
        .bind(shape.dimension)
        .bind(shape.vector_type.to_string())
        .execute(pool)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

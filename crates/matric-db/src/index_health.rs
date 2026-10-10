//! Embedding-set vector index health (#1181 R1).
//!
//! Reports, for one set, the shape HNSW index behind its vectors: index name
//! and validity, on-disk size, live/dead tuples and vacuum/analyze history for
//! `embedding`, current set row count, rows changed since the last shape build
//! (from the per-set row counts recorded in the build job's result), and an
//! optional on-demand recall probe comparing HNSW top-10 against exact top-10.

use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use crate::schema_validation::validate_schema_name;
use crate::vector_ef_search::resolve_ef_search;
use matric_core::{Error, Result};

/// Upper bound for the on-demand recall probe sample size.
pub const MAX_RECALL_PROBE_VECTORS: i32 = 200;

/// Rank cutoff compared by the recall probe.
pub const RECALL_PROBE_TOP_K: i64 = 10;

/// Clamp an optional `?probe=N` request to the allowed range.
///
/// `None` (or `Some(n)` with `n <= 0`) disables the probe; larger values are
/// capped at [`MAX_RECALL_PROBE_VECTORS`].
pub fn clamp_probe_count(requested: Option<i32>) -> Option<i32> {
    match requested {
        Some(n) if n > 0 => Some(n.min(MAX_RECALL_PROBE_VECTORS)),
        _ => None,
    }
}

/// Vector index health for one embedding set.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IndexHealth {
    pub set_id: Uuid,
    pub set_slug: String,
    pub dimension: i32,
    pub vector_type: String,
    /// Shape index name, e.g. `idx_embedding_hnsw_vector_1024`.
    pub index_name: String,
    /// `pg_index.indisvalid`; `None` when the index does not exist.
    pub index_valid: Option<bool>,
    /// On-disk index size in bytes; `None` when the index does not exist.
    pub index_size_bytes: Option<i64>,
    pub live_tuples: Option<i64>,
    pub dead_tuples: Option<i64>,
    pub last_vacuum: Option<DateTime<Utc>>,
    pub last_autovacuum: Option<DateTime<Utc>>,
    pub last_analyze: Option<DateTime<Utc>>,
    pub last_autoanalyze: Option<DateTime<Utc>>,
    /// Current rows in the set.
    pub set_rows: i64,
    pub last_indexed_at: Option<DateTime<Utc>>,
    /// Per-set row count recorded by the latest shape build, if trackable.
    pub build_row_count: Option<i64>,
    /// `set_rows` minus the build-time count (floored at zero), if trackable.
    pub rows_changed_since_build: Option<i64>,
    pub ef_search: Option<i32>,
    pub effective_ef_search: i32,
    /// Mean overlap of HNSW top-10 with exact top-10 over the probe sample.
    pub recall_at_10: Option<f32>,
    pub recall_probe_vectors: Option<i32>,
}

/// Collect index health for one set, running the recall probe when requested.
pub async fn set_index_health(
    pool: &Pool<Postgres>,
    schema: &str,
    set_id: Uuid,
    probe_vectors: Option<i32>,
) -> Result<IndexHealth> {
    validate_schema_name(schema)?;
    let set = load_set(pool, schema, set_id).await?;
    let index_name = format!("idx_embedding_hnsw_{}_{}", set.vector_type, set.dimension);

    let (index_valid, index_size_bytes) = index_identity(pool, schema, &index_name).await?;
    let table_stats = table_stats(pool, schema).await?;
    let set_rows = count_set_rows(pool, schema, set_id).await?;
    let build_row_count = latest_build_row_count(pool, schema, &set, set_id).await?;
    let rows_changed_since_build = build_row_count.map(|built| set_rows.saturating_sub(built));
    let effective_ef_search = resolve_ef_search(set.ef_search);

    let probe_count = clamp_probe_count(probe_vectors);
    let mut recall_at_10 = None;
    let mut recall_probe_vectors = None;
    if let Some(count) = probe_count {
        if let Some(recall) =
            probe_recall(pool, schema, set_id, &set, effective_ef_search, count).await?
        {
            recall_at_10 = Some(recall);
            recall_probe_vectors = Some(count);
        }
    }

    Ok(IndexHealth {
        set_id,
        set_slug: set.slug,
        dimension: set.dimension,
        vector_type: set.vector_type,
        index_name,
        index_valid,
        index_size_bytes,
        live_tuples: table_stats.as_ref().and_then(|stats| stats.live_tuples),
        dead_tuples: table_stats.as_ref().and_then(|stats| stats.dead_tuples),
        last_vacuum: table_stats.as_ref().and_then(|stats| stats.last_vacuum),
        last_autovacuum: table_stats.as_ref().and_then(|stats| stats.last_autovacuum),
        last_analyze: table_stats.as_ref().and_then(|stats| stats.last_analyze),
        last_autoanalyze: table_stats
            .as_ref()
            .and_then(|stats| stats.last_autoanalyze),
        set_rows,
        last_indexed_at: set.last_indexed_at,
        build_row_count,
        rows_changed_since_build,
        ef_search: set.ef_search,
        effective_ef_search,
        recall_at_10,
        recall_probe_vectors,
    })
}

struct SetShape {
    slug: String,
    dimension: i32,
    vector_type: String,
    ef_search: Option<i32>,
    last_indexed_at: Option<DateTime<Utc>>,
}

struct TableStats {
    live_tuples: Option<i64>,
    dead_tuples: Option<i64>,
    last_vacuum: Option<DateTime<Utc>>,
    last_autovacuum: Option<DateTime<Utc>>,
    last_analyze: Option<DateTime<Utc>>,
    last_autoanalyze: Option<DateTime<Utc>>,
}

async fn load_set(pool: &Pool<Postgres>, schema: &str, set_id: Uuid) -> Result<SetShape> {
    let query = format!(
        "SELECT es.slug, ec.dimension, ec.vector_type, es.ef_search, es.last_indexed_at
         FROM {schema}.embedding_set es
         JOIN public.embedding_config ec ON ec.id = es.embedding_config_id
         WHERE es.id = $1"
    );
    let row = sqlx::query(&query)
        .bind(set_id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| {
            Error::NotFound("Embedding set not found; set_id_present=true".to_string())
        })?;
    Ok(SetShape {
        slug: row.get("slug"),
        dimension: row.get("dimension"),
        vector_type: row.get("vector_type"),
        ef_search: row.get("ef_search"),
        last_indexed_at: row.get("last_indexed_at"),
    })
}

async fn index_identity(
    pool: &Pool<Postgres>,
    schema: &str,
    index_name: &str,
) -> Result<(Option<bool>, Option<i64>)> {
    let row = sqlx::query(
        "SELECT i.indisvalid AS valid, pg_relation_size(c.oid) AS bytes
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         LEFT JOIN pg_index i ON i.indexrelid = c.oid
         WHERE n.nspname = $1 AND c.relname = $2",
    )
    .bind(schema)
    .bind(index_name)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;
    match row {
        Some(row) => Ok((row.get("valid"), row.get("bytes"))),
        None => Ok((None, None)),
    }
}

async fn table_stats(pool: &Pool<Postgres>, schema: &str) -> Result<Option<TableStats>> {
    let row = sqlx::query(
        "SELECT n_live_tup, n_dead_tup, last_vacuum, last_autovacuum, last_analyze, last_autoanalyze
         FROM pg_stat_user_tables
         WHERE schemaname = $1 AND relname = 'embedding'",
    )
    .bind(schema)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(|row| TableStats {
        live_tuples: row.get("n_live_tup"),
        dead_tuples: row.get("n_dead_tup"),
        last_vacuum: row.get("last_vacuum"),
        last_autovacuum: row.get("last_autovacuum"),
        last_analyze: row.get("last_analyze"),
        last_autoanalyze: row.get("last_autoanalyze"),
    }))
}

async fn count_set_rows(pool: &Pool<Postgres>, schema: &str, set_id: Uuid) -> Result<i64> {
    let query = format!("SELECT count(*) FROM {schema}.embedding WHERE embedding_set_id = $1");
    sqlx::query_scalar(&query)
        .bind(set_id)
        .fetch_one(pool)
        .await
        .map_err(Error::Database)
}

/// Per-set row count recorded by the latest completed shape build, if any.
async fn latest_build_row_count(
    pool: &Pool<Postgres>,
    schema: &str,
    set: &SetShape,
    set_id: Uuid,
) -> Result<Option<i64>> {
    let row: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT result FROM public.job_queue
         WHERE job_type = 'build_set_index'
           AND status = 'completed'::job_status
           AND payload->>'schema' = $1
           AND (payload->>'dimension')::int = $2
           AND payload->>'vector_type' = $3
         ORDER BY completed_at DESC NULLS LAST, created_at DESC
         LIMIT 1",
    )
    .bind(schema)
    .bind(set.dimension)
    .bind(&set.vector_type)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?
    .flatten();
    Ok(row
        .as_ref()
        .and_then(|result| result.pointer(&format!("/set_row_counts/{set_id}")))
        .and_then(serde_json::Value::as_i64))
}

fn distance_order(vector_type: &str, dimension: i32, placeholder: &str) -> String {
    if vector_type == "halfvec" {
        format!("(vector::halfvec({dimension})) <=> ({placeholder}::vector::halfvec({dimension}))")
    } else {
        format!("(vector::vector({dimension})) <=> {placeholder}::vector({dimension})")
    }
}

/// Mean HNSW top-10 overlap with exact top-10 over up to `count` sampled vectors.
async fn probe_recall(
    pool: &Pool<Postgres>,
    schema: &str,
    set_id: Uuid,
    set: &SetShape,
    effective_ef_search: i32,
    count: i32,
) -> Result<Option<f32>> {
    let sample_query = format!(
        "SELECT vector FROM {schema}.embedding
         WHERE embedding_set_id = $1
           AND vector IS NOT NULL
           AND vector_dims(vector) = $2
         ORDER BY random() LIMIT $3"
    );
    let samples: Vec<Vector> = sqlx::query_scalar(&sample_query)
        .bind(set_id)
        .bind(set.dimension)
        .bind(i64::from(count))
        .fetch_all(pool)
        .await
        .map_err(Error::Database)?;
    if samples.is_empty() {
        return Ok(None);
    }

    let order = distance_order(&set.vector_type, set.dimension, "$2");
    let top_query = format!(
        "SELECT id FROM {schema}.embedding
         WHERE embedding_set_id = $1
           AND vector IS NOT NULL
           AND vector_dims(vector) = {dimension}
         ORDER BY {order} LIMIT {k}",
        dimension = set.dimension,
        k = RECALL_PROBE_TOP_K
    );

    let mut hnsw_tx = pool.begin().await.map_err(Error::Database)?;
    sqlx::query(&format!("SET LOCAL hnsw.ef_search = {effective_ef_search}"))
        .execute(&mut *hnsw_tx)
        .await
        .map_err(Error::Database)?;
    let mut hnsw_hits = Vec::with_capacity(samples.len());
    for sample in &samples {
        let ids: Vec<Uuid> = sqlx::query_scalar(&top_query)
            .bind(set_id)
            .bind(sample)
            .fetch_all(&mut *hnsw_tx)
            .await
            .map_err(Error::Database)?;
        hnsw_hits.push(ids);
    }
    hnsw_tx.commit().await.map_err(Error::Database)?;

    let mut exact_tx = pool.begin().await.map_err(Error::Database)?;
    sqlx::query("SET LOCAL enable_indexscan = off")
        .execute(&mut *exact_tx)
        .await
        .map_err(Error::Database)?;
    let mut recall_total = 0.0f32;
    let mut query_count = 0i64;
    for (sample, hnsw_ids) in samples.iter().zip(hnsw_hits.iter()) {
        let exact_ids: Vec<Uuid> = sqlx::query_scalar(&top_query)
            .bind(set_id)
            .bind(sample)
            .fetch_all(&mut *exact_tx)
            .await
            .map_err(Error::Database)?;
        if exact_ids.is_empty() {
            continue;
        }
        let denominator = exact_ids.len().min(RECALL_PROBE_TOP_K as usize) as f32;
        let overlap = hnsw_ids.iter().filter(|id| exact_ids.contains(id)).count() as f32;
        recall_total += overlap / denominator;
        query_count += 1;
    }
    exact_tx.commit().await.map_err(Error::Database)?;

    if query_count == 0 {
        return Ok(None);
    }
    Ok(Some(recall_total / query_count as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_count_clamping() {
        assert_eq!(clamp_probe_count(None), None);
        assert_eq!(clamp_probe_count(Some(0)), None);
        assert_eq!(clamp_probe_count(Some(-5)), None);
        assert_eq!(clamp_probe_count(Some(10)), Some(10));
        assert_eq!(clamp_probe_count(Some(200)), Some(200));
        assert_eq!(
            clamp_probe_count(Some(10_000)),
            Some(MAX_RECALL_PROBE_VECTORS)
        );
    }
}

use pgvector::Vector;
use serde_json::{Map, Value};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use matric_core::{new_v7, Error, Result, StrictTagFilter};

use crate::{
    embedding_storage_contract::{contract_for_set, EmbeddingStorageContract},
    search_candidates::{bind_params, SearchCandidateScope},
    strict_filter::QueryParam,
};

pub const DEFAULT_ENTITY_SIMILARITY_EF_SEARCH: i32 = 100;

#[derive(Debug, Clone)]
pub struct EntityProfileVectorRow {
    pub note_id: Uuid,
    pub profile_hash: String,
    pub template_version: String,
    pub profile_text: String,
    pub vector: Vector,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EntityProfileVectorBatchOutcome {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone)]
pub struct EntitySimilarityHit {
    pub note_id: Uuid,
    pub title: Option<String>,
    pub score: f32,
    pub source_namespace: Option<String>,
    pub external_source_id: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct EntitySimilarityFilter {
    pub metadata: Option<matric_core::metadata_search::MetadataPredicates>,
    pub strict: Option<StrictTagFilter>,
    pub legacy_filters: String,
}

impl EntitySimilarityFilter {
    pub fn is_filtered(&self) -> bool {
        self.metadata.is_some()
            || self
                .strict
                .as_ref()
                .is_some_and(|filter| !filter.is_empty())
            || !self.legacy_filters.trim().is_empty()
    }
}

pub async fn upsert_profile_vectors_tx(
    connection: &mut PgConnection,
    embedding_set_id: Uuid,
    rows: Vec<EntityProfileVectorRow>,
) -> Result<EntityProfileVectorBatchOutcome> {
    let contract = contract_for_set(&mut *connection, embedding_set_id).await?;
    let model: String = sqlx::query_scalar(
        r#"
        SELECT ec.model
        FROM embedding_set es
        JOIN public.embedding_config ec ON ec.id = es.embedding_config_id
        WHERE es.id = $1
        "#,
    )
    .bind(embedding_set_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(Error::Database)?
    .ok_or_else(|| Error::NotFound("Embedding set config not found".to_string()))?;

    let vector_param = contract.storage_param("$5");
    let insert_sql = format!(
        r#"
        INSERT INTO embedding (
            id, note_id, embedding_set_id, chunk_index, text, vector, model,
            created_at, vector_kind, template_version, profile_hash, profile_text
        )
        VALUES ($1, $2, $3, -1, $4, {vector_param}, $6, NOW(), 'profile', $7, $8, $9)
        ON CONFLICT (tenant_id, note_id, embedding_set_id)
            WHERE vector_kind = 'profile'
        DO UPDATE SET
            text = EXCLUDED.text,
            vector = EXCLUDED.vector,
            model = EXCLUDED.model,
            created_at = NOW(),
            template_version = EXCLUDED.template_version,
            profile_hash = EXCLUDED.profile_hash,
            profile_text = EXCLUDED.profile_text
        WHERE embedding.profile_hash IS DISTINCT FROM EXCLUDED.profile_hash
        RETURNING (xmax = 0) AS inserted
        "#
    );

    let mut outcome = EntityProfileVectorBatchOutcome::default();
    for row in rows {
        validate_profile_row(&row)?;
        contract.validate_vector(&row.vector)?;
        let result = sqlx::query(&insert_sql)
            .bind(new_v7())
            .bind(row.note_id)
            .bind(embedding_set_id)
            .bind(&row.profile_text)
            .bind(&row.vector)
            .bind(&model)
            .bind(&row.template_version)
            .bind(&row.profile_hash)
            .bind(&row.profile_text)
            .fetch_optional(&mut *connection)
            .await
            .map_err(Error::Database)?;
        match result {
            Some(row) if row.get::<bool, _>("inserted") => outcome.inserted += 1,
            Some(_) => outcome.updated += 1,
            None => outcome.unchanged += 1,
        }
    }

    Ok(outcome)
}

pub async fn delete_profile_vector_tx(
    connection: &mut PgConnection,
    embedding_set_id: Uuid,
    note_id: Uuid,
) -> Result<EntityProfileVectorBatchOutcome> {
    let deleted = sqlx::query(
        "DELETE FROM embedding
         WHERE note_id = $1 AND embedding_set_id = $2 AND vector_kind = 'profile'",
    )
    .bind(note_id)
    .bind(embedding_set_id)
    .execute(&mut *connection)
    .await
    .map_err(Error::Database)?
    .rows_affected() as usize;

    Ok(EntityProfileVectorBatchOutcome {
        deleted,
        ..Default::default()
    })
}

pub async fn find_similar_profiles_for_note_tx(
    connection: &mut PgConnection,
    query_note_id: Uuid,
    embedding_set_id: Uuid,
    limit: i64,
    filter: EntitySimilarityFilter,
    metadata_fields: Vec<String>,
) -> Result<Vec<EntitySimilarityHit>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let contract = contract_for_set(&mut *connection, embedding_set_id).await?;
    configure_hnsw(connection, embedding_set_id, filter.is_filtered()).await?;
    let Some(query_vector) = query_profile_vector(connection, query_note_id, &contract).await?
    else {
        return Ok(Vec::new());
    };

    let rows = profile_similarity_rows(
        connection,
        ProfileSimilarityQuery {
            query_note_id,
            query_vector: &query_vector,
            limit,
            filter,
            metadata_fields,
            explain: false,
            contract: &contract,
        },
    )
    .await?;

    rows.into_iter().map(hit_from_row).collect()
}

pub async fn explain_similar_profiles_for_note_tx(
    connection: &mut PgConnection,
    query_note_id: Uuid,
    embedding_set_id: Uuid,
    limit: i64,
    filter: EntitySimilarityFilter,
) -> Result<Vec<String>> {
    let contract = contract_for_set(&mut *connection, embedding_set_id).await?;
    configure_hnsw(connection, embedding_set_id, filter.is_filtered()).await?;
    let Some(query_vector) = query_profile_vector(connection, query_note_id, &contract).await?
    else {
        return Ok(Vec::new());
    };
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *connection)
        .await
        .map_err(Error::Database)?;
    let rows = profile_similarity_rows(
        connection,
        ProfileSimilarityQuery {
            query_note_id,
            query_vector: &query_vector,
            limit,
            filter,
            metadata_fields: Vec::new(),
            explain: true,
            contract: &contract,
        },
    )
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("QUERY PLAN").map_err(Error::Database))
        .collect()
}

pub async fn note_id_for_source_identity_tx(
    connection: &mut PgConnection,
    source_namespace: &str,
    external_id: &str,
) -> Result<Uuid> {
    sqlx::query_scalar(
        r#"
        SELECT si.note_id
        FROM source_identity si
        JOIN note n ON n.id = si.note_id AND n.tenant_id = si.tenant_id
        WHERE si.source_namespace = $1
          AND si.external_id = $2
          AND n.deleted_at IS NULL
        "#,
    )
    .bind(source_namespace)
    .bind(external_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(Error::Database)?
    .ok_or_else(|| Error::NotFound("Source entity not found".to_string()))
}

struct ProfileSimilarityQuery<'a> {
    query_note_id: Uuid,
    query_vector: &'a Vector,
    limit: i64,
    filter: EntitySimilarityFilter,
    metadata_fields: Vec<String>,
    explain: bool,
    contract: &'a EmbeddingStorageContract,
}

async fn query_profile_vector(
    connection: &mut PgConnection,
    query_note_id: Uuid,
    contract: &EmbeddingStorageContract,
) -> Result<Option<Vector>> {
    let set_predicate = contract.set_predicate("q", "$2");
    let sql = format!(
        r#"
        SELECT q.vector
        FROM embedding q
        WHERE q.note_id = $1
          AND q.vector_kind = 'profile'
          AND q.vector IS NOT NULL
          AND {set_predicate}
        LIMIT 1
        "#
    );
    sqlx::query_scalar(&sql)
        .bind(query_note_id)
        .bind(contract.embedding_set_id)
        .fetch_optional(connection)
        .await
        .map_err(Error::Database)
}

async fn profile_similarity_rows(
    connection: &mut PgConnection,
    query: ProfileSimilarityQuery<'_>,
) -> Result<Vec<sqlx::postgres::PgRow>> {
    let distance = query.contract.distance_expr("e.vector", "$3");
    let scope = SearchCandidateScope {
        metadata: query.filter.metadata,
        strict: query.filter.strict,
        legacy_filters: query.filter.legacy_filters,
        exclude_archived: true,
        ..Default::default()
    };
    let (cte, candidate, mut params) = scope.build(3);
    params.push(QueryParam::Uuid(query.contract.embedding_set_id));
    params.push(QueryParam::StringArray(query.metadata_fields));
    let set_placeholder = format!("${}", 3 + params.len() - 1);
    let metadata_placeholder = format!("${}", 3 + params.len());
    let set_predicate = query.contract.set_predicate("e", &set_placeholder);

    let select = format!(
        r#"
        {cte}
        SELECT n.id AS note_id,
               n.title,
               (1.0 - ({distance}))::real AS score,
               si.source_namespace,
               si.source_id AS external_source_id,
               CASE
                   WHEN cardinality({metadata_placeholder}::text[]) = 0 THEN '{{}}'::jsonb
                   ELSE COALESCE((
                       SELECT jsonb_object_agg(key, value)
                       FROM jsonb_each(n.metadata) entry(key, value)
                       WHERE key = ANY({metadata_placeholder}::text[])
                   ), '{{}}'::jsonb)
               END AS metadata
        FROM embedding e
        JOIN note n ON n.id = e.note_id AND n.tenant_id = e.tenant_id
        LEFT JOIN LATERAL (
            SELECT source_namespace, source_id
            FROM source_identity si
            WHERE si.note_id = n.id
              AND si.tenant_id = n.tenant_id
            ORDER BY si.updated_at DESC, si.id
            LIMIT 1
        ) si ON TRUE
        WHERE e.vector_kind = 'profile'
          AND e.vector IS NOT NULL
          AND e.note_id <> $1
          AND {set_predicate}
          AND ({candidate})
        ORDER BY {distance}, e.note_id
        LIMIT $2
        "#
    );
    let sql = if query.explain {
        format!("EXPLAIN {select}")
    } else {
        select
    };
    bind_params(
        sqlx::query(&sql)
            .bind(query.query_note_id)
            .bind(query.limit)
            .bind(query.query_vector),
        params,
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(Error::Database)
}

async fn configure_hnsw(
    connection: &mut PgConnection,
    embedding_set_id: Uuid,
    has_filters: bool,
) -> Result<()> {
    let ef_search = hnsw_ef_search(connection, embedding_set_id).await?;
    sqlx::query("SELECT set_config('hnsw.ef_search', $1, true)")
        .bind(ef_search.to_string())
        .execute(&mut *connection)
        .await
        .map_err(Error::Database)?;

    if has_filters && pgvector_supports_iterative_scan(connection).await? {
        sqlx::query("SELECT set_config('hnsw.iterative_scan', 'relaxed_order', true)")
            .execute(&mut *connection)
            .await
            .map_err(Error::Database)?;
    }

    Ok(())
}

async fn hnsw_ef_search(connection: &mut PgConnection, embedding_set_id: Uuid) -> Result<i32> {
    let value: Option<i32> = sqlx::query_scalar(
        r#"
        SELECT CASE
            WHEN es.agent_metadata->>'hnsw_ef_search' ~ '^[0-9]+$'
                THEN (es.agent_metadata->>'hnsw_ef_search')::int
            WHEN es.agent_metadata->>'entity_similarity_ef_search' ~ '^[0-9]+$'
                THEN (es.agent_metadata->>'entity_similarity_ef_search')::int
            ELSE NULL
        END
        FROM embedding_set es
        WHERE es.id = $1
        "#,
    )
    .bind(embedding_set_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(Error::Database)?
    .flatten();
    Ok(value
        .unwrap_or(DEFAULT_ENTITY_SIMILARITY_EF_SEARCH)
        .clamp(1, 1000))
}

async fn pgvector_supports_iterative_scan(connection: &mut PgConnection) -> Result<bool> {
    let extversion: Option<String> =
        sqlx::query_scalar("SELECT extversion FROM pg_extension WHERE extname = 'vector'")
            .fetch_optional(&mut *connection)
            .await
            .map_err(Error::Database)?;
    Ok(extversion
        .as_deref()
        .and_then(parse_major_minor)
        .is_some_and(|(major, minor)| major > 0 || minor >= 8))
}

fn parse_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts
        .next()
        .and_then(|value| {
            value
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(0);
    Some((major, minor))
}

fn validate_profile_row(row: &EntityProfileVectorRow) -> Result<()> {
    for (field, value) in [
        ("profile_hash", row.profile_hash.as_str()),
        ("template_version", row.template_version.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(Error::InvalidInput(format!("{field} is required")));
        }
    }
    Ok(())
}

fn hit_from_row(row: sqlx::postgres::PgRow) -> Result<EntitySimilarityHit> {
    Ok(EntitySimilarityHit {
        note_id: row.try_get("note_id").map_err(Error::Database)?,
        title: row.try_get("title").map_err(Error::Database)?,
        score: row.try_get("score").map_err(Error::Database)?,
        source_namespace: row.try_get("source_namespace").map_err(Error::Database)?,
        external_source_id: row.try_get("external_source_id").map_err(Error::Database)?,
        metadata: row
            .try_get::<Option<Value>, _>("metadata")
            .map_err(Error::Database)?
            .unwrap_or_else(|| Value::Object(Map::new())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pgvector_version_for_iterative_scan_gate() {
        assert_eq!(parse_major_minor("0.8.0"), Some((0, 8)));
        assert_eq!(parse_major_minor("0.8.0-dev"), Some((0, 8)));
        assert_eq!(parse_major_minor("1.0.0"), Some((1, 0)));
        assert_eq!(parse_major_minor("0.7.4"), Some((0, 7)));
    }
}

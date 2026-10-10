#![cfg(feature = "migrations")]

use std::time::Instant;

use matric_db::{
    create_pool,
    test_fixtures::DEFAULT_TEST_DATABASE_URL,
    vector_index::{build_vector_indexes, VectorIndexJobPayload},
    AutoEmbedRules, CreateEmbeddingConfigRequest, CreateEmbeddingSetRequest, Database,
    EmbeddingSetCriteria, EmbeddingSetMode, EmbeddingSetType, EmbeddingVectorSource,
    EmbeddingVectorType,
};
use pgvector::Vector;
use rand::{rngs::StdRng, Rng, SeedableRng};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

const DIMENSION: usize = 1024;
const QUERY_COUNT: usize = 200;
const TOP_K: usize = 10;
const MIN_RECALL: f32 = 0.95;

#[cfg(feature = "recall-100k")]
const VECTOR_COUNT: usize = 100_000;
#[cfg(not(feature = "recall-100k"))]
const VECTOR_COUNT: usize = 10_000;

async fn setup_test_db() -> Database {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    let pool = create_pool(&database_url)
        .await
        .expect("failed to connect to test database");
    let db = Database::new(pool);
    sqlx::query("DELETE FROM public.job_queue WHERE job_type = 'build_set_index'")
        .execute(&db.pool)
        .await
        .expect("clear vector index jobs");
    sqlx::query("DROP INDEX CONCURRENTLY IF EXISTS public.idx_embedding_hnsw_vector_1024")
        .execute(&db.pool)
        .await
        .expect("drop recall embedding index");
    sqlx::query("DROP INDEX CONCURRENTLY IF EXISTS public.idx_attach_emb_hnsw_vector_1024")
        .execute(&db.pool)
        .await
        .expect("drop recall attachment index");
    sqlx::query("TRUNCATE public.embedding CASCADE")
        .execute(&db.pool)
        .await
        .expect("clear embeddings");
    sqlx::query("DELETE FROM public.embedding_set WHERE slug LIKE 'recall-acceptance-%'")
        .execute(&db.pool)
        .await
        .expect("clear recall sets");
    sqlx::query("DELETE FROM public.embedding_config WHERE model LIKE 'recall-model-%'")
        .execute(&db.pool)
        .await
        .expect("clear recall configs");
    db
}

fn normalize(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in values {
            *value /= norm;
        }
    }
}

fn clustered_vector(centers: &[Vec<f32>], index: usize, rng: &mut StdRng) -> Vec<f32> {
    let center = &centers[index % centers.len()];
    let mut vector = center
        .iter()
        .map(|value| value + rng.gen_range(-0.01..0.01))
        .collect::<Vec<_>>();
    normalize(&mut vector);
    vector
}

fn cluster_centers() -> Vec<Vec<f32>> {
    let mut rng = StdRng::seed_from_u64(0x1181_2026);
    (0..100)
        .map(|_| {
            let mut center = (0..DIMENSION)
                .map(|_| rng.gen_range(-1.0..1.0))
                .collect::<Vec<_>>();
            normalize(&mut center);
            center
        })
        .collect()
}

async fn create_recall_set(db: &Database, unique: &str) -> Uuid {
    let config = db
        .embedding_sets
        .create_config(CreateEmbeddingConfigRequest {
            name: format!("recall-1024-{unique}"),
            description: Some("HNSW recall acceptance fixture".to_string()),
            model: format!("recall-model-{unique}"),
            dimension: DIMENSION as i32,
            vector_type: EmbeddingVectorType::Vector,
            chunk_size: 512,
            chunk_overlap: 0,
            provider: Default::default(),
            provider_config: serde_json::json!({}),
            supports_mrl: false,
            matryoshka_dims: None,
            default_truncate_dim: None,
            content_types: vec!["text".to_string()],
            hnsw_m: Some(16),
            hnsw_ef_construction: Some(128),
            document_composition: Default::default(),
            space_contract: None,
            space_id: None,
        })
        .await
        .expect("create recall config");

    db.embedding_sets
        .create(CreateEmbeddingSetRequest {
            name: format!("Recall Acceptance {unique}"),
            slug: Some(format!("recall-acceptance-{unique}")),
            description: None,
            purpose: None,
            usage_hints: None,
            keywords: vec![],
            set_type: EmbeddingSetType::Full,
            mode: EmbeddingSetMode::Manual,
            criteria: EmbeddingSetCriteria::default(),
            agent_metadata: Default::default(),
            embedding_config_id: Some(config.id),
            truncate_dim: None,
            auto_embed_rules: AutoEmbedRules::default(),
            vector_source: EmbeddingVectorSource::Internal,
            defer_index_build: true,
            ef_search: None,
        })
        .await
        .expect("create recall set")
        .id
}

async fn insert_vectors(db: &Database, set_id: Uuid) -> Vec<Vector> {
    let centers = cluster_centers();
    let mut rng = StdRng::seed_from_u64(0x1181_5eed);
    let mut query_vectors = Vec::with_capacity(QUERY_COUNT);
    let mut inserted = 0usize;
    let mut tx = db.pool.begin().await.expect("begin recall inserts");
    let note_id: Uuid = sqlx::query_scalar(
        "INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, title, metadata)
         VALUES (uuidv7(), 'markdown', 'vector-recall-acceptance', NOW(), NOW(), $1, '{}'::jsonb)
         RETURNING id",
    )
    .bind(format!("recall synthetic note {set_id}"))
    .fetch_one(&mut *tx)
    .await
    .expect("insert recall note");
    while inserted < VECTOR_COUNT {
        let batch = (VECTOR_COUNT - inserted).min(5_000);
        let mut embeddings = Vec::with_capacity(batch);
        let now = chrono::Utc::now();
        for offset in 0..batch {
            let index = inserted + offset;
            let embedding_id = Uuid::new_v4();
            let vector_values = clustered_vector(&centers, index, &mut rng);
            if query_vectors.len() < QUERY_COUNT {
                query_vectors.push(Vector::from(vector_values.clone()));
            }
            embeddings.push((
                embedding_id,
                note_id,
                index as i32,
                format!("recall synthetic chunk {index}"),
                Vector::from(vector_values),
            ));
        }

        let mut embedding_query = QueryBuilder::<Postgres>::new(
            "INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, embedding_set_id, chunk_hash, doc_hash, created_at) ",
        );
        embedding_query.push_values(
            embeddings.iter(),
            |mut values, (embedding_id, note_id, chunk_index, text, vector)| {
                values
                    .push_bind(*embedding_id)
                    .push_bind(*note_id)
                    .push_bind(*chunk_index)
                    .push_bind(text)
                    .push_bind(vector)
                    .push_bind("recall-model")
                    .push_bind(set_id)
                    .push_bind(matric_core::embedding_chunk_hash(text))
                    .push_bind(matric_core::embedding_doc_hash(text))
                    .push_bind(now);
            },
        );
        embedding_query
            .build()
            .execute(&mut *tx)
            .await
            .expect("insert recall embeddings");

        inserted += batch;
    }
    tx.commit().await.expect("commit recall inserts");
    query_vectors
}

async fn build_index_via_job_path(db: &Database, set_id: Uuid) {
    let mut tx = db.pool.begin().await.expect("begin build-index tx");
    matric_db::vector_index::clear_defer_and_enqueue_for_set_tx(&mut tx, "public", set_id)
        .await
        .expect("queue build-index job");
    tx.commit().await.expect("commit build-index enqueue");

    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM public.job_queue
         WHERE job_type = 'build_set_index'
           AND payload->>'schema' = 'public'
           AND (payload->>'dimension')::int = $1
           AND payload->>'vector_type' = 'vector'
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(DIMENSION as i32)
    .fetch_one(&db.pool)
    .await
    .expect("load build-index payload");
    let payload: VectorIndexJobPayload = serde_json::from_value(payload).expect("valid payload");
    let database_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db.pool)
        .await
        .expect("load database name");
    let database_ident = quote_ident(&database_name);
    sqlx::query(&format!(
        "ALTER DATABASE {database_ident} SET max_parallel_maintenance_workers = 0"
    ))
    .execute(&db.pool)
    .await
    .expect("set recall build workers");

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    let build_pool = create_pool(&database_url)
        .await
        .expect("connect recall build pool");
    let build_result = build_vector_indexes(&build_pool, payload).await;
    build_pool.close().await;

    sqlx::query(&format!(
        "ALTER DATABASE {database_ident} RESET max_parallel_maintenance_workers"
    ))
    .execute(&db.pool)
    .await
    .expect("reset recall build workers");
    build_result.expect("build vector index");
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

async fn recall_at_10(db: &Database, set_id: Uuid, query_vectors: &[Vector]) -> f32 {
    let search_sql = format!(
        "SELECT id FROM embedding
         WHERE embedding_set_id = $1
           AND vector IS NOT NULL
           AND vector_dims(vector) = {DIMENSION}
         ORDER BY (vector::vector({DIMENSION})) <=> $2::vector({DIMENSION})
         LIMIT {TOP_K}"
    );

    let mut hnsw_tx = db.pool.begin().await.expect("begin hnsw recall tx");
    sqlx::query(&format!(
        "SET LOCAL hnsw.ef_search = {}",
        matric_core::defaults::SEMANTIC_EF_SEARCH_DEFAULT
    ))
    .execute(&mut *hnsw_tx)
    .await
    .expect("set hnsw ef_search");
    let mut hnsw_results = Vec::with_capacity(query_vectors.len());
    for vector in query_vectors {
        let ids: Vec<Uuid> = sqlx::query_scalar(&search_sql)
            .bind(set_id)
            .bind(vector)
            .fetch_all(&mut *hnsw_tx)
            .await
            .expect("hnsw recall query");
        hnsw_results.push(ids);
    }
    hnsw_tx.commit().await.expect("commit hnsw recall tx");

    let mut exact_tx = db.pool.begin().await.expect("begin exact recall tx");
    sqlx::query("SET LOCAL enable_indexscan = off")
        .execute(&mut *exact_tx)
        .await
        .expect("disable index scans");
    let mut total = 0.0f32;
    for (vector, hnsw_ids) in query_vectors.iter().zip(hnsw_results.iter()) {
        let exact_ids: Vec<Uuid> = sqlx::query_scalar(&search_sql)
            .bind(set_id)
            .bind(vector)
            .fetch_all(&mut *exact_tx)
            .await
            .expect("exact recall query");
        let overlap = hnsw_ids.iter().filter(|id| exact_ids.contains(id)).count() as f32;
        total += overlap / exact_ids.len().min(TOP_K) as f32;
    }
    exact_tx.commit().await.expect("commit exact recall tx");
    total / query_vectors.len() as f32
}

#[tokio::test]
async fn hnsw_recall_acceptance_for_1024_dimensional_vectors() {
    let started = Instant::now();
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();
    let set_id = create_recall_set(&db, &unique).await;
    let query_vectors = insert_vectors(&db, set_id).await;
    build_index_via_job_path(&db, set_id).await;

    let recall = recall_at_10(&db, set_id, &query_vectors).await;
    println!(
        "vector recall acceptance: vectors={VECTOR_COUNT} queries={QUERY_COUNT} recall@10={recall:.4} elapsed={:?}",
        started.elapsed()
    );
    assert!(
        recall >= MIN_RECALL,
        "recall@10 {recall:.4} below required {MIN_RECALL:.2}"
    );
}

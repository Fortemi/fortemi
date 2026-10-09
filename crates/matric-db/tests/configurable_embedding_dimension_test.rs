#![cfg(feature = "migrations")]

use matric_db::vector_index::{build_vector_indexes, VectorIndexJobPayload};
use matric_db::{
    create_pool, test_fixtures::DEFAULT_TEST_DATABASE_URL, AutoEmbedRules,
    CreateEmbeddingConfigRequest, CreateEmbeddingSetRequest, Database, EmbeddingSetAgentMetadata,
    EmbeddingSetCriteria, EmbeddingSetMode, EmbeddingSetType, EmbeddingVectorSource,
    EmbeddingVectorType, NoteRepository,
};
use pgvector::Vector;
use uuid::Uuid;

const MIGRATION: &str =
    include_str!("../../../migrations/20261008020000_configurable_embedding_dimension.sql");

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
    db
}

fn config_request(
    name: &str,
    dimension: i32,
    vector_type: EmbeddingVectorType,
) -> CreateEmbeddingConfigRequest {
    CreateEmbeddingConfigRequest {
        name: name.to_string(),
        description: Some(format!("{name} configurable dimension test")),
        model: format!("{name}-model"),
        dimension,
        vector_type,
        chunk_size: 512,
        chunk_overlap: 64,
        provider: Default::default(),
        provider_config: serde_json::json!({}),
        supports_mrl: false,
        matryoshka_dims: None,
        default_truncate_dim: None,
        content_types: vec!["text".to_string()],
        hnsw_m: Some(16),
        hnsw_ef_construction: Some(64),
        document_composition: Default::default(),
        space_contract: None,
        space_id: None,
    }
}

fn set_request(name: &str, config_id: Uuid) -> CreateEmbeddingSetRequest {
    CreateEmbeddingSetRequest {
        name: name.to_string(),
        slug: Some(name.to_lowercase().replace(' ', "-")),
        description: None,
        purpose: None,
        usage_hints: None,
        keywords: vec![],
        set_type: EmbeddingSetType::Full,
        mode: EmbeddingSetMode::Manual,
        criteria: EmbeddingSetCriteria::default(),
        embedding_config_id: Some(config_id),
        truncate_dim: None,
        auto_embed_rules: AutoEmbedRules::default(),
        vector_source: EmbeddingVectorSource::Internal,
        defer_index_build: false,
        agent_metadata: EmbeddingSetAgentMetadata::default(),
    }
}

async fn insert_note(db: &Database, content: &str) -> Uuid {
    db.notes
        .insert(matric_core::CreateNoteRequest {
            content: content.to_string(),
            format: "markdown".to_string(),
            source: "configurable-dimension-test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
            title: Some(content.to_string()),
        })
        .await
        .expect("insert note")
}

async fn build_index_job_count(db: &Database, dimension: i32, vector_type: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM public.job_queue
         WHERE job_type = 'build_set_index'
           AND payload->>'schema' = current_schema()
           AND (payload->>'dimension')::int = $1
           AND payload->>'vector_type' = $2",
    )
    .bind(dimension)
    .bind(vector_type)
    .fetch_one(&db.pool)
    .await
    .expect("count vector index jobs")
}

async fn run_build_index_jobs(db: &Database) {
    let payloads: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT payload FROM public.job_queue
         WHERE job_type = 'build_set_index'
         ORDER BY created_at, id",
    )
    .fetch_all(&db.pool)
    .await
    .expect("load vector index jobs");
    for payload in payloads {
        let payload: VectorIndexJobPayload =
            serde_json::from_value(payload).expect("valid vector index payload");
        build_vector_indexes(&db.pool, payload)
            .await
            .expect("build vector index");
    }
}

#[tokio::test]
async fn embedding_config_space_contract_computes_space_id_and_sets_inherit_it() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();
    let contract = serde_json::json!({
        "provider": "ollama",
        "pipeline": { "truncate": 1024 },
        "normalization": "provider-native",
        "model": "mxbai"
    });
    let expected_space_id = matric_core::embedding_space_id(&contract);
    let mut request = config_request(
        &format!("space-contract-{unique}"),
        1024,
        EmbeddingVectorType::Vector,
    );
    request.space_contract = Some(contract.clone());

    let config = db
        .embedding_sets
        .create_config(request)
        .await
        .expect("create config with space contract");
    assert_eq!(config.space_contract, Some(contract));
    assert_eq!(config.space_id.as_deref(), Some(expected_space_id.as_str()));

    let set = db
        .embedding_sets
        .create(set_request(&format!("Space Set {unique}"), config.id))
        .await
        .expect("create set inheriting space contract");
    assert_eq!(set.space_id.as_deref(), Some(expected_space_id.as_str()));

    let mut filtered = db.pool.begin().await.expect("begin list transaction");
    let sets = db
        .embedding_sets
        .list_by_space_id_tx(&mut filtered, Some(&expected_space_id))
        .await
        .expect("list sets by space id");
    assert!(sets.iter().any(|summary| summary.id == set.id));
}

#[tokio::test]
async fn embedding_config_rejects_mismatched_client_space_id() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();
    let mut request = config_request(
        &format!("space-mismatch-{unique}"),
        1024,
        EmbeddingVectorType::Vector,
    );
    request.space_contract = Some(serde_json::json!({"model": "mxbai"}));
    request.space_id =
        Some("0000000000000000000000000000000000000000000000000000000000000000".to_string());

    let error = db
        .embedding_sets
        .create_config(request)
        .await
        .expect_err("mismatched space_id must be rejected");
    // Error display redacts messages; match on the variant's payload.
    assert!(matches!(
        &error,
        matric_core::Error::InvalidInput(message)
            if message.contains("space_id does not match canonical space_contract hash")
    ));
}

#[tokio::test]
async fn vector_and_halfvec_sets_queue_shape_indexes_and_search() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();
    let initial_vector_build_jobs = build_index_job_count(&db, 1024, "vector").await;

    let vector_config = db
        .embedding_sets
        .create_config(config_request(
            &format!("vector-1024-{unique}"),
            1024,
            EmbeddingVectorType::Vector,
        ))
        .await
        .expect("create vector config");
    let vector_set = db
        .embedding_sets
        .create(set_request(
            &format!("Vector 1024 {unique}"),
            vector_config.id,
        ))
        .await
        .expect("create vector set");
    let after_first_vector_set = build_index_job_count(&db, 1024, "vector").await;
    assert!(
        after_first_vector_set == initial_vector_build_jobs
            || after_first_vector_set == initial_vector_build_jobs + 1,
        "first set for a shape may enqueue the shape build unless it already exists"
    );
    let _same_shape_set = db
        .embedding_sets
        .create(set_request(
            &format!("Vector 1024 same shape {unique}"),
            vector_config.id,
        ))
        .await
        .expect("create second vector set with same shape");
    assert_eq!(
        build_index_job_count(&db, 1024, "vector").await,
        after_first_vector_set,
        "second set with the same shape must not enqueue another build"
    );
    let vector_note = insert_note(&db, "vector 1024 note").await;
    let mut vector_values = vec![0.0_f32; 1024];
    vector_values[0] = 1.0;
    db.embeddings
        .store_for_set(
            vector_note,
            vector_set.id,
            vec![(
                "vector chunk".to_string(),
                Vector::from(vector_values.clone()),
            )],
            &vector_config.model,
        )
        .await
        .expect("store 1024 vector");
    let (chunk_hash, doc_hash): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT chunk_hash, doc_hash FROM embedding WHERE note_id = $1 AND embedding_set_id = $2",
    )
    .bind(vector_note)
    .bind(vector_set.id)
    .fetch_one(&db.pool)
    .await
    .expect("read embedding hashes");
    let expected_chunk_hash = matric_core::embedding_chunk_hash("vector chunk");
    let expected_doc_hash = matric_core::embedding_doc_hash("vector chunk");
    assert_eq!(chunk_hash.as_deref(), Some(expected_chunk_hash.as_str()));
    assert_eq!(doc_hash.as_deref(), Some(expected_doc_hash.as_str()));
    let vector_hits = db
        .embeddings
        .find_similar_in_set(&Vector::from(vector_values), vector_set.id, 5, false)
        .await
        .expect("search 1024 vector");
    assert!(vector_hits.iter().any(|hit| hit.note_id == vector_note));

    let initial_halfvec_build_jobs = build_index_job_count(&db, 2560, "halfvec").await;
    let halfvec_config = db
        .embedding_sets
        .create_config(config_request(
            &format!("halfvec-2560-{unique}"),
            2560,
            EmbeddingVectorType::Halfvec,
        ))
        .await
        .expect("create halfvec config");
    let halfvec_set = db
        .embedding_sets
        .create(set_request(
            &format!("Halfvec 2560 {unique}"),
            halfvec_config.id,
        ))
        .await
        .expect("create halfvec set");
    let after_halfvec_set = build_index_job_count(&db, 2560, "halfvec").await;
    assert!(
        after_halfvec_set == initial_halfvec_build_jobs
            || after_halfvec_set == initial_halfvec_build_jobs + 1,
        "first halfvec set for a shape may enqueue the shape build unless it already exists"
    );
    let halfvec_note = insert_note(&db, "halfvec 2560 note").await;
    let mut halfvec_values = vec![0.0_f32; 2560];
    halfvec_values[0] = 1.0;
    db.embeddings
        .store_for_set(
            halfvec_note,
            halfvec_set.id,
            vec![(
                "halfvec chunk".to_string(),
                Vector::from(halfvec_values.clone()),
            )],
            &halfvec_config.model,
        )
        .await
        .expect("store 2560 halfvec");
    let halfvec_hits = db
        .embeddings
        .find_similar_in_set(&Vector::from(halfvec_values), halfvec_set.id, 5, false)
        .await
        .expect("search 2560 halfvec");
    assert!(halfvec_hits.iter().any(|hit| hit.note_id == halfvec_note));
    let before_status: String =
        sqlx::query_scalar("SELECT index_status::text FROM embedding_set WHERE id = $1")
            .bind(vector_set.id)
            .fetch_one(&db.pool)
            .await
            .expect("read pending index status");
    assert!(
        before_status == "pending" || before_status == "ready",
        "index status before build should be pending for a new shape or ready for an existing shape"
    );

    run_build_index_jobs(&db).await;
    let after_status: String =
        sqlx::query_scalar("SELECT index_status::text FROM embedding_set WHERE id = $1")
            .bind(vector_set.id)
            .fetch_one(&db.pool)
            .await
            .expect("read ready index status");
    assert_eq!(after_status, "ready");

    let index_defs: Vec<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes
         WHERE schemaname = current_schema()
           AND indexname = ANY($1::text[])
         ORDER BY indexname",
    )
    .bind(vec![
        "idx_embedding_hnsw_vector_1024".to_string(),
        "idx_embedding_hnsw_halfvec_2560".to_string(),
    ])
    .fetch_all(&db.pool)
    .await
    .expect("fetch index definitions");
    assert!(index_defs.iter().any(|def| def.contains("hnsw")
        && def.contains("vector(1024)")
        && def.contains("cosine_ops")));
    assert!(index_defs.iter().any(|def| def.contains("hnsw")
        && def.contains("halfvec(2560)")
        && def.contains("cosine_ops")));

    let mut explain_conn = db.pool.acquire().await.expect("acquire explain connection");
    sqlx::query("ANALYZE embedding")
        .execute(&mut *explain_conn)
        .await
        .expect("analyze embedding");
    sqlx::query("SET enable_seqscan = off")
        .execute(&mut *explain_conn)
        .await
        .expect("disable seqscan");
    let explain_rows: Vec<String> = sqlx::query_scalar(
        "EXPLAIN SELECT id FROM embedding
         WHERE embedding_set_id = $1
           AND vector IS NOT NULL
           AND vector_dims(vector) = 1024
         ORDER BY (vector::vector(1024)) <=> $2::vector(1024)
         LIMIT 5",
    )
    .bind(vector_set.id)
    .bind(Vector::from(vec![1.0_f32; 1024]))
    .fetch_all(&mut *explain_conn)
    .await
    .expect("explain vector search");
    let explain_plan = explain_rows.join("\n");
    if !explain_plan.contains("idx_embedding_hnsw_vector_1024") {
        let index_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pg_indexes
                 WHERE schemaname = current_schema()
                   AND indexname = 'idx_embedding_hnsw_vector_1024'
            )",
        )
        .fetch_one(&db.pool)
        .await
        .expect("check vector shape index exists");
        assert!(
            index_exists,
            "dims-scoped query should have a shape partial index available"
        );
    }
}

#[tokio::test]
async fn defer_index_build_suppresses_until_manual_build() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();
    let config = db
        .embedding_sets
        .create_config(config_request(
            &format!("defer-vector-1024-{unique}"),
            1024,
            EmbeddingVectorType::Vector,
        ))
        .await
        .expect("create defer config");
    let mut request = set_request(&format!("Deferred Vector {unique}"), config.id);
    request.defer_index_build = true;
    let set = db
        .embedding_sets
        .create(request)
        .await
        .expect("create deferred set");
    assert_eq!(build_index_job_count(&db, 1024, "vector").await, 0);

    let mut tx = db.pool.begin().await.expect("begin build-index tx");
    matric_db::vector_index::clear_defer_and_enqueue_for_set_tx(&mut tx, "public", set.id)
        .await
        .expect("queue manual build");
    tx.commit().await.expect("commit build-index tx");
    assert_eq!(build_index_job_count(&db, 1024, "vector").await, 1);
}

#[tokio::test]
async fn config_creation_rejects_dimensions_above_type_limits() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();

    let vector_error = db
        .embedding_sets
        .create_config(config_request(
            &format!("vector-2001-{unique}"),
            2001,
            EmbeddingVectorType::Vector,
        ))
        .await
        .expect_err("vector dimension above 2000 must fail");
    let matric_db::Error::InvalidInput(vector_message) = vector_error else {
        panic!("expected InvalidInput for vector dimension");
    };
    assert!(vector_message.contains("expected 1..=2000"));

    let halfvec_error = db
        .embedding_sets
        .create_config(config_request(
            &format!("halfvec-4001-{unique}"),
            4001,
            EmbeddingVectorType::Halfvec,
        ))
        .await
        .expect_err("halfvec dimension above 4000 must fail");
    let matric_db::Error::InvalidInput(halfvec_message) = halfvec_error else {
        panic!("expected InvalidInput for halfvec dimension");
    };
    assert!(halfvec_message.contains("expected 1..=4000"));
}

#[sqlx::test(migrations = false)]
async fn existing_768_rows_survive_configurable_dimension_migration(pool: sqlx::PgPool) {
    sqlx::raw_sql(
        r#"
        CREATE EXTENSION IF NOT EXISTS vector;
        CREATE TABLE public.archive_registry(schema_name text NOT NULL);
        CREATE TABLE public.embedding_config(
            id uuid PRIMARY KEY,
            name text NOT NULL,
            model text NOT NULL,
            dimension integer NOT NULL,
            hnsw_m integer,
            hnsw_ef_construction integer,
            is_default boolean NOT NULL DEFAULT false
        );
        CREATE TABLE public.embedding_set(
            id uuid PRIMARY KEY,
            embedding_config_id uuid REFERENCES public.embedding_config(id)
        );
        CREATE TABLE public.embedding(
            id uuid PRIMARY KEY,
            note_id uuid NOT NULL,
            chunk_index integer NOT NULL,
            text text NOT NULL,
            vector vector(768),
            model text NOT NULL,
            embedding_set_id uuid REFERENCES public.embedding_set(id)
        );
        CREATE INDEX idx_embedding_vector ON public.embedding USING ivfflat (vector vector_cosine_ops);
        "#,
    )
    .execute(&pool)
    .await
    .expect("create pre-migration fixture");

    let config_id = Uuid::new_v4();
    let set_id = Uuid::new_v4();
    let embedding_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.embedding_config(id, name, model, dimension, is_default)
         VALUES($1, 'default', 'nomic-embed-text', 768, true)",
    )
    .bind(config_id)
    .execute(&pool)
    .await
    .expect("insert config");
    sqlx::query("INSERT INTO public.embedding_set(id, embedding_config_id) VALUES($1, $2)")
        .bind(set_id)
        .bind(config_id)
        .execute(&pool)
        .await
        .expect("insert set");
    sqlx::query(
        "INSERT INTO public.embedding(id, note_id, chunk_index, text, vector, model, embedding_set_id)
         VALUES($1, $2, 0, 'legacy', $3, 'nomic-embed-text', $4)",
    )
    .bind(embedding_id)
    .bind(Uuid::new_v4())
    .bind(Vector::from(vec![0.25_f32; 768]))
    .bind(set_id)
    .execute(&pool)
    .await
    .expect("insert legacy embedding");

    sqlx::raw_sql(MIGRATION)
        .execute(&pool)
        .await
        .expect("apply configurable dimension migration");

    let (dims, column_type): (i32, String) = sqlx::query_as(
        "SELECT vector_dims(vector), format_type(a.atttypid, a.atttypmod)
         FROM public.embedding e
         JOIN pg_attribute a ON a.attrelid = 'public.embedding'::regclass
            AND a.attname = 'vector'
         WHERE e.id = $1",
    )
    .bind(embedding_id)
    .fetch_one(&pool)
    .await
    .expect("read migrated embedding");
    assert_eq!(dims, 768);
    assert_eq!(column_type, "vector");
}

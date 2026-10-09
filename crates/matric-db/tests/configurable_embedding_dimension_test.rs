#![cfg(feature = "migrations")]

use matric_db::{
    create_pool, test_fixtures::DEFAULT_TEST_DATABASE_URL, AutoEmbedRules,
    CreateEmbeddingConfigRequest, CreateEmbeddingSetRequest, Database, EmbeddingSetAgentMetadata,
    EmbeddingSetCriteria, EmbeddingSetMode, EmbeddingSetType, EmbeddingVectorType, NoteRepository,
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
    Database::new(pool)
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

#[tokio::test]
async fn vector_and_halfvec_sets_store_index_and_search() {
    let db = setup_test_db().await;
    let unique = Uuid::new_v4().simple().to_string();

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
    let vector_hits = db
        .embeddings
        .find_similar_in_set(&Vector::from(vector_values), vector_set.id, 5, false)
        .await
        .expect("search 1024 vector");
    assert!(vector_hits.iter().any(|hit| hit.note_id == vector_note));

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

    let index_defs: Vec<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes
         WHERE schemaname = current_schema()
           AND indexname = ANY($1::text[])
         ORDER BY indexname",
    )
    .bind(vec![
        format!("idx_embedding_hnsw_{}", vector_config.id.simple()),
        format!("idx_embedding_hnsw_{}", halfvec_config.id.simple()),
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

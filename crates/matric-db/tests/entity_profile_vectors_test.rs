#![cfg(feature = "migrations")]

use matric_core::metadata_search::MetadataPredicates;
use matric_core::{new_v7, CreateNoteRequest, NoteRepository};
use matric_db::test_fixtures::DEFAULT_TEST_DATABASE_URL;
use matric_db::vector_index::{build_vector_indexes, VectorIndexJobPayload};
use matric_db::{
    create_pool, explain_similar_profiles_for_note_tx, find_similar_profiles_for_note_tx,
    note_id_for_source_identity_tx, upsert_profile_vectors_tx, Database, EntityProfileVectorRow,
    EntitySimilarityFilter,
};
use pgvector::Vector;
use serde_json::json;
use uuid::Uuid;

async fn setup_test_db() -> Database {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    let pool = create_pool(&database_url)
        .await
        .expect("failed to connect to test database");
    Database::new(pool)
}

async fn default_set_id(db: &Database) -> Uuid {
    sqlx::query_scalar("SELECT get_default_embedding_set_id()")
        .fetch_one(&db.pool)
        .await
        .expect("default embedding set")
}

async fn insert_note(db: &Database, title: &str, role: &str) -> Uuid {
    db.notes
        .insert(CreateNoteRequest {
            content: format!("{title} content"),
            format: "markdown".to_string(),
            source: "entity-profile-test".to_string(),
            collection_id: None,
            tags: None,
            metadata: Some(json!({ "role": role })),
            document_type_id: None,
            title: Some(title.to_string()),
        })
        .await
        .expect("insert note")
}

fn vector(first: f32, second: f32) -> Vector {
    let mut values = vec![0.0_f32; 768];
    values[0] = first;
    values[1] = second;
    Vector::from(values)
}

fn profile_row(note_id: Uuid, hash: &str, first: f32, second: f32) -> EntityProfileVectorRow {
    EntityProfileVectorRow {
        note_id,
        profile_hash: hash.to_string(),
        template_version: "entity-profile-v1".to_string(),
        profile_text: format!("profile {hash}"),
        vector: vector(first, second),
    }
}

#[tokio::test]
async fn profile_upsert_is_hash_idempotent_and_unique_per_note_set() {
    let db = setup_test_db().await;
    let set_id = default_set_id(&db).await;
    let note_id = insert_note(&db, "Idempotent Profile", "customer").await;
    let mut connection = db.pool.acquire().await.expect("acquire connection");

    let outcome = upsert_profile_vectors_tx(
        &mut connection,
        set_id,
        vec![profile_row(note_id, "hash-a", 1.0, 0.0)],
    )
    .await
    .expect("insert profile");
    assert_eq!(outcome.inserted, 1);

    let outcome = upsert_profile_vectors_tx(
        &mut connection,
        set_id,
        vec![profile_row(note_id, "hash-a", 0.0, 1.0)],
    )
    .await
    .expect("same hash is unchanged");
    assert_eq!(outcome.unchanged, 1);

    let vector_text: String = sqlx::query_scalar(
        "SELECT vector::text FROM embedding
         WHERE note_id = $1 AND embedding_set_id = $2 AND vector_kind = 'profile'",
    )
    .bind(note_id)
    .bind(set_id)
    .fetch_one(&db.pool)
    .await
    .expect("read profile vector");
    assert!(vector_text.starts_with("[1,0,0,"));

    let duplicate = sqlx::query(
        "INSERT INTO embedding (
             id, note_id, embedding_set_id, chunk_index, text, vector, model,
             vector_kind, template_version, profile_hash, profile_text
         )
         VALUES ($1, $2, $3, -1, 'duplicate', $4, 'duplicate-model',
             'profile', 'entity-profile-v1', 'hash-duplicate', 'duplicate')",
    )
    .bind(new_v7())
    .bind(note_id)
    .bind(set_id)
    .bind(vector(0.0, 1.0))
    .execute(&db.pool)
    .await;
    assert!(duplicate.is_err());
}

#[tokio::test]
async fn profile_similarity_orders_filters_and_resolves_external_identity() {
    let db = setup_test_db().await;
    let set_id = default_set_id(&db).await;
    let query = insert_note(&db, "Query Entity", "customer").await;
    let near = insert_note(&db, "Near Entity", "customer").await;
    let far = insert_note(&db, "Far Entity", "customer").await;
    let filtered = insert_note(&db, "Filtered Entity", "internal").await;
    let mut connection = db.pool.acquire().await.expect("acquire connection");
    let external_id = format!("external-query-{}", Uuid::new_v4().simple());

    upsert_profile_vectors_tx(
        &mut connection,
        set_id,
        vec![
            profile_row(query, "hash-query", 1.0, 0.0),
            profile_row(near, "hash-near", 0.99, 0.01),
            profile_row(far, "hash-far", 0.2, 0.8),
            profile_row(filtered, "hash-filtered", 0.98, 0.02),
        ],
    )
    .await
    .expect("upsert profiles");

    sqlx::query(
        "INSERT INTO source_identity (
            source_namespace, external_id, note_id, source_id,
            source_schema_version, content_digest, import_run_id
         )
         VALUES ('entity-test', $2, $1, 'source-system',
             '1.0.0', $3, 'run-1')",
    )
    .bind(query)
    .bind(&external_id)
    .bind(format!("sha256:{}", "0".repeat(64)))
    .execute(&db.pool)
    .await
    .expect("insert source identity");

    let metadata = MetadataPredicates::try_from(json!([
        { "path": "role", "op": "eq", "value": "customer" }
    ]))
    .expect("metadata predicates");
    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("begin");
    let hits = find_similar_profiles_for_note_tx(
        &mut connection,
        query,
        set_id,
        10,
        EntitySimilarityFilter {
            metadata: Some(metadata),
            strict: None,
            legacy_filters: String::new(),
        },
        vec!["role".to_string()],
    )
    .await
    .expect("find similar profiles");
    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .expect("rollback");

    let ids = hits.iter().map(|hit| hit.note_id).collect::<Vec<_>>();
    assert_eq!(ids, vec![near, far]);
    assert!(!ids.contains(&query));
    assert!(!ids.contains(&filtered));
    assert_eq!(hits[0].metadata["role"], "customer");

    let resolved = note_id_for_source_identity_tx(&mut connection, "entity-test", &external_id)
        .await
        .expect("resolve external id");
    assert_eq!(resolved, query);
}

#[tokio::test]
async fn chunk_similarity_ignores_profile_only_rows_and_explain_uses_shape_index() {
    let db = setup_test_db().await;
    let set_id = default_set_id(&db).await;
    let query = insert_note(&db, "Query Chunk", "customer").await;
    let chunked = insert_note(&db, "Chunked Entity", "customer").await;
    let profile_only = insert_note(&db, "Profile Only Entity", "customer").await;
    let mut connection = db.pool.acquire().await.expect("acquire connection");

    upsert_profile_vectors_tx(
        &mut connection,
        set_id,
        vec![
            profile_row(query, "hash-query-explain", 1.0, 0.0),
            profile_row(chunked, "hash-chunked-explain", 0.9, 0.1),
            profile_row(profile_only, "hash-profile-only", 1.0, 0.0),
        ],
    )
    .await
    .expect("upsert profiles");

    db.embeddings
        .store_for_set(
            chunked,
            set_id,
            vec![("body chunk".to_string(), vector(0.0, 1.0))],
            "body-model",
        )
        .await
        .expect("store body chunk");

    let chunk_hits = db
        .embeddings
        .find_similar_in_set(&vector(1.0, 0.0), set_id, 10, true)
        .await
        .expect("chunk similarity");
    assert!(chunk_hits.iter().all(|hit| hit.note_id != profile_only));

    build_vector_indexes(
        &db.pool,
        VectorIndexJobPayload {
            schema: "public".to_string(),
            dimension: 768,
            vector_type: "vector".to_string(),
            hnsw_m: 16,
            hnsw_ef_construction: 64,
            force: true,
            reason: Some("entity_profile_vectors_test".to_string()),
        },
    )
    .await
    .expect("build vector index");

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("begin");
    let plan = explain_similar_profiles_for_note_tx(
        &mut connection,
        query,
        set_id,
        10,
        EntitySimilarityFilter {
            metadata: None,
            strict: None,
            legacy_filters: String::new(),
        },
    )
    .await
    .expect("explain similar profiles")
    .join("\n");
    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .expect("rollback");
    assert!(plan.contains("idx_embedding_hnsw_vector_768"), "{plan}");
}

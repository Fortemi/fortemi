use super::*;
use embedding_dimension_tests::database;
use matric_core::{EmbeddingRepository, NoteRepository, SearchHit, StrictTagFilter, Vector};
use matric_db::search_candidates::{vector_on_connection, SearchCandidateScope};

fn ranking(hits: Vec<SearchHit>) -> Vec<(Uuid, f32)> {
    hits.into_iter()
        .map(|hit| (hit.note_id, hit.score))
        .collect()
}

async fn assert_all_search_routes(
    db: &Database,
    set_id: Uuid,
    query: &Vector,
    expected: &[(Uuid, f32)],
) {
    let scope = SearchCandidateScope {
        embedding_set_id: Some(set_id),
        ..Default::default()
    };
    let mut connection = db.pool.acquire().await.unwrap();
    assert_eq!(
        ranking(
            vector_on_connection(&mut connection, query, 10, &scope)
                .await
                .unwrap()
        ),
        expected
    );
    assert_eq!(
        ranking(
            vector_on_connection(&mut connection, query, 10, &SearchCandidateScope::default())
                .await
                .unwrap()
        ),
        expected
    );
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_in_set(query, set_id, 10, true)
                .await
                .unwrap()
        ),
        expected
    );
    assert_eq!(
        ranking(db.embeddings.find_similar(query, 10, true).await.unwrap()),
        expected
    );
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_with_vectors(query, 10, true)
                .await
                .unwrap()
                .into_iter()
                .map(|(hit, _)| hit)
                .collect()
        ),
        expected
    );
    let filter = StrictTagFilter::default().exclude_concept(Uuid::new_v4());
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_with_strict_filter(query, &filter, 10, true)
                .await
                .unwrap()
        ),
        expected
    );
    let mut tx = db.pool.begin().await.unwrap();
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_tx(&mut tx, query, 10, true)
                .await
                .unwrap()
        ),
        expected
    );
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_with_vectors_tx(&mut tx, query, 10, true)
                .await
                .unwrap()
                .into_iter()
                .map(|(hit, _)| hit)
                .collect()
        ),
        expected
    );
    assert_eq!(
        ranking(
            db.embeddings
                .find_similar_with_strict_filter_tx(&mut tx, query, &filter, 10, true)
                .await
                .unwrap()
        ),
        expected
    );
    tx.rollback().await.unwrap();
}

async fn note(db: &Database, title: &str) -> Uuid {
    db.notes
        .insert(matric_core::CreateNoteRequest {
            content: title.into(),
            format: "markdown".into(),
            source: "test".into(),
            collection_id: None,
            document_type_id: None,
            title: Some(title.into()),
            tags: None,
            metadata: None,
        })
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn internal_legacy_profiles_do_not_change_body_only_ranking_and_external_profiles_are_searchable(
    admin: sqlx::PgPool,
) {
    let db = database(admin).await;
    let config = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_config(id,name,model,dimension) VALUES($1,'selection','synthetic',3)")
        .bind(config).execute(&db.pool).await.unwrap();
    let set: Uuid = sqlx::query_scalar(
        "UPDATE embedding_set SET embedding_config_id=$1 WHERE slug='default' RETURNING id",
    )
    .bind(config)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let first = note(&db, "first body").await;
    let second = note(&db, "second body").await;
    let profile_only = note(&db, "legacy profile without body").await;
    for (id, vector) in [(first, vec![0.8, 0.6, 0.]), (second, vec![0.6, 0.8, 0.])] {
        db.embeddings
            .store_for_set(
                id,
                set,
                vec![("body".into(), Vector::from(vector))],
                "synthetic",
            )
            .await
            .unwrap();
    }
    let query = Vector::from(vec![1., 0., 0.]);
    // This is the origin/main body-only selection, also matching restored 2.1 ownership.
    let before: Vec<(Uuid, f32)> = sqlx::query_as(
        "SELECT note_id,(1.0 - (vector <=> $1))::real AS score FROM embedding
         WHERE embedding_set_id=$2 AND vector_kind='body_chunk' ORDER BY score DESC",
    )
    .bind(&query)
    .bind(set)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    let mut connection = db.pool.acquire().await.unwrap();
    matric_db::upsert_profile_vectors_tx(
        &mut connection,
        set,
        vec![
            matric_db::EntityProfileVectorRow {
                note_id: second,
                profile_hash: "second".into(),
                template_version: "entity-profile-v1".into(),
                profile_text: "profile".into(),
                vector: Vector::from(vec![1., 0., 0.]),
            },
            matric_db::EntityProfileVectorRow {
                note_id: profile_only,
                profile_hash: "only".into(),
                template_version: "entity-profile-v1".into(),
                profile_text: "profile".into(),
                vector: Vector::from(vec![0.9, 0.1, 0.]),
            },
        ],
    )
    .await
    .unwrap();
    drop(connection);
    assert_all_search_routes(&db, set, &query, &before).await;
    sqlx::query("UPDATE embedding_set SET vector_source='external' WHERE id=$1")
        .bind(set)
        .execute(&db.pool)
        .await
        .unwrap();
    let external: Vec<(Uuid, f32)> = sqlx::query_as(
        "SELECT note_id,MAX((1.0 - (vector <=> $1))::real) AS score FROM embedding
         WHERE embedding_set_id=$2 GROUP BY note_id ORDER BY score DESC",
    )
    .bind(&query)
    .bind(set)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        external.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [second, profile_only, first]
    );
    assert_all_search_routes(&db, set, &query, &external).await;
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn body_only_ranking_survives_ownership_flip_on_every_search_route(admin: sqlx::PgPool) {
    let db = database(admin).await;
    let set: Uuid = sqlx::query_scalar("SELECT id FROM embedding_set WHERE slug='default'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let query = Vector::from(vec![1.; 768]);
    for vector in [
        vec![1.; 768],
        (0..768).map(|i| if i == 0 { 1. } else { 0. }).collect(),
    ] {
        let id = note(&db, "synthetic ownership body").await;
        db.embeddings
            .store_for_set(
                id,
                set,
                vec![("body".into(), Vector::from(vector))],
                "synthetic",
            )
            .await
            .unwrap();
    }
    let before = ranking(
        db.embeddings
            .find_similar_in_set(&query, set, 10, true)
            .await
            .unwrap(),
    );
    assert_eq!(before.len(), 2);
    assert_all_search_routes(&db, set, &query, &before).await;
    sqlx::query("UPDATE embedding_set SET vector_source='external' WHERE id=$1")
        .bind(set)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_all_search_routes(&db, set, &query, &before).await;
    db.pool.close().await;
}

async fn diversified(
    db: &Database,
    set: Option<Uuid>,
    query: &Vector,
    limit: i64,
    diversity: f32,
) -> Vec<Uuid> {
    let config = matric_search::HybridSearchConfig {
        embedding_set_id: set,
        diversity: Some(diversity),
        // Chain dedup re-sorts by score and would mask MMR ordering.
        deduplication: matric_search::DeduplicationConfig {
            deduplicate_chains: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut connection = db.pool.acquire().await.unwrap();
    matric_search::HybridSearchEngine::new(db.clone())
        .search_on_connection(
            &mut connection,
            "zzqxnomatch",
            Some(query),
            "",
            limit,
            &config,
        )
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.hit.note_id)
        .collect()
}

#[sqlx::test(migrations = false)]
async fn mmr_loads_external_profile_vectors_and_keeps_internal_sets_body_only(admin: sqlx::PgPool) {
    let db = database(admin).await;
    let config = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO embedding_config(id,name,model,dimension) VALUES($1,'mmr','synthetic',3)",
    )
    .bind(config)
    .execute(&db.pool)
    .await
    .unwrap();
    let set: Uuid = sqlx::query_scalar(
        "UPDATE embedding_set SET embedding_config_id=$1 WHERE slug='default' RETURNING id",
    )
    .bind(config)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let body = note(&db, "body note").await;
    let profile_only = note(&db, "profile only note").await;
    db.embeddings
        .store_for_set(
            body,
            set,
            vec![("body".into(), Vector::from(vec![0.9, 0.436, 0.]))],
            "synthetic",
        )
        .await
        .unwrap();
    let mut connection = db.pool.acquire().await.unwrap();
    matric_db::upsert_profile_vectors_tx(
        &mut connection,
        set,
        vec![matric_db::EntityProfileVectorRow {
            note_id: profile_only,
            profile_hash: "only".into(),
            template_version: "entity-profile-v1".into(),
            profile_text: "profile".into(),
            vector: Vector::from(vec![1., 0., 0.]),
        }],
    )
    .await
    .unwrap();
    drop(connection);
    let query = Vector::from(vec![1., 0., 0.]);
    // Internal set: profiles are not searchable, so diversity keeps origin/main's body-only result.
    assert_eq!(diversified(&db, Some(set), &query, 1, 0.1).await, [body]);
    sqlx::query("UPDATE embedding_set SET vector_source='external' WHERE id=$1")
        .bind(set)
        .execute(&db.pool)
        .await
        .unwrap();
    // External set: MMR must load the profile vector; otherwise the vectorless top hit sinks below the body hit.
    assert_eq!(
        diversified(&db, Some(set), &query, 1, 0.1).await,
        [profile_only]
    );
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn unscoped_mmr_never_borrows_profiles_from_another_external_set(admin: sqlx::PgPool) {
    let db = database(admin).await;
    let config = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_config(id,name,model,dimension) VALUES($1,'mmr-unscoped','synthetic',3)")
        .bind(config).execute(&db.pool).await.unwrap();
    let default_set: Uuid = sqlx::query_scalar(
        "UPDATE embedding_set SET embedding_config_id=$1 WHERE slug='default' RETURNING id",
    )
    .bind(config)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_set(id,name,slug,set_type,mode,embedding_config_id,vector_source,defer_index_build)
        VALUES($1,'unrelated external','unrelated-external','full','manual',$2,'external',true)")
        .bind(other).bind(config).execute(&db.pool).await.unwrap();
    let top = note(&db, "top near duplicate").await;
    let near = note(&db, "second near duplicate").await;
    let distinct = note(&db, "distinct direction").await;
    // Older profile in an unrelated external set: lower embedding id than `near`'s body vector,
    // and it points at `distinct`'s direction, so borrowing it would make MMR skip `distinct`.
    let mut connection = db.pool.acquire().await.unwrap();
    matric_db::upsert_profile_vectors_tx(
        &mut connection,
        other,
        vec![matric_db::EntityProfileVectorRow {
            note_id: near,
            profile_hash: "unrelated".into(),
            template_version: "entity-profile-v1".into(),
            profile_text: "profile".into(),
            vector: Vector::from(vec![0., 1., 0.]),
        }],
    )
    .await
    .unwrap();
    drop(connection);
    for (id, vector) in [
        (top, vec![0.98, 0.2, 0.]),
        (near, vec![1., 0.05, 0.]),
        (distinct, vec![0., 1., 0.]),
    ] {
        db.embeddings
            .store_for_set(
                id,
                default_set,
                vec![("body".into(), Vector::from(vector))],
                "synthetic",
            )
            .await
            .unwrap();
    }
    let query = Vector::from(vec![1., 1., 0.]);
    // Default-set body vectors: `near` duplicates `top`, so diversity prefers `distinct` second.
    assert_eq!(
        diversified(&db, None, &query, 2, 0.9).await,
        [top, distinct]
    );
    db.pool.close().await;
}

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
    let role = format!("customer-{}", Uuid::new_v4().simple());
    let query = insert_note(&db, "Query Entity", &role).await;
    let near = insert_note(&db, "Near Entity", &role).await;
    let far = insert_note(&db, "Far Entity", &role).await;
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
        { "path": "role", "op": "eq", "value": role }
    ]))
    .expect("metadata predicates");
    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("begin");
    sqlx::query("UPDATE embedding_set SET vector_source='external' WHERE id=$1")
        .bind(set_id)
        .execute(&mut *connection)
        .await
        .unwrap();
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
    assert_eq!(hits[0].metadata["role"], role);

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
    sqlx::query("ANALYZE embedding")
        .execute(&db.pool)
        .await
        .expect("analyze embedding");

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("begin");
    sqlx::query("UPDATE embedding_set SET vector_source='external' WHERE id=$1")
        .bind(set_id)
        .execute(&mut *connection)
        .await
        .unwrap();
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
    if !plan.contains("idx_embedding_hnsw_vector_768") {
        let index_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pg_indexes
                 WHERE schemaname = current_schema()
                   AND indexname = 'idx_embedding_hnsw_vector_768'
            )",
        )
        .fetch_one(&db.pool)
        .await
        .expect("check vector 768 index exists");
        assert!(index_exists, "{plan}");
    }
}

/// Private migrated PostgreSQL only. Synthetic archive tables have the shipped
/// column shapes and forced RLS, but no import/job triggers or external services.
#[tokio::test]
async fn profile_similarity_tenant_archive_eligibility_bounds_and_pool_reuse() {
    use matric_db::entity_vectors::require_profile_similarity_archive_tx;
    assert_eq!(
        std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
        Ok("1")
    );
    let url = std::env::var("DATABASE_URL").expect("runner-owned migrated PostgreSQL");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let baseline: (i32, String, Option<String>) = sqlx::query_as(
        "SELECT pg_backend_pid(), current_setting('search_path'), current_setting('app.current_tenant', true)",
    ).fetch_one(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let (tenant, config): (Uuid, Uuid) = sqlx::query_as(
        "SELECT tenant_id, id FROM public.embedding_config WHERE dimension=768 AND vector_type='vector' LIMIT 1",
    ).fetch_one(&mut *tx).await.expect("migrated 768-dimensional fixture configuration");
    let foreign = Uuid::new_v4();
    let role = format!("profile_{}", Uuid::new_v4().simple());
    let a = format!("archive_{}", Uuid::new_v4().simple());
    let b = format!("archive_{}", Uuid::new_v4().simple());
    let relations = [
        "note",
        "embedding",
        "embedding_set",
        "embedding_set_member",
        "source_identity",
        "note_tag",
        "note_skos_concept",
        "tag",
        "skos_concept",
        "skos_concept_label",
        "skos_concept_scheme",
    ];
    sqlx::query(&format!(
        "CREATE ROLE {role} NOLOGIN NOSUPERUSER NOBYPASSRLS"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT ON public.embedding_config TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let set = Uuid::new_v4();
    let foreign_set = Uuid::new_v4();
    let root = Uuid::new_v4();
    let foreign_root = Uuid::new_v4();
    let deleted_root = Uuid::new_v4();
    let empty_root = Uuid::new_v4();
    for schema in [&a, &b] {
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        for relation in relations {
            for statement in [
                format!("CREATE TABLE {schema}.{relation} (LIKE public.{relation} INCLUDING DEFAULTS)"),
                format!("ALTER TABLE {schema}.{relation} ENABLE ROW LEVEL SECURITY"),
                format!("ALTER TABLE {schema}.{relation} FORCE ROW LEVEL SECURITY"),
                format!("CREATE POLICY tenant_isolation ON {schema}.{relation} USING (tenant_id=current_setting('app.current_tenant')::uuid)"),
                format!("GRANT SELECT ON {schema}.{relation} TO {role}"),
            ] {
                sqlx::query(&statement).execute(&mut *tx).await.unwrap();
            }
        }
        sqlx::query(&format!("GRANT USAGE ON SCHEMA {schema} TO {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        // The same root/set UUIDs in the other archive must not contribute vectors.
        for (id, owner) in [(set, tenant), (foreign_set, foreign)] {
            sqlx::query(&format!(
                "INSERT INTO {schema}.embedding_set
                (id,tenant_id,name,slug,embedding_config_id,vector_source)
                VALUES ($1,$2,$1::text,$1::text,$3,'external')"
            ))
            .bind(id)
            .bind(owner)
            .bind(config)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        for (id, owner, deleted) in [
            (root, tenant, false),
            (foreign_root, foreign, false),
            (deleted_root, tenant, true),
            (empty_root, tenant, false),
        ] {
            sqlx::query(&format!("INSERT INTO {schema}.note
                (id,tenant_id,format,source,created_at_utc,updated_at_utc,deleted_at)
                VALUES ($1,$2,'markdown','profile-isolation',now(),now(),CASE WHEN $3 THEN now() END)"))
                .bind(id).bind(owner).bind(deleted).execute(&mut *tx).await.unwrap();
            if id != empty_root {
                isolation_profile(&mut tx, schema, owner, id, set, vector(1.0, 0.0)).await;
            }
        }
    }
    let mut eligible = Vec::new();
    for index in 0..105 {
        let id = Uuid::new_v4();
        eligible.push(id);
        sqlx::query(&format!(
            "INSERT INTO {a}.note
            (id,tenant_id,format,source,created_at_utc,updated_at_utc,metadata)
            VALUES ($1,$2,'markdown','profile-isolation',now(),now(),'{{\"role\":\"allowed\"}}')"
        ))
        .bind(id)
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
        isolation_profile(
            &mut tx,
            &a,
            tenant,
            id,
            set,
            vector(0.8, 0.6 + index as f32 / 1000.0),
        )
        .await;
        sqlx::query(&format!("INSERT INTO {a}.embedding_set_member (tenant_id,note_id,embedding_set_id) VALUES ($1,$2,$3)"))
            .bind(tenant).bind(id).bind(set).execute(&mut *tx).await.unwrap();
    }
    // Closer than every allowed result, but ineligible before LIMIT: missing
    // membership, wrong metadata, archived, deleted, wrong dimension, other tenant.
    for reason in [
        "nonmember",
        "metadata",
        "archived",
        "deleted",
        "dimension",
        "tenant",
        "kind",
        "set",
    ] {
        let id = Uuid::new_v4();
        let owner = if reason == "tenant" { foreign } else { tenant };
        sqlx::query(&format!("INSERT INTO {a}.note
            (id,tenant_id,format,source,created_at_utc,updated_at_utc,metadata,archived,deleted_at)
            VALUES ($1,$2,'markdown','profile-isolation',now(),now(),$3,$4,CASE WHEN $5 THEN now() END)"))
            .bind(id).bind(owner)
            .bind(json!({"role": if reason == "metadata" { "denied" } else { "allowed" }}))
            .bind(reason == "archived").bind(reason == "deleted")
            .execute(&mut *tx).await.unwrap();
        let value = if reason == "dimension" {
            Vector::from(vec![1.0_f32, 0.0])
        } else {
            vector(1.0, 0.0)
        };
        isolation_profile(&mut tx, &a, owner, id, set, value).await;
        if reason == "kind" {
            sqlx::query(&format!(
                "UPDATE {a}.embedding SET vector_kind='body_chunk' WHERE note_id=$1"
            ))
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        } else if reason == "set" {
            sqlx::query(&format!(
                "UPDATE {a}.embedding SET embedding_set_id=$1 WHERE note_id=$2"
            ))
            .bind(foreign_set)
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        if reason != "nonmember" {
            sqlx::query(&format!("INSERT INTO {a}.embedding_set_member (tenant_id,note_id,embedding_set_id) VALUES ($1,$2,$3)"))
                .bind(owner).bind(id).bind(set).execute(&mut *tx).await.unwrap();
        }
    }
    for (id, owner) in [(eligible[0], tenant), (eligible[1], foreign)] {
        sqlx::query(&format!(
            "INSERT INTO {a}.note_tag (tenant_id,note_id,tag_name) VALUES ($1,$2,'first-only')"
        ))
        .bind(owner)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let enforces_rls: bool = sqlx::query_scalar(
        "SELECT NOT rolsuper AND NOT rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(enforces_rls);
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    for (schema, expected) in [(&a, 100), (&b, 0), (&a, 100)] {
        sqlx::query("SELECT set_config('search_path',$1,true)")
            .bind(format!("{schema}, public"))
            .execute(&mut *tx)
            .await
            .unwrap();
        require_profile_similarity_archive_tx(&mut tx, schema, false)
            .await
            .unwrap();
        let hits = find_similar_profiles_for_note_tx(
            &mut tx,
            root,
            set,
            i64::MAX,
            isolation_filter(),
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), expected);
        assert!(hits.iter().all(|hit| eligible.contains(&hit.note_id)));
        if expected > 0 {
            assert!((hits[0].score - 0.8).abs() < 0.00001, "actual cosine score");
        }
    }
    for k in [i64::MIN, 0, 1] {
        let hits = find_similar_profiles_for_note_tx(
            &mut tx,
            root,
            set,
            k,
            isolation_filter(),
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
    }
    for strict in [false, true] {
        let mut filter = isolation_filter();
        if strict {
            let mut tags = matric_core::StrictTagFilter::new();
            tags.required_string_tags.push("first-only".into());
            filter.strict = Some(tags);
        } else {
            filter.legacy_filters = "tag:first-only".into();
        }
        let hits = find_similar_profiles_for_note_tx(&mut tx, root, set, 100, filter, Vec::new())
            .await
            .unwrap();
        assert_eq!(
            hits.iter().map(|hit| hit.note_id).collect::<Vec<_>>(),
            vec![eligible[0]]
        );
    }
    for id in [foreign_root, deleted_root, Uuid::new_v4()] {
        assert!(matches!(
            find_similar_profiles_for_note_tx(&mut tx, id, set, 10, isolation_filter(), Vec::new())
                .await,
            Err(matric_core::Error::NotFound(_))
        ));
    }
    assert!(matches!(
        find_similar_profiles_for_note_tx(
            &mut tx,
            root,
            foreign_set,
            10,
            isolation_filter(),
            Vec::new()
        )
        .await,
        Err(matric_core::Error::NotFound(_))
    ));
    assert!(find_similar_profiles_for_note_tx(
        &mut tx,
        empty_root,
        set,
        10,
        isolation_filter(),
        Vec::new()
    )
    .await
    .unwrap()
    .is_empty());
    // Remove each local data relation in turn while public still has one. The
    // guard must fail instead of accepting that public fallback. Savepoints also
    // recover the PostgreSQL error so the next probe uses the same connection.
    sqlx::query("RESET ROLE").execute(&mut *tx).await.unwrap();
    for relation in relations {
        sqlx::query("SAVEPOINT missing_relation")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(&format!(
            "ALTER TABLE {a}.{relation} RENAME TO hidden_relation"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(&format!("SET LOCAL ROLE {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            require_profile_similarity_archive_tx(&mut tx, &a, false)
                .await
                .is_err(),
            "{relation}"
        );
        sqlx::query("ROLLBACK TO SAVEPOINT missing_relation")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.rollback().await.unwrap();
    let reused: (i32, String, Option<String>) = sqlx::query_as(
        "SELECT pg_backend_pid(), current_setting('search_path'), NULLIF(current_setting('app.current_tenant', true),'')",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        reused,
        (
            baseline.0,
            baseline.1,
            baseline.2.filter(|value| !value.is_empty())
        )
    );
    pool.close().await;
}

async fn isolation_profile(
    connection: &mut sqlx::PgConnection,
    schema: &str,
    tenant: Uuid,
    note: Uuid,
    set: Uuid,
    value: Vector,
) {
    sqlx::query(&format!(
        "INSERT INTO {schema}.embedding
        (id,tenant_id,note_id,embedding_set_id,chunk_index,text,vector,model,
         vector_kind,template_version,profile_hash,profile_text)
        VALUES ($1,$2,$3,$4,-1,'profile',$5,'fixture','profile','v1','hash','profile')"
    ))
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(note)
    .bind(set)
    .bind(value)
    .execute(connection)
    .await
    .unwrap();
}

fn isolation_filter() -> EntitySimilarityFilter {
    EntitySimilarityFilter {
        metadata: Some(
            MetadataPredicates::try_from(json!([
                {"path":"role","op":"eq","value":"allowed"}
            ]))
            .unwrap(),
        ),
        strict: None,
        legacy_filters: String::new(),
    }
}

use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(super) async fn export(state: &AppState, schema: &str, version: Option<&str>) -> Vec<u8> {
    let response = knowledge_shard(
        State(state.clone()),
        Extension(ArchiveContext {
            schema: schema.into(),
            is_default: false,
            name: None,
        }),
        Query(ShardExportQuery {
            schema_version: version.map(str::to_string),
            profile: Some("full-v1".into()),
            include: None,
            include_blobs: true,
        }),
    )
    .await
    .unwrap()
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

fn rewrite_sets(bytes: &[u8], mutate: impl FnOnce(&mut Vec<Value>)) -> Vec<u8> {
    rewrite_component(bytes, "embedding_sets.json", mutate)
}

fn rewrite_component(
    bytes: &[u8],
    component: &str,
    mutate: impl FnOnce(&mut Vec<Value>),
) -> Vec<u8> {
    let mut files = read_shard_archive(bytes, ShardArchiveLimits::default()).unwrap();
    let mut sets: Vec<Value> = serde_json::from_slice(&files[component]).unwrap();
    mutate(&mut sets);
    let data = serde_json::to_vec(&sets).unwrap();
    let mut manifest = parse_and_validate_shard_manifest(&files["manifest.json"]).unwrap();
    manifest
        .checksums
        .insert(component.into(), hex::encode(Sha256::digest(&data)));
    files.insert(component.into(), data);
    files.insert(
        "manifest.json".into(),
        serialize_shard_manifest(&manifest).unwrap(),
    );
    let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    for (path, data) in files {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(data.len() as u64);
        header.set_cksum();
        archive
            .append_data(&mut header, path, data.as_slice())
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

#[test]
fn query_template_schema_2_2_golden_corpus_and_pins() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../contracts/knowledge-shard/2.2.0/contract.json"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, digest) in receipt["schemaBundle"]["files"].as_object().unwrap() {
        assert_eq!(
            hex::encode(Sha256::digest(std::fs::read(root.join(path)).unwrap())),
            digest.as_str().unwrap(),
            "{path}"
        );
    }
    let corpus = &receipt["canonicalCorpus"];
    let bytes = std::fs::read(root.join(corpus["path"].as_str().unwrap())).unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), corpus["sha256"]);
    let sets: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    let schema =
        shard_component_schema(SHARD_SCHEMA_2_2_VERSION, "full-v1", "embedding_sets").unwrap();
    for value in &sets {
        validate_shard_json_schema(value, schema, "invalid template fixture").unwrap();
        let set: ShardEmbeddingSetRecord = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            set.query_template.as_deref(),
            value.get("query_template").and_then(Value::as_str)
        );
    }
    let config_corpus = &receipt["providerDimensionCorpus"];
    let bytes = std::fs::read(root.join(config_corpus["path"].as_str().unwrap())).unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), config_corpus["sha256"]);
    let configs: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    let schema =
        shard_component_schema(SHARD_SCHEMA_2_2_VERSION, "full-v1", "embedding_configs").unwrap();
    for config in &configs {
        validate_shard_json_schema(config, schema, "invalid config fixture").unwrap();
    }
    let mut invalid = configs[0].clone();
    invalid["provider_dimension"] = json!(16385);
    assert!(validate_shard_json_schema(&invalid, schema, "raw dimension bound").is_err());
    let schema =
        shard_component_schema(SHARD_SCHEMA_2_2_VERSION, "full-v1", "embedding_sets").unwrap();
    let mut value = sets[0].clone();
    for invalid in [
        json!("missing"),
        json!("{text}{text}"),
        json!(17),
        json!("a".repeat(2049)),
    ] {
        value["query_template"] = invalid;
        assert!(validate_shard_json_schema(&value, schema, "invalid template fixture").is_err());
    }
    value["query_template"] = Value::Null;
    validate_shard_json_schema(&value, schema, "invalid template fixture").unwrap();
    let old_schema =
        shard_component_schema(SHARD_SCHEMA_2_1_VERSION, "full-v1", "embedding_sets").unwrap();
    assert!(validate_shard_json_schema(&sets[0], old_schema, "unknown field").is_err());
}

#[sqlx::test(migrations = false)]
async fn full_v1_query_template_round_trip_and_older_shard_defaults(admin: sqlx::PgPool) {
    let db = embedding_dimension_tests::database(admin).await;
    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    let source = db
        .archives
        .create_archive_schema("template-source", None)
        .await
        .unwrap();
    let destination = db
        .archives
        .create_archive_schema("template-destination", None)
        .await
        .unwrap();
    let set_id = Uuid::new_v4();
    let config_id = Uuid::new_v4();
    let template = "Instruct: Find synthetic companies\nQuery:{text}";
    let ctx = db.for_schema(&source.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query(
        "INSERT INTO embedding_config (id,name,model,dimension,vector_type,shard_export_present)
        VALUES ($1,'synthetic shard config','synthetic-8',8,'vector',true)",
    )
    .bind(config_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO embedding_set
        (id,name,slug,set_type,mode,embedding_config_id,query_template,vector_source,defer_index_build,shard_export_present)
        VALUES ($1,'synthetic templated set','templated','full','manual',$2,$3,'external',true,true)")
        .bind(set_id).bind(config_id).bind(template).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    let bytes = export(&state, &source.schema_name, None).await;
    let files = read_shard_archive(&bytes, ShardArchiveLimits::default()).unwrap();
    let manifest = parse_and_validate_shard_manifest(&files["manifest.json"]).unwrap();
    assert_eq!(manifest.version, SHARD_SCHEMA_2_2_VERSION);
    assert_eq!(
        manifest.min_reader_version.as_deref(),
        Some(SHARD_SCHEMA_2_2_VERSION)
    );
    validate_shard_manifest_contract(&manifest).unwrap();
    validate_shard_component_inventory(&manifest, &files).unwrap();
    let sets: Vec<Value> = serde_json::from_slice(&files["embedding_sets.json"]).unwrap();
    assert!(sets
        .iter()
        .any(|set| set["id"] == set_id.to_string() && set["query_template"] == template));
    let opts = ShardImportOptions {
        include: None,
        dry_run: false,
        on_conflict: ConflictStrategy::Replace,
        skip_embedding_regen: true,
    };
    knowledge_shard_import_internal(&state, &bytes, &opts, &destination.schema_name)
        .await
        .unwrap();
    let restored = export(
        &state,
        &destination.schema_name,
        Some(SHARD_SCHEMA_2_2_VERSION),
    )
    .await;
    let restored_files = read_shard_archive(&restored, ShardArchiveLimits::default()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&restored_files["embedding_sets.json"]).unwrap(),
        serde_json::from_slice::<Value>(&files["embedding_sets.json"]).unwrap()
    );

    for version in ["1.2.0", SHARD_SCHEMA_2_VERSION, SHARD_SCHEMA_2_1_VERSION] {
        let result = knowledge_shard(
            State(state.clone()),
            Extension(ArchiveContext {
                schema: source.schema_name.clone(),
                is_default: false,
                name: None,
            }),
            Query(ShardExportQuery {
                schema_version: Some(version.into()),
                profile: Some("full-v1".into()),
                include: None,
                include_blobs: true,
            }),
        )
        .await;
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }
    // Rebound checksums ensure the failure exercises template preflight, not integrity.
    for invalid in [
        json!("missing placeholder"),
        json!("{text}{text}"),
        json!(format!("{}{text}", "é".repeat(1022), text = "{text}")),
    ] {
        let invalid_bytes = rewrite_sets(&bytes, |sets| {
            sets.iter_mut()
                .find(|set| set["id"] == set_id.to_string())
                .unwrap()["query_template"] = invalid;
        });
        assert!(knowledge_shard_import_internal(
            &state,
            &invalid_bytes,
            &opts,
            &destination.schema_name
        )
        .await
        .is_err());
        let ctx = db.for_schema(&destination.schema_name).unwrap();
        let mut tx = ctx.begin_tx().await.unwrap();
        let stored: Option<String> =
            sqlx::query_scalar("SELECT query_template FROM embedding_set WHERE id=$1")
                .bind(set_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some(template));
        tx.rollback().await.unwrap();
    }

    // A historical producer omits the field. Replace must clear a destination's
    // existing wrapper to the documented legacy NULL default.
    let ctx = db.for_schema(&source.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query(
        "UPDATE embedding_set SET query_template=NULL,vector_source='internal' WHERE id=$1",
    )
    .bind(set_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let older = export(&state, &source.schema_name, Some(SHARD_SCHEMA_2_1_VERSION)).await;
    let older_files = read_shard_archive(&older, ShardArchiveLimits::default()).unwrap();
    let older_sets: Vec<Value> =
        serde_json::from_slice(&older_files["embedding_sets.json"]).unwrap();
    assert!(older_sets
        .iter()
        .all(|set| set.get("query_template").is_none()));
    knowledge_shard_import_internal(&state, &older, &opts, &destination.schema_name)
        .await
        .unwrap();
    let ctx = db.for_schema(&destination.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let stored: Option<String> =
        sqlx::query_scalar("SELECT query_template FROM embedding_set WHERE id=$1")
            .bind(set_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(stored, None);
    let ownership: String =
        sqlx::query_scalar("SELECT vector_source FROM embedding_set WHERE id=$1")
            .bind(set_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(ownership, "internal");
    tx.rollback().await.unwrap();
    // Optional omission also remains valid in 2.2.0 with the same NULL default.
    let absent = rewrite_sets(&bytes, |sets| {
        for set in sets {
            set.as_object_mut().unwrap().remove("query_template");
        }
    });
    knowledge_shard_import_internal(&state, &absent, &opts, &destination.schema_name)
        .await
        .unwrap();
    db.pool.close().await;
}

pub(super) async fn assert_external_restore_search(
    state: &AppState,
    provider_url: &str,
    set_id: Uuid,
    expected_note: Uuid,
    baseline: &Value,
) {
    let bytes = export(state, "public", None).await;
    let files = read_shard_archive(&bytes, ShardArchiveLimits::default()).unwrap();
    let manifest = parse_and_validate_shard_manifest(&files["manifest.json"]).unwrap();
    assert_eq!(manifest.version, SHARD_SCHEMA_2_2_VERSION);
    let configs: Vec<Value> = serde_json::from_slice(&files["embedding_configs.json"]).unwrap();
    assert!(configs
        .iter()
        .any(|config| config["provider_dimension"] == 4096 && config["dimension"] == 1024));
    let sets: Vec<Value> = serde_json::from_slice(&files["embedding_sets.json"]).unwrap();
    assert!(sets
        .iter()
        .any(|set| set["id"] == set_id.to_string() && set["vector_source"] == "external"));
    let destination = state
        .db
        .archives
        .create_archive_schema("raw-provider-restore", None)
        .await
        .unwrap();
    for raw in [512, 16385] {
        let invalid = rewrite_component(&bytes, "embedding_configs.json", |configs| {
            let config = configs
                .iter_mut()
                .find(|config| config["provider_dimension"] == 4096)
                .unwrap();
            config["provider_dimension"] = json!(raw);
        });
        assert!(knowledge_shard_import_internal(
            state,
            &invalid,
            &ShardImportOptions {
                include: None,
                dry_run: false,
                on_conflict: ConflictStrategy::Replace,
                skip_embedding_regen: true,
            },
            &destination.schema_name
        )
        .await
        .is_err());
        let ctx = state.db.for_schema(&destination.schema_name).unwrap();
        let mut tx = ctx.begin_tx().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM embedding WHERE embedding_set_id=$1")
                .bind(set_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(count, 0, "invalid raw dimension must fail before writes");
        tx.rollback().await.unwrap();
    }
    knowledge_shard_import_internal(
        state,
        &bytes,
        &ShardImportOptions {
            include: None,
            dry_run: false,
            on_conflict: ConflictStrategy::Replace,
            skip_embedding_regen: true,
        },
        &destination.schema_name,
    )
    .await
    .unwrap();
    let restored = export(
        state,
        &destination.schema_name,
        Some(SHARD_SCHEMA_2_2_VERSION),
    )
    .await;
    let restored_files = read_shard_archive(&restored, ShardArchiveLimits::default()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&files["embedding_sets.json"]).unwrap(),
        serde_json::from_slice::<Value>(&restored_files["embedding_sets.json"]).unwrap()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&files["embedding_configs.json"]).unwrap(),
        serde_json::from_slice::<Value>(&restored_files["embedding_configs.json"]).unwrap()
    );
    state.inference_runtime.write().unwrap().provider_registry =
        Arc::new(query_template_tests::registry("openai", provider_url));
    let app = Router::new()
        .route("/api/v1/search", get(search_notes))
        .layer(Extension(ArchiveContext {
            schema: destination.schema_name.clone(),
            is_default: false,
            name: None,
        }))
        .with_state(state.clone());
    let (status, result) = query_template_tests::response(
        &app,
        Method::GET,
        "/api/v1/search?q=mrl-external&mode=semantic&set=mrl-external",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    embedding_dimension_tests::assert_rankings_equal(baseline, &result);
    assert_eq!(result["results"][0]["note_id"], expected_note.to_string());
    embedding_ownership_tests::assert_external_writes_suppressed(
        &state.db,
        &destination.schema_name,
        set_id,
        expected_note,
    )
    .await;
}

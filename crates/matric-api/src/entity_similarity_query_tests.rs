//! #1325: exercise URL query extraction and both production similarity routes.

use super::*;
use serde_json::{json, Value};
use tower::ServiceExt;

#[test]
fn entity_similarity_query_numeric_and_required_parameters() {
    for (suffix, expected) in [
        ("", None),
        ("&k=10", Some(10)),
        ("&k=40", Some(40)),
        ("&k=0", Some(0)),
        ("&k=-1", Some(-1)),
        ("&k=9223372036854775807", Some(i64::MAX)),
        ("&k=-9223372036854775808", Some(i64::MIN)),
    ] {
        let uri = format!(
            "/api/v1/entities/similar?source=fixture&external_id=root&set=profiles{suffix}"
        )
        .parse::<axum::http::Uri>()
        .unwrap();
        let Query(identity) = Query::<ExternalEntitySimilarityQuery>::try_from_uri(&uri).unwrap();
        let Query(similarity) = Query::<EntitySimilarityQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(identity.source, "fixture");
        assert_eq!(identity.external_id, "root");
        assert_eq!(similarity.embedding_set, "profiles");
        assert_eq!(similarity.k, expected);
    }
    for suffix in [
        "&k=",
        "&k=invalid",
        "&k=1.5",
        "&k=9223372036854775808",
        "&k=-9223372036854775809",
        "&k=1&k=2",
    ] {
        let uri = format!("/api/v1/entities/similar?set=profiles{suffix}")
            .parse::<axum::http::Uri>()
            .unwrap();
        assert!(Query::<EntitySimilarityQuery>::try_from_uri(&uri).is_err());
    }
    for query in [
        "source=fixture",
        "external_id=root",
        "source=a&source=b&external_id=root",
    ] {
        let uri = format!("/api/v1/entities/similar?{query}")
            .parse::<axum::http::Uri>()
            .unwrap();
        assert!(Query::<ExternalEntitySimilarityQuery>::try_from_uri(&uri).is_err());
    }
    let uri = "/api/v1/entities/similar?source=fixture&external_id=root"
        .parse::<axum::http::Uri>()
        .unwrap();
    assert!(Query::<EntitySimilarityQuery>::try_from_uri(&uri).is_err());
}

async fn get_similarity(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    // Axum query rejections are plain text; handler errors are problem JSON.
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).unwrap()));
    (status, body)
}

#[sqlx::test(migrations = false)]
async fn entity_similarity_query_real_router_k_limits_filters_and_rejections(admin: sqlx::PgPool) {
    // Reuse the migrated, disposable database fixture; no host service fallback.
    let db = embedding_dimension_tests::database(admin).await;
    let config = Uuid::new_v4();
    let set = Uuid::new_v4();
    let root = Uuid::new_v4();
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO embedding_config (id,name,model,dimension,vector_type)
         VALUES ($1,'similarity query fixture','synthetic-3',3,'vector')",
    )
    .bind(config)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO embedding_set
         (id,name,slug,set_type,mode,embedding_config_id,vector_source,defer_index_build)
         VALUES ($1,'Similarity profiles','similarity-profiles','full','manual',$2,'external',true)",
    )
    .bind(set)
    .bind(config)
    .execute(&mut *tx)
    .await
    .unwrap();
    let mut candidates = Vec::new();
    let mut excluded = Uuid::nil();
    // More than 100 neighbours makes defaults and both clamps observable.
    // The excluded candidate is closer than every eligible neighbour.
    for index in 0..107 {
        let id = if index == 0 { root } else { Uuid::new_v4() };
        let role = if index == 106 { "internal" } else { "customer" };
        if index == 106 {
            excluded = id;
        } else if index != 0 {
            candidates.push(id);
        }
        sqlx::query(
            "INSERT INTO note (id,format,source,created_at_utc,updated_at_utc,metadata)
             VALUES ($1,'markdown','similarity query fixture',now(),now(),$2)",
        )
        .bind(id)
        .bind(json!({"role":role,"private_field":"not requested"}))
        .execute(&mut *tx)
        .await
        .unwrap();
        let second = if index == 106 {
            0.0
        } else {
            index as f32 / 100.0
        };
        sqlx::query(
            "INSERT INTO embedding
             (id,note_id,embedding_set_id,chunk_index,text,vector,model,
              vector_kind,template_version,profile_hash,profile_text)
             VALUES ($1,$2,$3,-1,'profile',$4,'synthetic-3','profile','v1','hash','profile')",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(set)
        .bind(pgvector::Vector::from(vec![1.0, second, 0.0]))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO source_identity
         (note_id,source_namespace,external_id,source_schema_version,content_digest,import_run_id)
         VALUES ($1,'fixture source','root/id+42','1',$2,'query-fixture')",
    )
    .bind(root)
    .bind(format!("sha256:{}", "0".repeat(64)))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    let app = production_router(state, None);
    let external = "/api/v1/entities/similar?source=fixture%20source&external_id=root%2Fid%2B42&set=similarity-profiles";
    let note = format!("/api/v1/notes/{root}/similar?set={set}");
    let predicates = json!([{"path":"role","op":"eq","value":"customer"}]).to_string();
    let filter = format!(
        "&kind=profile&metadata=role&metadata_predicates={}",
        urlencoding::encode(&predicates)
    );
    for (suffix, expected) in [
        ("", 10),
        ("&k=40", 40),
        ("&k=1", 1),
        ("&k=0", 1),
        ("&k=-1", 1),
        ("&k=-9223372036854775808", 1),
        ("&k=100", 100),
        ("&k=101", 100),
        ("&k=9223372036854775807", 100),
    ] {
        let mut bodies = Vec::new();
        for path in [external, note.as_str()] {
            let (status, body) = get_similarity(&app, &format!("{path}{filter}{suffix}")).await;
            assert_eq!(status, StatusCode::OK, "{path}{suffix}: {body}");
            assert_eq!(body["query_note_id"], root.to_string());
            assert_eq!(body["set_id"], set.to_string());
            assert_eq!(body["kind"], "profile");
            let results = body["results"].as_array().unwrap();
            assert_eq!(results.len(), expected, "{path}{suffix}");
            for (hit, id) in results.iter().zip(&candidates) {
                assert_eq!(hit["note_id"], id.to_string());
                assert_ne!(hit["note_id"], root.to_string());
                assert_ne!(hit["note_id"], excluded.to_string());
                assert_eq!(hit["metadata"], json!({"role":"customer"}));
            }
            bodies.push(body);
        }
        assert_eq!(bodies[0], bodies[1], "external/note parity for {suffix}");
    }
    // Legacy metadata-filter spelling and strict-filter JSON retain their paths.
    for path in [external, note.as_str()] {
        let (status, body) = get_similarity(
            &app,
            &format!(
                "{path}&k=40&filter={}&strict_filter=%7B%7D",
                urlencoding::encode(&predicates)
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["results"].as_array().unwrap().len(), 40);
        assert_eq!(body["results"][0]["note_id"], candidates[0].to_string());
        assert_eq!(body["results"][0]["metadata"], json!({}));

        for suffix in [
            "&k=",
            "&k=invalid",
            "&k=1.5",
            "&k=9223372036854775808",
            "&k=-9223372036854775809",
            "&k=1&k=2",
            "&k=40&kind=body_chunk",
            "&k=40&metadata=invalid-field",
            "&k=40&metadata_predicates=invalid",
            "&k=40&strict_filter=invalid",
        ] {
            let (status, body) = get_similarity(&app, &format!("{path}{suffix}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}{suffix}: {body}");
        }
    }
    for path in [
        "/api/v1/entities/similar?source=fixture&external_id=root&k=40",
        "/api/v1/entities/similar?external_id=root&set=similarity-profiles&k=40",
        "/api/v1/entities/similar?source=fixture&set=similarity-profiles&k=40",
    ] {
        let (status, body) = get_similarity(&app, path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
    }
    let (status, body) = get_similarity(
        &app,
        "/api/v1/entities/similar?source=fixture%20source&external_id=missing&set=similarity-profiles&k=40",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    db.pool.close().await;
}

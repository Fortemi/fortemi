use super::*;
use matric_core::search_evidence::{
    EvidenceKind, EvidenceSource, EvidenceText, EvidenceUnit, SearchEvidenceLocator,
};

const RAW: &str = "\u{feff}needle \u{1f642}e\u{301}\r\n";

async fn mutate(admin: &PgPool, schema: &str, tenant: Uuid, note: Uuid, sql: &str) {
    let mut tx = admin.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant',$1,true), set_config('search_path',$2,true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema}, public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(sql).bind(note).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn expect(
    app: &Router,
    token: Option<&str>,
    memory: &str,
    request: Value,
    expected: StatusCode,
    text: Option<&str>,
) -> usize {
    let (status, response) = resolve_response(app, token, memory, request).await;
    assert_eq!(status, expected, "resolution fixture response: {response}");
    if let Some(text) = text {
        static VALIDATOR: std::sync::LazyLock<jsonschema::Validator> = std::sync::LazyLock::new(
            || {
                let doc: serde_yaml::Value =
                    serde_yaml::from_str(&openapi_yaml_with_problem_contract()).unwrap();
                let doc = serde_json::to_value(doc).unwrap();
                jsonschema::options().should_validate_formats(true).build(&json!({
                "$schema":"https://json-schema.org/draft/2020-12/schema",
                "$ref":"#/components/schemas/SearchEvidenceResolveResponse","components":doc["components"]
            })).unwrap()
            },
        );
        assert!(
            VALIDATOR.is_valid(&response),
            "resolved response must match generated authority"
        );
        assert_eq!(response, json!({"text":text}));
    }
    if expected == StatusCode::NOT_FOUND {
        assert_eq!(response["detail"], "SEARCH_EVIDENCE_UNAVAILABLE");
    }
    if expected == StatusCode::BAD_REQUEST {
        assert_eq!(response["detail"], "SEARCH_EVIDENCE_INVALID");
    }
    assert!(!response.to_string().contains("private-marker"));
    1
}

pub(super) async fn verify(
    app: &Router,
    admin: &PgPool,
    a: Uuid,
    b: Uuid,
    archive: &str,
    a_public: Uuid,
    b_public: Uuid,
    a_archive: Uuid,
) -> usize {
    let mut checks = 0;
    // The same native UUID and text may exist in two archive tables. A source
    // digest from one archive must not resolve as the other archive's identity.
    let mut tx = admin.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(a.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    matric_db::validate_schema_name(archive).unwrap();
    sqlx::query(&format!("INSERT INTO {archive}.note(id,format,source,created_at_utc,updated_at_utc,title,metadata,tenant_id)
        SELECT id,format,source,created_at_utc,updated_at_utc,title,metadata,tenant_id FROM public.note WHERE id=$1"))
        .bind(a_public).execute(&mut *tx).await.unwrap();
    sqlx::query(&format!(
        "INSERT INTO {archive}.note_revised_current(note_id,content,tenant_id)
        SELECT note_id,content,tenant_id FROM public.note_revised_current WHERE note_id=$1"
    ))
    .bind(a_public)
    .execute(&mut *tx)
    .await
    .unwrap();
    let original: String =
        sqlx::query_scalar("SELECT content FROM public.note_revised_current WHERE note_id=$1")
            .bind(a_public)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    for schema in ["public", archive] {
        sqlx::query(&format!("INSERT INTO {schema}.source_identity(note_id,source_namespace,external_id,source_schema_version,content_digest,import_run_id)
            VALUES($1,'collision','same-key','1',$2,'collision-run')"))
            .bind(a_public).bind(format!("sha256:{}","0".repeat(64))).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let unit = EvidenceUnit {
        kind: EvidenceKind::Current,
        id: a_public.to_string(),
        index: 0,
    };
    let mut source = EvidenceSource {
        namespace: "collision".into(),
        import_run_id: "collision-run".into(),
        schema_version: "1".into(),
        external_id_hash: matric_core::source_upsert::source_identity_digest(
            a,
            "public",
            "collision",
            "same-key",
        ),
    };
    let locator = SearchEvidenceLocator::bind(
        EvidenceText {
            note_id: &a_public.to_string(),
            unit: &unit,
            content: &original,
            source: Some(&source),
        },
        0,
        original.len(),
    )
    .unwrap();
    checks += expect(
        app,
        Some("fixture-a"),
        "public",
        json!({"locator":locator}),
        StatusCode::OK,
        Some(&original),
    )
    .await;
    checks += expect(
        app,
        Some("fixture-a"),
        "search_fixture",
        json!({"locator":locator}),
        StatusCode::NOT_FOUND,
        None,
    )
    .await;
    source.external_id_hash =
        matric_core::source_upsert::source_identity_digest(a, archive, "collision", "same-key");
    let locator = SearchEvidenceLocator::bind(
        EvidenceText {
            note_id: &a_public.to_string(),
            unit: &unit,
            content: &original,
            source: Some(&source),
        },
        0,
        original.len(),
    )
    .unwrap();
    checks += expect(
        app,
        Some("fixture-a"),
        "search_fixture",
        json!({"locator":locator}),
        StatusCode::OK,
        Some(&original),
    )
    .await;
    mutate(admin, archive, a, a_public, "DELETE FROM note WHERE id=$1").await;
    for (token, tenant, memory, schema, note) in [
        ("fixture-a", a, "public", "public", a_public),
        ("fixture-b", b, "public", "public", b_public),
        ("fixture-a", a, "search_fixture", archive, a_archive),
    ] {
        let mut tx = admin.begin().await.unwrap();
        sqlx::query(
            "SELECT set_config('app.current_tenant',$1,true), set_config('search_path',$2,true)",
        )
        .bind(tenant.to_string())
        .bind(format!("{schema}, public"))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("UPDATE note SET title=$2 WHERE id=$1")
            .bind(note)
            .bind(RAW)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE note_revised_current SET content=$2 WHERE note_id=$1")
            .bind(note)
            .bind(RAW)
            .execute(&mut *tx)
            .await
            .unwrap();
        let embedding: Uuid = sqlx::query_scalar(
            "UPDATE embedding SET text=$2 WHERE note_id=$1 AND chunk_index=0 RETURNING id",
        )
        .bind(note)
        .bind(RAW)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let blob = Uuid::new_v4();
        let attachment = Uuid::new_v4();
        sqlx::query("INSERT INTO attachment_blob(id,content_hash,content_type,size_bytes,data) VALUES($1,$2,'text/plain',1,$3)")
            .bind(blob).bind(format!("{:064x}",blob.as_u128())).bind(vec![1_u8]).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment(id,note_id,blob_id,filename,status,extracted_text) VALUES($1,$2,$3,'synthetic.txt','completed',$4)")
            .bind(attachment).bind(note).bind(blob).bind(RAW).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let source = EvidenceSource {
            namespace: "synthetic".into(),
            external_id_hash: matric_core::source_upsert::source_identity_digest(
                tenant,
                schema,
                "synthetic",
                "wanted-0",
            ),
            import_run_id: "run-42".into(),
            schema_version: "1".into(),
        };
        let mut locators = Vec::new();
        for (kind, id) in [
            (EvidenceKind::Current, note),
            (EvidenceKind::Title, note),
            (EvidenceKind::Embedding, embedding),
            (EvidenceKind::Attachment, attachment),
        ] {
            let unit = EvidenceUnit {
                kind,
                id: id.to_string(),
                index: 0,
            };
            for (start, end) in [(0, RAW.len()), (3, RAW.len()), (RAW.len(), RAW.len())] {
                let locator = SearchEvidenceLocator::bind(
                    EvidenceText {
                        note_id: &note.to_string(),
                        unit: &unit,
                        content: RAW,
                        source: Some(&source),
                    },
                    start,
                    end,
                )
                .unwrap();
                checks += expect(
                    app,
                    Some(token),
                    memory,
                    json!({"locator":locator}),
                    StatusCode::OK,
                    Some(&RAW[start..end]),
                )
                .await;
                if start == 0 {
                    locators.push(serde_json::to_value(locator).unwrap());
                }
            }
        }
        let current = &locators[0];
        for locator in &locators {
            for field in [
                "namespace",
                "external_id_hash",
                "import_run_id",
                "schema_version",
            ] {
                let mut changed = locator.clone();
                changed["source"][field] = if field == "external_id_hash" {
                    json!(format!("sha256:{}", "0".repeat(64)))
                } else {
                    json!("private-marker")
                };
                checks += expect(
                    app,
                    Some(token),
                    memory,
                    json!({"locator":changed}),
                    StatusCode::NOT_FOUND,
                    None,
                )
                .await;
            }
            let mut changed = locator.clone();
            changed["content_digest"] = json!(format!("sha256:{}", "0".repeat(64)));
            checks += expect(
                app,
                Some(token),
                memory,
                json!({"locator":changed}),
                StatusCode::NOT_FOUND,
                None,
            )
            .await;
            changed = locator.clone();
            changed["span"]["start"] = json!(1);
            checks += expect(
                app,
                Some(token),
                memory,
                json!({"locator":changed}),
                StatusCode::NOT_FOUND,
                None,
            )
            .await;
            let mut absent = locator.clone();
            absent.as_object_mut().unwrap().remove("source");
            checks += expect(
                app,
                Some(token),
                memory,
                json!({"locator":absent}),
                StatusCode::OK,
                Some(RAW),
            )
            .await;
        }
        for (path, value) in [
            ("provider", json!("fixture")),
            ("model", json!(42)),
            ("import_run_id", json!("run-42")),
        ] {
            checks += expect(app,Some(token),memory,json!({"locator":current,"metadata_predicates":[{"path":path,"op":"eq","value":value}]}),StatusCode::OK,Some(RAW)).await;
        }
        for (path, value) in [
            ("provider", json!("wrong")),
            ("model", json!("42")),
            ("import_run_id", json!("different")),
        ] {
            checks += expect(app,Some(token),memory,json!({"locator":current,"metadata_predicates":[{"path":path,"op":"eq","value":value}]}),StatusCode::NOT_FOUND,None).await;
        }
        for key in ["tenant_id", "archive_id", "private-marker"] {
            let mut invalid = json!({"locator":current});
            invalid[key] = json!("private-marker");
            checks += expect(
                app,
                Some(token),
                memory,
                invalid,
                StatusCode::BAD_REQUEST,
                None,
            )
            .await;
        }
        for invalid in [
            json!({"locator":null}),
            json!({"locator":current,"include_archived":null}),
            json!({"locator":current,"metadata_predicates":[{"path":"private-marker","op":"exists"}]}),
            json!({"locator":current,"metadata_predicates":[{"path":"model","op":"range","gte":4,"lte":1}]}),
        ] {
            checks += expect(
                app,
                Some(token),
                memory,
                invalid,
                StatusCode::BAD_REQUEST,
                None,
            )
            .await;
        }
        for (identity, status) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("invalid"), StatusCode::UNAUTHORIZED),
            (Some("fixture-denied"), StatusCode::FORBIDDEN),
        ] {
            // The denied token is tenant A; use its public memory so this checks scope, not archive existence.
            checks += expect(
                app,
                identity,
                "public",
                json!({"locator":current}),
                status,
                None,
            )
            .await;
        }
        let other = if token == "fixture-a" {
            "fixture-b"
        } else {
            "fixture-a"
        };
        checks += expect(
            app,
            Some(other),
            "public",
            json!({"locator":current}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        let other_memory = if memory == "public" {
            "search_fixture"
        } else {
            "public"
        };
        if token == "fixture-a" {
            checks += expect(
                app,
                Some(token),
                other_memory,
                json!({"locator":current}),
                StatusCode::NOT_FOUND,
                None,
            )
            .await;
        }
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE note SET archived=true WHERE id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":current}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":current,"include_archived":true}),
            StatusCode::OK,
            Some(RAW),
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE note SET archived=false, deleted_at=now() WHERE id=$1",
        )
        .await;
        for locator in &locators {
            checks += expect(
                app,
                Some(token),
                memory,
                json!({"locator":locator,"include_archived":true}),
                StatusCode::NOT_FOUND,
                None,
            )
            .await;
        }
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE note SET deleted_at=NULL WHERE id=$1",
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE source_identity SET import_run_id='changed' WHERE note_id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":current}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE source_identity SET import_run_id='run-42' WHERE note_id=$1",
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE attachment SET status='uploaded' WHERE note_id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":locators[3]}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "DELETE FROM attachment WHERE note_id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":locators[3]}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE embedding SET text='changed' WHERE note_id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":locators[2]}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE note SET title='changed' WHERE id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":locators[1]}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(
            admin,
            schema,
            tenant,
            note,
            "UPDATE note_revised_current SET content=repeat('x',16777217) WHERE note_id=$1",
        )
        .await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":current}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
        mutate(admin, schema, tenant, note, "DELETE FROM note WHERE id=$1").await;
        checks += expect(
            app,
            Some(token),
            memory,
            json!({"locator":current}),
            StatusCode::NOT_FOUND,
            None,
        )
        .await;
    }
    for (content_type, body) in [
        ("application/json", "{private-marker".to_owned()),
        ("application/json", " ".repeat(65537)),
        ("text/plain", "{}".to_owned()),
    ] {
        let request = axum::http::Request::builder()
            .method(Method::POST)
            .uri("/api/v1/search/evidence/resolve")
            .header(header::CONTENT_TYPE, content_type)
            .header(header::AUTHORIZATION, "Bearer fixture-a")
            .header("X-Fortemi-Memory", "public")
            .body(Body::from(body))
            .unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            app.clone().oneshot(request),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 1048576)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["detail"], "SEARCH_EVIDENCE_INVALID");
        assert!(!body.to_string().contains("private-marker"));
        checks += 1;
    }
    checks
}

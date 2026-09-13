//! Real TCP consumer acceptance, with optional production JWT verification.
use super::*;

// Keep the BOM, but do not attach it to the queried lexeme: PostgreSQL's
// dictionary treats that prefix as part of the word.
const RAW: &str = "\u{feff}prefix needle \u{1f642}e\u{301}\r\n";

type Traffic = Arc<std::sync::Mutex<Vec<Value>>>;

async fn serve(
    app: Router,
    traffic: Traffic,
    name: &'static str,
) -> (String, tokio::task::JoinHandle<()>) {
    let app = app.layer(axum::middleware::from_fn(move |request: axum::extract::Request, next: axum::middleware::Next| {
        let traffic = traffic.clone();
        async move {
            let method = request.method().to_string();
            let path = match request.uri().path() {
                "/api/v1/search" => "/api/v1/search",
                "/api/v1/search/evidence/resolve" => "/api/v1/search/evidence/resolve",
                path if path.starts_with("/api/v1/notes/") => "/api/v1/notes/{id}",
                _ => panic!("unexpected installed consumer route"),
            };
            let response = next.run(request).await;
            let mut events = traffic.lock().unwrap();
            assert!(events.len() < 512, "bounded fixture traffic");
            events.push(json!({"router":name,"method":method,"path":path,"status":response.status().as_u16()}));
            response
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, task)
}

async fn run_driver(root: &std::path::Path, phase: &str) {
    let log = std::fs::File::create(root.join(format!("{phase}.log"))).unwrap();
    let mut child = tokio::process::Command::new("node")
        .arg(format!(
            "{}/src/scoped_search_tests/installed-http-consumer.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(root)
        .arg(phase)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let status = tokio::time::timeout(std::time::Duration::from_secs(90), child.wait())
        .await
        .expect("installed consumer deadline")
        .unwrap();
    assert!(
        status.success(),
        "installed consumer {phase} failed; inspect its bounded log"
    );
}

pub(super) async fn verify(
    app: &Router,
    denied: &Router,
    untrusted: Option<&Router>,
    cold: Option<&Router>,
    admin: &PgPool,
    runtime: &PgPool,
    a: Uuid,
    b: Uuid,
    archive: &str,
    embeddings: &str,
) {
    let root = std::path::PathBuf::from(
        std::env::var("FORTEMI_HTTP_ARTIFACTS").expect("HTTP artifact directory required"),
    );
    assert!(
        std::env::var_os("FORTEMI_INSTALLED_CORE_ROOT").is_some(),
        "clean installed package required"
    );
    let db = Database::new(admin.clone());
    let mut fixtures = Vec::new();
    for (tenant, schema, token, memory) in [
        (a, "public", "fixture-a", "public"),
        (b, "public", "fixture-b", "public"),
        (a, archive, "fixture-a", "search_fixture"),
    ] {
        matric_db::validate_schema_name(schema).unwrap();
        let mut tx = admin.begin().await.unwrap();
        sqlx::query(
            "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
        )
        .bind(tenant.to_string())
        .bind(format!("{schema}, public"))
        .execute(&mut *tx)
        .await
        .unwrap();
        let config: Uuid = sqlx::query_scalar(
            "SELECT id FROM public.embedding_config WHERE tenant_id=$1 AND name=$2",
        )
        .bind(tenant)
        .bind(format!("profile-{schema}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        // The public Core search subset has no embedding-set option. Give this
        // owned fixture a real tenant-local default profile, not a URL rewrite.
        let set: Uuid = sqlx::query_scalar("INSERT INTO embedding_set(name,slug,embedding_config_id,tenant_id) VALUES('default','default',$1,$2) RETURNING id")
            .bind(config).bind(tenant).fetch_one(&mut *tx).await.unwrap();
        let note = db
            .notes
            .insert_tx(
                &mut tx,
                CreateNoteRequest {
                    content: RAW.into(),
                    title: Some(RAW.into()),
                    format: "markdown".into(),
                    source: "http-fixture".into(),
                    collection_id: None,
                    document_type_id: None,
                    tags: Some(vec!["http-fixture-only".into()]),
                    metadata: Some(json!({"provider":"fixture","model":42})),
                },
            )
            .await
            .unwrap();
        sqlx::query("INSERT INTO embedding(note_id,chunk_index,text,vector,model,embedding_set_id) VALUES($1,7,$2,$3,'synthetic-only',$4)")
            .bind(note).bind(RAW).bind(pgvector::Vector::from(vec![1.0_f32;768])).bind(set).execute(&mut *tx).await.unwrap();
        let blob = Uuid::new_v4();
        sqlx::query("INSERT INTO attachment_blob(id,content_hash,content_type,size_bytes,data) VALUES($1,$2,'text/plain',1,$3)")
            .bind(blob).bind(format!("{:064x}",blob.as_u128())).bind(vec![1_u8]).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment(note_id,blob_id,filename,status,extracted_text) VALUES($1,$2,'http-fixture.txt','completed',$3)")
            .bind(note).bind(blob).bind(RAW).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO source_identity(note_id,source_namespace,external_id,source_schema_version,content_digest,import_run_id) VALUES($1,'http-fixture','private-key','1',$2,'http-run')")
            .bind(note).bind(format!("sha256:{}","0".repeat(64))).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        // Diagnose the actual repository read with the fixture's restricted
        // role. Roll back so only HTTP enrichment contributes access counts.
        let mut probe = runtime.begin().await.unwrap();
        sqlx::query(
            "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
        )
        .bind(tenant.to_string())
        .bind(format!("{schema}, public"))
        .execute(&mut *probe)
        .await
        .unwrap();
        let result = db.notes.fetch_tx(&mut probe, note).await;
        probe.rollback().await.unwrap();
        if fixtures.is_empty() {
            let Err(matric_core::Error::Database(sqlx::Error::Database(error))) = result else {
                panic!("fixture must first reproduce the access-counter trigger permission requirement");
            };
            assert_eq!(error.code().as_deref(), Some("42501"));
            assert_eq!(
                error.message(),
                "permission denied for table embedding_set_member"
            );
            let role: String = sqlx::query_scalar("SELECT current_user")
                .fetch_one(runtime)
                .await
                .unwrap();
            assert!(
                role.starts_with("search_")
                    && role.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            );
            for schema in ["public", archive] {
                sqlx::query(&format!(
                    "GRANT INSERT ON {schema}.embedding_set_member TO {role}"
                ))
                .execute(admin)
                .await
                .unwrap();
            }
        } else {
            result.expect("restricted fixture note-detail prerequisites");
        }
        fixtures.push(
            json!({"tenant":tenant,"schema":schema,"token":token,"memory":memory,"note":note}),
        );
    }
    let traffic = Traffic::default();
    let binary = if std::env::var_os("FORTEMI_TEST_BINARY").is_some() {
        Some(super::binary_http::BinaryServer::start(admin, runtime, &root, embeddings).await)
    } else {
        None
    };
    if let Some(binary) = &binary {
        binary.verify_worker(admin, runtime, a, b, archive).await;
        std::fs::write(
            root.join("browser-notes-fixture.json"),
            serde_json::to_vec_pretty(
                &fixtures
                    .iter()
                    .map(|f| {
                        json!({
                            "tenant": f["tenant"], "memory": f["memory"], "note": f["note"],
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        )
        .unwrap();
        binary.verify_browser(admin).await;
    }
    let browser_reads: i32 =
        if binary.is_some() && std::env::var_os("FORTEMI_BROWSER_DRIVER").is_some() {
            1
        } else {
            0
        };
    let mut browser_access = Vec::new();
    for f in &fixtures {
        let schema = f["schema"].as_str().unwrap();
        let counts: (i32, i64) = sqlx::query_as(&format!("SELECT access_count,(SELECT count(*) FROM {schema}.note_access_log WHERE note_id=n.id) FROM {schema}.note n WHERE id=$1"))
            .bind(Uuid::parse_str(f["note"].as_str().unwrap()).unwrap()).fetch_one(admin).await.unwrap();
        assert_eq!(
            counts,
            (browser_reads, i64::from(browser_reads)),
            "exactly one browser selection per fixture note"
        );
        browser_access.push(
            json!({"tenant":f["tenant"],"memory":f["memory"],"note":f["note"],"counts":counts}),
        );
    }
    std::fs::write(
        root.join("browser-note-access.json"),
        serde_json::to_vec_pretty(&browser_access).unwrap(),
    )
    .unwrap();
    let primary = binary
        .as_ref()
        .map(|b| b.observer_target())
        .unwrap_or_else(|| app.clone());
    let primary_name = if binary.is_some() {
        "launched-binary"
    } else {
        "scoped"
    };
    let (url, server) = serve(primary, traffic.clone(), primary_name).await;
    let (denied_url, denied_server) = serve(denied.clone(), traffic.clone(), "note-denied").await;
    let untrusted_server = match untrusted {
        Some(app) => Some(serve(app.clone(), traffic.clone(), "untrusted-ca").await),
        None => None,
    };
    let cold_server = match cold {
        Some(app) => Some(serve(app.clone(), traffic.clone(), "cold-jwks").await),
        None => None,
    };
    std::fs::write(
        root.join("fixture.json"),
        serde_json::to_vec_pretty(
            &json!({"url":url,"deniedUrl":denied_url,"untrustedUrl":untrusted_server.as_ref().map(|(url,_)|url),"coldUrl":cold_server.as_ref().map(|(url,_)|url),"text":RAW,"fixtures":fixtures}),
        )
        .unwrap(),
    )
    .unwrap();
    run_driver(&root, "initial").await;
    let auth_start = traffic.lock().unwrap().len();
    if untrusted.is_some() {
        run_driver(&root, "jwt").await;
        for status in ["suspended", "soft_deleted", "active"] {
            sqlx::query("UPDATE tenant_registry SET status=$1 WHERE id=$2")
                .bind(status)
                .bind(a)
                .execute(admin)
                .await
                .unwrap();
            run_driver(&root, &format!("tenant-{status}")).await;
        }
    }
    let auth_end = traffic.lock().unwrap().len();
    for (phase, sql) in [
        (
            "stale",
            "UPDATE note_revised_current SET content='needle changed' WHERE note_id=$1",
        ),
        ("archived", "UPDATE note SET archived=true WHERE id=$1"),
        ("deleted", "UPDATE note SET deleted_at=now() WHERE id=$1"),
        ("purged", "DELETE FROM note WHERE id=$1"),
    ] {
        for f in &fixtures {
            let mut tx = admin.begin().await.unwrap();
            sqlx::query(
                "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
            )
            .bind(f["tenant"].as_str().unwrap())
            .bind(format!("{}, public", f["schema"].as_str().unwrap()))
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(sql)
                .bind(Uuid::parse_str(f["note"].as_str().unwrap()).unwrap())
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        run_driver(&root, phase).await;
        if phase == "stale" {
            for f in &fixtures {
                let schema = f["schema"].as_str().unwrap();
                let counts: (i32, i64) = sqlx::query_as(&format!("SELECT access_count,(SELECT count(*) FROM {schema}.note_access_log WHERE note_id=n.id) FROM {schema}.note n WHERE id=$1"))
                    .bind(Uuid::parse_str(f["note"].as_str().unwrap()).unwrap()).fetch_one(admin).await.unwrap();
                assert_eq!(
                    counts,
                    (browser_reads + 4, i64::from(browser_reads) + 4),
                    "search enrichment adds exactly four reads after the verified browser baseline"
                );
            }
        }
    }
    for task in [server, denied_server] {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    if let Some((_, task)) = untrusted_server {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    if let Some((_, task)) = cold_server {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    let mut binary_healthy = true;
    if let Some(binary) = binary {
        binary_healthy = binary.stop().await;
        let audit: Vec<(Uuid, i64)> = sqlx::query_as("SELECT tenant_id,count(*) FROM public.audit_event WHERE tenant_id IN ($1,$2) GROUP BY tenant_id ORDER BY tenant_id")
            .bind(a).bind(b).fetch_all(admin).await.unwrap();
        assert_eq!(
            audit.len(),
            2,
            "actual binary must persist audit for both fixture tenants"
        );
        assert!(audit.iter().all(|(_, n)| *n > 0));
        std::fs::write(
            root.join("binary-audit.json"),
            serde_json::to_vec_pretty(&json!({"tenants":audit,"durable":true})).unwrap(),
        )
        .unwrap();
        println!("hosted_installed_core_binary: actual startup, primary HTTP matrix, durable audit and graceful shutdown");
    }
    let events = traffic.lock().unwrap();
    std::fs::write(
        root.join("http-traffic.json"),
        serde_json::to_vec_pretty(&*events).unwrap(),
    )
    .unwrap();
    for (path, expected) in [
        ("/api/v1/search", 21),
        ("/api/v1/notes/{id}", 12),
        ("/api/v1/search/evidence/resolve", 183),
    ] {
        assert_eq!(
            events
                .iter()
                .enumerate()
                .filter(|(i, e)| (*i < auth_start || *i >= auth_end) && e["path"] == path)
                .count(),
            expected,
            "actual HTTP request count for {path}"
        );
    }
    if untrusted.is_some() {
        assert!(auth_end > auth_start);
        println!("hosted_installed_core_jwt: production HTTPS verifier; tenant registry states; {} additional requests; listeners stopped", auth_end-auth_start);
    }
    println!("hosted_installed_core_http: 5 phases; 3 tenant/archive scopes; 4 text units; six synthetic embeddings; listeners stopped");
    assert!(
        binary_healthy,
        "launched binary startup health is incomplete; inspect binary-startup-health.json"
    );
}

//! Launch the production binary for the primary installed-consumer HTTP matrix.
use super::*;
use matric_core::{JobRepository, JobRetryPolicy};
use sqlx::ConnectOptions;
use std::{path::Path, process::Stdio, time::Duration};

pub(super) struct BinaryServer {
    child: tokio::process::Child,
    pub url: String,
    root: std::path::PathBuf,
}

impl BinaryServer {
    async fn sse_database_diagnostic(&self, admin: &PgPool, phase: &str) {
        let rows: Vec<Value> = tokio::time::timeout(Duration::from_secs(2),sqlx::query_scalar("SELECT jsonb_build_object('pid',pid,'state',state,'wait_type',wait_event_type,'wait',wait_event,'query',left(query,512),'blockers',pg_blocking_pids(pid)) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() LIMIT 64").fetch_all(admin)).await.unwrap().unwrap();
        std::fs::write(
            self.root.join(format!("sse-database-{phase}.json")),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }

    pub async fn start(admin: &PgPool, runtime: &PgPool, root: &Path, embeddings: &str) -> Self {
        let private =
            std::env::var("FORTEMI_BOOTSTRAP_ROOT").expect("private native fixture required");
        assert!(private.starts_with("/tmp/fortemi-bootstrap-"));
        let role: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(runtime)
            .await
            .unwrap();
        assert!(
            role.starts_with("search_")
                && role.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        );
        sqlx::query(&format!("GRANT INSERT ON public.audit_event TO {role}"))
            .execute(admin)
            .await
            .unwrap();
        // The reused request connection has no tenant outside its transaction.
        let jobs = matric_db::PgJobRepository::new(runtime.clone());
        let unscoped = jobs
            .reap_stale_running(3600, &JobRetryPolicy::default())
            .await
            .unwrap_err();
        match unscoped {
            matric_core::Error::Database(sqlx::Error::Database(error)) => {
                assert_eq!(error.code().as_deref(), Some("22P02"));
                assert_eq!(error.message(), "invalid input syntax for type uuid: \"\"");
            }
            _ => panic!("expected missing tenant context on reused request connection"),
        }
        // Scope only this owned probe, never the launched binary or the database role.
        sqlx::query(
            "SELECT set_config('app.current_tenant','00000000-0000-0000-0000-000000000000',false)",
        )
        .execute(runtime)
        .await
        .unwrap();
        let denied = jobs
            .reap_stale_running(3600, &JobRetryPolicy::default())
            .await
            .unwrap_err();
        match denied {
            matric_core::Error::Database(sqlx::Error::Database(error)) => {
                assert_eq!(error.code().as_deref(), Some("42501"))
            }
            _ => panic!("expected job-recovery privilege denial"),
        }
        for grant in [
            "UPDATE (status,retry_count,error_message,started_at,progress_percent,progress_message,next_attempt_at,failure_class,failure_code,completed_at) ON public.job_queue",
            "UPDATE (outcome,completed_at,retry_at,duration_ms,failure_class,failure_code) ON public.job_attempt",
        ] {
            sqlx::query(&format!("GRANT {grant} TO {role}")).execute(admin).await.unwrap();
        }
        assert_eq!(
            jobs.reap_stale_running(3600, &JobRetryPolicy::default())
                .await
                .unwrap(),
            0
        );
        sqlx::query("RESET app.current_tenant")
            .execute(runtime)
            .await
            .unwrap();
        let fresh = matric_db::create_pool_with_config(
            runtime.connect_options().to_url_lossy().as_str(),
            matric_db::PoolConfig::new()
                .max_connections(1)
                .hosted_unscoped(),
        )
        .await
        .unwrap();
        let fresh_error = matric_db::PgJobRepository::new(fresh.clone())
            .reap_stale_running(3600, &JobRetryPolicy::default())
            .await
            .unwrap_err();
        match fresh_error {
            matric_core::Error::Database(sqlx::Error::Database(error)) => {
                assert_eq!(error.code().as_deref(), Some("42704"));
                assert_eq!(
                    error.message(),
                    "unrecognized configuration parameter \"app.current_tenant\""
                );
            }
            _ => panic!("expected unset tenant context on fresh hosted connection"),
        }
        fresh.close().await;
        std::fs::write(root.join("binary-job-grants.json"), serde_json::to_vec_pretty(&json!({
            "unscopedSqlstate":"22P02","scopedBeforeSqlstate":"42501","scopedAfterReaped":0,
            "freshHostedSqlstate":"42704","freshPoolClosed":true,
            "probeScopeReset":true,"scope":"owned runtime job-state columns only; binary remains hosted-unscoped"
        })).unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}");
        for name in ["files", "tus", "backups"] {
            std::fs::create_dir(format!("{private}/{name}")).unwrap();
        }
        let log = std::fs::File::create(root.join("production-binary.log")).unwrap();
        let mut child = tokio::process::Command::new(std::env::var("FORTEMI_TEST_BINARY").unwrap())
            .current_dir(&private)
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .env(
                "DATABASE_URL",
                runtime.connect_options().to_url_lossy().to_string(),
            )
            .env(
                "MIGRATION_DATABASE_URL",
                admin.connect_options().to_url_lossy().to_string(),
            )
            .env("FORTEMI_MULTI_TENANT", "true")
            .env("REQUIRE_AUTH", "true")
            .env("I_UNDERSTAND_NO_AUTH", "false")
            .env("FORTEMI_ALLOW_LOCAL_ISSUER", "true")
            .env("ISSUER_URL", std::env::var("FORTEMI_TEST_ISSUER").unwrap())
            .env(
                "FORTEMI_AUTH_CA_BUNDLE",
                std::env::var("FORTEMI_TEST_CA").unwrap(),
            )
            .env("FORTEMI_AUTH_AUDIENCE", "fortemi-http-fixture")
            .env("FORTEMI_AUTH_CLOCK_SKEW_SECONDS", "0")
            .env("FORTEMI_AUTH_JWKS_CACHE_CAPACITY", "2")
            .env("FORTEMI_AUTH_HTTP_TIMEOUT_SECONDS", "2")
            .env("FILE_STORAGE_PATH", format!("{private}/files"))
            .env("TUS_STAGING_DIR", format!("{private}/tus"))
            .env("BACKUP_DEST", format!("{private}/backups"))
            .env("WORKER_ENABLED", "true")
            .env("JOB_WORKER_ENABLED", "true")
            .env("JOB_MAX_CONCURRENT", "1")
            .env("JOB_POLL_INTERVAL_MS", "100")
            .env("TOKIO_WORKER_THREADS", "2")
            .env("OPENAI_API_KEY", "synthetic-fixture-only")
            .env("OPENAI_BASE_URL", embeddings)
            .env("OPENAI_TIMEOUT", "2")
            .env("OPENAI_EMBED_MODEL", "synthetic-only")
            .env("OPENAI_GEN_MODEL", "synthetic-only")
            .env("MATRIC_INFERENCE_DEFAULT", "openai")
            .env("MATRIC_EMBEDDING_PROVIDER", "openai")
            .env("REDIS_ENABLED", "true")
            .env(
                "REDIS_URL",
                std::env::var("FORTEMI_QUOTA_REDIS_URL").expect("owned Redis required"),
            )
            .env("RATE_LIMIT_ENABLED", "true")
            .env("RATE_LIMIT_REQUESTS", "10000")
            .env("RATE_LIMIT_PERIOD_SECS", "60")
            .env("MATRIC_SHUTDOWN_GRACE_SECS", "5")
            .env("LOG_FORMAT", "text")
            .env("LOG_ANSI", "false")
            .env(
                "RUST_LOG",
                "info,matric_jobs::worker=debug,matric_api=debug",
            )
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        std::fs::write(root.join("binary-process.json"), serde_json::to_vec_pretty(&json!({
            "pid":child.id(),"url":url,"sha256":std::env::var("FORTEMI_TEST_BINARY_SHA256").unwrap(),
            "started":true,"ready":false,"scanning":"required","workerEnabled":true,"rateLimitEnabled":true
        })).unwrap()).unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "binary exited before readiness; inspect production-binary.log"
            );
            if let Ok(response) = client.get(format!("{url}/readyz")).send().await {
                if response.status() == StatusCode::OK {
                    break;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "binary readiness deadline; inspect production-binary.log"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        std::fs::write(
            root.join("binary-ready.json"),
            serde_json::to_vec_pretty(&json!({"pid":child.id(),"ready":true,"url":url})).unwrap(),
        )
        .unwrap();
        Self {
            child,
            url,
            root: root.to_path_buf(),
        }
    }

    pub async fn verify_worker(
        &self,
        admin: &PgPool,
        runtime: &PgPool,
        a: Uuid,
        b: Uuid,
        archive: &str,
    ) {
        let role: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(runtime)
            .await
            .unwrap();
        assert!(
            role.starts_with("search_")
                && role.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        );
        matric_db::validate_schema_name(archive).unwrap();
        for grant in [
            "INSERT ON public.job_queue,public.job_attempt,public.job_history",
            "UPDATE(result,actual_duration_ms) ON public.job_queue",
            "UPDATE(is_active) ON public.document_type",
        ] {
            sqlx::query(&format!("GRANT {grant} TO {role}"))
                .execute(admin)
                .await
                .unwrap();
        }
        for schema in ["public", archive] {
            for grant in [
                format!("UPDATE(document_type_id) ON {schema}.note"),
                format!("UPDATE(content) ON {schema}.note_original,{schema}.note_revised_current"),
                format!("UPDATE ON {schema}.embedding_set"),
                format!("INSERT ON {schema}.provenance_activity"),
                format!("DELETE ON {schema}.shard_embedding_set_bootstrap"),
            ] {
                sqlx::query(&format!("GRANT {grant} TO {role}"))
                    .execute(admin)
                    .await
                    .unwrap();
            }
        }
        let mut types = std::collections::HashMap::new();
        for tenant in [a, b] {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO public.document_type(id,tenant_id,name,display_name,category,filename_patterns) VALUES($1,$2,$3,'Owned worker control','custom',ARRAY['worker.fixture'])")
                .bind(id).bind(tenant).bind(format!("worker_{}",id.simple())).execute(admin).await.unwrap();
            types.insert(tenant, id);
        }
        let controls: Vec<_> = [
            (a, "public", "public"),
            (b, "public", "public"),
            (a, archive, "search_fixture"),
        ]
        .into_iter()
        .map(|(tenant, schema, memory)| (tenant, schema, memory, Uuid::new_v4(), Uuid::new_v4()))
        .collect();
        let audit_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.audit_event WHERE tenant_id IN ($1,$2)",
        )
        .bind(a)
        .bind(b)
        .fetch_one(admin)
        .await
        .unwrap();
        std::fs::write(self.root.join("worker-sse-fixture.json"),serde_json::to_vec_pretty(&json!({"url":self.url,"controls":controls.iter().map(|(tenant,schema,memory,note,job)|json!({"tenant":tenant,"schema":schema,"memory":memory,"note":note,"job":job})).collect::<Vec<_>>()})).unwrap()).unwrap();
        let log = std::fs::File::create(self.root.join("worker-sse.log")).unwrap();
        let mut consumer = tokio::process::Command::new("node")
            .arg(format!(
                "{}/src/scoped_search_tests/worker-sse-consumer.mjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .arg(&self.root)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let consumer_pid = consumer.id().unwrap();
        std::fs::write(
            self.root.join("worker-sse-process.json"),
            serde_json::to_vec_pretty(&json!({"pid":consumer_pid})).unwrap(),
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let began = tokio::time::Instant::now();
            let mut sampled = false;
            while !self.root.join("worker-sse-ready.json").exists() {
                assert!(
                    consumer.try_wait().unwrap().is_none(),
                    "SSE consumer exited before readiness; inspect worker-sse.log"
                );
                if !sampled && began.elapsed() > Duration::from_secs(3) {
                    self.sse_database_diagnostic(admin, "admission").await;
                    sampled = true;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("SSE admission deadline");
        let mut receipts = Vec::new();
        for (tenant, schema, _, note, job) in controls {
            let mut tx = admin.begin().await.unwrap();
            sqlx::query(
                "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
            )
            .bind(tenant.to_string())
            .bind(format!("{schema},public"))
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query("INSERT INTO note(id,tenant_id,format,source,metadata,created_at_utc,updated_at_utc) VALUES($1,$2,'markdown','owned-worker-control',$3,NOW(),NOW())")
                .bind(note).bind(tenant).bind(json!({"source_file":"worker.fixture"})).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO note_original(id,note_id,content,hash) VALUES($1,$2,'worker acceptance control','fixture')")
                .bind(Uuid::new_v4()).bind(note).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO note_revised_current(note_id,content) VALUES($1,'')")
                .bind(note)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'document_type_inference',0,$4)")
                .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let status: String =
                        sqlx::query_scalar("SELECT status::text FROM public.job_queue WHERE id=$1")
                            .bind(job)
                            .fetch_one(admin)
                            .await
                            .unwrap();
                    assert_ne!(
                        status, "failed",
                        "production worker failed owned control job"
                    );
                    if status == "completed" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .expect("production worker must actually settle a committed queue insertion");
            let actual: (Uuid,i32,i64) = sqlx::query_as(&format!("SELECT document_type_id,access_count,(SELECT count(*) FROM {schema}.provenance_activity WHERE id=$2 AND note_id=$1 AND ended_at IS NOT NULL) FROM {schema}.note WHERE id=$1"))
                .bind(note).bind(job).fetch_one(admin).await.unwrap();
            assert_eq!(actual, (types[&tenant], 1, 1));
            let attempt: (Uuid, String) =
                sqlx::query_as("SELECT id,outcome FROM public.job_attempt WHERE job_id=$1")
                    .bind(job)
                    .fetch_one(admin)
                    .await
                    .unwrap();
            assert_eq!(attempt.1, "completed");
            receipts.push(json!({"tenant":tenant,"schema":schema,"note":note,"job":job,"attempt":attempt.0,"status":"completed","assignment":true,"accessCount":1,"provenanceCount":1}));
        }
        self.sse_database_diagnostic(admin, "after-jobs").await;
        let consumer_exit = tokio::time::timeout(Duration::from_secs(30), async {
            let wait = consumer.wait();
            tokio::pin!(wait);
            tokio::select! {
                status = &mut wait => status,
                _ = tokio::time::sleep(Duration::from_secs(3)) => {
                    self.sse_database_diagnostic(admin,"replay-wait").await;
                    wait.await
                }
            }
        })
        .await
        .expect("bounded SSE consumer exit")
        .unwrap();
        std::fs::write(
            self.root.join("worker-sse-stopped.json"),
            serde_json::to_vec_pretty(&json!({"pid":consumer_pid,"exit":consumer_exit.code()}))
                .unwrap(),
        )
        .unwrap();
        assert!(
            consumer_exit.success(),
            "worker SSE failed; inspect worker-sse.log"
        );
        let audit_after: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.audit_event WHERE tenant_id IN ($1,$2)",
        )
        .bind(a)
        .bind(b)
        .fetch_one(admin)
        .await
        .unwrap();
        std::fs::write(self.root.join("binary-worker-execution.json"),serde_json::to_vec_pretty(&json!({"status":"PASS","runtime":"actual hosted binary","controls":receipts,"sseAuditRows":audit_after-audit_before,"sharedInferenceUsed":false,"unqualified":"other handlers, followup execution, queue summaries, complete lifecycle and HotM application archive selection"})).unwrap()).unwrap();
        println!("hosted_binary_worker: three real queued document-type jobs; two tenants and archive; assignment/access/provenance and attempt settlement committed");
    }

    pub async fn verify_browser(&self, admin: &PgPool) {
        let Ok(driver) = std::env::var("FORTEMI_BROWSER_DRIVER") else {
            return;
        };
        let worker: serde_json::Value = serde_json::from_slice(
            &std::fs::read(self.root.join("worker-sse-fixture.json")).unwrap(),
        )
        .unwrap();
        let jobs: Vec<_> = worker["controls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|control| {
                let mut job = control.clone();
                job["job"] = json!(Uuid::new_v4());
                job
            })
            .collect();
        assert_eq!(jobs.len(), 3);
        std::fs::write(
            self.root.join("browser-jobs-fixture.json"),
            serde_json::to_vec_pretty(&jobs).unwrap(),
        )
        .unwrap();
        let log = std::fs::File::create(self.root.join("browser.log")).unwrap();
        let mut child = tokio::process::Command::new("node")
            .arg(driver)
            .arg(&self.root)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        std::fs::write(
            self.root.join("browser-process.json"),
            serde_json::to_vec_pretty(&json!({"pid":child.id()})).unwrap(),
        )
        .unwrap();
        let mut executed = Vec::new();
        let status = tokio::time::timeout(Duration::from_secs(150), async {
            // Only the three predeclared owned controls can be enqueued by this handshake.
            for index in [0usize, 2, 1] {
                let request = self.root.join(format!("browser-job-request-{index}.json"));
                while !request.exists() {
                    assert!(child.try_wait().unwrap().is_none(), "browser exited before job handshake");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                let bytes = std::fs::read(request).unwrap();
                assert!(bytes.len() <= 64);
                assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(), json!({"index":index}));
                let control = &jobs[index];
                let tenant = Uuid::parse_str(control["tenant"].as_str().unwrap()).unwrap();
                let note = Uuid::parse_str(control["note"].as_str().unwrap()).unwrap();
                let job = Uuid::parse_str(control["job"].as_str().unwrap()).unwrap();
                let schema = control["schema"].as_str().unwrap();
                matric_db::validate_schema_name(schema).unwrap();
                let mut tx = admin.begin().await.unwrap();
                sqlx::query("SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)")
                    .bind(tenant.to_string()).bind(format!("{schema},public")).execute(&mut *tx).await.unwrap();
                sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'document_type_inference',0,$4)")
                    .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).execute(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
                tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        let status: String = sqlx::query_scalar("SELECT status::text FROM public.job_queue WHERE id=$1")
                            .bind(job).fetch_one(admin).await.unwrap();
                        assert_ne!(status, "failed", "browser-owned worker control failed");
                        if status == "completed" { break; }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }).await.expect("browser-owned worker settlement deadline");
                let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_attempt WHERE job_id=$1 AND outcome='completed'")
                    .bind(job).fetch_one(admin).await.unwrap();
                assert_eq!(attempts, 1);
                executed.push(json!({"index":index,"tenant":tenant,"schema":schema,"note":note,"job":job,"status":"completed","completedAttempts":attempts}));
            }
            child.wait().await
        })
            .await.expect("bounded browser deadline").unwrap();
        std::fs::write(
            self.root.join("browser-job-execution.json"),
            serde_json::to_vec_pretty(&executed).unwrap(),
        )
        .unwrap();
        std::fs::write(
            self.root.join("browser-stopped.json"),
            serde_json::to_vec_pretty(&json!({"exit":status.code()})).unwrap(),
        )
        .unwrap();
        assert!(
            status.success(),
            "browser acceptance failed; inspect bounded browser.log"
        );
        println!("hosted_browser: actual production UI, cross-tab context and automatic reconnect; browser closed");
    }

    pub fn observer_target(&self) -> Router {
        let url = self.url.clone();
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Router::new().fallback(move |request: axum::extract::Request| {
            let client = client.clone();
            let url = url.clone();
            async move {
                let (parts, body) = request.into_parts();
                let bytes = axum::body::to_bytes(body, 65536).await.unwrap();
                let mut headers = parts.headers;
                for name in ["host", "connection", "transfer-encoding"] {
                    headers.remove(name);
                }
                let target = format!("{url}{}", parts.uri.path_and_query().unwrap());
                let mut response = client
                    .request(parts.method, target)
                    .headers(headers)
                    .body(bytes)
                    .send()
                    .await
                    .unwrap();
                let status = response.status();
                let mut headers = response.headers().clone();
                for name in ["connection", "transfer-encoding"] {
                    headers.remove(name);
                }
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await.unwrap() {
                    assert!(
                        body.len() + chunk.len() <= 1048576,
                        "bounded binary observer response"
                    );
                    body.extend_from_slice(&chunk);
                }
                let mut output = axum::response::Response::new(Body::from(body));
                *output.status_mut() = status;
                *output.headers_mut() = headers;
                output
            }
        })
    }

    pub async fn stop(mut self) -> bool {
        let pid = self
            .child
            .id()
            .expect("binary must remain live through the matrix");
        assert!(self.child.try_wait().unwrap().is_none());
        let signal = tokio::process::Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .await
            .unwrap();
        assert!(signal.success());
        let status = tokio::time::timeout(Duration::from_secs(10), self.child.wait())
            .await
            .expect("binary graceful shutdown deadline")
            .unwrap();
        assert!(status.success(), "binary shutdown must succeed");
        let log = std::fs::read(self.root.join("production-binary.log")).unwrap();
        assert!(log.len() <= 1048576, "bounded binary log");
        let log = std::str::from_utf8(&log).unwrap();
        let components = [
            "Redis search cache enabled",
            "Chat-stream resumption store enabled",
            "Ingest cursor store enabled",
            "Ingest token store enabled",
            "Idempotency store enabled",
            "OpenAI health check passed",
            "Stale-running job sweep completed",
        ]
        .map(|marker| (marker, log.contains(marker)));
        let worker_recovery_error = has_worker_recovery_error(log);
        let healthy = components.iter().all(|(_, present)| *present)
            && !worker_recovery_error
            && !log.contains("Redis connect timed out")
            && !log.contains("Redis search cache connect timed out");
        std::fs::write(
            self.root.join("binary-startup-health.json"),
            serde_json::to_vec_pretty(&json!({
                "status":if healthy {"PASS"} else {"NOT_PASS"},"components":components,
                "workerRecoveryError":worker_recovery_error
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            self.root.join("binary-stopped.json"),
            serde_json::to_vec_pretty(&json!({"pid":pid,"exit":status.code(),"stopped":true}))
                .unwrap(),
        )
        .unwrap();
        healthy
    }
}

fn has_worker_recovery_error(log: &str) -> bool {
    log.contains("Failed to recover stale running jobs")
        || log.contains("Hosted stale-running recovery pass incomplete")
}

#[test]
fn recovery_health_rejects_partial_failures() {
    let success = "Stale-running job sweep completed";
    assert!(!has_worker_recovery_error(success));
    for failure in [
        "Failed to recover stale running jobs",
        "Hosted stale-running recovery pass incomplete",
    ] {
        assert!(has_worker_recovery_error(failure));
        assert!(has_worker_recovery_error(&format!("{failure}\n{success}")));
    }
}

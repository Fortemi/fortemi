//! Payload-selected jobs, private job reads, and the actual MCP forwarding path.
use super::*;

const SUBMITTABLE: &[&str] = &[
    "ai_revision",
    "embedding",
    "linking",
    "context_update",
    "title_generation",
    "concept_tagging",
    "reference_extraction",
    "related_concept_inference",
    "re_embed_all",
    "extraction",
    "exif_extraction",
    "graph_maintenance",
    "speaker_diarization",
    "speaker_relabel",
    "media_optimize",
    "audio_transcription",
    "audio_chunk_transcription",
    "keyframe_vision",
    "keyframe_assembly",
    "view_vision",
    "view_assembly",
    "thumbnail_sprite",
];

#[sqlx::test(migrations = false)]
async fn every_submittable_worker_authorizes_its_payload_schema(admin: sqlx::PgPool) {
    let (state, app, _, token, a, b) = restricted(&admin).await;
    for kind in SUBMITTABLE {
        let mut results = Vec::new();
        for schema in [&b, "unknown_schema"] {
            results.push(call_as(&app,&token,Method::POST,"/api/v1/jobs",
                serde_json::json!({"job_type":kind,"payload":{"schema":schema},"deduplicate":false}),Some("archive_a")).await);
        }
        assert_eq!(
            results[0].0,
            StatusCode::NOT_FOUND,
            "{kind}: {:?}",
            results[0]
        );
        assert_eq!(results[0], results[1], "{kind}");
    }
    // A forwarded memory hint cannot override the actual schema, or smuggle B.
    for payload in [
        serde_json::json!({"schema":b,"memory":"archive_a"}),
        serde_json::json!({"schema":a,"memory":"archive_b"}),
        serde_json::json!({"schema":"archive_a"}),
        serde_json::json!({}),
    ] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::POST,
            "/api/v1/jobs",
            serde_json::json!({"job_type":"graph_maintenance","payload":payload}),
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }
    for key in [
        "_source_path",
        "_checkpoint",
        "_skip_transcode",
        "_skip_vision",
        "_skip_transcription",
        "_future_internal",
    ] {
        let (status, body) = call_as(&app, &token, Method::POST, "/api/v1/jobs",
            serde_json::json!({"job_type":"extraction","payload":{"schema":a,"data":"inline","config":{key:"/denied/archive.wav"}}}), Some("archive_a")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{key}: {body}");
    }
    let (status, body) = call_as(&app, &token, Method::POST, "/api/v1/jobs",
        serde_json::json!({"job_type":"extraction","payload":{"schema":a,"nested":[{"_source_path":"/denied/archive.wav"}]}}), Some("archive_a")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rejections must not queue work");
    let (status, body) = call_as(
        &app,
        &token,
        Method::POST,
        "/api/v1/jobs",
        serde_json::json!({"job_type":"graph_maintenance","payload":{"schema":a}}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn jobs_are_filtered_before_pagination_counts_and_detail(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let mut ids = Vec::new();
    for payload in [
        Some(serde_json::json!({"schema":a})),
        Some(serde_json::json!({"schema":b})),
        None,
    ] {
        ids.push(
            state
                .db
                .jobs
                .queue(None, JobType::GraphMaintenance, 1, payload, None)
                .await
                .unwrap(),
        );
    }
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        "/api/v1/jobs?limit=1",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert_eq!(body["pending"], 1);
    assert_eq!(body["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(body["jobs"][0]["id"], ids[0].to_string());
    let missing = call_as(
        &app,
        &token,
        Method::GET,
        &format!("/api/v1/jobs/{}", Uuid::new_v4()),
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    for id in &ids[1..] {
        let hidden = call_as(
            &app,
            &token,
            Method::GET,
            &format!("/api/v1/jobs/{id}"),
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(hidden.0, StatusCode::NOT_FOUND);
        assert_eq!(hidden, missing);
    }
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        &format!("/api/v1/jobs/{}", ids[0]),
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = call_as(
        &original,
        &token,
        Method::GET,
        "/api/v1/jobs",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(body["total"], 3, "default policy retains unscoped jobs");
    assert_eq!(body["jobs"].as_array().unwrap().len(), 3);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn mcp_submission_reaches_the_same_payload_gate(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/archives/archive_a/set-default",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let script = r#"
        import assert from 'node:assert/strict';
        import { Client } from '@modelcontextprotocol/sdk/client/index.js';
        import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
        const client = new Client({name:'archive-policy-test',version:'1'});
        const transport = new StdioClientTransport({command:process.execPath,args:['index.js'],env:{...process.env,MCP_TRANSPORT:'stdio'},stderr:'pipe'});
        try {
            await client.connect(transport);
            for (const schema of [process.env.TEST_B_SCHEMA,'unknown_schema']) {
                const result = await client.callTool({name:'manage_jobs',arguments:{action:'create',job_type:'graph_maintenance',payload:{schema},deduplicate:false}});
                assert.equal(result.isError,true,JSON.stringify(result));
                assert.match(JSON.stringify(result),/404/);
            }
            for (const key of ['_source_path','_checkpoint','_skip_transcode','_skip_vision','_skip_transcription']) {
                const result = await client.callTool({name:'manage_jobs',arguments:{action:'create',job_type:'extraction',payload:{schema:process.env.TEST_A_SCHEMA,data:'inline',config:{[key]:'/denied/archive.wav'}},deduplicate:false}});
                assert.equal(result.isError,true,JSON.stringify(result));
                assert.match(JSON.stringify(result),/400/);
            }
            const allowed = await client.callTool({name:'manage_jobs',arguments:{action:'create',job_type:'graph_maintenance',payload:{schema:process.env.TEST_A_SCHEMA},deduplicate:false}});
            assert.ok(!allowed.isError,JSON.stringify(allowed));
        } finally { await client.close(); }
    "#;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        tokio::process::Command::new("node")
            .args(["--input-type=module", "--eval", script])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mcp-server"))
            .env("FORTEMI_URL", url)
            .env("FORTEMI_API_KEY", &token)
            .env("MCP_TOOL_MODE", "full")
            .env("TEST_A_SCHEMA", a)
            .env("TEST_B_SCHEMA", b)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    server.abort();
    assert!(
        output.status.success(),
        "MCP regression: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "only the permitted MCP job was queued");
    state.db.pool.close().await;
}

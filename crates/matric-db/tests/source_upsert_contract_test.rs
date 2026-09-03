use matric_core::{
    ArchiveRepository, SourceUpsertBatchOutcome, SourceUpsertItemOutcome, SourceUpsertRequest,
};
use matric_db::{Database, PgSourceUpsertRepository};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Deserialize)]
struct Fixture {
    source: Value,
    cases: Vec<FixtureCase>,
}

#[derive(Deserialize)]
struct FixtureCase {
    id: String,
    request: Option<Value>,
    repeat: Option<String>,
    expected: FixtureExpected,
}

#[derive(Deserialize)]
struct FixtureExpected {
    batch: SourceUpsertBatchOutcome,
    items: Vec<SourceUpsertItemOutcome>,
    material_changes: usize,
    reason_code: Option<String>,
}

async fn execute(
    db: &Database,
    schema: &str,
    request: SourceUpsertRequest,
) -> matric_core::Result<matric_core::SourceUpsertResponse> {
    let repository = PgSourceUpsertRepository::new(db.pool.clone());
    db.for_schema(schema)?
        .execute(move |tx| Box::pin(async move { repository.upsert_tx(tx, request).await }))
        .await
}

fn build_request(source: &Value, request: &Value, namespace: &str) -> SourceUpsertRequest {
    let mut value = request.clone();
    let object = value.as_object_mut().expect("fixture request object");
    for (key, source_value) in source.as_object().expect("fixture source object") {
        object.insert(key.clone(), source_value.clone());
    }
    object.insert(
        "source_namespace".to_string(),
        Value::String(namespace.to_string()),
    );
    serde_json::from_value(value).expect("fixture request follows authority schema")
}

async fn counts_for_note(
    db: &Database,
    note_id: Uuid,
    namespace: &str,
) -> (i64, i64, i64, i64, i64) {
    let namespace = namespace.to_string();
    db.default_schema()
        .query(move |tx| {
            Box::pin(async move {
                let revisions = sqlx::query_scalar("SELECT COUNT(*) FROM note_revision WHERE note_id = $1")
                    .bind(note_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?;
                let activity = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE note_id = $1")
                    .bind(note_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?;
                let jobs = sqlx::query_scalar("SELECT COUNT(*) FROM job_queue WHERE note_id = $1")
                    .bind(note_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?;
                let outbox = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM event_outbox WHERE payload::text LIKE '%' || $1::text || '%'",
                )
                .bind(note_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                let journals = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM source_import_batch WHERE source_namespace = $1",
                )
                .bind(namespace)
                .fetch_one(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                Ok((revisions, activity, jobs, outbox, journals))
            })
        })
        .await
        .expect("read conformance counts")
}

#[tokio::test]
async fn server_executes_shared_source_upsert_conformance_fixture() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping source upsert conformance: DATABASE_URL unavailable");
        return;
    };
    let db = Database::connect(&database_url)
        .await
        .expect("connect database");
    db.migrate().await.expect("migrate database");

    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../contracts/source-note-upsert/conformance/v1.json"
    ))
    .expect("parse shared source upsert fixture");
    let namespace = format!("conformance.test.{}", Uuid::new_v4());
    let mut requests: std::collections::HashMap<String, SourceUpsertRequest> =
        std::collections::HashMap::new();
    let mut inserted_note = None;
    let mut replay_baseline = None;

    for case in fixture.cases {
        let mut request = if let Some(repeat) = &case.repeat {
            requests.get(repeat).expect("repeat target exists").clone()
        } else {
            build_request(
                &fixture.source,
                case.request.as_ref().expect("case request exists"),
                &namespace,
            )
        };
        if case.id == "insert" {
            request.items[0].caller_stable_id = Some(Uuid::now_v7());
        }
        requests.insert(case.id.clone(), request.clone());

        let response = execute(&db, "public", request)
            .await
            .expect("execute conformance case");
        assert_eq!(response.outcome, case.expected.batch, "case {}", case.id);
        assert_eq!(
            response
                .items
                .iter()
                .map(|item| item.outcome)
                .collect::<Vec<_>>(),
            case.expected.items,
            "case {}",
            case.id
        );
        let persisted_changes = if response.outcome == SourceUpsertBatchOutcome::Committed {
            response.counts.material_changes()
        } else {
            0
        };
        assert_eq!(
            persisted_changes, case.expected.material_changes,
            "case {}",
            case.id
        );
        if let Some(reason_code) = case.expected.reason_code {
            assert!(response
                .items
                .iter()
                .all(|item| item.reason_code.as_deref() == Some(reason_code.as_str())));
        }
        if case.id == "insert" {
            let note_id = response.items[0].note_id.expect("insert returns note ID");
            inserted_note = Some(note_id);
            replay_baseline = Some(counts_for_note(&db, note_id, &namespace).await);
        }
        if case.id == "exact-batch-replay" {
            assert_eq!(
                counts_for_note(&db, inserted_note.unwrap(), &namespace).await,
                replay_baseline.unwrap(),
                "exact replay must not add revisions, activity, jobs, outbox, or journals"
            );
        }
    }

    let note_id = inserted_note.expect("fixture inserted a note");
    let (revisions, _, jobs, outbox, _) = counts_for_note(&db, note_id, &namespace).await;
    assert_eq!(revisions, 2, "replace does not add a revision");
    assert_eq!(jobs, 0);
    assert_eq!(outbox, 0);

    let leaked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM activity_log WHERE note_id = $1 AND (meta::text LIKE '%' || $2 || '%' OR meta::text LIKE '%' || $3 || '%'))",
    )
    .bind(note_id)
    .bind("fixture-a")
    .bind("alpha replacement")
    .fetch_one(&db.pool)
    .await
    .expect("inspect redacted activity");
    assert!(
        !leaked,
        "operation activity must not contain raw key or content"
    );

    // An unexpected second-item database conflict must roll the first insert back.
    let failure_namespace = format!("conformance.failure.{}", Uuid::new_v4());
    let shared_id = Uuid::now_v7();
    let mut failure = requests["insert"].clone();
    failure.source_namespace = failure_namespace.clone();
    failure.import_run_id = "failure-run".to_string();
    failure.batch_id = Some("failure-batch".to_string());
    failure.items[0].caller_stable_id = Some(shared_id);
    let mut second = failure.items[0].clone();
    second.external_id = "fixture-failure-b".to_string();
    second.content = "second item".to_string();
    second.content_digest = None;
    second.caller_stable_id = Some(shared_id);
    failure.items.push(second);
    assert!(execute(&db, "public", failure).await.is_err());
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM source_identity WHERE source_namespace = $1")
            .bind(&failure_namespace)
            .fetch_one(&db.pool)
            .await
            .expect("check rollback");
    assert_eq!(remaining, 0, "failed batch must roll back every item");

    // Equal external keys in another memory are distinct identities.
    let archive_name = format!("source-upsert-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("source upsert conformance"))
        .await
        .expect("create conformance memory");
    let mut archive_request = requests["insert"].clone();
    archive_request.import_run_id = "archive-run".to_string();
    archive_request.batch_id = Some("archive-batch".to_string());
    archive_request.items[0].caller_stable_id = Some(Uuid::now_v7());
    let archive_response = execute(&db, &archive.schema_name, archive_request)
        .await
        .expect("upsert same source key in another memory");
    assert_eq!(
        archive_response.items[0].outcome,
        SourceUpsertItemOutcome::Inserted
    );
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("drop conformance memory");
}

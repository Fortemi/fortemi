//! Live regression cases: the caller owns a fresh migrated disposable DATABASE_URL.
//! Missing DB configuration is a failure, including in required-live mode.
use super::*;
use futures::{channel::mpsc, stream};
use matric_db::EmbeddingImportRunReport;
use serde_json::Value;
use std::time::Duration;
use tokio::{task::JoinHandle, time::timeout};

const BOUND: Duration = Duration::from_secs(30);
const LINE_LIMIT: usize = 1024 * 1024;

// A panic/timeout must never leave the paused importer running in the background.
struct AbortOnDrop<T>(JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn live_db() -> Database {
    let url = std::env::var("DATABASE_URL")
        .expect("explicit DATABASE_URL must select the parent's migrated disposable DB");
    timeout(BOUND, Database::connect(&url))
        .await
        .expect("DB connect bounded")
        .expect("connect live migrated fixture")
}

async fn raw_run(db: &Database, set_id: Uuid, run_id: &str) -> Option<Value> {
    sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM embedding_import_run r WHERE set_id = $1 AND run_id = $2",
    )
    .bind(set_id)
    .bind(run_id)
    .fetch_optional(&db.pool)
    .await
    .expect("read raw journal row")
}

fn initial_report(run_id: &str) -> Value {
    json!({
        "run_id": run_id, "status": "applying",
        "inserted": 0, "updated": 0, "unchanged": 0, "deleted": 0,
        "rejected": {"total": 0, "rows": []}
    })
}

// Snapshot effects owned by this fixture, including receipts and content, so replay
// cannot quietly update a note/vector or add another source admission.
async fn effects(db: &Database, set_id: Uuid, source: &str) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
          'vectors', (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.id), '[]'::jsonb)
                      FROM embedding e WHERE e.embedding_set_id = $1),
          'identities', (SELECT coalesce(jsonb_agg(to_jsonb(s) ORDER BY s.external_id), '[]'::jsonb)
                         FROM source_identity s WHERE s.source_namespace = $2),
          'notes', (SELECT coalesce(jsonb_agg(to_jsonb(n) ORDER BY n.id), '[]'::jsonb)
                    FROM note n WHERE n.id IN
                    (SELECT note_id FROM source_identity WHERE source_namespace = $2)),
          'runs', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.import_run_id), '[]'::jsonb)
                   FROM source_import_run r WHERE r.source_namespace = $2),
          'batches', (SELECT coalesce(jsonb_agg(to_jsonb(b) ORDER BY b.batch_id), '[]'::jsonb)
                      FROM source_import_batch b WHERE b.source_namespace = $2))",
    )
    .bind(set_id)
    .bind(source)
    .fetch_one(&db.pool)
    .await
    .expect("snapshot fixture effects")
}

#[tokio::test]
async fn applying_report_is_persisted_readable_and_duplicate_cannot_admit() {
    timeout(BOUND, async {
        let db = live_db().await;
        let slug = format!("report-live-{}", Uuid::new_v4().simple());
        let run_id = format!("{slug}-run");
        let source = format!("{slug}-source");
        let (set_id, space_id) = external_set(&db, &slug).await;
        let repo = PgEmbeddingImportRepository::new(db.pool.clone());
        let mut row = profile("entity", &source, "live report", [1.0, 0.0, 0.0], &space_id);
        row["type"] = json!("profile");
        let body = format!("{}\n", serde_json::to_string(&row).unwrap()).into_bytes();
        let manifest = format!(
            "{}\n",
            json!({
                "type": "manifest", "run_id": run_id, "previous_run_id": null,
                "space_id": space_id, "created_at": "2026-10-31T00:00:00Z",
                "counts": {"profiles": 1, "deletions": 0}, "files": [],
                "template_version": "profile-v1",
                "body_sha256": hex::encode(Sha256::digest(&body))
            })
        )
        .into_bytes();
        let before = effects(&db, set_id, &source).await;
        let (sender, receiver) = mpsc::unbounded::<Result<Vec<u8>, std::io::Error>>();
        sender.unbounded_send(Ok(manifest.clone())).unwrap();
        let task_repo = PgEmbeddingImportRepository::new(db.pool.clone());
        let task_slug = slug.clone();
        let mut task = AbortOnDrop(tokio::spawn(async move {
            task_repo
                .import_run_ndjson("public", &task_slug, receiver, LINE_LIMIT, None, None)
                .await
        }));

        // The body sender stays open and empty: completion is impossible until we
        // release it. Observe the committed row, never a guessed sleep interval.
        let applying = timeout(Duration::from_secs(10), async {
            loop {
                if let Some(row) = raw_run(&db, set_id, &run_id).await {
                    break row;
                }
                assert!(
                    !task.0.is_finished(),
                    "import ended before journal admission"
                );
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("start_run must commit while body is paused");
        assert!(!task.0.is_finished());
        assert_eq!(applying["status"], "applying");
        assert_eq!(applying["finished_at"], Value::Null);
        assert_eq!(applying["profiles_count"], 1);
        assert_eq!(applying["deletions_count"], 0);
        assert_eq!(applying["report"], initial_report(&run_id));
        // Strict serde decoding of the raw JSON proves this new row needs no
        // compatibility fallback to supply its required fields.
        let strict: EmbeddingImportRunReport =
            serde_json::from_value(applying["report"].clone()).expect("complete raw typed report");
        assert_eq!(strict.run_id, run_id);
        let get = repo
            .get_run("public", &slug, &run_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(get.status, "applying");
        assert!(get.finished_at.is_none());
        assert_eq!(
            serde_json::to_value(&get.report).unwrap(),
            initial_report(&run_id)
        );
        let list = repo
            .list_runs("public", &slug)
            .await
            .expect("list during applying");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].run_id, run_id);
        assert_eq!(
            serde_json::to_value(&list[0].report).unwrap(),
            initial_report(&run_id)
        );

        // An open second body contains only the manifest. Refusal must happen
        // before reading its body or attempting another start_run INSERT.
        let (retry_sender, retry_receiver) = mpsc::unbounded::<Result<Vec<u8>, std::io::Error>>();
        retry_sender.unbounded_send(Ok(manifest.clone())).unwrap();
        let error = timeout(
            Duration::from_secs(5),
            repo.import_run_ndjson("public", &slug, retry_receiver, LINE_LIMIT, None, None),
        )
        .await
        .expect("duplicate refusal must not wait for body")
        .unwrap_err();
        assert!(
            matches!(error, matric_core::Error::InvalidInput(ref message)
            if message == matric_db::vector_import::IMPORT_RUN_IN_PROGRESS)
        );
        drop(retry_sender);
        // Exercise the folder admission path against the same paused original.
        let root = tempfile::tempdir().unwrap();
        let folder = write_run(
            root.path(),
            &run_id,
            None,
            &space_id,
            vec![profile(
                "entity",
                &source,
                "live report",
                [1.0, 0.0, 0.0],
                &space_id,
            )],
            vec![],
        );
        let error = repo
            .import_run_folder("public", &slug, folder.path(), None, None)
            .await
            .unwrap_err();
        assert!(
            matches!(error, matric_core::Error::InvalidInput(ref message)
            if message == matric_db::vector_import::IMPORT_RUN_IN_PROGRESS)
        );
        assert_eq!(raw_run(&db, set_id, &run_id).await.unwrap(), applying);
        assert_eq!(effects(&db, set_id, &source).await, before);
        assert!(!task.0.is_finished());

        sender.unbounded_send(Ok(body)).unwrap();
        drop(sender); // EOF permits the original import to finish exactly once.
        let report = (&mut task.0)
            .await
            .expect("import task")
            .expect("original completes");
        assert_eq!(report.status, "applied");
        assert_eq!(report.inserted, 1);
        assert_eq!(
            report.updated + report.unchanged + report.deleted + report.rejected.total,
            0
        );
        let completed = raw_run(&db, set_id, &run_id).await.unwrap();
        assert_eq!(completed["id"], applying["id"]);
        assert_eq!(completed["started_at"], applying["started_at"]);
        assert_eq!(completed["status"], "applied");
        assert!(!completed["finished_at"].is_null());
        assert_eq!(completed["report"], serde_json::to_value(&report).unwrap());
        let after = effects(&db, set_id, &source).await;
        assert_eq!(after["vectors"].as_array().unwrap().len(), 1);
        assert_eq!(after["notes"].as_array().unwrap().len(), 1);
        assert_eq!(after["identities"].as_array().unwrap().len(), 1);
        assert_eq!(after["runs"].as_array().unwrap().len(), 1);
        assert_eq!(after["batches"].as_array().unwrap().len(), 1);
        let replay = repo
            .import_run_ndjson(
                "public",
                &slug,
                stream::iter(vec![Ok::<_, std::io::Error>(manifest)]),
                LINE_LIMIT,
                None,
                None,
            )
            .await
            .expect("applied replay");
        assert_eq!(replay.status, "already_applied");
        assert_eq!(replay.inserted, 1);
        assert_eq!(raw_run(&db, set_id, &run_id).await.unwrap(), completed);
        assert_eq!(effects(&db, set_id, &source).await, after);
        let rows = repo.list_runs("public", &slug).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "applied");
    })
    .await
    .expect("live report lifecycle regression bounded");
}

#[tokio::test]
async fn legacy_applying_reads_are_nonmutating_but_completed_corruption_is_error() {
    timeout(BOUND, async {
        let db = live_db().await;
        let slug = format!("report-legacy-{}", Uuid::new_v4().simple());
        let (set_id, space_id) = external_set(&db, &slug).await;
        let repo = PgEmbeddingImportRepository::new(db.pool.clone());
        let root = tempfile::tempdir().unwrap();
        let applied_id = format!("{slug}-applied");
        let run = write_run(
            root.path(),
            &applied_id,
            None,
            &space_id,
            vec![profile(
                "entity",
                &slug,
                "completed",
                [1.0, 0.0, 0.0],
                &space_id,
            )],
            vec![],
        );
        let applied = repo
            .import_run_folder("public", &slug, run.path(), None, None)
            .await
            .unwrap();
        assert_eq!(applied.inserted, 1);
        let second_id = format!("{slug}-second");
        let second = write_run(
            root.path(),
            &second_id,
            Some(&applied_id),
            &space_id,
            vec![],
            vec![],
        );
        repo.import_run_folder("public", &slug, second.path(), None, None)
            .await
            .unwrap();
        let legacy_id = format!("{slug}-legacy");
        // Exact historical sentinel, unfinished applying; retain nonzero journal
        // accounting to make explicit that synthesized zero means UNKNOWN progress.
        sqlx::query(
            "INSERT INTO embedding_import_run
            (set_id, run_id, status, manifest, report, inserted_count)
            VALUES ($1, $2, 'applying', '{}'::jsonb, '{}'::jsonb, 7)",
        )
        .bind(set_id)
        .bind(&legacy_id)
        .execute(&db.pool)
        .await
        .unwrap();
        let legacy_before = raw_run(&db, set_id, &legacy_id).await.unwrap();
        let completed_before = raw_run(&db, set_id, &applied_id).await.unwrap();
        let second_before = raw_run(&db, set_id, &second_id).await.unwrap();
        let effects_before = effects(&db, set_id, &slug).await;
        for _ in 0..2 {
            let legacy = repo
                .get_run("public", &slug, &legacy_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(legacy.run_id, legacy_id);
            assert_eq!(legacy.status, "applying");
            assert!(legacy.finished_at.is_none());
            assert_eq!(
                serde_json::to_value(&legacy.report).unwrap(),
                initial_report(&legacy_id)
            );
            let rows = repo
                .list_runs("public", &slug)
                .await
                .expect("mixed legacy/completed list");
            assert_eq!(rows.len(), 3);
            let legacy = rows.iter().find(|r| r.run_id == legacy_id).unwrap();
            assert_eq!(
                serde_json::to_value(&legacy.report).unwrap(),
                initial_report(&legacy_id)
            );
            let completed = rows.iter().find(|r| r.run_id == applied_id).unwrap();
            assert_eq!(completed.status, "applied");
            assert_eq!(
                serde_json::to_value(&completed.report).unwrap(),
                serde_json::to_value(&applied).unwrap()
            );
            assert!(rows
                .iter()
                .find(|r| r.run_id == second_id)
                .unwrap()
                .finished_at
                .is_some());
        }
        assert_eq!(legacy_before["report"], json!({}));
        assert_eq!(legacy_before["inserted_count"], 7);
        assert_eq!(
            raw_run(&db, set_id, &legacy_id).await.unwrap(),
            legacy_before
        );
        assert_eq!(
            raw_run(&db, set_id, &applied_id).await.unwrap(),
            completed_before
        );
        assert_eq!(
            raw_run(&db, set_id, &second_id).await.unwrap(),
            second_before
        );
        assert_eq!(effects(&db, set_id, &slug).await, effects_before);

        for status in ["applied", "failed"] {
            let corrupt_id = format!("{slug}-corrupt-{status}");
            sqlx::query(
                "INSERT INTO embedding_import_run
                (set_id, run_id, status, manifest, report, finished_at)
                VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now())",
            )
            .bind(set_id)
            .bind(&corrupt_id)
            .bind(status)
            .execute(&db.pool)
            .await
            .unwrap();
            let before = raw_run(&db, set_id, &corrupt_id).await.unwrap();
            assert!(matches!(
                repo.get_run("public", &slug, &corrupt_id).await,
                Err(matric_core::Error::Config(_))
            ));
            assert!(matches!(
                repo.list_runs("public", &slug).await,
                Err(matric_core::Error::Config(_))
            ));
            assert_eq!(raw_run(&db, set_id, &corrupt_id).await.unwrap(), before);
        }
        assert_eq!(
            raw_run(&db, set_id, &legacy_id).await.unwrap(),
            legacy_before
        );
        assert_eq!(effects(&db, set_id, &slug).await, effects_before);
    })
    .await
    .expect("live legacy readability regression bounded");
}

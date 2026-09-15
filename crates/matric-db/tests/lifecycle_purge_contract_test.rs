use matric_core::{
    ArchiveRepository, PurgeCounts, PurgeOutcome, PurgeRequest, PurgeSelector,
    LIFECYCLE_PURGE_CONTRACT_VERSION,
};
use matric_db::{Database, FilesystemBackend, PgFileStorageRepository};
use tempfile::TempDir;
use uuid::Uuid;

async fn migrate(db: &Database) {
    if let Err(error) = db.migrate().await {
        match error {
            matric_core::Error::Database(sqlx::Error::Database(error)) => panic!(
                "migration failed: code={:?} table={:?} constraint={:?} message={}",
                error.code(),
                error.table(),
                error.constraint(),
                error.message()
            ),
            matric_core::Error::Database(sqlx::Error::Migrate(error)) => {
                panic!("migration failed: {error}")
            }
            error => panic!("migration failed: {error:?}"),
        }
    }
}

fn fixture_counts(case_id: &str) -> PurgeCounts {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/lifecycle-purge/conformance/v1.json"
    ))
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == case_id)
        .unwrap_or_else(|| panic!("missing lifecycle purge fixture case {case_id}"));
    serde_json::from_value(case["expected_counts"].clone()).unwrap()
}

async fn insert_note(db: &Database, schema: &str, note_id: Uuid) {
    db.for_schema(schema)
        .unwrap()
        .execute(move |tx| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO note (id, format, source, created_at_utc, updated_at_utc) \
                     VALUES ($1, 'markdown', 'synthetic-purge-test', now(), now())",
                )
                .bind(note_id)
                .execute(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                Ok(())
            })
        })
        .await
        .unwrap();
}

async fn preview(
    db: &Database,
    schema: &str,
    selector: PurgeSelector,
) -> matric_core::PurgePreview {
    let repository = db.lifecycle_purge.clone();
    db.for_schema(schema)
        .unwrap()
        .execute(move |tx| Box::pin(async move { repository.preview_tx(tx, selector).await }))
        .await
        .unwrap()
}

async fn begin(
    db: &Database,
    schema: &str,
    request: PurgeRequest,
) -> matric_core::Result<matric_core::PurgeStatus> {
    let repository = db.lifecycle_purge.clone();
    db.for_schema(schema)?
        .execute(move |tx| Box::pin(async move { repository.begin_tx(tx, request).await }))
        .await
}

async fn finish_cleanup(
    db: &Database,
    storage: &PgFileStorageRepository,
    schema: &str,
    operation_id: Uuid,
    acknowledge_first_delete: bool,
) -> matric_core::PurgeStatus {
    let repository = db.lifecycle_purge.clone();
    let cleanup = db
        .for_schema(schema)
        .unwrap()
        .execute(move |tx| {
            Box::pin(async move { repository.pending_blob_cleanup_tx(tx, operation_id).await })
        })
        .await
        .unwrap();

    for (index, item) in cleanup.iter().enumerate() {
        if index == 0 && !acknowledge_first_delete {
            // Simulate duplicate resume workers around a crash before durable
            // acknowledgement. Both external effects must be idempotent.
            let (first, second) = tokio::join!(
                storage.delete_lifecycle_purge_blob(item),
                storage.delete_lifecycle_purge_blob(item)
            );
            first.unwrap();
            second.unwrap();
        } else {
            storage.delete_lifecycle_purge_blob(item).await.unwrap();
        }
    }
    let blob_ids = cleanup.iter().map(|item| item.blob_id).collect::<Vec<_>>();
    let repository = db.lifecycle_purge.clone();
    db.for_schema(schema)
        .unwrap()
        .execute(move |tx| {
            Box::pin(async move {
                for blob_id in blob_ids {
                    repository
                        .mark_blob_cleanup_complete_tx(tx, operation_id, blob_id)
                        .await?;
                }
                repository
                    .mark_search_cleanup_complete_tx(tx, operation_id)
                    .await?;
                repository.finalize_tx(tx, operation_id).await?;
                repository
                    .status_tx(tx, operation_id)
                    .await?
                    .ok_or_else(|| {
                        matric_core::Error::NotFound(
                            "synthetic purge operation missing".to_string(),
                        )
                    })
            })
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn purge_is_preview_exact_shared_blob_safe_resumable_and_restore_sticky() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping lifecycle purge contract: DATABASE_URL unavailable");
        return;
    };
    let db = Database::connect(&database_url).await.unwrap();
    migrate(&db).await;
    let archive_name = format!("purge-contract-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("synthetic lifecycle purge contract"))
        .await
        .unwrap();
    let schema = archive.schema_name.clone();
    let temporary = TempDir::new().unwrap();
    let storage =
        PgFileStorageRepository::new(db.pool.clone(), FilesystemBackend::new(temporary.path()), 0);

    let target = Uuid::now_v7();
    let survivor = Uuid::now_v7();
    insert_note(&db, &schema, target).await;
    insert_note(&db, &schema, survivor).await;
    let schema_context = db.for_schema(&schema).unwrap();
    let mut tx = schema_context.begin_tx().await.unwrap();
    let shared_target = storage
        .store_file_tx(
            &mut tx,
            target,
            "target-shared.txt",
            "text/plain",
            b"shared synthetic",
        )
        .await
        .unwrap();
    let shared_survivor = storage
        .store_file_tx(
            &mut tx,
            survivor,
            "survivor-shared.txt",
            "text/plain",
            b"shared synthetic",
        )
        .await
        .unwrap();
    let unique = storage
        .store_file_tx(
            &mut tx,
            target,
            "target-only.txt",
            "text/plain",
            b"target synthetic",
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(shared_target.blob_id, shared_survivor.blob_id);

    let selector = PurgeSelector {
        note_ids: vec![target],
        source: None,
    };
    let snapshot = preview(&db, &schema, selector.clone()).await;
    assert_eq!(snapshot.contract_version, LIFECYCLE_PURGE_CONTRACT_VERSION);
    assert_eq!(snapshot.counts, fixture_counts("shared-blob"));

    let operation_id = Uuid::now_v7();
    let request = PurgeRequest {
        operation_id,
        preview_id: snapshot.preview_id,
    };
    let (first, second) = tokio::join!(
        begin(&db, &schema, request.clone()),
        begin(&db, &schema, request.clone())
    );
    let pending = first.unwrap();
    let concurrent = second.unwrap();
    assert_eq!(pending.outcome, PurgeOutcome::CleanupPending);
    assert_eq!(pending.blob_cleanup_pending, 1);
    assert!(pending.search_cleanup_pending);
    assert!(pending.receipt.is_none());
    assert_eq!(
        concurrent, pending,
        "concurrent execution converges on one operation"
    );
    let replay = begin(&db, &schema, request).await.unwrap();
    assert_eq!(
        replay, pending,
        "exact operation replay is side-effect free"
    );

    let (target_exists, survivor_exists, shared_exists, unique_exists): (bool, bool, bool, bool) =
        db.for_schema(&schema)
            .unwrap()
            .query(move |tx| {
                Box::pin(async move {
                    Ok((
                        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                            .bind(target)
                            .fetch_one(&mut **tx)
                            .await
                            .map_err(matric_core::Error::Database)?,
                        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                            .bind(survivor)
                            .fetch_one(&mut **tx)
                            .await
                            .map_err(matric_core::Error::Database)?,
                        sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM attachment_blob WHERE id=$1)",
                        )
                        .bind(shared_target.blob_id)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?,
                        sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM attachment_blob WHERE id=$1)",
                        )
                        .bind(unique.blob_id)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?,
                    ))
                })
            })
            .await
            .unwrap();
    assert!(!target_exists);
    assert!(survivor_exists);
    assert!(shared_exists);
    assert!(!unique_exists);

    let completed = finish_cleanup(&db, &storage, &schema, operation_id, false).await;
    assert_eq!(completed.outcome, PurgeOutcome::Completed);
    assert_eq!(completed.blob_cleanup_pending, 0);
    assert!(!completed.search_cleanup_pending);
    let receipt_json = serde_json::to_string(&completed.receipt).unwrap();
    for forbidden in [
        target.to_string(),
        unique.blob_id.to_string(),
        "target-only.txt".to_string(),
        "blobs/".to_string(),
        "sha256:".to_string(),
        "target synthetic".to_string(),
    ] {
        assert!(!receipt_json.contains(&forbidden));
    }
    let receipt_count: i64 = db
        .for_schema(&schema)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                sqlx::query_scalar("SELECT COUNT(*) FROM deletion_receipt WHERE operation_id=$1")
                    .bind(operation_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)
            })
        })
        .await
        .unwrap();
    assert_eq!(receipt_count, 1);

    // A later restore of the same identity is erased in the restore transaction
    // and requires external cleanup before the replacement receipt is terminal.
    insert_note(&db, &schema, target).await;
    let schema_context = db.for_schema(&schema).unwrap();
    let mut tx = schema_context.begin_tx().await.unwrap();
    storage
        .store_file_tx(
            &mut tx,
            target,
            "restored.txt",
            "text/plain",
            b"restored synthetic",
        )
        .await
        .unwrap();
    let (operations, notes) = db
        .lifecycle_purge
        .reerase_restored_tx(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(operations, vec![operation_id]);
    assert_eq!(notes, vec![target]);
    let schema_context = db.for_schema(&schema).unwrap();
    let mut retry_tx = schema_context.begin_tx().await.unwrap();
    let (retry_operations, retry_notes) = db
        .lifecycle_purge
        .reerase_restored_tx(&mut retry_tx)
        .await
        .unwrap();
    retry_tx.commit().await.unwrap();
    assert_eq!(
        retry_operations,
        vec![operation_id],
        "retry retains the cleanup operation after a post-commit crash"
    );
    assert!(retry_notes.is_empty());
    let restored_exists: bool = db
        .for_schema(&schema)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                    .bind(target)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)
            })
        })
        .await
        .unwrap();
    assert!(!restored_exists);
    assert_eq!(
        finish_cleanup(&db, &storage, &schema, operation_id, true)
            .await
            .outcome,
        PurgeOutcome::Completed
    );

    // A changed graph invalidates its preview without deleting anything.
    let stale = preview(
        &db,
        &schema,
        PurgeSelector {
            note_ids: vec![survivor],
            source: None,
        },
    )
    .await;
    let schema_context = db.for_schema(&schema).unwrap();
    let mut tx = schema_context.begin_tx().await.unwrap();
    storage
        .store_file_tx(
            &mut tx,
            survivor,
            "late.txt",
            "text/plain",
            b"late synthetic",
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let stale_error = begin(
        &db,
        &schema,
        PurgeRequest {
            operation_id: Uuid::now_v7(),
            preview_id: stale.preview_id,
        },
    )
    .await;
    assert!(stale_error.is_err());
    let stale_preview_id = stale.preview_id;
    db.for_schema(&schema)
        .unwrap()
        .execute(move |tx| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE lifecycle_purge_preview \
                     SET expires_at = now() - interval '1 second' WHERE id=$1",
                )
                .bind(stale_preview_id)
                .execute(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                Ok(())
            })
        })
        .await
        .unwrap();
    preview(&db, &schema, selector).await;
    let expired_exists: bool = db
        .for_schema(&schema)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM lifecycle_purge_preview WHERE id=$1)",
                )
                .bind(stale_preview_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)
            })
        })
        .await
        .unwrap();
    assert!(!expired_exists);

    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .unwrap();
}

#[tokio::test]
async fn source_selector_intersects_ids_and_whole_source_removes_journals() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping lifecycle purge source contract: DATABASE_URL unavailable");
        return;
    };
    let db = Database::connect(&database_url).await.unwrap();
    migrate(&db).await;
    let archive_name = format!("purge-source-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("synthetic source purge contract"))
        .await
        .unwrap();
    let schema = archive.schema_name.clone();

    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let outside = Uuid::now_v7();
    insert_note(&db, &schema, first).await;
    insert_note(&db, &schema, second).await;
    insert_note(&db, &schema, outside).await;
    let namespace = format!("synthetic.purge.{}", Uuid::new_v4());
    let namespace_for_insert = namespace.clone();
    db.for_schema(&schema)
        .unwrap()
        .execute(move |tx| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO source_import_run \
                     (source_namespace, import_run_id, source_schema_version) \
                     VALUES ($1, 'run-1', '1')",
                )
                .bind(&namespace_for_insert)
                .execute(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                sqlx::query(
                    "INSERT INTO source_import_batch \
                     (source_namespace, import_run_id, batch_id, request_digest, receipt) \
                     VALUES ($1, 'run-1', 'batch-1', $2, '{}'::jsonb)",
                )
                .bind(&namespace_for_insert)
                .bind(format!("sha256:{}", "0".repeat(64)))
                .execute(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                for (note_id, external_id) in [(first, "first"), (second, "second")] {
                    sqlx::query(
                        "INSERT INTO source_identity \
                         (source_namespace, external_id, note_id, source_schema_version, \
                          content_digest, import_run_id) \
                         VALUES ($1, $2, $3, '1', $4, 'run-1')",
                    )
                    .bind(&namespace_for_insert)
                    .bind(external_id)
                    .bind(note_id)
                    .bind(format!("sha256:{}", "1".repeat(64)))
                    .execute(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?;
                }
                Ok(())
            })
        })
        .await
        .unwrap();

    let intersected = preview(
        &db,
        &schema,
        PurgeSelector {
            note_ids: vec![first, outside],
            source: Some(matric_core::PurgeSourceSelector {
                namespace: namespace.clone(),
                external_id: None,
            }),
        },
    )
    .await;
    assert_eq!(intersected.counts, fixture_counts("source-intersection"));
    let first_operation = Uuid::now_v7();
    let first_status = begin(
        &db,
        &schema,
        PurgeRequest {
            operation_id: first_operation,
            preview_id: intersected.preview_id,
        },
    )
    .await
    .unwrap();
    assert_eq!(first_status.counts, intersected.counts);
    let first_temporary = TempDir::new().unwrap();
    let first_storage = PgFileStorageRepository::new(
        db.pool.clone(),
        FilesystemBackend::new(first_temporary.path()),
        0,
    );
    assert_eq!(
        finish_cleanup(&db, &first_storage, &schema, first_operation, true,)
            .await
            .counts,
        intersected.counts
    );

    let namespace_for_check = namespace.clone();
    let (first_exists, second_exists, outside_exists, journals): (bool, bool, bool, i64) = db
        .for_schema(&schema)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                Ok((
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                        .bind(first)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?,
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                        .bind(second)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?,
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id=$1)")
                        .bind(outside)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?,
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM source_import_run WHERE source_namespace=$1",
                    )
                    .bind(namespace_for_check)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?,
                ))
            })
        })
        .await
        .unwrap();
    assert!(!first_exists);
    assert!(second_exists);
    assert!(outside_exists);
    assert_eq!(journals, 1, "a partial source purge retains its journal");

    let whole_source = preview(
        &db,
        &schema,
        PurgeSelector {
            note_ids: Vec::new(),
            source: Some(matric_core::PurgeSourceSelector {
                namespace: namespace.clone(),
                external_id: None,
            }),
        },
    )
    .await;
    assert_eq!(whole_source.counts, fixture_counts("source-intersection"));
    let whole_operation = Uuid::now_v7();
    begin(
        &db,
        &schema,
        PurgeRequest {
            operation_id: whole_operation,
            preview_id: whole_source.preview_id,
        },
    )
    .await
    .unwrap();
    let temporary = TempDir::new().unwrap();
    let storage =
        PgFileStorageRepository::new(db.pool.clone(), FilesystemBackend::new(temporary.path()), 0);
    let completed = finish_cleanup(&db, &storage, &schema, whole_operation, true).await;
    assert_eq!(completed.counts, whole_source.counts);
    let remaining: (i64, i64, i64) = db
        .for_schema(&schema)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                Ok((
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM source_identity WHERE source_namespace=$1",
                    )
                    .bind(&namespace)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?,
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM source_import_run WHERE source_namespace=$1",
                    )
                    .bind(&namespace)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?,
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM source_import_batch WHERE source_namespace=$1",
                    )
                    .bind(&namespace)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(matric_core::Error::Database)?,
                ))
            })
        })
        .await
        .unwrap();
    assert_eq!(remaining, (0, 0, 0));

    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .unwrap();
}

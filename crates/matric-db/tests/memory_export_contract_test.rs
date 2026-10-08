//! Executable acceptance tests for point-in-time memory export (#1157).
//!
//! Each test works in its own freshly created archive schema so concurrent
//! tests never observe each other's rows.

use std::time::Duration;

use matric_core::{
    apply_export_records, export_state_records, ArchiveRepository, ExportEntityType, ExportOp,
    ExportState, HighWaterMark, MemoryExport, MemoryExportMode, MemoryExportProducer,
    MemoryExportRequest,
};
use matric_db::{Database, MemoryExportContext, PgMemoryExportRepository};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection};
use std::str::FromStr;
use uuid::Uuid;

const LOCAL_TENANT: &str = "00000000-0000-0000-0000-000000000000";

struct Fixture {
    db: Database,
    schema: String,
    context: MemoryExportContext,
}

async fn fixture(label: &str) -> Option<Fixture> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping memory export contract: DATABASE_URL unavailable");
        return None;
    };
    let db = Database::connect(&database_url).await.unwrap();
    db.migrate().await.expect("migrations apply");
    let name = format!("export-{label}-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&name, Some("synthetic memory export contract"))
        .await
        .unwrap();
    let context = MemoryExportContext {
        memory_name: Some(archive.name.clone()),
        schema: archive.schema_name.clone(),
        producer: MemoryExportProducer {
            name: "fortemi".to_string(),
            version: "test".to_string(),
            git_sha: "unknown".to_string(),
        },
        schema_version: Database::latest_migration_version().to_string(),
    };
    Some(Fixture {
        db,
        schema: archive.schema_name,
        context,
    })
}

impl Fixture {
    async fn exec(&self, sql: &str) {
        let mut transaction = self.db.pool.begin().await.unwrap();
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", self.schema))
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::raw_sql(sql).execute(&mut *transaction).await.unwrap();
        transaction.commit().await.unwrap();
    }

    async fn export(&self, request: &MemoryExportRequest) -> MemoryExport {
        PgMemoryExportRepository::new()
            .export_snapshot(&self.db.pool, &self.context, request)
            .await
            .unwrap()
    }

    async fn full(&self) -> MemoryExport {
        self.export(&MemoryExportRequest::default()).await
    }

    async fn incremental(&self, since: HighWaterMark) -> MemoryExport {
        self.export(&MemoryExportRequest {
            mode: MemoryExportMode::Incremental,
            since: Some(since),
            ..Default::default()
        })
        .await
    }

    /// A connection with its own open transaction targeting the archive.
    async fn writer(&self) -> PgConnection {
        let url = std::env::var("DATABASE_URL").unwrap();
        let mut connection = PgConnection::connect(&url).await.unwrap();
        sqlx::raw_sql(&format!(
            "BEGIN; SELECT set_config('app.current_tenant', '{LOCAL_TENANT}', true); \
             SET LOCAL search_path TO {}, public",
            self.schema
        ))
        .execute(&mut connection)
        .await
        .unwrap();
        connection
    }

    async fn seed(&self) {
        self.exec(
            "INSERT INTO collection (id, name, description, created_at_utc) VALUES \
               ('01900000-0000-7000-8000-0000000000c1', 'research', 'synthetic', '2026-01-01T00:00:00Z');
             INSERT INTO tag (name, created_at_utc) VALUES \
               ('topic/a', '2026-01-01T00:00:00Z'), ('topic/b', '2026-01-01T00:00:00Z');
             INSERT INTO note (id, collection_id, format, source, title, created_at_utc, updated_at_utc) VALUES \
               ('01900000-0000-7000-8000-000000000001', '01900000-0000-7000-8000-0000000000c1', 'markdown', 'synthetic', 'one', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
               ('01900000-0000-7000-8000-000000000002', NULL, 'markdown', 'synthetic', 'two', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO note_original (note_id, content, hash) VALUES \
               ('01900000-0000-7000-8000-000000000001', 'first body', 'h1'),
               ('01900000-0000-7000-8000-000000000002', 'second body', 'h2');
             INSERT INTO note_revised_current (note_id, content) VALUES \
               ('01900000-0000-7000-8000-000000000001', 'first revised');
             INSERT INTO note_tag (note_id, tag_name) VALUES \
               ('01900000-0000-7000-8000-000000000001', 'topic/a'),
               ('01900000-0000-7000-8000-000000000002', 'topic/b');
             INSERT INTO link (id, from_note_id, to_note_id, kind, score, created_at_utc) VALUES \
               ('01900000-0000-7000-8000-0000000000a1', '01900000-0000-7000-8000-000000000001', '01900000-0000-7000-8000-000000000002', 'semantic', 0.75, '2026-01-01T00:00:00Z');",
        )
        .await;
    }
}

fn body_bytes(export: &MemoryExport) -> Vec<u8> {
    let mut manifest = export.manifest.clone();
    manifest.generated_at = chrono::DateTime::UNIX_EPOCH;
    serde_json::to_vec(&MemoryExport {
        manifest,
        records: export.records.clone(),
    })
    .unwrap()
}

fn canonical_lines(records: &[matric_core::ExportRecord]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.canonical_line())
        .collect()
}

fn applied(base: &MemoryExport, increment: &MemoryExport) -> Vec<String> {
    let mut state = ExportState::new();
    apply_export_records(&mut state, &base.records);
    apply_export_records(&mut state, &increment.records);
    canonical_lines(&export_state_records(&state))
}

/// Two exports taken while the memory is unchanged; retried only to step past
/// unrelated cluster transactions that can transiently lower the mark.
async fn two_quiescent_exports(fixture: &Fixture) -> (MemoryExport, MemoryExport) {
    for _ in 0..50 {
        let first = fixture.full().await;
        // A read that bumps access counters must not change the export.
        fixture
            .exec("UPDATE note SET access_count = access_count + 1, last_accessed_at = now()")
            .await;
        let second = fixture.full().await;
        if first.manifest.high_water_mark == second.manifest.high_water_mark {
            return (first, second);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("memory never reached a quiescent high-water mark");
}

#[tokio::test]
async fn exports_at_the_same_high_water_mark_are_byte_identical() {
    let Some(fixture) = fixture("determinism").await else {
        return;
    };
    fixture.seed().await;

    let (first, second) = two_quiescent_exports(&fixture).await;
    assert_eq!(body_bytes(&first), body_bytes(&second));
    assert_eq!(first.manifest.record_count, 9);
    assert_eq!(first.manifest.entities[&ExportEntityType::Note].upserts, 2);
    assert_eq!(
        first.manifest.entities[&ExportEntityType::NoteTag].upserts,
        2
    );
    assert!(first.manifest.content_sha256.starts_with("sha256:"));
    assert_eq!(first.manifest.memory.schema, fixture.schema);
    assert_eq!(first.manifest.memory.tenant_id.to_string(), LOCAL_TENANT);
    assert!(first
        .records
        .iter()
        .all(|record| record.op == ExportOp::Upsert));
    let note = first
        .records
        .iter()
        .find(|record| record.entity == ExportEntityType::Note)
        .unwrap();
    let note = note.record.as_ref().unwrap();
    assert!(note.get("access_count").is_none());
    assert_eq!(note["created_at_utc"], "2026-01-01T00:00:00+00:00");

    // Field selection narrows records but keeps the stable key.
    let narrowed: MemoryExportRequest = serde_json::from_value(serde_json::json!({
        "entity_types": ["note"],
        "fields": {"note": ["title"]}
    }))
    .unwrap();
    let narrowed = fixture.export(&narrowed).await;
    assert_eq!(narrowed.records.len(), 2);
    assert_eq!(
        narrowed.records[0].record.as_ref().unwrap(),
        &serde_json::json!({"id": "01900000-0000-7000-8000-000000000001", "title": "one"})
    );
}

#[tokio::test]
async fn incremental_applied_to_previous_full_equals_full_at_new_mark() {
    let Some(fixture) = fixture("incremental").await else {
        return;
    };
    fixture.seed().await;
    let previous = fixture.full().await;

    fixture
        .exec(
            "UPDATE note SET title = 'one (edited)' WHERE id = '01900000-0000-7000-8000-000000000001';
             UPDATE note_original SET content = 'first body v2', hash = 'h1b' WHERE note_id = '01900000-0000-7000-8000-000000000001';
             INSERT INTO note (id, format, source, title, created_at_utc, updated_at_utc) VALUES \
               ('01900000-0000-7000-8000-000000000003', 'markdown', 'synthetic', 'three', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z');
             INSERT INTO note_tag (note_id, tag_name) VALUES ('01900000-0000-7000-8000-000000000003', 'topic/a');
             DELETE FROM note_tag WHERE note_id = '01900000-0000-7000-8000-000000000001';
             DELETE FROM note WHERE id = '01900000-0000-7000-8000-000000000002';
             UPDATE collection SET description = 'renamed' WHERE name = 'research';",
        )
        .await;

    let increment = fixture.incremental(previous.manifest.high_water_mark).await;
    let current = fixture.full().await;

    assert_eq!(increment.manifest.mode, MemoryExportMode::Incremental);
    assert_eq!(
        increment.manifest.window.from,
        Some(previous.manifest.high_water_mark)
    );
    assert!(increment.manifest.high_water_mark >= previous.manifest.high_water_mark);
    assert_eq!(
        applied(&previous, &increment),
        canonical_lines(&current.records)
    );

    // Hard deletes surface as tombstones, including cascaded children.
    let deletes: Vec<_> = increment
        .records
        .iter()
        .filter(|record| record.op == ExportOp::Delete)
        .map(|record| record.entity)
        .collect();
    for entity in [
        ExportEntityType::Note,
        ExportEntityType::NoteOriginal,
        ExportEntityType::NoteTag,
        ExportEntityType::Link,
    ] {
        assert!(deletes.contains(&entity), "missing {entity} tombstone");
    }
    // An unchanged record may be re-sent only when an older transaction held
    // the mark back (see ADR-109); a re-sent copy is always identical.
    let unchanged: Vec<_> = increment
        .records
        .iter()
        .filter(|record| record.entity == ExportEntityType::NoteRevisedCurrent)
        .map(|record| record.canonical_line())
        .collect();
    let original: Vec<_> = previous
        .records
        .iter()
        .filter(|record| record.entity == ExportEntityType::NoteRevisedCurrent)
        .map(|record| record.canonical_line())
        .collect();
    assert!(unchanged.is_empty() || unchanged == original);

    // Re-inserting a deleted key replaces its tombstone with the live row.
    fixture
        .exec(
            "INSERT INTO note_tag (note_id, tag_name) VALUES ('01900000-0000-7000-8000-000000000001', 'topic/a');",
        )
        .await;
    let second_increment = fixture.incremental(current.manifest.high_water_mark).await;
    let latest = fixture.full().await;
    assert_eq!(
        applied(&current, &second_increment),
        canonical_lines(&latest.records)
    );
    let since_start = fixture.incremental(previous.manifest.high_water_mark).await;
    assert_eq!(
        applied(&previous, &since_start),
        canonical_lines(&latest.records)
    );
    assert!(!since_start.records.iter().any(|record| {
        record.entity == ExportEntityType::NoteTag
            && record.op == ExportOp::Delete
            && record.key[0] == "01900000-0000-7000-8000-000000000001"
    }));

    // A cursor from the future is rejected rather than silently skipping data.
    let future = HighWaterMark(latest.manifest.high_water_mark.0 + 1_000_000);
    let error = PgMemoryExportRepository::new()
        .export_snapshot(
            &fixture.db.pool,
            &fixture.context,
            &MemoryExportRequest {
                mode: MemoryExportMode::Incremental,
                since: Some(future),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, matric_core::Error::InvalidInput(_)));
}

#[tokio::test]
async fn concurrent_writes_never_tear_an_export_and_are_picked_up_later() {
    let Some(fixture) = fixture("concurrency").await else {
        return;
    };
    fixture.seed().await;
    let baseline = fixture.full().await;

    // (1) An export transaction whose snapshot predates concurrent commits.
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut reader = PgConnection::connect(&url).await.unwrap();
    sqlx::raw_sql(&format!(
        "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; \
         SELECT set_config('app.current_tenant', '{LOCAL_TENANT}', true); \
         SET LOCAL search_path TO {}, public; SELECT pg_current_snapshot();",
        fixture.schema
    ))
    .execute(&mut reader)
    .await
    .unwrap();
    fixture
        .exec(
            "INSERT INTO note (id, format, source, title, created_at_utc, updated_at_utc) VALUES \
               ('01900000-0000-7000-8000-000000000004', 'markdown', 'synthetic', 'four', now(), now());
             UPDATE note SET title = 'one (concurrent)' WHERE id = '01900000-0000-7000-8000-000000000001';
             DELETE FROM note_tag WHERE note_id = '01900000-0000-7000-8000-000000000002';",
        )
        .await;
    let pinned = PgMemoryExportRepository::new()
        .export_tx(
            &mut reader,
            &fixture.context,
            &MemoryExportRequest::default(),
        )
        .await
        .unwrap();
    sqlx::raw_sql("COMMIT").execute(&mut reader).await.unwrap();
    assert_eq!(
        canonical_lines(&pinned.records),
        canonical_lines(&baseline.records)
    );
    let after_pinned = fixture.incremental(pinned.manifest.high_water_mark).await;
    let now = fixture.full().await;
    assert_eq!(
        applied(&pinned, &after_pinned),
        canonical_lines(&now.records)
    );

    // (2) The torn-read hazard: an older writer commits after a newer one.
    // The older writer touches only existing rows so it holds no lock the
    // newer writer's note insert (embedding-set triggers) would wait on.
    let mut older = fixture.writer().await;
    sqlx::raw_sql(
        "UPDATE note SET title = 'older writer' WHERE id = '01900000-0000-7000-8000-000000000001';",
    )
    .execute(&mut older)
    .await
    .unwrap();
    let mut newer = fixture.writer().await;
    sqlx::raw_sql(
        "INSERT INTO note (id, format, source, title, created_at_utc, updated_at_utc) VALUES \
           ('01900000-0000-7000-8000-000000000006', 'markdown', 'synthetic', 'newer writer', now(), now());
         DELETE FROM link;
         COMMIT;",
    )
    .execute(&mut newer)
    .await
    .unwrap();

    let during = fixture.full().await;
    let titles: Vec<_> = during
        .records
        .iter()
        .filter_map(|record| record.record.as_ref()?.get("title")?.as_str())
        .collect();
    assert!(titles.contains(&"newer writer"));
    assert!(!titles.contains(&"older writer"));

    sqlx::raw_sql("COMMIT").execute(&mut older).await.unwrap();
    let increment = fixture.incremental(during.manifest.high_water_mark).await;
    let settled = fixture.full().await;
    assert!(increment.records.iter().any(|record| {
        record.record.as_ref().and_then(|value| value.get("title"))
            == Some(&serde_json::json!("older writer"))
    }));
    assert_eq!(
        applied(&during, &increment),
        canonical_lines(&settled.records)
    );
}

#[tokio::test]
async fn export_runs_under_a_read_only_role_without_mutating_state() {
    let Some(fixture) = fixture("readonly").await else {
        return;
    };
    fixture.seed().await;
    let expected = fixture.full().await;

    const ROLE: &str = "fortemi_memory_export_readonly_test";
    const PASSWORD: &str = "fortemi-memory-export-readonly-test-only";
    let admin = &fixture.db.pool;
    sqlx::raw_sql(&format!(
        "DO $$ BEGIN
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{ROLE}') THEN
             CREATE ROLE {ROLE} LOGIN PASSWORD '{PASSWORD}'
               NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;
           END IF;
         END $$;
         GRANT USAGE ON SCHEMA public, {schema} TO {ROLE};
         GRANT SELECT ON ALL TABLES IN SCHEMA public TO {ROLE};
         GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO {ROLE};",
        schema = fixture.schema
    ))
    .execute(admin)
    .await
    .unwrap();

    let url = std::env::var("DATABASE_URL").unwrap();
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .username(ROLE)
        .password(PASSWORD);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
                    .bind(LOCAL_TENANT)
                    .execute(connection)
                    .await
                    .map(|_| ())
            })
        })
        .connect_with(options)
        .await
        .unwrap();

    let read_only = PgMemoryExportRepository::new()
        .export_snapshot(&pool, &fixture.context, &MemoryExportRequest::default())
        .await
        .unwrap();
    assert_eq!(body_bytes(&read_only), body_bytes(&expected));

    // The export marks its transaction read-only: a write afterwards fails.
    let mut transaction = fixture.db.pool.begin().await.unwrap();
    sqlx::query(&format!(
        "SET LOCAL search_path TO {}, public",
        fixture.schema
    ))
    .execute(&mut *transaction)
    .await
    .unwrap();
    PgMemoryExportRepository::new()
        .export_tx(
            &mut transaction,
            &fixture.context,
            &MemoryExportRequest::default(),
        )
        .await
        .unwrap();
    let write = sqlx::query("UPDATE note SET title = 'mutated'")
        .execute(&mut *transaction)
        .await
        .unwrap_err();
    assert!(write.to_string().contains("read-only transaction"));
    pool.close().await;
}

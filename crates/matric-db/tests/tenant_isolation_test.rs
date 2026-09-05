use std::str::FromStr;

use matric_core::audit::{AuditEvent, AuditOutcome, AuditSink};
use matric_db::{
    assert_hosted_runtime_role, create_pool, inspect_tenant_catalog, Database, PgLinkRepository,
    PgNoteRepository, PgUserSecretRepository, PostgresAuditSink, TenantScopedConn,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::Executor;
use uuid::Uuid;

const TEST_RUNTIME_ROLE: &str = "fortemi_tenant_isolation_test";
const TEST_RUNTIME_PASSWORD: &str = "fortemi-tenant-isolation-test-only";
static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn setup() -> Option<(sqlx::PgPool, sqlx::PgPool)> {
    let _guard = SETUP_LOCK.lock().await;
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };
    let admin = create_pool(&database_url).await.unwrap();
    let schema_is_provisioned =
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('public.tenant_registry') IS NOT NULL")
            .fetch_one(&admin)
            .await
            .unwrap();
    if !schema_is_provisioned {
        Database::new(admin.clone()).migrate().await.unwrap();
    }

    sqlx::query(&format!(
        r#"
        DO $$
        BEGIN
          IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{TEST_RUNTIME_ROLE}') THEN
            CREATE ROLE {TEST_RUNTIME_ROLE}
              LOGIN PASSWORD '{TEST_RUNTIME_PASSWORD}'
              NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;
          ELSE
            ALTER ROLE {TEST_RUNTIME_ROLE}
              WITH LOGIN PASSWORD '{TEST_RUNTIME_PASSWORD}'
              NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;
          END IF;
        END
        $$;
        "#
    ))
    .execute(&admin)
    .await
    .unwrap();
    admin
        .execute(format!("GRANT USAGE ON SCHEMA public TO {TEST_RUNTIME_ROLE}").as_str())
        .await
        .unwrap();
    admin
        .execute(
            format!(
                "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {TEST_RUNTIME_ROLE}"
            )
            .as_str(),
        )
        .await
        .unwrap();
    admin
        .execute(
            format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {TEST_RUNTIME_ROLE}")
                .as_str(),
        )
        .await
        .unwrap();

    let options = PgConnectOptions::from_str(&database_url)
        .unwrap()
        .username(TEST_RUNTIME_ROLE)
        .password(TEST_RUNTIME_PASSWORD);
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    Some((admin, runtime))
}

async fn register_tenant(admin: &sqlx::PgPool, tenant_id: Uuid, slug: &str) {
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(slug)
    .execute(admin)
    .await
    .unwrap();
}

/// Executes the shipped operator recipe, not a reimplementation of its SQL.
#[tokio::test]
async fn operator_read_only_psql_recipe_enforces_scope() {
    use matric_core::ArchiveRepository;
    let Some((admin, runtime)) = setup().await else {
        assert_ne!(
            std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
            Ok("1")
        );
        eprintln!("skipping psql recipe test: DATABASE_URL unavailable");
        return;
    };
    let db = Database::new(admin.clone());
    let archive = db
        .archives
        .create_archive_schema(&format!("psql-{}", Uuid::new_v4().simple()), None)
        .await
        .unwrap();
    let schema = &archive.schema_name;
    admin
        .execute(format!("GRANT USAGE ON SCHEMA {schema} TO {TEST_RUNTIME_ROLE}").as_str())
        .await
        .unwrap();
    admin
        .execute(
            format!("GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO {TEST_RUNTIME_ROLE}")
                .as_str(),
        )
        .await
        .unwrap();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_b, &format!("psql-{tenant_b}")).await;
    for (tenant, deleted, archived) in [
        (Uuid::nil(), false, false),
        (Uuid::nil(), false, true),
        (Uuid::nil(), true, false),
        (tenant_b, false, false),
    ] {
        sqlx::query(&format!(
            "INSERT INTO {schema}.note (id, tenant_id, format, source, created_at_utc, updated_at_utc, deleted_at, archived)
             VALUES ($1, $2, 'markdown', 'psql-recipe-test', now(), now(), CASE WHEN $3 THEN now() END, $4)"
        )).bind(Uuid::new_v4()).bind(tenant).bind(deleted).bind(archived).execute(&admin).await.unwrap();
    }
    let options = PgConnectOptions::from_str(&std::env::var("DATABASE_URL").unwrap()).unwrap();
    let recipe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/sql/read-only-notes.sql");
    let command = || {
        let mut cmd = std::process::Command::new("psql");
        cmd.args([
            "-X",
            "-A",
            "-t",
            "-v",
            "ON_ERROR_STOP=1",
            "-h",
            options.get_host(),
            "-p",
            &options.get_port().to_string(),
            "-U",
            TEST_RUNTIME_ROLE,
            "-d",
            options.get_database().unwrap_or("matric"),
        ]);
        cmd.env("PGPASSWORD", TEST_RUNTIME_PASSWORD);
        cmd
    };
    let scoped = command()
        .args([
            "-v",
            &format!("tenant_id={}", Uuid::nil()),
            "-v",
            &format!("archive_name={}", archive.name),
            "-f",
        ])
        .arg(&recipe)
        .args([
            "-c",
            "SELECT nullif(current_setting('app.current_tenant', true), '') IS NULL AS scope_reset",
        ])
        .output()
        .unwrap();
    assert!(
        scoped.status.success(),
        "{}",
        String::from_utf8_lossy(&scoped.stderr)
    );
    let output = String::from_utf8(scoped.stdout).unwrap();
    assert_eq!(
        output.lines().last(),
        Some("t"),
        "SET LOCAL must reset after COMMIT"
    );
    assert!(
        output.contains(&format!("{TEST_RUNTIME_ROLE}|{}|{schema}|on", Uuid::nil())),
        "{output}"
    );
    assert!(
        output.lines().any(|line| line == "2"),
        "must count live and archived, exclude deleted and other tenant: {output}"
    );
    for tenant in [tenant_b.to_string(), "not-a-uuid".to_string()] {
        let output = command()
            .args([
                "-v",
                &format!("tenant_id={tenant}"),
                "-v",
                &format!("archive_name={}", archive.name),
                "-f",
            ])
            .arg(&recipe)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "invalid/invisible scope must fail"
        );
    }
    let missing_archive = command()
        .args([
            "-v",
            &format!("tenant_id={}", Uuid::nil()),
            "-v",
            "archive_name=missing-archive",
            "-f",
        ])
        .arg(&recipe)
        .output()
        .unwrap();
    assert!(!missing_archive.status.success());
    // A separate psql connection must bind context again.
    let missing = command()
        .args(["-c", &format!("SELECT count(*) FROM {schema}.note")])
        .output()
        .unwrap();
    assert!(
        !missing.status.success(),
        "unscoped read must fail on populated guarded table"
    );
    let read_only = command().args(["-c", &format!(
        "BEGIN READ ONLY; SET LOCAL app.current_tenant = '{}'; DELETE FROM public.note; COMMIT", Uuid::nil()
    )]).output().unwrap();
    assert!(!read_only.status.success());
    assert!(String::from_utf8_lossy(&read_only.stderr).contains("read-only"));
    // The checked recipe refuses privileged roles too, even on forced-RLS tables.
    let privileged = command()
        .args([
            "-U",
            options.get_username(),
            "-v",
            &format!("tenant_id={}", Uuid::nil()),
            "-v",
            &format!("archive_name={}", archive.name),
            "-f",
        ])
        .arg(&recipe)
        .output()
        .unwrap();
    assert!(
        !privileged.status.success(),
        "recipe must refuse a privileged role"
    );
    admin
        .execute(format!("DROP SCHEMA {schema} CASCADE").as_str())
        .await
        .unwrap();
    sqlx::query("DELETE FROM archive_registry WHERE schema_name = $1")
        .bind(schema)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenant_registry WHERE id = $1")
        .bind(tenant_b)
        .execute(&admin)
        .await
        .unwrap();
    runtime.close().await;
}

async fn insert_note(scope: &mut TenantScopedConn<'_>, note_id: Uuid, title: &str) {
    sqlx::query(
        r#"
        INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, title)
        VALUES ($1, 'markdown', 'tenant-isolation-test', now(), now(), $2)
        "#,
    )
    .bind(note_id)
    .bind(title)
    .execute(scope.executor())
    .await
    .unwrap();
}

#[tokio::test]
async fn forced_rls_inventory_and_runtime_role_are_clean() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let report = inspect_tenant_catalog(&runtime).await.unwrap();
    assert!(report.is_clean(), "tenant catalog drift: {report:?}");
    assert_hosted_runtime_role(&runtime).await.unwrap();
    assert!(assert_hosted_runtime_role(&admin).await.is_err());
}

#[tokio::test]
async fn transaction_scope_isolates_reused_connection_and_missing_scope_fails_closed() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_a, &format!("test-{tenant_a}")).await;
    register_tenant(&admin, tenant_b, &format!("test-{tenant_b}")).await;

    let note_a = Uuid::new_v4();
    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    insert_note(&mut scope_a, note_a, "tenant-a-only").await;
    scope_a.commit().await.unwrap();

    // The pool has one physical connection, forcing the tenant-B request to
    // reuse the connection previously scoped to tenant A.
    let mut scope_b = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM note WHERE id = $1")
        .bind(note_a)
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(visible, 0);
    scope_b.rollback().await.unwrap();

    let missing_scope = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note")
        .fetch_one(&runtime)
        .await;
    assert!(missing_scope.is_err(), "an unscoped tenant query must fail");
}

#[tokio::test]
async fn manual_link_visibility_rejects_cross_tenant_and_archived_targets_without_mutation() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_a, &format!("test-{tenant_a}")).await;
    register_tenant(&admin, tenant_b, &format!("test-{tenant_b}")).await;

    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    insert_note(&mut scope_a, source, "tenant-a-source").await;
    scope_a.commit().await.unwrap();
    let mut scope_b = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    insert_note(&mut scope_b, target, "tenant-b-target").await;
    scope_b.commit().await.unwrap();

    let notes = PgNoteRepository::new(runtime.clone());
    let links = PgLinkRepository::new(runtime.clone());
    let mut request_scope = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    assert!(notes
        .active_exists_tx(request_scope.executor(), source)
        .await
        .unwrap());
    assert!(!notes
        .active_exists_tx(request_scope.executor(), target)
        .await
        .unwrap());
    let source_visible = notes
        .active_exists_tx(request_scope.executor(), source)
        .await
        .unwrap();
    let target_visible = notes
        .active_exists_tx(request_scope.executor(), target)
        .await
        .unwrap();
    if source_visible && target_visible {
        links
            .create_idempotent_tx(
                request_scope.executor(),
                source,
                target,
                "explicit",
                1.0,
                None,
            )
            .await
            .unwrap();
    }
    request_scope.rollback().await.unwrap();
    let cross_tenant_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM link WHERE from_note_id = $1 AND to_note_id = $2")
            .bind(source)
            .bind(target)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(cross_tenant_count, 0);

    let mut archive_scope = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    sqlx::query("UPDATE note SET archived = TRUE WHERE id = $1")
        .bind(target)
        .execute(archive_scope.executor())
        .await
        .unwrap();
    assert!(!notes
        .active_exists_tx(archive_scope.executor(), target)
        .await
        .unwrap());
    archive_scope.commit().await.unwrap();

    sqlx::query("DELETE FROM note WHERE id = ANY($1)")
        .bind([source, target])
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenant_registry WHERE id = ANY($1)")
        .bind([tenant_a, tenant_b])
        .execute(&admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn tenant_qualified_foreign_key_rejects_cross_tenant_association() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_a, &format!("test-{tenant_a}")).await;
    register_tenant(&admin, tenant_b, &format!("test-{tenant_b}")).await;

    let collection_id = Uuid::new_v4();
    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    sqlx::query("INSERT INTO collection (id, name, created_at_utc) VALUES ($1, $2, now())")
        .bind(collection_id)
        .bind(format!("collection-{collection_id}"))
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    let result = sqlx::query(
        r#"
        INSERT INTO note (
            id, collection_id, format, source, created_at_utc, updated_at_utc, title
        ) VALUES ($1, $2, 'markdown', 'tenant-isolation-test', now(), now(), 'cross-tenant')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(collection_id)
    .execute(scope_b.executor())
    .await;
    assert!(result.is_err(), "cross-tenant foreign key must be rejected");
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn audit_sink_rows_are_visible_only_in_the_emitting_tenant() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_a, &format!("test-{tenant_a}")).await;
    register_tenant(&admin, tenant_b, &format!("test-{tenant_b}")).await;

    let event_id = Uuid::new_v4();
    let mut event = AuditEvent::new("authorization", "auth.decision", AuditOutcome::Denied)
        .with_tenant(tenant_a.to_string());
    event.id = event_id;
    PostgresAuditSink::new(runtime.clone())
        .emit(event)
        .await
        .unwrap();

    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    let visible_a: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE id = $1")
        .bind(event_id)
        .fetch_one(scope_a.executor())
        .await
        .unwrap();
    assert_eq!(visible_a, 1);
    scope_a.rollback().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    let visible_b: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE id = $1")
        .bind(event_id)
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(visible_b, 0);
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn user_secret_repository_isolates_tenant_and_user_and_revokes_idempotently() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping tenant isolation test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&admin, tenant_a, &format!("test-{tenant_a}")).await;
    register_tenant(&admin, tenant_b, &format!("test-{tenant_b}")).await;

    let credential_id = Uuid::now_v7();
    let encrypted_blob = serde_json::json!({
        "version": 1,
        "ciphertext": "test-envelope-ciphertext",
        "wrapped_key": {"provider": "test"}
    });
    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    let created = PgUserSecretRepository::create_tx(
        scope_a.executor(),
        credential_id,
        tenant_a,
        "user_a",
        "openai",
        "personal",
        encrypted_blob,
    )
    .await
    .unwrap();
    assert_eq!(created.id, credential_id);

    let stored = PgUserSecretRepository::get_active_tx(
        scope_a.executor(),
        tenant_a,
        "user_a",
        credential_id,
    )
    .await
    .unwrap()
    .unwrap();
    let debug = format!("{stored:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("test-envelope-ciphertext"));
    assert!(
        PgUserSecretRepository::get_active_tx(
            scope_a.executor(),
            tenant_a,
            "user_b",
            credential_id,
        )
        .await
        .unwrap()
        .is_none(),
        "a second user in the tenant must not see the credential"
    );
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&runtime, tenant_b).await.unwrap();
    assert!(
        PgUserSecretRepository::list_tx(scope_b.executor(), tenant_b, "user_a")
            .await
            .unwrap()
            .is_empty(),
        "a second tenant must not see the credential"
    );
    assert!(
        !PgUserSecretRepository::revoke_tx(scope_b.executor(), tenant_b, "user_a", credential_id,)
            .await
            .unwrap(),
        "cross-tenant revocation must not find the credential"
    );
    scope_b.commit().await.unwrap();

    let mut scope_a = TenantScopedConn::begin(&runtime, tenant_a).await.unwrap();
    assert!(PgUserSecretRepository::revoke_tx(
        scope_a.executor(),
        tenant_a,
        "user_a",
        credential_id,
    )
    .await
    .unwrap());
    assert!(
        PgUserSecretRepository::revoke_tx(scope_a.executor(), tenant_a, "user_a", credential_id,)
            .await
            .unwrap(),
        "repeated revocation must remain idempotent"
    );
    assert!(PgUserSecretRepository::get_active_tx(
        scope_a.executor(),
        tenant_a,
        "user_a",
        credential_id,
    )
    .await
    .unwrap()
    .is_none());
    let listed = PgUserSecretRepository::list_tx(scope_a.executor(), tenant_a, "user_a")
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].revoked_at.is_some());
    scope_a.commit().await.unwrap();

    let missing_scope = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM user_secrets")
        .fetch_one(&runtime)
        .await;
    assert!(
        missing_scope.is_err(),
        "an unscoped credential query must fail"
    );
}

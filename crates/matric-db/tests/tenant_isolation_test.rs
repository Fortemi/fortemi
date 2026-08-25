use std::str::FromStr;

use matric_core::audit::{AuditEvent, AuditOutcome, AuditSink};
use matric_db::{
    assert_hosted_runtime_role, create_pool, inspect_tenant_catalog, Database,
    PgUserSecretRepository, PostgresAuditSink, TenantScopedConn,
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

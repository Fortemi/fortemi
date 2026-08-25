use std::str::FromStr;

use matric_core::audit::{AuditEvent, AuditOutcome, AuditSink};
use matric_core::CreateNoteRequest;
use matric_db::{
    assert_hosted_runtime_role, create_pool, inspect_tenant_catalog, Database, PgNoteRepository,
    PgUserSecretRepository, PostgresAuditSink, TenantScopedConn, TENANT_SCOPED_TABLES,
};
use pgvector::Vector;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, Row};
use uuid::Uuid;

const TEST_RUNTIME_ROLE: &str = "fortemi_tenant_matrix_test";
const TEST_RUNTIME_PASSWORD: &str = "fortemi-tenant-matrix-test-only";
const REQUIRE_LIVE_POSTGRES: &str = "FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS";
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct TestDatabase {
    _guard: tokio::sync::MutexGuard<'static, ()>,
    admin: sqlx::PgPool,
    runtime: sqlx::PgPool,
}

fn live_postgres_required() -> bool {
    std::env::var(REQUIRE_LIVE_POSTGRES).is_ok_and(|value| value == "1")
}

fn setup_failure(message: impl std::fmt::Display) -> Option<TestDatabase> {
    if live_postgres_required() {
        panic!("live PostgreSQL tenant gate is required: {message}");
    }
    eprintln!("skipping live PostgreSQL tenant gate: {message}");
    None
}

async fn setup() -> Option<TestDatabase> {
    let guard = TEST_LOCK.lock().await;
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(error) => return setup_failure(error),
    };
    let admin = match create_pool(&database_url).await {
        Ok(pool) => pool,
        Err(error) => return setup_failure(error),
    };
    if let Err(error) = Database::new(admin.clone()).migrate().await {
        return setup_failure(error);
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
    admin
        .execute(
            format!(
                "REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON public.tenant_registry FROM {TEST_RUNTIME_ROLE}"
            )
            .as_str(),
        )
        .await
        .unwrap();
    admin
        .execute(format!("REVOKE CREATE ON SCHEMA public FROM {TEST_RUNTIME_ROLE}").as_str())
        .await
        .unwrap();

    let options = PgConnectOptions::from_str(&database_url)
        .unwrap()
        .username(TEST_RUNTIME_ROLE)
        .password(TEST_RUNTIME_PASSWORD);
    let runtime = match PgPoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .connect_with(options)
        .await
    {
        Ok(pool) => pool,
        Err(error) => return setup_failure(error),
    };
    Some(TestDatabase {
        _guard: guard,
        admin,
        runtime,
    })
}

async fn register_tenant(admin: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("ti-{tenant_id}"))
    .execute(admin)
    .await
    .unwrap();
}

async fn two_tenants(database: &TestDatabase) -> (Uuid, Uuid) {
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    register_tenant(&database.admin, tenant_a).await;
    register_tenant(&database.admin, tenant_b).await;
    (tenant_a, tenant_b)
}

async fn insert_note(
    runtime: &sqlx::PgPool,
    scope: &mut TenantScopedConn<'_>,
    title: &str,
    content: &str,
) -> Uuid {
    PgNoteRepository::new(runtime.clone())
        .insert_tx(
            scope.executor(),
            CreateNoteRequest {
                content: content.to_string(),
                format: "markdown".to_string(),
                source: "tenant-isolation-test".to_string(),
                collection_id: None,
                tags: None,
                metadata: None,
                document_type_id: None,
                title: Some(title.to_string()),
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn ti_01_user_b_cannot_list_user_a_notes() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    insert_note(&database.runtime, &mut scope_a, "tenant-a", "private alpha").await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM note")
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(visible, 0);
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_02_user_b_cannot_fetch_user_a_note_by_uuid() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let note_id = insert_note(&database.runtime, &mut scope_a, "tenant-a", "private alpha").await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let fetched: Option<Uuid> = sqlx::query_scalar("SELECT id FROM note WHERE id = $1")
        .bind(note_id)
        .fetch_optional(scope_b.executor())
        .await
        .unwrap();
    assert!(
        fetched.is_none(),
        "cross-tenant fetch must normalize to not found"
    );
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_03_user_b_cannot_update_user_a_note() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let note_id = insert_note(&database.runtime, &mut scope_a, "original", "private alpha").await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let changed = sqlx::query("UPDATE note SET title = 'forged' WHERE id = $1")
        .bind(note_id)
        .execute(scope_b.executor())
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 0);
    scope_b.commit().await.unwrap();

    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM note WHERE id = $1")
        .bind(note_id)
        .fetch_one(scope_a.executor())
        .await
        .unwrap();
    assert_eq!(title, "original");
    scope_a.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_04_user_b_cannot_delete_user_a_note() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let note_id = insert_note(&database.runtime, &mut scope_a, "original", "private alpha").await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let changed = sqlx::query("UPDATE note SET deleted_at = now() WHERE id = $1")
        .bind(note_id)
        .execute(scope_b.executor())
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 0);
    scope_b.commit().await.unwrap();

    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM note WHERE id = $1")
            .bind(note_id)
            .fetch_one(scope_a.executor())
            .await
            .unwrap();
    assert!(deleted_at.is_none());
    scope_a.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_05_user_b_cannot_insert_into_user_a_collection() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let collection_id = Uuid::new_v4();
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collection (id, name, created_at_utc) VALUES ($1, $2, now())")
        .bind(collection_id)
        .bind(format!("collection-{collection_id}"))
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc, title) VALUES ($1, $2, 'markdown', 'ti-05', now(), now(), 'cross-tenant')",
    )
    .bind(Uuid::new_v4())
    .bind(collection_id)
    .execute(scope_b.executor())
    .await;
    assert!(
        result.is_err(),
        "tenant-qualified collection FK must reject the association"
    );
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_06_same_connection_reused_between_users_isolates() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    insert_note(&database.runtime, &mut scope_a, "tenant-a", "private alpha").await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let bound: String = sqlx::query_scalar("SELECT current_setting('app.current_tenant')")
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM note")
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(bound, tenant_b.to_string());
    assert_eq!(visible, 0);
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_07_sql_injection_in_search_string_cannot_bypass_rls() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    insert_note(
        &database.runtime,
        &mut scope_a,
        "needle",
        "tenant alpha needle",
    )
    .await;
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let rows: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id WHERE nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)",
    )
    .bind("needle') OR true --")
    .fetch_all(scope_b.executor())
    .await
    .unwrap();
    assert!(rows.is_empty());
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_08_vector_similarity_search_filters_by_tenant_before_or_during_scoring() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let vector = Vector::from(vec![0.25_f32; 768]);
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    let note_id = insert_note(&database.runtime, &mut scope_a, "vector", "private vector").await;
    sqlx::query("INSERT INTO embedding (id, note_id, chunk_index, text, vector, model) VALUES ($1, $2, 0, 'private vector', $3, 'ti-08')")
        .bind(Uuid::new_v4())
        .bind(note_id)
        .bind(vector.clone())
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let hits: Vec<Uuid> = sqlx::query_scalar(
        "SELECT note_id FROM embedding WHERE vector IS NOT NULL ORDER BY vector <=> $1 LIMIT 10",
    )
    .bind(vector)
    .fetch_all(scope_b.executor())
    .await
    .unwrap();
    assert!(
        hits.is_empty(),
        "cross-tenant vectors must not enter the scored result set"
    );
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_09_new_table_without_rls_fails_ci() {
    let Some(database) = setup().await else {
        return;
    };
    let table = format!("ti_unprotected_{}", Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE public.{table} (id UUID PRIMARY KEY, tenant_id UUID NOT NULL)"
    ))
    .execute(&database.admin)
    .await
    .unwrap();

    let report = inspect_tenant_catalog(&database.runtime).await.unwrap();
    assert!(
        report.unclassified_tables.iter().any(|name| name == &table),
        "contrived unprotected table must fail the catalog inventory: {report:?}"
    );
    assert!(assert_hosted_runtime_role(&database.runtime).await.is_err());

    sqlx::query(&format!("DROP TABLE public.{table}"))
        .execute(&database.admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn ti_10_role_lacks_bypassrls_and_superuser() {
    let Some(database) = setup().await else {
        return;
    };
    assert_hosted_runtime_role(&database.runtime).await.unwrap();
    assert!(assert_hosted_runtime_role(&database.admin).await.is_err());

    let row =
        sqlx::query("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&database.runtime)
            .await
            .unwrap();
    assert!(!row.get::<bool, _>("rolsuper"));
    assert!(!row.get::<bool, _>("rolbypassrls"));
}

#[tokio::test]
async fn ti_11_unset_or_empty_current_tenant_fails_closed() {
    let Some(database) = setup().await else {
        return;
    };
    assert!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note")
        .fetch_one(&database.runtime)
        .await
        .is_err());

    let mut connection = database.runtime.acquire().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', '', false)")
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note")
        .fetch_one(&mut *connection)
        .await
        .is_err());
}

#[tokio::test]
async fn ti_12_archive_search_path_and_tenant_scope_do_not_leak_across_requests() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let schema = format!("archive_ti_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&database.admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE TABLE {schema}.tenant_probe (id UUID PRIMARY KEY, tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid)"
    ))
    .execute(&database.admin)
    .await
    .unwrap();
    for statement in [
        format!("ALTER TABLE {schema}.tenant_probe ENABLE ROW LEVEL SECURITY"),
        format!("ALTER TABLE {schema}.tenant_probe FORCE ROW LEVEL SECURITY"),
        format!("CREATE POLICY tenant_isolation ON {schema}.tenant_probe USING (tenant_id = current_setting('app.current_tenant')::uuid) WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid)"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {TEST_RUNTIME_ROLE}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON {schema}.tenant_probe TO {TEST_RUNTIME_ROLE}"),
    ] {
        sqlx::query(&statement).execute(&database.admin).await.unwrap();
    }

    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('search_path', $1, true)")
        .bind(format!("{schema}, public"))
        .execute(scope_a.executor())
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenant_probe (id) VALUES ($1)")
        .bind(Uuid::new_v4())
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('search_path', $1, true)")
        .bind(format!("{schema}, public"))
        .execute(scope_b.executor())
        .await
        .unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_probe")
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(visible, 0);
    scope_b.rollback().await.unwrap();

    let search_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&database.runtime)
        .await
        .unwrap();
    assert!(!search_path.contains(&schema));
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&database.admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn ti_13_background_job_and_outbox_rows_are_tenant_scoped() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO job_queue (id, job_type, priority) VALUES ($1, 'embedding', 5)")
        .bind(Uuid::new_v4())
        .execute(scope_a.executor())
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_outbox (id, event_type, entity_type, entity_id) VALUES ($1, 'ti.13', 'note', $2)")
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    for table in ["job_queue", "event_outbox"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(scope_b.executor())
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} leaked across tenants");
    }
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_14_byo_secret_rows_are_tenant_and_user_scoped() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let credential_id = Uuid::now_v7();
    let encrypted_blob = serde_json::json!({
        "version": 1,
        "ciphertext": "test-envelope-ciphertext",
        "wrapped_key": {"provider": "test"}
    });
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    PgUserSecretRepository::create_tx(
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
    assert!(PgUserSecretRepository::get_active_tx(
        scope_a.executor(),
        tenant_a,
        "user_b",
        credential_id,
    )
    .await
    .unwrap()
    .is_none());
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    assert!(
        PgUserSecretRepository::list_tx(scope_b.executor(), tenant_b, "user_a")
            .await
            .unwrap()
            .is_empty()
    );
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn ti_15_public_routes_cannot_reach_tenant_helpers_without_context() {
    let Some(database) = setup().await else {
        return;
    };
    let public_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_registry")
        .fetch_one(&database.runtime)
        .await
        .unwrap();
    assert!(
        public_count >= 1,
        "system-scoped tenant admission reads remain available"
    );
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note")
            .fetch_one(&database.runtime)
            .await
            .is_err(),
        "a public/unscoped connection must not reach tenant data"
    );
}

#[tokio::test]
async fn ti_16_tenant_qualified_constraints_prevent_cross_tenant_associations() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let collection_id = Uuid::new_v4();
    let mut scope_a = TenantScopedConn::begin(&database.runtime, tenant_a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collection (id, name, created_at_utc) VALUES ($1, $2, now())")
        .bind(collection_id)
        .bind(format!("collection-{collection_id}"))
        .execute(scope_a.executor())
        .await
        .unwrap();
    scope_a.commit().await.unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc, title) VALUES ($1, $2, 'markdown', 'ti-16', now(), now(), 'cross-tenant')",
    )
    .bind(Uuid::new_v4())
    .bind(collection_id)
    .execute(scope_b.executor())
    .await;
    assert!(result.is_err());
    scope_b.rollback().await.unwrap();
}

#[tokio::test]
async fn tenant_qualified_constraints_preserve_referential_actions() {
    let Some(database) = setup().await else {
        return;
    };
    let drift_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
          FROM pg_constraint original
          JOIN pg_class child ON child.oid = original.conrelid
          JOIN pg_class parent ON parent.oid = original.confrelid
          LEFT JOIN pg_constraint guard
            ON guard.conrelid = original.conrelid
           AND guard.conname = format(
               'fk_tenant_guard_%s_%s',
               left(child.relname, 20),
               left(md5(child.relname || ':' || original.conname), 10)
           )
         WHERE original.contype = 'f'
           AND child.relname = ANY($1)
           AND parent.relname = ANY($1)
           AND NOT EXISTS (
               SELECT 1
                 FROM unnest(original.conkey) key(attnum)
                 JOIN pg_attribute attribute
                   ON attribute.attrelid = original.conrelid
                  AND attribute.attnum = key.attnum
                WHERE attribute.attname = 'tenant_id'
           )
           AND (
               guard.oid IS NULL
               OR guard.confupdtype <> original.confupdtype
               OR guard.confdeltype <> original.confdeltype
           )
        "#,
    )
    .bind(TENANT_SCOPED_TABLES)
    .fetch_one(&database.admin)
    .await
    .unwrap();
    assert_eq!(
        drift_count, 0,
        "tenant-qualified guards must preserve source FK update/delete actions"
    );
}

#[tokio::test]
async fn role_catalog_ci_runtime_grants_are_least_privilege() {
    let Some(database) = setup().await else {
        return;
    };
    let report = inspect_tenant_catalog(&database.runtime).await.unwrap();
    assert!(report.is_clean(), "tenant catalog drift: {report:?}");
    assert_hosted_runtime_role(&database.runtime).await.unwrap();

    let owns_tenant_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_roles r ON r.oid = c.relowner WHERE n.nspname = 'public' AND c.relname = ANY($1) AND r.rolname = current_user",
    )
    .bind(TENANT_SCOPED_TABLES)
    .fetch_one(&database.runtime)
    .await
    .unwrap();
    assert_eq!(owns_tenant_tables, 0);

    for privilege in ["TRUNCATE", "REFERENCES", "TRIGGER"] {
        let granted: bool = sqlx::query_scalar(
            "SELECT COALESCE(bool_or(has_table_privilege(current_user, format('public.%I', table_name), $2)), false) FROM unnest($1::text[]) AS table_name",
        )
        .bind(TENANT_SCOPED_TABLES)
        .bind(privilege)
        .fetch_one(&database.runtime)
        .await
        .unwrap();
        assert!(!granted, "runtime role unexpectedly has {privilege}");
    }
    let can_create_public: bool =
        sqlx::query_scalar("SELECT has_schema_privilege(current_user, 'public', 'CREATE')")
            .fetch_one(&database.runtime)
            .await
            .unwrap();
    assert!(!can_create_public);
}

#[tokio::test]
async fn durable_audit_rows_are_visible_only_in_the_emitting_tenant() {
    let Some(database) = setup().await else {
        return;
    };
    let (tenant_a, tenant_b) = two_tenants(&database).await;
    let event_id = Uuid::new_v4();
    let mut event = AuditEvent::new("authorization", "auth.decision", AuditOutcome::Denied)
        .with_tenant(tenant_a.to_string());
    event.id = event_id;
    PostgresAuditSink::new(database.runtime.clone())
        .emit(event)
        .await
        .unwrap();

    let mut scope_b = TenantScopedConn::begin(&database.runtime, tenant_b)
        .await
        .unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE id = $1")
        .bind(event_id)
        .fetch_one(scope_b.executor())
        .await
        .unwrap();
    assert_eq!(visible, 0);
    scope_b.rollback().await.unwrap();
}

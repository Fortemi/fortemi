use std::str::FromStr;

use matric_api::services::{
    ensure_user_secret_rewrap_job, erase_user_secrets_for_confirmed_dsar,
    run_user_secret_rewrap_batch, seal_user_secret, unseal_user_secret, user_secret_context,
    UserSecretErasureError, UserSecretRewrapStatus,
};
use matric_crypto::{rewrap_between, DeploymentMode, EncryptedBlob, EnvKeyProvider};
use matric_db::{
    create_pool, Database, PgUserSecretRepository, PostgresAuditSink, TenantScopedConn,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::Executor;
use sqlx::Row;
use uuid::Uuid;
use zeroize::Zeroizing;

const TEST_RUNTIME_ROLE: &str = "fortemi_user_secret_storage_test";
const TEST_RUNTIME_PASSWORD: &str = "fortemi-user-secret-storage-test-only";

async fn setup() -> Option<(sqlx::PgPool, sqlx::PgPool)> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };
    let admin = create_pool(&database_url).await.unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();

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

#[tokio::test]
async fn stored_user_secret_rewrap_preserves_ciphertext_and_remains_decryptable() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping stored credential rewrap test: DATABASE_URL unavailable");
        return;
    };

    let tenant_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("test-{tenant_id}"))
    .execute(&admin)
    .await
    .unwrap();

    let source = EnvKeyProvider::new(
        Zeroizing::new([11u8; 32]),
        "stored-credential-source",
        1,
        DeploymentMode::Development,
    )
    .unwrap();
    let target = EnvKeyProvider::new(
        Zeroizing::new([19u8; 32]),
        "stored-credential-target",
        2,
        DeploymentMode::Development,
    )
    .unwrap();
    let user_id = "user_rewrap";
    let credential_id = Uuid::now_v7();
    let sealed = seal_user_secret(
        &source,
        tenant_id,
        user_id,
        credential_id,
        "openai",
        "sk-stored-row-rewrap-test",
    )
    .await
    .unwrap();

    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    PgUserSecretRepository::create_tx(
        scope.executor(),
        credential_id,
        tenant_id,
        user_id,
        "openai",
        "rotation receipt",
        sealed.encrypted_blob,
    )
    .await
    .unwrap();
    scope.commit().await.unwrap();

    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    let stored =
        PgUserSecretRepository::get_active_tx(scope.executor(), tenant_id, user_id, credential_id)
            .await
            .unwrap()
            .unwrap();
    let mut envelope: EncryptedBlob = serde_json::from_value(stored.encrypted_blob).unwrap();
    let ciphertext_before = envelope.ciphertext().to_vec();
    let context = user_secret_context(tenant_id, user_id, credential_id).unwrap();
    let next_wrapped = rewrap_between(&source, &target, envelope.wrapped_key(), &context)
        .await
        .unwrap();
    assert!(next_wrapped.rewrapped_at().is_some());
    let next_wrapped_json = serde_json::to_value(&next_wrapped).unwrap();
    envelope.replace_wrapped_key(next_wrapped).unwrap();
    assert_eq!(envelope.ciphertext(), ciphertext_before);
    assert!(PgUserSecretRepository::replace_wrapped_key_tx(
        scope.executor(),
        tenant_id,
        user_id,
        credential_id,
        next_wrapped_json,
    )
    .await
    .unwrap());
    scope.commit().await.unwrap();

    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    let stored =
        PgUserSecretRepository::get_active_tx(scope.executor(), tenant_id, user_id, credential_id)
            .await
            .unwrap()
            .unwrap();
    let persisted: EncryptedBlob = serde_json::from_value(stored.encrypted_blob.clone()).unwrap();
    assert_eq!(persisted.ciphertext(), ciphertext_before);
    let plaintext = unseal_user_secret(
        &target,
        tenant_id,
        user_id,
        credential_id,
        "openai",
        stored.encrypted_blob,
    )
    .await
    .unwrap();
    assert_eq!(plaintext.as_str(), "sk-stored-row-rewrap-test");
    scope.rollback().await.unwrap();
}

#[tokio::test]
async fn rewrap_job_checkpoints_resumes_and_emits_metadata_only_receipts() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping rewrap lifecycle test: DATABASE_URL unavailable");
        return;
    };
    let tenant_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("rewrap-{tenant_id}"))
    .execute(&admin)
    .await
    .unwrap();
    let provider = EnvKeyProvider::new(
        Zeroizing::new([31u8; 32]),
        "rewrap-worker-provider",
        1,
        DeploymentMode::Development,
    )
    .unwrap();
    let mut ciphertexts = std::collections::HashMap::new();
    for index in 0..3 {
        let id = Uuid::now_v7();
        let sealed = seal_user_secret(
            &provider,
            tenant_id,
            "user_batch",
            id,
            "openai",
            &format!("sk-batch-rewrap-{index}"),
        )
        .await
        .unwrap();
        let envelope: EncryptedBlob =
            serde_json::from_value(sealed.encrypted_blob.clone()).unwrap();
        ciphertexts.insert(id, envelope.ciphertext().to_vec());
        let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
        PgUserSecretRepository::create_tx(
            scope.executor(),
            id,
            tenant_id,
            "user_batch",
            "openai",
            "batch receipt",
            sealed.encrypted_blob,
        )
        .await
        .unwrap();
        scope.commit().await.unwrap();
    }

    let job_id = Uuid::now_v7();
    let audit = PostgresAuditSink::new(runtime.clone());
    let created = ensure_user_secret_rewrap_job(&runtime, tenant_id, job_id, 2)
        .await
        .unwrap();
    assert_eq!(created.status, UserSecretRewrapStatus::Pending);
    let first = run_user_secret_rewrap_batch(&runtime, &audit, &provider, tenant_id, job_id)
        .await
        .unwrap();
    assert_eq!(first.status, UserSecretRewrapStatus::Pending);
    assert_eq!(first.scanned_count, 2);
    let completed = run_user_secret_rewrap_batch(&runtime, &audit, &provider, tenant_id, job_id)
        .await
        .unwrap();
    assert_eq!(completed.status, UserSecretRewrapStatus::Completed);
    assert_eq!(completed.scanned_count, 3);
    assert_eq!(completed.rewrapped_count, 3);
    assert_eq!(completed.skipped_count, 0);

    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    let rows =
        sqlx::query("SELECT id, encrypted_blob, rewrapped_at FROM user_secrets WHERE user_id = $1")
            .bind("user_batch")
            .fetch_all(scope.executor())
            .await
            .unwrap();
    for row in rows {
        let id: Uuid = row.try_get("id").unwrap();
        let blob: serde_json::Value = row.try_get("encrypted_blob").unwrap();
        let envelope: EncryptedBlob = serde_json::from_value(blob).unwrap();
        assert_eq!(envelope.ciphertext(), ciphertexts[&id]);
        assert!(row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("rewrapped_at")
            .unwrap()
            .is_some());
    }
    let audit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE category = 'key_lifecycle'")
            .fetch_one(scope.executor())
            .await
            .unwrap();
    assert!(audit_count >= 4);
    scope.rollback().await.unwrap();

    let rendered = format!("{completed:?}");
    assert!(!rendered.contains("sk-batch"));
    assert!(!rendered.contains("rewrap-worker-provider"));
}

#[tokio::test]
async fn confirmed_dsar_hard_deletes_secret_material_and_retains_safe_audit() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping DSAR secret erasure test: DATABASE_URL unavailable");
        return;
    };
    let tenant_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("dsar-{tenant_id}"))
    .execute(&admin)
    .await
    .unwrap();
    let provider = EnvKeyProvider::new(
        Zeroizing::new([47u8; 32]),
        "dsar-test-provider",
        1,
        DeploymentMode::Development,
    )
    .unwrap();
    for (user_id, secret) in [
        ("subject_user", "sk-dsar-active"),
        ("subject_user", "sk-dsar-revoked"),
        ("other_user", "sk-dsar-other"),
    ] {
        let id = Uuid::now_v7();
        let sealed = seal_user_secret(&provider, tenant_id, user_id, id, "openai", secret)
            .await
            .unwrap();
        let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
        PgUserSecretRepository::create_tx(
            scope.executor(),
            id,
            tenant_id,
            user_id,
            "openai",
            "dsar receipt",
            sealed.encrypted_blob,
        )
        .await
        .unwrap();
        if secret.ends_with("revoked") {
            PgUserSecretRepository::revoke_tx(scope.executor(), tenant_id, user_id, id)
                .await
                .unwrap();
        }
        scope.commit().await.unwrap();
    }

    let request_id = Uuid::now_v7();
    let audit = PostgresAuditSink::new(runtime.clone());
    let receipt = erase_user_secrets_for_confirmed_dsar(
        &runtime,
        &audit,
        tenant_id,
        "subject_user",
        request_id,
    )
    .await
    .unwrap();
    assert_eq!(receipt.deleted_secret_rows, 2);
    assert_eq!(receipt.local_secret_outcome, "encrypted_secret_deleted");
    assert_eq!(
        receipt.provider_account_outcome,
        "provider_account_action_required"
    );

    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    let subject_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_secrets WHERE user_id = 'subject_user'")
            .fetch_one(scope.executor())
            .await
            .unwrap();
    let other_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_secrets WHERE user_id = 'other_user'")
            .fetch_one(scope.executor())
            .await
            .unwrap();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event WHERE resource_id = $1 AND category = 'privacy'",
    )
    .bind(request_id.to_string())
    .fetch_one(scope.executor())
    .await
    .unwrap();
    assert_eq!(subject_count, 0);
    assert_eq!(other_count, 1);
    assert_eq!(audit_count, 2);
    scope.rollback().await.unwrap();

    let rendered = format!("{receipt:?}");
    assert!(!rendered.contains("subject_user"));
    assert!(!rendered.contains("sk-dsar"));
}

#[tokio::test]
async fn confirmed_dsar_rolls_back_secret_deletion_when_completion_audit_fails() {
    let Some((admin, runtime)) = setup().await else {
        eprintln!("skipping DSAR atomicity test: DATABASE_URL unavailable");
        return;
    };
    let tenant_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("dsar-atomic-{tenant_id}"))
    .execute(&admin)
    .await
    .unwrap();

    let provider = EnvKeyProvider::new(
        Zeroizing::new([53u8; 32]),
        "dsar-atomic-provider",
        1,
        DeploymentMode::Development,
    )
    .unwrap();
    let secret_id = Uuid::now_v7();
    let sealed = seal_user_secret(
        &provider,
        tenant_id,
        "atomic_subject",
        secret_id,
        "openai",
        "sk-dsar-atomic",
    )
    .await
    .unwrap();
    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    PgUserSecretRepository::create_tx(
        scope.executor(),
        secret_id,
        tenant_id,
        "atomic_subject",
        "openai",
        "atomic receipt",
        sealed.encrypted_blob,
    )
    .await
    .unwrap();
    scope.commit().await.unwrap();

    let request_id = Uuid::now_v7();
    let suffix = request_id.simple();
    let function_name = format!("fail_dsar_completion_{suffix}");
    let trigger_name = format!("fail_dsar_completion_{suffix}");
    admin
        .execute(
            format!(
                r#"
                CREATE FUNCTION {function_name}() RETURNS trigger
                LANGUAGE plpgsql AS $$
                BEGIN
                  RAISE EXCEPTION 'injected completion audit failure';
                END
                $$;
                CREATE TRIGGER {trigger_name}
                  BEFORE INSERT ON public.audit_event
                  FOR EACH ROW
                  WHEN (NEW.resource_id = '{request_id}' AND NEW.action = 'dsar_secret_erasure_completed')
                  EXECUTE FUNCTION {function_name}();
                "#
            )
            .as_str(),
        )
        .await
        .unwrap();

    let audit = PostgresAuditSink::new(runtime.clone());
    let result = erase_user_secrets_for_confirmed_dsar(
        &runtime,
        &audit,
        tenant_id,
        "atomic_subject",
        request_id,
    )
    .await;

    admin
        .execute(
            format!(
                "DROP TRIGGER {trigger_name} ON public.audit_event; DROP FUNCTION {function_name}();"
            )
            .as_str(),
        )
        .await
        .unwrap();

    assert!(matches!(
        result,
        Err(UserSecretErasureError::AuditUnavailable)
    ));
    let mut scope = TenantScopedConn::begin(&runtime, tenant_id).await.unwrap();
    let secret_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_secrets WHERE user_id = 'atomic_subject'")
            .fetch_one(scope.executor())
            .await
            .unwrap();
    let started_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event WHERE resource_id = $1 AND action = 'dsar_secret_erasure_started'",
    )
    .bind(request_id.to_string())
    .fetch_one(scope.executor())
    .await
    .unwrap();
    let completed_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event WHERE resource_id = $1 AND action = 'dsar_secret_erasure_completed'",
    )
    .bind(request_id.to_string())
    .fetch_one(scope.executor())
    .await
    .unwrap();
    assert_eq!(secret_count, 1);
    assert_eq!(started_count, 1);
    assert_eq!(completed_count, 0);
    scope.rollback().await.unwrap();
}

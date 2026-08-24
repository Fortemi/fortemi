use std::str::FromStr;

use matric_api::services::{seal_user_secret, unseal_user_secret, user_secret_context};
use matric_crypto::{rewrap_between, DeploymentMode, EncryptedBlob, EnvKeyProvider};
use matric_db::{create_pool, Database, PgUserSecretRepository, TenantScopedConn};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::Executor;
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

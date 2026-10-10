use chrono::{Duration, Utc};
use matric_db::{
    create_pool, AppUserUpsert, Database, PersonalAccessTokenCrypto, PgAppUserRepository,
    PgPersonalAccessTokenRepository,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup() -> Option<(
    PgPool,
    PgAppUserRepository,
    PgPersonalAccessTokenRepository,
    Uuid,
)> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };
    let pool = create_pool(&database_url).await.ok()?;
    let schema_is_provisioned = sqlx::query_scalar::<_, bool>(
        "SELECT to_regclass('public.personal_access_token') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .ok()?;
    if !schema_is_provisioned {
        Database::new(pool.clone()).migrate().await.ok()?;
    }
    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(format!("pat-test-{tenant_id}"))
    .execute(&pool)
    .await
    .ok()?;
    Some((
        pool.clone(),
        PgAppUserRepository::new(pool.clone()),
        PgPersonalAccessTokenRepository::new(pool, PersonalAccessTokenCrypto::for_tests()),
        tenant_id,
    ))
}

fn user(tenant_id: Uuid, scopes: &[&str]) -> AppUserUpsert {
    AppUserUpsert {
        tenant_id,
        iss: "https://issuer.example".to_string(),
        sub: format!("subject-{}", Uuid::new_v4()),
        email: None,
        email_verified: false,
        display_name: None,
        groups: Vec::new(),
        current_scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
        kind: "user".to_string(),
        azp: Some("fortemi-web".to_string()),
    }
}

#[tokio::test]
async fn sr26_pat_is_shown_once_and_metadata_lists_only_prefix_last4() {
    let Some((_pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let user = users
        .upsert_oidc(user(tenant_id, &["read", "mcp"]), true)
        .await
        .unwrap();
    let created = pats
        .create(
            tenant_id,
            user.id,
            "tool",
            &["read".to_string()],
            Utc::now() + Duration::days(30),
        )
        .await
        .unwrap();
    assert!(created.token.starts_with("mm_pat_"));
    assert!(PgPersonalAccessTokenRepository::token_format_valid(
        &created.token
    ));
    let listed = pats.list_for_user(tenant_id, user.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].token_prefix, created.record.token_prefix);
    assert_eq!(listed[0].token_last4, created.record.token_last4);
    assert!(!format!("{listed:?}").contains(&created.token));
}

#[tokio::test]
async fn sr27_repeated_use_records_ip_without_sliding_expiry() {
    let Some((_pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let user = users
        .upsert_oidc(user(tenant_id, &["read"]), true)
        .await
        .unwrap();
    let expires_at = Utc::now() + Duration::days(30);
    let created = pats
        .create(
            tenant_id,
            user.id,
            "curl",
            &["read".to_string()],
            expires_at,
        )
        .await
        .unwrap();
    for _ in 0..2 {
        assert!(pats
            .validate_for_tenant(
                tenant_id,
                &created.token,
                30,
                Some("127.0.0.1".parse().unwrap())
            )
            .await
            .unwrap()
            .is_some());
    }
    let listed = pats.list_for_user(tenant_id, user.id).await.unwrap();
    assert_eq!(listed[0].expires_at, created.record.expires_at);
    assert_eq!(listed[0].use_count, 2);
    assert_eq!(listed[0].last_used_ip.as_deref(), Some("127.0.0.1"));
}

#[tokio::test]
async fn sr29_demoted_user_pat_loses_scopes() {
    let Some((_pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let mut input = user(tenant_id, &["read", "write"]);
    let sub = input.sub.clone();
    let user = users.upsert_oidc(input.clone(), true).await.unwrap();
    let created = pats
        .create(
            tenant_id,
            user.id,
            "script",
            &["read".to_string(), "write".to_string()],
            Utc::now() + Duration::days(30),
        )
        .await
        .unwrap();
    input.sub = sub;
    input.current_scopes = vec!["read".to_string()];
    users.upsert_oidc(input, true).await.unwrap();
    let validated = pats
        .validate_for_tenant(tenant_id, &created.token, 30, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(validated.effective_scopes, ["read"]);
}

#[tokio::test]
async fn sr30_stale_oidc_user_suspends_pat() {
    let Some((pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let user = users
        .upsert_oidc(user(tenant_id, &["read"]), true)
        .await
        .unwrap();
    let created = pats
        .create(
            tenant_id,
            user.id,
            "old",
            &["read".to_string()],
            Utc::now() + Duration::days(30),
        )
        .await
        .unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
        .bind(tenant_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE app_user SET last_oidc_at = now() - interval '31 days' WHERE id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(pats
        .validate_for_tenant(tenant_id, &created.token, 30, None)
        .await
        .unwrap()
        .is_none());
    let listed = pats.list_for_user(tenant_id, user.id).await.unwrap();
    assert_eq!(listed[0].status, "suspended");
}

#[tokio::test]
async fn sr31_sr32_disabled_user_fails_and_disable_revokes_tokens() {
    let Some((_pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let user = users
        .upsert_oidc(user(tenant_id, &["read"]), true)
        .await
        .unwrap();
    let created = pats
        .create(
            tenant_id,
            user.id,
            "disable",
            &["read".to_string()],
            Utc::now() + Duration::days(30),
        )
        .await
        .unwrap();
    users
        .set_status(tenant_id, user.id, "disabled")
        .await
        .unwrap();
    assert!(pats
        .validate_for_tenant(tenant_id, &created.token, 30, None)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        pats.revoke_for_disabled_user(tenant_id, user.id)
            .await
            .unwrap(),
        1
    );
    let listed = pats.list_for_user(tenant_id, user.id).await.unwrap();
    assert_eq!(listed[0].status, "revoked");
}

#[tokio::test]
async fn sr34_plain_sha256_row_does_not_validate() {
    let Some((pool, users, pats, tenant_id)) = setup().await else {
        eprintln!("skipping PAT test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let user = users
        .upsert_oidc(user(tenant_id, &["read"]), true)
        .await
        .unwrap();
    let token = PgPersonalAccessTokenRepository::generate_token();
    let plain_sha = hex::encode(Sha256::digest(token.as_bytes()));
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO personal_access_token
         (id, tenant_id, user_id, token_hash, token_prefix, token_last4, name, scopes, expires_at)
         VALUES ($1, $2, $3, $4, 'mm_pat_plain', 'abcd', 'plain', ARRAY['read'], now() + interval '30 days')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(user.id)
    .bind(plain_sha)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(pats
        .validate_for_tenant(tenant_id, &token, 30, None)
        .await
        .unwrap()
        .is_none());
}

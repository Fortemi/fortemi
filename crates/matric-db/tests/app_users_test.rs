use matric_core::JobType;
use matric_db::{
    create_pool, AppUserUpsert, Database, PgAppUserRepository, PgJobRepository, LOCAL_TENANT_ID,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup() -> Option<(PgPool, PgAppUserRepository, Uuid)> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };
    let pool = create_pool(&database_url).await.ok()?;
    let schema_is_provisioned =
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('public.tenant_registry') IS NOT NULL")
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
    .bind(format!("app-user-test-{tenant_id}"))
    .execute(&pool)
    .await
    .ok()?;

    Some((pool.clone(), PgAppUserRepository::new(pool), tenant_id))
}

fn user(tenant_id: Uuid, iss: &str, sub: &str, email: &str) -> AppUserUpsert {
    AppUserUpsert {
        tenant_id,
        iss: iss.to_string(),
        sub: sub.to_string(),
        email: Some(email.to_string()),
        email_verified: true,
        display_name: Some("Display Name".to_string()),
        groups: vec!["operators".to_string()],
        current_scopes: vec!["notes:read".to_string()],
        kind: "user".to_string(),
        azp: Some("fortemi-web".to_string()),
    }
}

#[tokio::test]
async fn same_email_from_two_issuers_creates_distinct_users() {
    let Some((_pool, repo, tenant_id)) = setup().await else {
        eprintln!("skipping app user test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let first = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer-a.example",
                "same-sub",
                "same@example.com",
            ),
            true,
        )
        .await
        .unwrap();
    let second = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer-b.example",
                "same-sub",
                "same@example.com",
            ),
            true,
        )
        .await
        .unwrap();

    assert_ne!(first.id, second.id);
    assert_eq!(first.email.as_deref(), Some("same@example.com"));
    assert_eq!(second.email.as_deref(), Some("same@example.com"));
}

#[tokio::test]
async fn same_issuer_subject_updates_email_on_existing_user() {
    let Some((_pool, repo, tenant_id)) = setup().await else {
        eprintln!("skipping app user test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let first = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer.example",
                "stable-sub",
                "old@example.com",
            ),
            true,
        )
        .await
        .unwrap();
    let updated = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer.example",
                "stable-sub",
                "new@example.com",
            ),
            true,
        )
        .await
        .unwrap();

    assert_eq!(first.id, updated.id);
    assert_eq!(updated.email.as_deref(), Some("new@example.com"));
}

#[tokio::test]
async fn disabled_status_is_persisted_for_principal_gate() {
    let Some((_pool, repo, tenant_id)) = setup().await else {
        eprintln!("skipping app user test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let created = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer.example",
                "disable-me",
                "user@example.com",
            ),
            true,
        )
        .await
        .unwrap();
    let disabled = repo
        .set_status(tenant_id, created.id, "disabled")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(disabled.id, created.id);
    assert_eq!(disabled.status, "disabled");
}

#[tokio::test]
async fn oidc_refresh_without_display_claims_preserves_stored_email_state() {
    let Some((_pool, repo, tenant_id)) = setup().await else {
        eprintln!("skipping app user test: DATABASE_URL unavailable or setup failed");
        return;
    };

    let created = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer.example",
                "email-state",
                "verified@example.com",
            ),
            true,
        )
        .await
        .unwrap();
    assert!(created.email_verified);

    let refreshed = repo
        .upsert_oidc(
            AppUserUpsert {
                tenant_id,
                iss: "https://issuer.example".to_string(),
                sub: "email-state".to_string(),
                email: None,
                email_verified: false,
                display_name: None,
                groups: Vec::new(),
                current_scopes: vec!["notes:read".to_string(), "mcp".to_string()],
                kind: "user".to_string(),
                azp: None,
            },
            true,
        )
        .await
        .unwrap();

    assert_eq!(refreshed.id, created.id);
    assert_eq!(refreshed.email.as_deref(), Some("verified@example.com"));
    assert!(refreshed.email_verified);
    assert_eq!(refreshed.azp.as_deref(), Some("fortemi-web"));
    assert_eq!(refreshed.current_scopes, ["notes:read", "mcp"]);
}

#[tokio::test]
async fn job_queue_records_initiating_app_user() {
    let Some((pool, repo, _tenant_id)) = setup().await else {
        eprintln!("skipping app user test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let tenant_id = Uuid::parse_str(LOCAL_TENANT_ID).expect("local tenant id");
    let user = repo
        .upsert_oidc(
            user(
                tenant_id,
                "https://issuer.example",
                "queue-initiator",
                "queue@example.com",
            ),
            true,
        )
        .await
        .unwrap();
    let jobs = PgJobRepository::new(pool.clone());

    let job_id = jobs
        .queue_with_initiator(
            None,
            JobType::ContextUpdate,
            5,
            Some(serde_json::json!({"tenant_id": tenant_id, "schema": "public"})),
            None,
            Some(user.id),
        )
        .await
        .unwrap();

    let stored: (Uuid, Uuid) =
        sqlx::query_as("SELECT tenant_id, initiated_by_user_id FROM job_queue WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, (tenant_id, user.id));
}

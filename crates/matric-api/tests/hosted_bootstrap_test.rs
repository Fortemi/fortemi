//! Hosted tenant bootstrap against a disposable, migrated PostgreSQL database.
//!
//! The URL comes only from FORTEMI_HOSTED_BOOTSTRAP_TEST_ADMIN_URL (never
//! DATABASE_URL) so the test cannot write to an unintended database.

use matric_api::admin_bootstrap::{
    bootstrap_tenant, derive_tenant_id, BootstrapError, BootstrapRequest, StepAction,
};
use sqlx::PgPool;
use uuid::Uuid;

const CLAIM: &str = "fortemi:tenant_id";

async fn admin_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("FORTEMI_HOSTED_BOOTSTRAP_TEST_ADMIN_URL") else {
        eprintln!("SKIP: FORTEMI_HOSTED_BOOTSTRAP_TEST_ADMIN_URL not supplied");
        return None;
    };
    Some(
        matric_db::create_pool(&url)
            .await
            .expect("explicit disposable test database"),
    )
}

fn unique_slug() -> String {
    format!("bootstrap-{}", Uuid::new_v4().simple())
}

async fn tenant_counts(pool: &PgPool, tenant: Uuid) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM tenant_registry WHERE id = $1 AND status = 'active'),
                (SELECT count(*) FROM embedding_config WHERE tenant_id = $1 AND is_default),
                (SELECT count(*) FROM embedding_set WHERE tenant_id = $1 AND slug = 'default'),
                (SELECT count(*) FROM skos_concept_scheme WHERE tenant_id = $1 AND notation = 'default'),
                (SELECT count(*) FROM shard_embedding_set_bootstrap WHERE tenant_id = $1),
                (SELECT count(*) FROM shard_skos_scheme_bootstrap WHERE tenant_id = $1)",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn bootstrap_is_idempotent_and_dry_run_writes_nothing() {
    let Some(pool) = admin_pool().await else {
        return;
    };
    let slug = unique_slug();
    let request = BootstrapRequest::new(&slug, None, Some("Acme Research"), false).unwrap();
    assert_eq!(request.tenant_id, derive_tenant_id(&slug));
    let local_before = tenant_counts(&pool, Uuid::nil()).await;

    let planned = bootstrap_tenant(&pool, &request, true, CLAIM)
        .await
        .unwrap();
    assert!(planned.dry_run && planned.changed);
    assert!(planned.steps.iter().all(|s| s.action == StepAction::Create));
    assert_eq!(
        tenant_counts(&pool, request.tenant_id).await,
        (0, 0, 0, 0, 0, 0)
    );

    let first = bootstrap_tenant(&pool, &request, false, CLAIM)
        .await
        .unwrap();
    assert!(first.changed && !first.dry_run);
    assert_eq!(first.tenant_claim.name, CLAIM);
    assert_eq!(first.tenant_claim.value, request.tenant_id.to_string());
    assert_eq!(
        tenant_counts(&pool, request.tenant_id).await,
        (1, 1, 1, 1, 1, 1)
    );

    let second = bootstrap_tenant(&pool, &request, false, CLAIM)
        .await
        .unwrap();
    assert!(!second.changed, "second run must be a no-op: {second:?}");
    assert!(second.steps.iter().all(|s| s.action == StepAction::None));
    assert_eq!(
        tenant_counts(&pool, request.tenant_id).await,
        (1, 1, 1, 1, 1, 1)
    );
    // Seeding another tenant never touches the local tenant's rows.
    assert_eq!(tenant_counts(&pool, Uuid::nil()).await, local_before);

    let renamed = BootstrapRequest::new(&slug, None, Some("Acme Labs"), false).unwrap();
    let update = bootstrap_tenant(&pool, &renamed, false, CLAIM)
        .await
        .unwrap();
    assert_eq!(update.steps[0].action, StepAction::Update);
    assert!(update.steps[1..]
        .iter()
        .all(|s| s.action == StepAction::None));
}

#[tokio::test]
async fn bootstrap_refuses_conflicts_and_respects_lifecycle() {
    let Some(pool) = admin_pool().await else {
        return;
    };
    let slug = unique_slug();
    let request = BootstrapRequest::new(&slug, None, None, false).unwrap();
    bootstrap_tenant(&pool, &request, false, CLAIM)
        .await
        .unwrap();

    // Same slug, different explicit id.
    let other = BootstrapRequest::new(&slug, Some(Uuid::new_v4()), None, false).unwrap();
    assert!(matches!(
        bootstrap_tenant(&pool, &other, false, CLAIM).await,
        Err(BootstrapError::Conflict(_))
    ));
    // Same id, different slug.
    let rename =
        BootstrapRequest::new(&unique_slug(), Some(request.tenant_id), None, false).unwrap();
    assert!(matches!(
        bootstrap_tenant(&pool, &rename, false, CLAIM).await,
        Err(BootstrapError::Conflict(_))
    ));

    sqlx::query(
        "UPDATE tenant_registry SET status = 'suspended', suspended_at = now() WHERE id = $1",
    )
    .bind(request.tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        bootstrap_tenant(&pool, &request, false, CLAIM).await,
        Err(BootstrapError::Conflict(_))
    ));
    let reactivate = BootstrapRequest::new(&slug, None, None, true).unwrap();
    let report = bootstrap_tenant(&pool, &reactivate, false, CLAIM)
        .await
        .unwrap();
    assert_eq!(report.steps[0].action, StepAction::Update);
    let status: String = sqlx::query_scalar("SELECT status FROM tenant_registry WHERE id = $1")
        .bind(request.tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");

    sqlx::query(
        "UPDATE tenant_registry SET status = 'soft_deleted', deleted_at = now() WHERE id = $1",
    )
    .bind(request.tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        bootstrap_tenant(&pool, &reactivate, false, CLAIM).await,
        Err(BootstrapError::Conflict(_))
    ));
}

/// The released admission decision (Strategy A) over the production tenant
/// store: a bootstrapped tenant's claim is admitted, an unknown one is not.
#[cfg(feature = "hosted-auth")]
mod admission {
    use super::*;
    use chrono::Utc;
    use fortemi_auth_core::{
        extract_tenant_id_strategy_a, AuthContext, AuthError, Credential, JwtToken, TenantStatus,
        TenantStore, VerifiedClaims,
    };
    use matric_api::hosted_auth::PgTenantStore;

    struct Claims(String);

    impl VerifiedClaims for Claims {
        fn issuer(&self) -> &str {
            "https://issuer.invalid"
        }
        fn audience(&self) -> &str {
            "fortemi"
        }
        fn tenant_claim(&self) -> Option<&str> {
            Some(&self.0)
        }
        fn into_context(self, tenant_id: Uuid) -> AuthContext {
            AuthContext {
                tenant_id,
                principal_id: "bootstrap-operator".into(),
                credential: Credential::Bearer(JwtToken {
                    jti: None,
                    algorithm: "RS256".into(),
                    key_id: "fixture".into(),
                }),
                issued_at: Utc::now(),
                expires_at: Utc::now(),
                scopes: vec!["read".into()],
                session_id: None,
                principal_kind: fortemi_auth_core::PrincipalKind::Human,
                scope_grants: Vec::new(),
                dropped_scope_count: 0,
            }
        }
    }

    #[tokio::test]
    async fn bootstrapped_tenant_is_admitted_and_unknown_fails_closed() {
        let Some(pool) = admin_pool().await else {
            return;
        };
        let store = PgTenantStore::new(pool.clone());
        let request = BootstrapRequest::new(&unique_slug(), None, None, false).unwrap();

        // Before bootstrap the claim is rejected without manual SQL fallback.
        assert_eq!(store.lookup(request.tenant_id).await.unwrap(), None);
        assert_eq!(
            extract_tenant_id_strategy_a(&Claims(request.tenant_id.to_string()), &store).await,
            Err(AuthError::UnknownTenant)
        );

        let report = bootstrap_tenant(&pool, &request, false, CLAIM)
            .await
            .unwrap();
        let record = store.lookup(request.tenant_id).await.unwrap().unwrap();
        assert_eq!(record.status, TenantStatus::Active);
        let claim = Claims(report.tenant_claim.value.clone());
        assert_eq!(
            extract_tenant_id_strategy_a(&claim, &store).await,
            Ok(request.tenant_id)
        );

        let unknown = Uuid::new_v4();
        assert_eq!(store.lookup(unknown).await.unwrap(), None);
        assert_eq!(
            extract_tenant_id_strategy_a(&Claims(unknown.to_string()), &store).await,
            Err(AuthError::UnknownTenant)
        );
    }
}

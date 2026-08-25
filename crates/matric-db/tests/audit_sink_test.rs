use matric_core::audit::{
    AuditEvent, AuditFailureDisposition, AuditFailurePolicy, AuditOutcome, AuditSink,
};
use matric_db::{create_pool, AuditSinkHealthStatus, Database, PostgresAuditSink};
use serde_json::json;
use uuid::Uuid;

async fn setup() -> Option<(sqlx::PgPool, Uuid)> {
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
    .bind(format!("audit-test-{tenant_id}"))
    .execute(&pool)
    .await
    .ok()?;
    Some((pool, tenant_id))
}

fn event(tenant_id: Uuid, key: &str) -> AuditEvent {
    let mut event = AuditEvent::new("authorization", "auth.decision", AuditOutcome::Denied)
        .with_tenant(tenant_id.to_string())
        .with_failure_policy(AuditFailurePolicy::FailClosed)
        .with_attr("authorization", "Bearer raw-secret")
        .with_attr("input", "line1\nline2|forged")
        .with_attr("nested", json!({"client_secret": "raw-secret"}));
    event.idempotency_key = Some(key.to_string());
    event
}

#[tokio::test]
async fn durable_sink_is_idempotent_sanitized_and_append_only() {
    let Some((pool, tenant_id)) = setup().await else {
        eprintln!("skipping audit sink test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let sink = PostgresAuditSink::new(pool.clone());
    let audit_event = event(tenant_id, "same-operation");
    sink.emit(audit_event.clone()).await.unwrap();
    sink.emit(audit_event).await.unwrap();

    let (count, attrs): (i64, serde_json::Value) =
        sqlx::query_as("SELECT count(*) OVER (), attrs FROM audit_event WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    assert_eq!(attrs["authorization"], "[REDACTED]");
    assert_eq!(attrs["input"], "line1 line2,forged");
    assert_eq!(attrs["nested"]["client_secret"], "[REDACTED]");
    let update = sqlx::query("UPDATE audit_event SET action = 'forged' WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;
    assert!(update.is_err(), "audit rows must reject mutation");
}

#[tokio::test]
async fn sink_requires_tenant_and_surfaces_outage_health_and_failure_policy() {
    let Some((pool, tenant_id)) = setup().await else {
        eprintln!("skipping audit sink test: DATABASE_URL unavailable or setup failed");
        return;
    };
    let sink = PostgresAuditSink::new(pool.clone());
    assert!(sink
        .emit(AuditEvent::new("system", "startup", AuditOutcome::Success))
        .await
        .is_err());

    pool.close().await;
    let sensitive = event(tenant_id, "outage");
    assert!(sink.emit(sensitive.clone()).await.is_err());
    let health = sink.health();
    assert_eq!(health.status, AuditSinkHealthStatus::Unavailable);
    assert!(health.consecutive_failures > 0);
    assert_eq!(
        health.disposition_for(&sensitive),
        AuditFailureDisposition::RejectOperation
    );
}

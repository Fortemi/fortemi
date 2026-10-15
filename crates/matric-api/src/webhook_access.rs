//! Current complete-catalog authorization for global outgoing subscriptions.
use super::*;
use sqlx::Row;

pub(crate) async fn allows_delivery(
    db: &Database,
    policy: &dyn AuthorizationPolicy,
    webhook: &matric_core::Webhook,
) -> bool {
    let mut caller = archive_access::Caller {
        principal: AuthPrincipal::Anonymous,
        tenant_id: Some(webhook.tenant_id),
        user_id: webhook.created_by_user_id,
    };
    if let Some(user) = webhook.created_by_user_id {
        let Ok(mut tx) = db.pool.begin().await else {
            return false;
        };
        if sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
            .bind(webhook.tenant_id.to_string())
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return false;
        }
        let row = sqlx::query("SELECT sub, azp, current_scopes FROM public.app_user WHERE tenant_id=$1 AND id=$2 AND status='active'")
            .bind(webhook.tenant_id).bind(user).fetch_optional(&mut *tx).await;
        if tx.rollback().await.is_err() {
            return false;
        }
        let Ok(Some(row)) = row else {
            return false;
        };
        caller.principal = AuthPrincipal::OAuthClient {
            client_id: row.get::<Option<String>, _>("azp").unwrap_or_default(),
            user_id: Some(row.get("sub")),
            scope: row.get::<Vec<String>, _>("current_scopes").join(" "),
        };
    }
    archive_access::complete_catalog(db, &caller, policy).await
}

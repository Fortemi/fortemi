//! Webhook repository for outbound HTTP notification management (Issue #44).

use chrono::Utc;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{CreateWebhookRequest, Error, Result, Webhook, WebhookDelivery};

/// PostgreSQL webhook repository.
pub struct PgWebhookRepository {
    pool: Pool<Postgres>,
}

impl PgWebhookRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Create a new webhook registration.
    pub async fn create(&self, req: CreateWebhookRequest) -> Result<Uuid> {
        let id = matric_core::new_v7();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO webhook (id, url, secret, events, max_retries, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(&req.url)
        .bind(&req.secret)
        .bind(&req.events)
        .bind(req.max_retries)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(id)
    }

    /// List all webhooks.
    pub async fn list(&self) -> Result<Vec<Webhook>> {
        let rows = sqlx::query(
            "SELECT id, url, secret, events, is_active, created_at, updated_at,
                    last_triggered_at, failure_count, max_retries
             FROM webhook ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    /// Get a webhook by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<Webhook>> {
        let row = sqlx::query(
            "SELECT id, url, secret, events, is_active, created_at, updated_at,
                    last_triggered_at, failure_count, max_retries
             FROM webhook WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.as_ref().map(Self::parse_row))
    }

    /// Delete a webhook.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM webhook WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Update webhook fields. Only non-None fields are updated.
    pub async fn update(
        &self,
        id: Uuid,
        url: Option<&str>,
        events: Option<&[String]>,
        secret: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "UPDATE webhook SET
                url = COALESCE($1, url),
                events = COALESCE($2, events),
                secret = COALESCE($3, secret),
                is_active = COALESCE($4, is_active),
                updated_at = $5
             WHERE id = $6",
        )
        .bind(url)
        .bind(events)
        .bind(secret)
        .bind(is_active)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    /// List active webhooks subscribed to a specific event type.
    pub async fn list_active_for_event(&self, event_type: &str) -> Result<Vec<Webhook>> {
        let rows = sqlx::query(
            "SELECT id, url, secret, events, is_active, created_at, updated_at,
                    last_triggered_at, failure_count, max_retries
             FROM webhook
             WHERE is_active = true AND ($1 = ANY(events) OR events = '{}')",
        )
        .bind(event_type)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    /// Record a delivery attempt.
    pub async fn record_delivery(
        &self,
        webhook_id: Uuid,
        event_type: &str,
        payload: &serde_json::Value,
        status_code: Option<i32>,
        response_body: Option<&str>,
        success: bool,
    ) -> Result<()> {
        let id = matric_core::new_v7();
        sqlx::query(
            "INSERT INTO webhook_delivery (id, webhook_id, event_type, payload, status_code, response_body, success)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(webhook_id)
        .bind(event_type)
        .bind(payload)
        .bind(status_code)
        .bind(response_body)
        .bind(success)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Update webhook last_triggered_at and failure tracking
        if success {
            sqlx::query(
                "UPDATE webhook SET last_triggered_at = now(), failure_count = 0, updated_at = now() WHERE id = $1",
            )
            .bind(webhook_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        } else {
            // Increment failure count; auto-disable after 10 consecutive failures
            sqlx::query(
                "UPDATE webhook SET failure_count = failure_count + 1, updated_at = now(),
                 is_active = CASE WHEN failure_count + 1 >= 10 THEN false ELSE is_active END
                 WHERE id = $1",
            )
            .bind(webhook_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
    }

    /// Get delivery history for a webhook.
    pub async fn list_deliveries(
        &self,
        webhook_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>> {
        let rows = sqlx::query(
            "SELECT id, webhook_id, event_type, payload, status_code, response_body, delivered_at, success
             FROM webhook_delivery
             WHERE webhook_id = $1
             ORDER BY delivered_at DESC
             LIMIT $2",
        )
        .bind(webhook_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| WebhookDelivery {
                id: r.get("id"),
                webhook_id: r.get("webhook_id"),
                event_type: r.get("event_type"),
                payload: r.get("payload"),
                status_code: r.get("status_code"),
                response_body: r.get("response_body"),
                delivered_at: r.get("delivered_at"),
                success: r.get("success"),
            })
            .collect())
    }

    fn parse_row(r: &sqlx::postgres::PgRow) -> Webhook {
        Webhook {
            id: r.get("id"),
            url: r.get("url"),
            secret: r.get("secret"),
            events: r.get("events"),
            is_active: r.get("is_active"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            last_triggered_at: r.get("last_triggered_at"),
            failure_count: r.get("failure_count"),
            max_retries: r.get("max_retries"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::DEFAULT_TEST_DATABASE_URL;

    async fn setup() -> PgWebhookRepository {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
        let pool = crate::create_pool(&database_url)
            .await
            .expect("Failed to connect to test DB");
        PgWebhookRepository::new(pool)
    }

    fn test_url() -> String {
        format!("https://test-{}.example.com/webhook", Uuid::new_v4())
    }

    fn test_request(url: &str) -> CreateWebhookRequest {
        CreateWebhookRequest {
            url: url.to_string(),
            secret: Some("test-secret".to_string()),
            events: vec!["JobCompleted".to_string(), "NoteUpdated".to_string()],
            max_retries: 3,
        }
    }

    #[tokio::test]
    async fn test_webhook_create_and_get() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();

        let webhook = repo.get(id).await.unwrap().expect("webhook should exist");
        assert_eq!(webhook.id, id);
        assert_eq!(webhook.url, url);
        assert_eq!(webhook.secret, Some("test-secret".to_string()));
        assert_eq!(webhook.events, vec!["JobCompleted", "NoteUpdated"]);
        assert!(webhook.is_active);
        assert_eq!(webhook.failure_count, 0);
        assert_eq!(webhook.max_retries, 3);
        assert!(webhook.last_triggered_at.is_none());

        // Cleanup
        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_list() {
        let repo = setup().await;
        let suffix = Uuid::new_v4();

        let id1 = repo
            .create(CreateWebhookRequest {
                url: format!("https://list-test-{}-1.example.com", suffix),
                secret: None,
                events: vec![],
                max_retries: 3,
            })
            .await
            .unwrap();
        let id2 = repo
            .create(CreateWebhookRequest {
                url: format!("https://list-test-{}-2.example.com", suffix),
                secret: None,
                events: vec![],
                max_retries: 3,
            })
            .await
            .unwrap();
        let id3 = repo
            .create(CreateWebhookRequest {
                url: format!("https://list-test-{}-3.example.com", suffix),
                secret: None,
                events: vec![],
                max_retries: 3,
            })
            .await
            .unwrap();

        let all = repo.list().await.unwrap();
        // Verify our 3 are present (other tests may have created some too)
        let our_ids: Vec<Uuid> = all
            .iter()
            .filter(|w| w.url.contains(&suffix.to_string()))
            .map(|w| w.id)
            .collect();
        assert_eq!(our_ids.len(), 3);
        // Ordered by created_at DESC, so id3 should be first among ours
        assert_eq!(our_ids[0], id3);

        // Cleanup
        for id in [id1, id2, id3] {
            repo.delete(id).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_webhook_delete() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();

        assert!(repo.get(id).await.unwrap().is_some());
        repo.delete(id).await.unwrap();
        assert!(repo.get(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_webhook_update_partial_fields() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();

        let before = repo.get(id).await.unwrap().unwrap();

        // Update only URL, leave everything else as None
        repo.update(id, Some("https://updated.example.com"), None, None, None)
            .await
            .unwrap();

        let after = repo.get(id).await.unwrap().unwrap();
        assert_eq!(after.url, "https://updated.example.com");
        // Events should be unchanged (COALESCE)
        assert_eq!(after.events, before.events);
        assert_eq!(after.secret, before.secret);
        assert_eq!(after.is_active, before.is_active);
        // updated_at should have changed
        assert!(after.updated_at > before.updated_at);

        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_update_is_active() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();

        assert!(repo.get(id).await.unwrap().unwrap().is_active);

        repo.update(id, None, None, None, Some(false))
            .await
            .unwrap();
        assert!(!repo.get(id).await.unwrap().unwrap().is_active);

        repo.update(id, None, None, None, Some(true)).await.unwrap();
        assert!(repo.get(id).await.unwrap().unwrap().is_active);

        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_list_active_for_event_filters_correctly() {
        let repo = setup().await;
        let suffix = Uuid::new_v4();

        // Webhook A: subscribes to JobCompleted + NoteUpdated, active
        let id_a = repo
            .create(CreateWebhookRequest {
                url: format!("https://filter-a-{}.example.com", suffix),
                secret: None,
                events: vec!["JobCompleted".to_string(), "NoteUpdated".to_string()],
                max_retries: 3,
            })
            .await
            .unwrap();

        // Webhook B: subscribes to JobFailed only, active
        let id_b = repo
            .create(CreateWebhookRequest {
                url: format!("https://filter-b-{}.example.com", suffix),
                secret: None,
                events: vec!["JobFailed".to_string()],
                max_retries: 3,
            })
            .await
            .unwrap();

        // Webhook C: subscribes to JobCompleted, but INACTIVE
        let id_c = repo
            .create(CreateWebhookRequest {
                url: format!("https://filter-c-{}.example.com", suffix),
                secret: None,
                events: vec!["JobCompleted".to_string()],
                max_retries: 3,
            })
            .await
            .unwrap();
        repo.update(id_c, None, None, None, Some(false))
            .await
            .unwrap();

        let job_completed = repo.list_active_for_event("JobCompleted").await.unwrap();
        let our_jc: Vec<Uuid> = job_completed
            .iter()
            .filter(|w| w.url.contains(&suffix.to_string()))
            .map(|w| w.id)
            .collect();
        assert!(our_jc.contains(&id_a));
        assert!(!our_jc.contains(&id_b)); // wrong event
        assert!(!our_jc.contains(&id_c)); // inactive

        let note_updated = repo.list_active_for_event("NoteUpdated").await.unwrap();
        let our_nu: Vec<Uuid> = note_updated
            .iter()
            .filter(|w| w.url.contains(&suffix.to_string()))
            .map(|w| w.id)
            .collect();
        assert!(our_nu.contains(&id_a));
        assert!(!our_nu.contains(&id_b));

        let job_failed = repo.list_active_for_event("JobFailed").await.unwrap();
        let our_jf: Vec<Uuid> = job_failed
            .iter()
            .filter(|w| w.url.contains(&suffix.to_string()))
            .map(|w| w.id)
            .collect();
        assert!(our_jf.contains(&id_b));
        assert!(!our_jf.contains(&id_a));

        for id in [id_a, id_b, id_c] {
            repo.delete(id).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_webhook_list_active_for_event_empty_events_matches_all() {
        let repo = setup().await;
        let suffix = Uuid::new_v4();

        // Empty events array = subscribe to all events
        let id = repo
            .create(CreateWebhookRequest {
                url: format!("https://catch-all-{}.example.com", suffix),
                secret: None,
                events: vec![],
                max_retries: 3,
            })
            .await
            .unwrap();

        let results = repo.list_active_for_event("JobCompleted").await.unwrap();
        assert!(results.iter().any(|w| w.id == id));

        let results2 = repo.list_active_for_event("NoteUpdated").await.unwrap();
        assert!(results2.iter().any(|w| w.id == id));

        let results3 = repo.list_active_for_event("SomeRandomEvent").await.unwrap();
        assert!(results3.iter().any(|w| w.id == id));

        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_record_delivery_success_resets_failure_count() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();
        let payload = serde_json::json!({"type": "test"});

        // Record 3 failed deliveries
        for _ in 0..3 {
            repo.record_delivery(id, "test", &payload, Some(500), Some("error"), false)
                .await
                .unwrap();
        }
        assert_eq!(repo.get(id).await.unwrap().unwrap().failure_count, 3);

        // Record 1 success
        repo.record_delivery(id, "test", &payload, Some(200), Some("ok"), true)
            .await
            .unwrap();

        let webhook = repo.get(id).await.unwrap().unwrap();
        assert_eq!(webhook.failure_count, 0);
        assert!(webhook.last_triggered_at.is_some());

        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_auto_disable_after_10_failures() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();
        let payload = serde_json::json!({"type": "test"});

        // Record 9 failures — should still be active
        for _ in 0..9 {
            repo.record_delivery(id, "test", &payload, Some(500), Some("error"), false)
                .await
                .unwrap();
        }
        let webhook = repo.get(id).await.unwrap().unwrap();
        assert!(webhook.is_active);
        assert_eq!(webhook.failure_count, 9);

        // 10th failure — auto-disable
        repo.record_delivery(id, "test", &payload, Some(500), Some("error"), false)
            .await
            .unwrap();
        let webhook = repo.get(id).await.unwrap().unwrap();
        assert!(!webhook.is_active);
        assert_eq!(webhook.failure_count, 10);

        repo.delete(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_webhook_list_deliveries() {
        let repo = setup().await;
        let url = test_url();
        let id = repo.create(test_request(&url)).await.unwrap();

        // Record 5 deliveries
        for i in 0..5 {
            let success = i % 2 == 0;
            let status = if success { 200 } else { 500 };
            let event_type = if i < 3 { "JobCompleted" } else { "NoteUpdated" };
            repo.record_delivery(
                id,
                event_type,
                &serde_json::json!({"index": i}),
                Some(status),
                Some("body"),
                success,
            )
            .await
            .unwrap();
        }

        // List with limit=3
        let deliveries = repo.list_deliveries(id, 3).await.unwrap();
        assert_eq!(deliveries.len(), 3);
        // Should be ordered by delivered_at DESC (newest first)
        assert!(deliveries[0].delivered_at >= deliveries[1].delivered_at);
        assert!(deliveries[1].delivered_at >= deliveries[2].delivered_at);
        // All should have correct webhook_id
        for d in &deliveries {
            assert_eq!(d.webhook_id, id);
        }

        // List all
        let all = repo.list_deliveries(id, 100).await.unwrap();
        assert_eq!(all.len(), 5);

        repo.delete(id).await.unwrap();
    }
}

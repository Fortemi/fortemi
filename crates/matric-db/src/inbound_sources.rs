//! Inbound external event source registrations + dead-letter queue (#833).
//!
//! Stores connector configs (`inbound_source`) that the matric-jobs inbound
//! supervisor loads and runs, plus the dead-letter queue (`inbound_dlq`) for
//! events that fail processing repeatedly.

use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{CreateInboundSourceRequest, Error, InboundSource, Result};

/// PostgreSQL repository for inbound event source connectors.
pub struct PgInboundSourceRepository {
    pool: Pool<Postgres>,
}

impl PgInboundSourceRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateInboundSourceRequest) -> Result<Uuid> {
        validate_name(&req.name)?;
        validate_kind(&req.kind)?;

        let id = matric_core::new_v7();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO inbound_source
                (id, name, kind, config, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(normalize_token(&req.name))
        .bind(normalize_token(&req.kind))
        .bind(&req.config)
        .bind(req.enabled)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(id)
    }

    pub async fn list(&self) -> Result<Vec<InboundSource>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, config, enabled, created_at, updated_at
             FROM inbound_source ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    /// Enabled connectors only — the set the supervisor starts.
    pub async fn list_enabled(&self) -> Result<Vec<InboundSource>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, config, enabled, created_at, updated_at
             FROM inbound_source WHERE enabled = true ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    pub async fn get_by_name(&self, name: &str) -> Result<Option<InboundSource>> {
        let row = sqlx::query(
            "SELECT id, name, kind, config, enabled, created_at, updated_at
             FROM inbound_source WHERE name = $1",
        )
        .bind(normalize_token(name))
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(row.as_ref().map(Self::parse_row))
    }

    /// Delete a connector registration by name. Idempotent: returns `false`
    /// when no row matched.
    pub async fn delete_by_name(&self, name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM inbound_source WHERE name = $1")
            .bind(normalize_token(name))
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(result.rows_affected() > 0)
    }

    /// Park a poison event in the dead-letter queue (#833).
    pub async fn record_dlq(
        &self,
        source_name: &str,
        source_offset: Option<&str>,
        payload: Option<&Value>,
        error: &str,
        attempts: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO inbound_dlq
                (id, source_name, source_offset, payload, error, attempts, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(matric_core::new_v7())
        .bind(inbound_dlq_source_ref(source_name))
        .bind(source_offset.map(inbound_dlq_offset_metadata))
        .bind(payload)
        .bind(inbound_dlq_error_metadata(error))
        .bind(attempts)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    /// Count of dead-lettered events for a source (used in tests/diagnostics).
    pub async fn dlq_count(&self, source_name: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM inbound_dlq WHERE source_name = $1")
            .bind(inbound_dlq_source_ref(source_name))
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("n"))
    }

    fn parse_row(r: &sqlx::postgres::PgRow) -> InboundSource {
        InboundSource {
            id: r.get("id"),
            name: r.get("name"),
            kind: r.get("kind"),
            config: r.get("config"),
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

fn validate_name(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.len() > 96 {
        return Err(Error::InvalidInput(
            "inbound source name must be 1-96 characters".to_string(),
        ));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(Error::InvalidInput(
            "inbound source name must use lowercase letters, numbers, '-' or '_'".to_string(),
        ));
    }
    Ok(())
}

fn validate_kind(kind: &str) -> Result<()> {
    let kind = kind.trim();
    if kind.is_empty() || kind.len() > 64 {
        return Err(Error::InvalidInput(
            "inbound source kind must be 1-64 characters".to_string(),
        ));
    }
    if !kind.bytes().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
    }) {
        return Err(Error::InvalidInput(
            "inbound source kind must use lowercase letters, numbers, '.', '-' or '_'".to_string(),
        ));
    }
    Ok(())
}

fn normalize_token(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

pub fn inbound_dlq_source_ref(source_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_name.as_bytes());
    format!("source_sha256:{}", hex::encode(hasher.finalize()))
}

fn inbound_dlq_offset_metadata(offset: &str) -> String {
    format!("offset_present=true;offset_len={}", offset.chars().count())
}

fn inbound_dlq_error_metadata(error: &str) -> String {
    format!(
        "reason_code={};error_len={}",
        inbound_dlq_error_reason_code(error),
        error.chars().count()
    )
}

fn inbound_dlq_error_reason_code(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("event_type") {
        "invalid_event_type"
    } else if error.contains("payload") {
        "invalid_payload"
    } else if error.contains("outbox") {
        "outbox_emit_failed"
    } else {
        "processing_failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(validate_name("redis-prod_1").is_ok());
        assert!(validate_name("Bad Name").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn kind_validation() {
        assert!(validate_kind("redis-stream").is_ok());
        assert!(validate_kind("sse").is_ok());
        assert!(validate_kind("Kafka!").is_err());
    }

    #[test]
    fn inbound_dlq_metadata_redacts_source_offset_and_error_text() {
        let source_name = "tenant-alpha postgres://user:secret@db.internal/mm_key_source";
        let offset = "redis://user:secret@redis.internal:6379/0-1";
        let error =
            "event_type is empty for postgres://user:secret@db.internal /srv/private/mm_key_dlq";

        let source_ref = inbound_dlq_source_ref(source_name);
        let offset_meta = inbound_dlq_offset_metadata(offset);
        let error_meta = inbound_dlq_error_metadata(error);
        let rendered = format!("{source_ref}\n{offset_meta}\n{error_meta}");

        assert!(source_ref.starts_with("source_sha256:"));
        assert_eq!(source_ref.len(), "source_sha256:".len() + 64);
        assert_eq!(
            offset_meta,
            format!("offset_present=true;offset_len={}", offset.chars().count())
        );
        assert_eq!(
            error_meta,
            format!(
                "reason_code=invalid_event_type;error_len={}",
                error.chars().count()
            )
        );

        for raw in [
            "tenant-alpha",
            "postgres://user:secret",
            "db.internal",
            "mm_key_source",
            "redis://user:secret",
            "redis.internal",
            "/srv/private",
            "mm_key_dlq",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }
}

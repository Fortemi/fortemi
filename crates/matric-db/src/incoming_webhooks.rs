//! Incoming webhook receiver registration and schema-shape validation.

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    CreateIncomingWebhookReceiverRequest, Error, IncomingWebhookReceiver,
    IncomingWebhookValidationResponse, Result,
};

/// PostgreSQL repository for incoming webhook receiver registrations.
pub struct PgIncomingWebhookReceiverRepository {
    pool: Pool<Postgres>,
}

impl PgIncomingWebhookReceiverRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateIncomingWebhookReceiverRequest) -> Result<Uuid> {
        validate_receiver_request(&req)?;
        validate_schema_ref(&req.schema_ref)?;

        let id = matric_core::new_v7();
        let now = Utc::now();
        let secret_hash = hash_hmac_secret(&req.hmac_secret);
        sqlx::query(
            "INSERT INTO incoming_webhook_receiver
                (id, slug, provider, schema_ref, hmac_secret_hash, is_active, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(normalize_token(&req.slug))
        .bind(normalize_token(&req.provider))
        .bind(normalize_schema_ref(&req.schema_ref))
        .bind(secret_hash)
        .bind(req.is_active)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(id)
    }

    pub async fn list(&self) -> Result<Vec<IncomingWebhookReceiver>> {
        let rows = sqlx::query(
            "SELECT id, slug, provider, schema_ref, hmac_secret_hash, is_active, created_at, updated_at
             FROM incoming_webhook_receiver ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<IncomingWebhookReceiver>> {
        let row = sqlx::query(
            "SELECT id, slug, provider, schema_ref, hmac_secret_hash, is_active, created_at, updated_at
             FROM incoming_webhook_receiver WHERE slug = $1",
        )
        .bind(normalize_token(slug))
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.as_ref().map(Self::parse_row))
    }

    fn parse_row(r: &sqlx::postgres::PgRow) -> IncomingWebhookReceiver {
        let secret_hash: String = r.get("hmac_secret_hash");
        IncomingWebhookReceiver {
            id: r.get("id"),
            slug: r.get("slug"),
            provider: r.get("provider"),
            schema_ref: r.get("schema_ref"),
            secret_set: !secret_hash.is_empty(),
            is_active: r.get("is_active"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

pub fn validate_incoming_webhook_payload(
    schema_ref: &str,
    payload: &serde_json::Value,
) -> Result<IncomingWebhookValidationResponse> {
    let schema_ref = normalize_schema_ref(schema_ref);
    validate_schema_ref(&schema_ref)?;
    let errors = match schema_ref.as_str() {
        "twilio.voice.v1" => validate_twilio_voice_payload(payload),
        "twilio.media-stream.v1" => validate_twilio_media_stream_payload(payload),
        other => vec![format!("unsupported incoming webhook schema_ref: {other}")],
    };

    Ok(IncomingWebhookValidationResponse {
        valid: errors.is_empty(),
        schema_ref,
        errors,
    })
}

fn validate_receiver_request(req: &CreateIncomingWebhookReceiverRequest) -> Result<()> {
    validate_slug(&req.slug)?;
    validate_provider(&req.provider)?;
    if req.hmac_secret.trim().len() < 16 {
        return Err(Error::InvalidInput(
            "incoming webhook hmac_secret must be at least 16 characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_slug(slug: &str) -> Result<()> {
    let slug = slug.trim();
    if slug.is_empty() || slug.len() > 96 {
        return Err(Error::InvalidInput(
            "incoming webhook slug must be 1-96 characters".to_string(),
        ));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(Error::InvalidInput(
            "incoming webhook slug must use lowercase letters, numbers, '-' or '_'".to_string(),
        ));
    }
    Ok(())
}

fn validate_provider(provider: &str) -> Result<()> {
    let provider = provider.trim();
    if provider.is_empty() || provider.len() > 64 {
        return Err(Error::InvalidInput(
            "incoming webhook provider must be 1-64 characters".to_string(),
        ));
    }
    if !provider
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Error::InvalidInput(
            "incoming webhook provider must use lowercase letters, numbers, or '-'".to_string(),
        ));
    }
    Ok(())
}

fn validate_schema_ref(schema_ref: &str) -> Result<()> {
    match normalize_schema_ref(schema_ref).as_str() {
        "twilio.voice.v1" | "twilio.media-stream.v1" => Ok(()),
        other => Err(Error::InvalidInput(format!(
            "unsupported incoming webhook schema_ref: {other}"
        ))),
    }
}

fn normalize_token(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn normalize_schema_ref(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn hash_hmac_secret(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())
}

fn validate_twilio_voice_payload(payload: &serde_json::Value) -> Vec<String> {
    let Some(obj) = payload.as_object() else {
        return vec!["payload must be a JSON object".to_string()];
    };
    let mut errors = Vec::new();
    require_string(obj, "CallSid", &mut errors);

    let has_call_status = obj
        .get("CallStatus")
        .and_then(|v| v.as_str())
        .is_some_and(|v| !v.trim().is_empty());
    let recording_completed = obj
        .get("RecordingStatus")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("completed"));

    if !has_call_status && !recording_completed {
        errors.push("payload must include CallStatus or RecordingStatus=completed".to_string());
    }
    if recording_completed {
        require_string(obj, "RecordingUrl", &mut errors);
    }

    errors
}

fn validate_twilio_media_stream_payload(payload: &serde_json::Value) -> Vec<String> {
    let Some(obj) = payload.as_object() else {
        return vec!["payload must be a JSON object".to_string()];
    };
    let mut errors = Vec::new();
    require_string(obj, "event", &mut errors);
    require_string(obj, "sequenceNumber", &mut errors);

    match obj.get("event").and_then(|v| v.as_str()) {
        Some("start") if !obj.get("start").is_some_and(|v| v.is_object()) => {
            errors.push("start event must include start object".to_string());
        }
        Some("start") => {}
        Some("media") => match obj.get("media") {
            Some(media) if media.is_object() => {
                if let Some(media_obj) = media.as_object() {
                    require_string(media_obj, "payload", &mut errors);
                }
            }
            _ => errors.push("media event must include media object".to_string()),
        },
        Some("stop" | "mark" | "dtmf") => {}
        Some(other) => errors.push(format!("unsupported Twilio Media Streams event: {other}")),
        None => {}
    }

    errors
}

fn require_string(
    obj: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    errors: &mut Vec<String>,
) {
    if obj
        .get(field)
        .and_then(|v| v.as_str())
        .is_none_or(|v| v.trim().is_empty())
    {
        errors.push(format!("payload must include non-empty {field}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_twilio_voice_recording_completed() {
        let response = validate_incoming_webhook_payload(
            "twilio.voice.v1",
            &serde_json::json!({
                "CallSid": "CA123",
                "RecordingStatus": "completed",
                "RecordingUrl": "https://api.twilio.com/recording.wav"
            }),
        )
        .unwrap();

        assert!(response.valid);
        assert!(response.errors.is_empty());
    }

    #[test]
    fn rejects_twilio_voice_without_status() {
        let response = validate_incoming_webhook_payload(
            "twilio.voice.v1",
            &serde_json::json!({"CallSid": "CA123"}),
        )
        .unwrap();

        assert!(!response.valid);
        assert_eq!(
            response.errors,
            vec!["payload must include CallStatus or RecordingStatus=completed"]
        );
    }

    #[test]
    fn validates_twilio_media_payload_shape() {
        let response = validate_incoming_webhook_payload(
            "twilio.media-stream.v1",
            &serde_json::json!({
                "event": "media",
                "sequenceNumber": "7",
                "media": {"payload": "AQIDBA=="}
            }),
        )
        .unwrap();

        assert!(response.valid);
    }

    #[test]
    fn rejects_unsupported_schema_ref() {
        let err = validate_incoming_webhook_payload("unknown.v1", &serde_json::json!({}))
            .expect_err("schema must be rejected");
        assert!(matches!(err, Error::InvalidInput(_)));
    }
}

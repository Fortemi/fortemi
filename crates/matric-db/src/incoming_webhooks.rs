//! Incoming webhook receiver registration and schema-shape validation.
//!
//! Validation is driven by JSON Schema (#821). A receiver either carries its
//! own `schema_doc` (an arbitrary JSON Schema document) or names a built-in
//! schema via `schema_ref` (e.g. `twilio.voice.v1`). All payloads — built-in
//! or custom — are validated through the `jsonschema` crate, so there is a
//! single validation path. Built-in schemas are embedded JSON Schema documents
//! (converted from the previous hand-coded validators), pre-registered in
//! [`built_in_schema`].

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    CreateIncomingWebhookReceiverRequest, Error, IncomingWebhookReceiver,
    IncomingWebhookValidationResponse, Result, UpdateIncomingWebhookReceiverRequest,
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
        let schema_ref = normalize_schema_ref(&req.schema_ref);
        validate_schema_ref_shape(&schema_ref)?;
        // A receiver must be validatable at registration: either it carries a
        // compilable custom schema, or its schema_ref names a built-in.
        ensure_validatable(&schema_ref, req.schema_doc.as_ref())?;

        let id = matric_core::new_v7();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO incoming_webhook_receiver
                (id, slug, provider, schema_ref, schema_doc, hmac_secret, signature_header, is_active, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(id)
        .bind(normalize_token(&req.slug))
        .bind(normalize_token(&req.provider))
        .bind(&schema_ref)
        .bind(req.schema_doc.as_ref())
        .bind(req.hmac_secret.trim())
        .bind(req.signature_header.trim())
        .bind(req.is_active)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(id)
    }

    /// Update a receiver in place (#821 PATCH). Only the provided fields change;
    /// slug, provider, and HMAC secret are preserved. Returns `true` if a row
    /// was updated, `false` if no receiver matched the slug.
    pub async fn update_by_slug(
        &self,
        slug: &str,
        req: UpdateIncomingWebhookReceiverRequest,
    ) -> Result<bool> {
        let Some(current) = self.get_by_slug(slug).await? else {
            return Ok(false);
        };

        let schema_ref = match req.schema_ref {
            Some(ref s) => normalize_schema_ref(s),
            None => current.schema_ref,
        };
        validate_schema_ref_shape(&schema_ref)?;

        // Present overrides the stored schema; absent keeps it.
        let schema_doc = match req.schema_doc {
            Some(doc) => Some(doc),
            None => current.schema_doc,
        };
        ensure_validatable(&schema_ref, schema_doc.as_ref())?;

        let signature_header = match req.signature_header {
            Some(ref h) if h.trim().is_empty() => {
                return Err(Error::InvalidInput(
                    "incoming webhook signature_header cannot be empty".to_string(),
                ));
            }
            Some(h) => h.trim().to_string(),
            None => current.signature_header,
        };
        let is_active = req.is_active.unwrap_or(current.is_active);

        let result = sqlx::query(
            "UPDATE incoming_webhook_receiver
                SET schema_ref = $2, schema_doc = $3, signature_header = $4,
                    is_active = $5, updated_at = $6
             WHERE slug = $1",
        )
        .bind(normalize_token(slug))
        .bind(&schema_ref)
        .bind(schema_doc.as_ref())
        .bind(&signature_header)
        .bind(is_active)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list(&self) -> Result<Vec<IncomingWebhookReceiver>> {
        let rows = sqlx::query(
            "SELECT id, slug, provider, schema_ref, schema_doc, signature_header, hmac_secret, is_active, created_at, updated_at
             FROM incoming_webhook_receiver ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| Self::parse_row(&r)).collect())
    }

    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<IncomingWebhookReceiver>> {
        let row = sqlx::query(
            "SELECT id, slug, provider, schema_ref, schema_doc, signature_header, hmac_secret, is_active, created_at, updated_at
             FROM incoming_webhook_receiver WHERE slug = $1",
        )
        .bind(normalize_token(slug))
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.as_ref().map(Self::parse_row))
    }

    pub async fn get_active_secret_by_slug(&self, slug: &str) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT hmac_secret FROM incoming_webhook_receiver WHERE slug = $1 AND is_active = true",
        )
        .bind(normalize_token(slug))
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| r.get("hmac_secret")))
    }

    /// Delete a receiver registration by slug (#819).
    ///
    /// Returns `true` if a row was removed, `false` if no receiver matched the
    /// slug. The delete is idempotent — calling it twice yields `false` on the
    /// second call rather than erroring.
    pub async fn delete_by_slug(&self, slug: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM incoming_webhook_receiver WHERE slug = $1")
            .bind(normalize_token(slug))
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    fn parse_row(r: &sqlx::postgres::PgRow) -> IncomingWebhookReceiver {
        let secret: String = r.get("hmac_secret");
        IncomingWebhookReceiver {
            id: r.get("id"),
            slug: r.get("slug"),
            provider: r.get("provider"),
            schema_ref: r.get("schema_ref"),
            signature_header: r.get("signature_header"),
            secret_set: !secret.is_empty(),
            is_active: r.get("is_active"),
            schema_doc: r.get("schema_doc"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

/// Validate a payload against a receiver's schema.
///
/// Resolution order: a custom `schema_doc` (if present) wins; otherwise the
/// built-in schema named by `schema_ref` is used. Returns metadata-only
/// validation diagnostics on failure.
pub fn validate_incoming_webhook_payload(
    schema_ref: &str,
    schema_doc: Option<&Value>,
    payload: &Value,
) -> Result<IncomingWebhookValidationResponse> {
    let schema_ref = normalize_schema_ref(schema_ref);
    let schema = resolve_schema(&schema_ref, schema_doc)?;

    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| incoming_webhook_schema_error("schema_ref_compile", schema_ref.as_str(), e))?;

    let errors: Vec<String> = validator
        .iter_errors(payload)
        .map(|err| format_validation_error(&err))
        .collect();

    Ok(IncomingWebhookValidationResponse {
        valid: errors.is_empty(),
        schema_ref,
        errors,
    })
}

/// Render a single `jsonschema` error as metadata. Validator messages can echo
/// JSON pointers, payload values, and schema property names, so this boundary
/// exposes only stable classes and lengths.
fn format_validation_error(err: &jsonschema::ValidationError<'_>) -> String {
    let path = err.instance_path().to_string();
    let diagnostic = err.to_string();
    format!(
        "incoming webhook payload validation failed; path_len={}; diagnostic_class={}; diagnostic_len={}",
        incoming_webhook_text_len(&path),
        incoming_webhook_diagnostic_class(&diagnostic),
        incoming_webhook_text_len(&diagnostic)
    )
}

/// Resolve the effective schema document for a receiver: custom doc if present,
/// else the built-in named by `schema_ref`.
fn resolve_schema(schema_ref: &str, schema_doc: Option<&Value>) -> Result<Value> {
    if let Some(doc) = schema_doc {
        return Ok(doc.clone());
    }
    built_in_schema(schema_ref).ok_or_else(|| incoming_webhook_unsupported_schema_ref(schema_ref))
}

/// Ensure a (schema_ref, schema_doc) pair is usable: a custom doc must compile
/// as a JSON Schema; otherwise schema_ref must name a built-in.
fn ensure_validatable(schema_ref: &str, schema_doc: Option<&Value>) -> Result<()> {
    match schema_doc {
        Some(doc) => {
            jsonschema::validator_for(doc)
                .map_err(|e| incoming_webhook_schema_error("schema_doc_compile", schema_ref, e))?;
            Ok(())
        }
        None if built_in_schema(schema_ref).is_some() => Ok(()),
        None => Err(incoming_webhook_unsupported_schema_ref(schema_ref)),
    }
}

fn incoming_webhook_unsupported_schema_ref(schema_ref: &str) -> Error {
    Error::InvalidInput(format!(
        "unsupported incoming webhook schema_ref; schema_ref_len={}",
        incoming_webhook_text_len(schema_ref)
    ))
}

fn incoming_webhook_schema_error(
    context: &'static str,
    schema_ref: &str,
    err: impl std::fmt::Display,
) -> Error {
    let diagnostic = err.to_string();
    Error::InvalidInput(format!(
        "invalid incoming webhook JSON schema; context={context}; schema_ref_len={}; diagnostic_class={}; diagnostic_len={}",
        incoming_webhook_text_len(schema_ref),
        incoming_webhook_diagnostic_class(&diagnostic),
        incoming_webhook_text_len(&diagnostic)
    ))
}

fn incoming_webhook_text_len(value: &str) -> usize {
    value.chars().count()
}

fn incoming_webhook_diagnostic_class(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if value.is_empty() {
        "empty"
    } else if value.chars().any(char::is_control) {
        "control_chars"
    } else if lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("sk-")
    {
        "secret_candidate"
    } else if lower.contains("://") || lower.starts_with("http") {
        "url_like"
    } else if value.contains('/') || value.contains('\\') {
        "path_like"
    } else {
        "text"
    }
}

/// Built-in JSON Schema documents, keyed by `schema_ref`. These replace the
/// previous hand-coded Twilio validators with equivalent JSON Schema (#821).
fn built_in_schema(schema_ref: &str) -> Option<Value> {
    match schema_ref {
        "twilio.voice.v1" => Some(twilio_voice_schema()),
        "twilio.media-stream.v1" => Some(twilio_media_stream_schema()),
        _ => None,
    }
}

/// `twilio.voice.v1`: requires a non-empty `CallSid`, and either a non-empty
/// `CallStatus` or `RecordingStatus == "completed"` (case-insensitive). When
/// the recording is completed, `RecordingUrl` is required.
fn twilio_voice_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["CallSid"],
        "properties": {
            "CallSid": { "type": "string", "minLength": 1 },
            "CallStatus": { "type": "string" },
            "RecordingStatus": { "type": "string" },
            "RecordingUrl": { "type": "string", "minLength": 1 }
        },
        "anyOf": [
            {
                "required": ["CallStatus"],
                "properties": { "CallStatus": { "type": "string", "minLength": 1 } }
            },
            {
                "required": ["RecordingStatus"],
                "properties": { "RecordingStatus": { "type": "string", "pattern": "(?i)^completed$" } }
            }
        ],
        "if": {
            "required": ["RecordingStatus"],
            "properties": { "RecordingStatus": { "type": "string", "pattern": "(?i)^completed$" } }
        },
        "then": { "required": ["RecordingUrl"] }
    })
}

/// `twilio.media-stream.v1`: requires non-empty `event` (one of the known
/// events) and `sequenceNumber`. `start` events require a `start` object;
/// `media` events require a `media` object carrying a non-empty `payload`.
fn twilio_media_stream_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["event", "sequenceNumber"],
        "properties": {
            "event": {
                "type": "string",
                "minLength": 1,
                "enum": ["start", "media", "stop", "mark", "dtmf"]
            },
            "sequenceNumber": { "type": "string", "minLength": 1 },
            "start": { "type": "object" },
            "media": {
                "type": "object",
                "required": ["payload"],
                "properties": { "payload": { "type": "string", "minLength": 1 } }
            }
        },
        "allOf": [
            {
                "if": { "required": ["event"], "properties": { "event": { "const": "start" } } },
                "then": { "required": ["start"] }
            },
            {
                "if": { "required": ["event"], "properties": { "event": { "const": "media" } } },
                "then": { "required": ["media"] }
            }
        ]
    })
}

fn validate_receiver_request(req: &CreateIncomingWebhookReceiverRequest) -> Result<()> {
    validate_slug(&req.slug)?;
    validate_provider(&req.provider)?;
    if req.signature_header.trim().is_empty() {
        return Err(Error::InvalidInput(
            "incoming webhook signature_header is required".to_string(),
        ));
    }
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

/// Shape check for `schema_ref` (the label/version tag). Matches the DB CHECK
/// `incoming_webhook_receiver_schema_ref_shape`.
fn validate_schema_ref_shape(schema_ref: &str) -> Result<()> {
    let s = schema_ref.trim();
    if s.is_empty() || s.len() > 128 {
        return Err(Error::InvalidInput(
            "incoming webhook schema_ref must be 1-128 characters".to_string(),
        ));
    }
    if !s.bytes().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
    }) {
        return Err(Error::InvalidInput(
            "incoming webhook schema_ref must use lowercase letters, numbers, '.', '-' or '_'"
                .to_string(),
        ));
    }
    Ok(())
}

fn normalize_token(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn normalize_schema_ref(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(schema_ref: &str, payload: Value) -> IncomingWebhookValidationResponse {
        validate_incoming_webhook_payload(schema_ref, None, &payload).unwrap()
    }

    #[test]
    fn validates_twilio_voice_recording_completed() {
        let response = validate(
            "twilio.voice.v1",
            json!({
                "CallSid": "CA123",
                "RecordingStatus": "completed",
                "RecordingUrl": "https://api.twilio.com/recording.wav"
            }),
        );
        assert!(response.valid, "errors: {:?}", response.errors);
        assert!(response.errors.is_empty());
    }

    #[test]
    fn validates_twilio_voice_with_call_status() {
        let response = validate(
            "twilio.voice.v1",
            json!({ "CallSid": "CA123", "CallStatus": "ringing" }),
        );
        assert!(response.valid, "errors: {:?}", response.errors);
    }

    #[test]
    fn rejects_twilio_voice_without_status() {
        // Only CallSid present: fails the anyOf (neither CallStatus nor a
        // completed RecordingStatus).
        let response = validate("twilio.voice.v1", json!({ "CallSid": "CA123" }));
        assert!(!response.valid);
        assert!(!response.errors.is_empty());
    }

    #[test]
    fn rejects_twilio_voice_missing_call_sid() {
        let response = validate("twilio.voice.v1", json!({ "CallStatus": "ringing" }));
        assert!(!response.valid);
        assert!(
            response
                .errors
                .iter()
                .any(|e| e.contains("payload validation failed")),
            "expected a redacted validation error, got: {:?}",
            response.errors
        );
        assert!(!response.errors.iter().any(|e| e.contains("CallSid")));
    }

    #[test]
    fn rejects_twilio_voice_completed_without_url() {
        let response = validate(
            "twilio.voice.v1",
            json!({ "CallSid": "CA123", "RecordingStatus": "completed" }),
        );
        assert!(!response.valid);
        assert!(
            response
                .errors
                .iter()
                .any(|e| e.contains("payload validation failed")),
            "expected a redacted validation error, got: {:?}",
            response.errors
        );
        assert!(!response.errors.iter().any(|e| e.contains("RecordingUrl")));
    }

    #[test]
    fn validates_twilio_media_payload_shape() {
        let response = validate(
            "twilio.media-stream.v1",
            json!({
                "event": "media",
                "sequenceNumber": "7",
                "media": { "payload": "AQIDBA==" }
            }),
        );
        assert!(response.valid, "errors: {:?}", response.errors);
    }

    #[test]
    fn rejects_twilio_media_unknown_event() {
        let response = validate(
            "twilio.media-stream.v1",
            json!({ "event": "explode", "sequenceNumber": "1" }),
        );
        assert!(!response.valid);
        assert!(
            response
                .errors
                .iter()
                .any(|e| e.contains("payload validation failed")),
            "expected a redacted validation error, got: {:?}",
            response.errors
        );
        assert!(!response.errors.iter().any(|e| e.contains("explode")));
        assert!(!response.errors.iter().any(|e| e.contains("event")));
    }

    #[test]
    fn rejects_twilio_media_missing_sequence() {
        let response = validate("twilio.media-stream.v1", json!({ "event": "stop" }));
        assert!(!response.valid);
        assert!(
            response
                .errors
                .iter()
                .any(|e| e.contains("payload validation failed")),
            "expected a redacted validation error, got: {:?}",
            response.errors
        );
        assert!(!response.errors.iter().any(|e| e.contains("sequenceNumber")));
    }

    #[test]
    fn rejects_unsupported_schema_ref_without_doc() {
        let err = validate_incoming_webhook_payload("unknown.v1", None, &json!({}))
            .expect_err("schema must be rejected");
        let message = match err {
            Error::InvalidInput(message) => message,
            other => panic!("unexpected error: {other:?}"),
        };
        assert!(message.contains("unsupported incoming webhook schema_ref"));
        assert!(message.contains("schema_ref_len=10"));
        assert!(!message.contains("unknown.v1"));
    }

    #[test]
    fn validates_against_custom_schema_doc() {
        let schema = json!({
            "type": "object",
            "required": ["amount"],
            "properties": { "amount": { "type": "number" } }
        });
        let ok = validate_incoming_webhook_payload(
            "stripe.charge.v1",
            Some(&schema),
            &json!({ "amount": 42 }),
        )
        .unwrap();
        assert!(ok.valid, "errors: {:?}", ok.errors);

        let bad = validate_incoming_webhook_payload(
            "stripe.charge.v1",
            Some(&schema),
            &json!({ "amount": "not-a-number" }),
        )
        .unwrap();
        assert!(!bad.valid);
        assert!(
            bad.errors
                .iter()
                .any(|e| e.contains("payload validation failed")),
            "expected a redacted validation error, got: {:?}",
            bad.errors
        );
        assert!(!bad.errors.iter().any(|e| e.contains("amount")));
        assert!(!bad.errors.iter().any(|e| e.contains("not-a-number")));
    }

    #[test]
    fn custom_schema_doc_overrides_builtin_ref() {
        // Even with a built-in-looking schema_ref, a provided schema_doc wins.
        let schema = json!({ "type": "object", "required": ["x"] });
        let resp =
            validate_incoming_webhook_payload("twilio.voice.v1", Some(&schema), &json!({ "x": 1 }))
                .unwrap();
        assert!(resp.valid, "errors: {:?}", resp.errors);
    }

    #[test]
    fn ensure_validatable_rejects_bad_schema_doc() {
        // A schema that is not an object/array/bool is not a valid JSON Schema.
        let bad = json!("definitely not a schema");
        assert!(ensure_validatable("custom.v1", Some(&bad)).is_err());
    }

    #[test]
    fn schema_diagnostics_report_metadata_without_raw_values() {
        let diagnostic = "invalid token sk-live-secret at /tmp/schema/private.json";
        let error = incoming_webhook_schema_error(
            "schema_doc_compile",
            "tenant-secret.webhook.v1",
            diagnostic,
        );
        let message = match error {
            Error::InvalidInput(message) => message,
            other => panic!("unexpected error: {other:?}"),
        };

        assert!(message.contains("context=schema_doc_compile"));
        assert!(message.contains("schema_ref_len=24"));
        assert!(message.contains("diagnostic_class=secret_candidate"));
        assert!(message.contains(&format!("diagnostic_len={}", diagnostic.chars().count())));
        for raw in [
            "tenant-secret.webhook.v1",
            "sk-live-secret",
            "/tmp/schema/private.json",
            "token ",
        ] {
            assert!(!message.contains(raw), "raw diagnostic leaked: {raw}");
        }
    }

    #[test]
    fn payload_validation_errors_report_metadata_without_raw_values() {
        let schema = json!({
            "type": "object",
            "required": ["secretWebhookToken"],
            "properties": {
                "secretWebhookToken": { "type": "number" }
            }
        });

        let response = validate_incoming_webhook_payload(
            "tenant-secret.webhook.v1",
            Some(&schema),
            &json!({ "secretWebhookToken": "sk-live-secret" }),
        )
        .unwrap();

        assert!(!response.valid);
        let rendered = response.errors.join("\n");
        assert!(rendered.contains("payload validation failed"));
        assert!(rendered.contains("diagnostic_class=secret_candidate"));
        assert!(rendered.contains("path_len="));
        assert!(rendered.contains("diagnostic_len="));
        for raw in [
            "tenant-secret.webhook.v1",
            "secretWebhookToken",
            "sk-live-secret",
            "/secretWebhookToken",
        ] {
            assert!(
                !rendered.contains(raw),
                "raw validation detail leaked: {raw}"
            );
        }
    }

    #[test]
    fn schema_ref_shape_rejects_uppercase_and_spaces() {
        assert!(validate_schema_ref_shape("Twilio.Voice").is_err());
        assert!(validate_schema_ref_shape("has space").is_err());
        assert!(validate_schema_ref_shape("twilio.voice.v1").is_ok());
    }
}

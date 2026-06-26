//! PKE (Public Key Encryption) HTTP handlers.

use axum::{extract::Path, http::StatusCode, response::IntoResponse, Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use tracing::warn;

use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass, AuthPrincipal, TracingSink,
};
use matric_crypto::pke::{
    decrypt_pke, encrypt_pke, get_pke_recipients, key_storage, Address, Keypair, PrivateKey,
    PublicKey,
};

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PkeKeygenRequest {
    pub passphrase: String,
    pub label: Option<String>,
}

impl std::fmt::Debug for PkeKeygenRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeKeygenRequest")
            .field("passphrase_present", &!self.passphrase.is_empty())
            .field("passphrase_len", &self.passphrase.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .finish()
    }
}

#[derive(Serialize)]
pub struct PkeKeygenResponse {
    pub public_key: String,            // base64
    pub encrypted_private_key: String, // base64 (encrypted with passphrase)
    pub address: String,               // mm:... address
    pub label: Option<String>,
}

impl std::fmt::Debug for PkeKeygenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeKeygenResponse")
            .field("public_key_len", &self.public_key.chars().count())
            .field(
                "encrypted_private_key_len",
                &self.encrypted_private_key.chars().count(),
            )
            .field("address", &self.address)
            .field("label_len", &optional_text_len(&self.label))
            .finish()
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PkeAddressRequest {
    pub public_key: String, // base64 public key bytes
}

impl std::fmt::Debug for PkeAddressRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeAddressRequest")
            .field("public_key_len", &self.public_key.chars().count())
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct PkeAddressResponse {
    pub address: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PkeEncryptRequest {
    pub plaintext: String,       // base64 encoded
    pub recipients: Vec<String>, // mm:... addresses or base64 public keys
    pub original_filename: Option<String>,
}

impl std::fmt::Debug for PkeEncryptRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeEncryptRequest")
            .field("plaintext_len", &self.plaintext.chars().count())
            .field("recipient_count", &self.recipients.len())
            .field(
                "original_filename_len",
                &optional_text_len(&self.original_filename),
            )
            .finish()
    }
}

#[derive(Serialize)]
pub struct PkeEncryptResponse {
    pub ciphertext: String,      // base64 encoded MMPKE01 format
    pub recipients: Vec<String>, // mm:... addresses
}

impl std::fmt::Debug for PkeEncryptResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeEncryptResponse")
            .field("ciphertext_len", &self.ciphertext.chars().count())
            .field("recipient_count", &self.recipients.len())
            .finish()
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PkeDecryptRequest {
    pub ciphertext: String,            // base64 MMPKE01
    pub encrypted_private_key: String, // base64
    pub passphrase: String,
}

impl std::fmt::Debug for PkeDecryptRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeDecryptRequest")
            .field("ciphertext_len", &self.ciphertext.chars().count())
            .field(
                "encrypted_private_key_len",
                &self.encrypted_private_key.chars().count(),
            )
            .field("passphrase_present", &!self.passphrase.is_empty())
            .field("passphrase_len", &self.passphrase.chars().count())
            .finish()
    }
}

#[derive(Serialize)]
pub struct PkeDecryptResponse {
    pub plaintext: String, // base64
    pub original_filename: Option<String>,
}

impl std::fmt::Debug for PkeDecryptResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeDecryptResponse")
            .field("plaintext_len", &self.plaintext.chars().count())
            .field(
                "original_filename_len",
                &optional_text_len(&self.original_filename),
            )
            .finish()
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PkeRecipientsRequest {
    pub ciphertext: String, // base64 MMPKE01
}

impl std::fmt::Debug for PkeRecipientsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeRecipientsRequest")
            .field("ciphertext_len", &self.ciphertext.chars().count())
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct PkeRecipientsResponse {
    pub recipients: Vec<String>, // mm:... addresses
}

#[derive(Debug, Serialize)]
pub struct PkeVerifyResponse {
    pub address: String,
    pub valid: bool,
    pub version: Option<u8>,
}

fn optional_text_len(value: &Option<String>) -> Option<usize> {
    value.as_deref().map(|value| value.chars().count())
}

const PKE_KEY_GENERATION_FAILURE_DETAIL: &str =
    "PKE key generation failed. Check server logs for diagnostics.";
const PKE_ENCRYPTION_FAILURE_DETAIL: &str =
    "PKE encryption failed. Check server logs for diagnostics.";
const PKE_KEYSET_CREATION_FAILURE_DETAIL: &str =
    "PKE keyset creation failed. Check server logs for diagnostics.";
const PKE_KEYSET_AUDIT_EMIT_FAILURE_DETAIL: &str = "pke_keyset_audit_emit_failed";

fn invalid_pke_public_key_length() -> ApiError {
    ApiError::BadRequest("Public key must be 32 bytes.".to_string())
}

fn invalid_pke_recipient_public_key_length() -> ApiError {
    ApiError::BadRequest("Recipient public key must be 32 bytes.".to_string())
}

// =============================================================================
// HANDLERS
// =============================================================================

/// Generate a new PKE keypair.
///
/// POST /api/v1/pke/keygen
#[utoipa::path(post, path = "/api/v1/pke/keygen", tag = "PKE",
    request_body = PkeKeygenRequest,
    responses((status = 201, description = "Created")))]
pub async fn pke_keygen(Json(req): Json<PkeKeygenRequest>) -> Result<impl IntoResponse, ApiError> {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Encode public key as base64
    let public_key_b64 = BASE64.encode(keypair.public.as_bytes());

    // Encrypt private key with passphrase
    let encrypted_private =
        key_storage::encrypt_private_key(keypair.private.as_bytes(), &req.passphrase).map_err(
            |e| {
                let diagnostic = e.to_string();
                warn!(
                    error_len = diagnostic.chars().count(),
                    "PKE key generation failed"
                );
                ApiError::OperationFailed {
                    operation: "PKE key generation",
                    detail: PKE_KEY_GENERATION_FAILURE_DETAIL.to_string(),
                }
            },
        )?;
    let encrypted_private_b64 = BASE64.encode(&encrypted_private);

    Ok((
        StatusCode::CREATED,
        Json(PkeKeygenResponse {
            public_key: public_key_b64,
            encrypted_private_key: encrypted_private_b64,
            address: address.to_string(),
            label: req.label,
        }),
    ))
}

/// Compute the address for a public key.
///
/// POST /api/v1/pke/address
#[utoipa::path(post, path = "/api/v1/pke/address", tag = "PKE",
    request_body = PkeAddressRequest,
    responses((status = 200, description = "Success")))]
pub async fn pke_address(
    Json(req): Json<PkeAddressRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let public_key_bytes = BASE64
        .decode(&req.public_key)
        .map_err(|_| ApiError::BadRequest("Invalid base64 for public_key.".to_string()))?;

    if public_key_bytes.len() != 32 {
        return Err(invalid_pke_public_key_length());
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&public_key_bytes);

    let public_key = PublicKey::from_bytes(arr);
    let address = public_key.to_address();

    Ok(Json(PkeAddressResponse {
        address: address.to_string(),
    }))
}

/// Encrypt data for multiple recipients.
///
/// POST /api/v1/pke/encrypt
#[utoipa::path(post, path = "/api/v1/pke/encrypt", tag = "PKE",
    request_body = PkeEncryptRequest,
    responses((status = 200, description = "Success")))]
pub async fn pke_encrypt(
    Json(req): Json<PkeEncryptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Size limit: 10MB
    let plaintext = BASE64
        .decode(&req.plaintext)
        .map_err(|_| ApiError::BadRequest("Invalid base64 for plaintext.".to_string()))?;

    if plaintext.len() > 10 * 1024 * 1024 {
        return Err(ApiError::BadRequest(
            "Plaintext exceeds 10MB limit.".to_string(),
        ));
    }

    // Parse recipient public keys (base64 encoded)
    // For now, we only support base64 public keys (not mm: addresses, which require lookup)
    let mut recipient_keys = Vec::new();
    let mut recipient_addresses = Vec::new();

    for r in &req.recipients {
        if r.starts_with("mm:") {
            return Err(ApiError::BadRequest(
                "Pass base64 public keys directly. Address-based lookup requires a key server."
                    .to_string(),
            ));
        }
        let key_bytes = BASE64
            .decode(r)
            .map_err(|_| ApiError::BadRequest("Invalid recipient public key.".to_string()))?;

        if key_bytes.len() != 32 {
            return Err(invalid_pke_recipient_public_key_length());
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);

        let pk = PublicKey::from_bytes(arr);
        recipient_addresses.push(pk.to_address().to_string());
        recipient_keys.push(pk);
    }

    let ciphertext =
        encrypt_pke(&plaintext, &recipient_keys, req.original_filename).map_err(|e| {
            let diagnostic = e.to_string();
            warn!(
                error_len = diagnostic.chars().count(),
                "PKE encryption failed"
            );
            ApiError::OperationFailed {
                operation: "PKE encryption",
                detail: PKE_ENCRYPTION_FAILURE_DETAIL.to_string(),
            }
        })?;

    Ok(Json(PkeEncryptResponse {
        ciphertext: BASE64.encode(&ciphertext),
        recipients: recipient_addresses,
    }))
}

/// Decrypt data with a private key.
///
/// POST /api/v1/pke/decrypt
#[utoipa::path(post, path = "/api/v1/pke/decrypt", tag = "PKE",
    request_body = PkeDecryptRequest,
    responses((status = 200, description = "Success")))]
pub async fn pke_decrypt(
    Json(req): Json<PkeDecryptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ciphertext = BASE64
        .decode(&req.ciphertext)
        .map_err(|_| ApiError::BadRequest("Invalid ciphertext base64.".to_string()))?;

    let encrypted_key = BASE64
        .decode(&req.encrypted_private_key)
        .map_err(|_| ApiError::BadRequest("Invalid private key base64.".to_string()))?;

    // Decrypt private key with passphrase
    let private_key_bytes = key_storage::decrypt_private_key(&encrypted_key, &req.passphrase)
        .map_err(|_| {
            ApiError::Forbidden("Invalid passphrase or corrupted private key.".to_string())
        })?;

    let private_key = PrivateKey::from_bytes(private_key_bytes);

    let (plaintext, header) = decrypt_pke(&ciphertext, &private_key)
        .map_err(|_| ApiError::Forbidden("Unable to decrypt PKE payload.".to_string()))?;

    Ok(Json(PkeDecryptResponse {
        plaintext: BASE64.encode(&plaintext),
        original_filename: header.original_filename,
    }))
}

/// Get the list of recipients for encrypted data.
///
/// POST /api/v1/pke/recipients
#[utoipa::path(post, path = "/api/v1/pke/recipients", tag = "PKE",
    request_body = PkeRecipientsRequest,
    responses((status = 200, description = "Success")))]
pub async fn pke_recipients(
    Json(req): Json<PkeRecipientsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ciphertext = BASE64
        .decode(&req.ciphertext)
        .map_err(|_| ApiError::BadRequest("Invalid ciphertext base64.".to_string()))?;

    let recipients = get_pke_recipients(&ciphertext)
        .map_err(|_| ApiError::BadRequest("Not a valid PKE payload.".to_string()))?;

    Ok(Json(PkeRecipientsResponse {
        recipients: recipients.iter().map(|a| a.to_string()).collect(),
    }))
}

/// Verify and parse a PKE address.
///
/// GET /api/v1/pke/verify/:address
#[utoipa::path(get, path = "/api/v1/pke/verify/{address}", tag = "PKE",
    params(("address" = String, Path, description = "PKE address to verify")),
    responses((status = 200, description = "Success")))]
pub async fn pke_verify(Path(address): Path<String>) -> Json<PkeVerifyResponse> {
    match Address::parse(&address) {
        Ok(addr) => Json(PkeVerifyResponse {
            address: addr.to_string(),
            valid: true,
            version: Some(addr.version()),
        }),
        Err(_) => Json(PkeVerifyResponse {
            address,
            valid: false,
            version: None,
        }),
    }
}

// =============================================================================
// KEYSET MANAGEMENT HANDLERS (Issues #328, #332)
// =============================================================================

use crate::{ApiError, AppState, Auth};
use axum::extract::State;
use matric_db::{CreateKeysetRequest, ExportedKeyset};
use uuid::Uuid;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateKeysetApiRequest {
    pub name: String,
    pub passphrase: String,
    pub label: Option<String>,
}

impl std::fmt::Debug for CreateKeysetApiRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateKeysetApiRequest")
            .field("name_len", &self.name.chars().count())
            .field("passphrase_present", &!self.passphrase.is_empty())
            .field("passphrase_len", &self.passphrase.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct KeysetResponse {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ActiveKeysetResponse {
    pub active: bool,
    pub keyset: Option<KeysetResponse>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImportKeysetRequest {
    pub name: String,
    pub exported: ExportedKeyset,
}

impl std::fmt::Debug for ImportKeysetRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportKeysetRequest")
            .field("name_len", &self.name.chars().count())
            .field("exported", &self.exported)
            .finish()
    }
}

/// List all PKE keysets.
///
/// GET /api/v1/pke/keysets
#[utoipa::path(get, path = "/api/v1/pke/keysets", tag = "PKE",
    responses((status = 200, description = "Success")))]
pub async fn list_keysets(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let keysets = state.db.pke_keysets.list().await.map_err(ApiError::from)?;

    let response: Vec<KeysetResponse> = keysets
        .into_iter()
        .map(|k| KeysetResponse {
            id: k.id,
            name: k.name,
            address: k.address,
            label: k.label,
            is_active: k.is_active,
            created_at: k.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Create a new PKE keyset.
///
/// POST /api/v1/pke/keysets
#[utoipa::path(post, path = "/api/v1/pke/keysets", tag = "PKE",
    request_body = CreateKeysetApiRequest,
    responses((status = 201, description = "Created")))]
pub async fn create_keyset(
    auth: Auth,
    State(state): State<AppState>,
    Json(req): Json<CreateKeysetApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_crypto::pke::{key_storage, Keypair};

    // Generate new keypair
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Encode public key
    let public_key = keypair.public.as_bytes().to_vec();

    // Encrypt private key with passphrase
    let encrypted_private_key =
        key_storage::encrypt_private_key(keypair.private.as_bytes(), &req.passphrase).map_err(
            |e| {
                let diagnostic = e.to_string();
                warn!(
                    error_len = diagnostic.chars().count(),
                    "PKE keyset creation failed"
                );
                ApiError::OperationFailed {
                    operation: "PKE keyset creation",
                    detail: PKE_KEYSET_CREATION_FAILURE_DETAIL.to_string(),
                }
            },
        )?;

    // Store in database
    let keyset = state
        .db
        .pke_keysets
        .create(CreateKeysetRequest {
            name: req.name,
            public_key,
            encrypted_private_key,
            address: address.to_string(),
            label: req.label,
        })
        .await
        .map_err(|e| match &e {
            matric_core::Error::InvalidInput(_) => {
                ApiError::Conflict("PKE keyset already exists.".to_string())
            }
            _ => ApiError::from(e),
        })?;

    emit_pke_keyset_audit_event(pke_keyset_audit_event(
        &auth,
        "keyset_create",
        AuditOutcome::Success,
        keyset.id,
        &keyset.name,
        &keyset.address,
        None,
    ))
    .await;

    Ok((
        StatusCode::CREATED,
        Json(KeysetResponse {
            id: keyset.id,
            name: keyset.name,
            address: keyset.address,
            label: keyset.label,
            is_active: false,
            created_at: keyset.created_at,
        }),
    ))
}

/// Get the active PKE keyset.
///
/// GET /api/v1/pke/keysets/active
#[utoipa::path(get, path = "/api/v1/pke/keysets/active", tag = "PKE",
    responses((status = 200, description = "Success")))]
pub async fn get_active_keyset(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let keyset = state
        .db
        .pke_keysets
        .get_active()
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ActiveKeysetResponse {
        active: keyset.is_some(),
        keyset: keyset.map(|k| KeysetResponse {
            id: k.id,
            name: k.name.clone(),
            address: k.address,
            label: k.label,
            is_active: true,
            created_at: k.created_at,
        }),
    }))
}

/// Set the active PKE keyset by name or ID.
///
/// PUT /api/v1/pke/keysets/:name_or_id/active
#[utoipa::path(put, path = "/api/v1/pke/keysets/{name_or_id}/active", tag = "PKE",
    params(("name_or_id" = String, Path, description = "Keyset name or UUID")),
    responses((status = 200, description = "Success")))]
pub async fn set_active_keyset(
    auth: Auth,
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Try to parse as UUID first, otherwise treat as name
    let keyset = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.get_by_id(uuid).await
    } else {
        state.db.pke_keysets.get_by_name(&name_or_id).await
    }
    .map_err(ApiError::from)?;

    let keyset = keyset.ok_or_else(|| ApiError::NotFound("PKE keyset not found.".to_string()))?;

    state
        .db
        .pke_keysets
        .set_active(keyset.id)
        .await
        .map_err(ApiError::from)?;

    emit_pke_keyset_audit_event(pke_keyset_audit_event(
        &auth,
        "keyset_activate",
        AuditOutcome::Success,
        keyset.id,
        &keyset.name,
        &keyset.address,
        None,
    ))
    .await;

    Ok(Json(serde_json::json!({
        "success": true,
        "active_keyset": keyset.name
    })))
}

/// Delete a PKE keyset by name or ID.
///
/// DELETE /api/v1/pke/keysets/:name_or_id
#[utoipa::path(delete, path = "/api/v1/pke/keysets/{name_or_id}", tag = "PKE",
    params(("name_or_id" = String, Path, description = "Keyset name or UUID")),
    responses((status = 200, description = "Success")))]
pub async fn delete_keyset(
    auth: Auth,
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Try to parse as UUID first, otherwise treat as name
    let keyset = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.get_by_id(uuid).await
    } else {
        state.db.pke_keysets.get_by_name(&name_or_id).await
    }
    .map_err(ApiError::from)?;

    let keyset = keyset.ok_or_else(|| ApiError::NotFound("PKE keyset not found.".to_string()))?;

    state
        .db
        .pke_keysets
        .delete(keyset.id)
        .await
        .map_err(ApiError::from)?;

    emit_pke_keyset_audit_event(pke_keyset_audit_event(
        &auth,
        "keyset_delete",
        AuditOutcome::Success,
        keyset.id,
        &keyset.name,
        &keyset.address,
        None,
    ))
    .await;

    Ok(Json(serde_json::json!({
        "success": true,
        "deleted": name_or_id
    })))
}

/// Export a PKE keyset by name or ID.
///
/// GET /api/v1/pke/keysets/:name_or_id/export
#[utoipa::path(get, path = "/api/v1/pke/keysets/{name_or_id}/export", tag = "PKE",
    params(("name_or_id" = String, Path, description = "Keyset name or UUID")),
    responses((status = 200, description = "Success")))]
pub async fn export_keyset(
    auth: Auth,
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Try to parse as UUID first, otherwise treat as name
    let keyset = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.get_by_id(uuid).await
    } else {
        state.db.pke_keysets.get_by_name(&name_or_id).await
    }
    .map_err(ApiError::from)?;

    let keyset = keyset.ok_or_else(|| ApiError::NotFound("PKE keyset not found.".to_string()))?;

    let exported = state
        .db
        .pke_keysets
        .export(keyset.id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("PKE keyset not found.".to_string()))?;

    emit_pke_keyset_audit_event(pke_keyset_audit_event(
        &auth,
        "keyset_export",
        AuditOutcome::Success,
        keyset.id,
        &keyset.name,
        &keyset.address,
        None,
    ))
    .await;

    Ok(Json(exported))
}

/// Import a PKE keyset.
///
/// POST /api/v1/pke/keysets/import
#[utoipa::path(post, path = "/api/v1/pke/keysets/import", tag = "PKE",
    request_body = ImportKeysetRequest,
    responses((status = 201, description = "Created")))]
pub async fn import_keyset(
    auth: Auth,
    State(state): State<AppState>,
    Json(req): Json<ImportKeysetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let keyset = state
        .db
        .pke_keysets
        .import(req.name, req.exported)
        .await
        .map_err(|e| match &e {
            matric_core::Error::InvalidInput(_) => {
                ApiError::BadRequest("Invalid PKE keyset import payload.".to_string())
            }
            _ => ApiError::from(e),
        })?;

    emit_pke_keyset_audit_event(pke_keyset_audit_event(
        &auth,
        "keyset_import",
        AuditOutcome::Success,
        keyset.id,
        &keyset.name,
        &keyset.address,
        None,
    ))
    .await;

    Ok((
        StatusCode::CREATED,
        Json(KeysetResponse {
            id: keyset.id,
            name: keyset.name,
            address: keyset.address,
            label: keyset.label,
            is_active: false,
            created_at: keyset.created_at,
        }),
    ))
}

async fn emit_pke_keyset_audit_event(event: AuditEvent) {
    if let Err(err) = TracingSink.emit(event).await {
        let diagnostic = err.to_string();
        warn!(
            error_len = diagnostic.chars().count(),
            detail = PKE_KEYSET_AUDIT_EMIT_FAILURE_DETAIL,
            "failed to emit PKE keyset audit event"
        );
    }
}

fn pke_keyset_audit_event(
    auth: &Auth,
    action: &str,
    outcome: AuditOutcome,
    keyset_id: Uuid,
    keyset_name: &str,
    keyset_address: &str,
    reason: Option<&str>,
) -> AuditEvent {
    let mut event = AuditEvent::new("pke", action, outcome)
        .with_principal(pke_principal_audit_id(&auth.principal))
        .with_resource("pke_keyset", keyset_id.to_string())
        .with_attr("keyset_name", keyset_name.to_string())
        .with_attr("keyset_address", keyset_address.to_string());

    if let Some(reason) = reason {
        event.reason = Some(reason.to_string());
        event = event.with_attr("reason_code", reason.to_string());
    }

    event.source = AuditSource::Api;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.failure_policy = AuditFailurePolicy::BestEffort;
    event.severity = match outcome {
        AuditOutcome::Success => AuditSeverity::Info,
        AuditOutcome::Denied => AuditSeverity::Warn,
        AuditOutcome::Failure | AuditOutcome::Error => AuditSeverity::Error,
        AuditOutcome::Unknown => AuditSeverity::Warn,
    };
    event.sanitized()
}

fn pke_principal_audit_id(principal: &AuthPrincipal) -> String {
    match principal {
        AuthPrincipal::OAuthClient {
            client_id, user_id, ..
        } => user_id
            .as_ref()
            .map(|user_id| format!("oauth_user:{user_id}"))
            .unwrap_or_else(|| format!("oauth_client:{client_id}")),
        AuthPrincipal::ApiKey { key_id, .. } => format!("api_key:{key_id}"),
        AuthPrincipal::Anonymous => "anonymous".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_problem_response(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem = serde_json::from_slice(&body).unwrap();
        (status, problem)
    }

    #[tokio::test]
    async fn pke_public_key_length_validation_does_not_echo_lengths() {
        let submitted_public_key_len = 17;
        let err = invalid_pke_public_key_length();
        let (status, problem) = read_problem_response(err).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/validation-error"
        );
        assert_eq!(problem["detail"], "Public key must be 32 bytes.");

        let body = problem.to_string();
        assert!(!body.contains(&submitted_public_key_len.to_string()));
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());

        let submitted_recipient_key_len = 48;
        let err = invalid_pke_recipient_public_key_length();
        let (status, problem) = read_problem_response(err).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/validation-error"
        );
        assert_eq!(problem["detail"], "Recipient public key must be 32 bytes.");

        let body = problem.to_string();
        assert!(!body.contains(&submitted_recipient_key_len.to_string()));
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
    }

    #[test]
    fn pke_operation_failure_details_are_fixed_and_redacted() {
        let raw_diagnostics = [
            "postgres://fortemi:secret@db.internal/fortemi",
            "Bearer pke-token-secret",
            "/srv/fortemi/private/keyset.bin",
            "crypto backend failed: passphrase=top-secret",
        ];
        let details = [
            PKE_KEY_GENERATION_FAILURE_DETAIL,
            PKE_ENCRYPTION_FAILURE_DETAIL,
            PKE_KEYSET_CREATION_FAILURE_DETAIL,
            PKE_KEYSET_AUDIT_EMIT_FAILURE_DETAIL,
        ];

        assert_eq!(
            PKE_KEY_GENERATION_FAILURE_DETAIL,
            "PKE key generation failed. Check server logs for diagnostics."
        );
        assert_eq!(
            PKE_ENCRYPTION_FAILURE_DETAIL,
            "PKE encryption failed. Check server logs for diagnostics."
        );
        assert_eq!(
            PKE_KEYSET_CREATION_FAILURE_DETAIL,
            "PKE keyset creation failed. Check server logs for diagnostics."
        );
        assert_eq!(
            PKE_KEYSET_AUDIT_EMIT_FAILURE_DETAIL,
            "pke_keyset_audit_emit_failed"
        );

        for detail in details {
            for raw in raw_diagnostics {
                assert!(!detail.contains(raw));
            }
            assert!(!detail.contains("postgres://"));
            assert!(!detail.contains("Bearer "));
            assert!(!detail.contains("/srv/"));
            assert!(!detail.contains("passphrase="));
        }
    }

    #[test]
    fn pke_http_debug_redacts_secret_material() {
        let keygen = PkeKeygenRequest {
            passphrase: "secret-passphrase".to_string(),
            label: Some("private label".to_string()),
        };
        let keygen_response = PkeKeygenResponse {
            public_key: "public-key-bytes".to_string(),
            encrypted_private_key: "encrypted-private-key-secret".to_string(),
            address: "mm:example-address".to_string(),
            label: Some("private label".to_string()),
        };
        let address = PkeAddressRequest {
            public_key: "public-key-bytes".to_string(),
        };
        let encrypt = PkeEncryptRequest {
            plaintext: "secret-plaintext-base64".to_string(),
            recipients: vec!["recipient-public-key-secret".to_string()],
            original_filename: Some("patient-secret.pdf".to_string()),
        };
        let encrypt_response = PkeEncryptResponse {
            ciphertext: "secret-ciphertext-base64".to_string(),
            recipients: vec!["mm:recipient-address".to_string()],
        };
        let decrypt = PkeDecryptRequest {
            ciphertext: "secret-ciphertext-base64".to_string(),
            encrypted_private_key: "encrypted-private-key-secret".to_string(),
            passphrase: "secret-passphrase".to_string(),
        };
        let decrypt_response = PkeDecryptResponse {
            plaintext: "secret-plaintext-base64".to_string(),
            original_filename: Some("patient-secret.pdf".to_string()),
        };
        let recipients = PkeRecipientsRequest {
            ciphertext: "secret-ciphertext-base64".to_string(),
        };
        let create_keyset = CreateKeysetApiRequest {
            name: "tenant-secret-keyset".to_string(),
            passphrase: "secret-passphrase".to_string(),
            label: Some("private label".to_string()),
        };
        let import_keyset = ImportKeysetRequest {
            name: "tenant-secret-keyset".to_string(),
            exported: ExportedKeyset {
                name: "export-secret-name".to_string(),
                public_key_base64: "public-key-bytes".to_string(),
                encrypted_private_key_base64: "encrypted-private-key-secret".to_string(),
                address: "mm:example-address".to_string(),
                label: Some("private label".to_string()),
                exported_at: chrono::Utc::now(),
            },
        };

        let rendered = format!(
            "{keygen:?}\n{keygen_response:?}\n{address:?}\n{encrypt:?}\n{encrypt_response:?}\n{decrypt:?}\n{decrypt_response:?}\n{recipients:?}\n{create_keyset:?}\n{import_keyset:?}"
        );

        assert!(rendered.contains("passphrase_present: true"));
        assert!(rendered.contains("encrypted_private_key_len"));
        assert!(rendered.contains("plaintext_len"));
        assert!(rendered.contains("ciphertext_len"));
        assert!(rendered.contains("recipient_count"));
        assert!(!rendered.contains("secret-passphrase"));
        assert!(!rendered.contains("encrypted-private-key-secret"));
        assert!(!rendered.contains("secret-plaintext-base64"));
        assert!(!rendered.contains("secret-ciphertext-base64"));
        assert!(!rendered.contains("recipient-public-key-secret"));
        assert!(!rendered.contains("patient-secret.pdf"));
        assert!(!rendered.contains("tenant-secret-keyset"));
        assert!(!rendered.contains("export-secret-name"));
        assert!(!rendered.contains("private label"));
        assert!(!rendered.contains("public-key-bytes"));
    }

    #[test]
    fn pke_keyset_audit_event_uses_metadata_only() {
        let auth = Auth {
            principal: AuthPrincipal::ApiKey {
                key_id: Uuid::parse_str("018fd1a0-0000-7000-8000-000000000101").unwrap(),
                scope: "admin passphrase=should-not-appear".to_string(),
            },
        };

        let event = pke_keyset_audit_event(
            &auth,
            "keyset_export",
            AuditOutcome::Success,
            Uuid::parse_str("018fd1a0-0000-7000-8000-000000000102").unwrap(),
            "primary\nkeyset|name",
            "mm:example-address",
            Some("exported"),
        );

        assert_eq!(event.category, "pke");
        assert_eq!(event.action, "keyset_export");
        assert_eq!(event.outcome, AuditOutcome::Success);
        assert_eq!(event.source, AuditSource::Api);
        assert_eq!(event.visibility, AuditVisibilityClass::SecurityRestricted);
        assert_eq!(event.resource_kind.as_deref(), Some("pke_keyset"));
        assert_eq!(
            event.resource_id.as_deref(),
            Some("018fd1a0-0000-7000-8000-000000000102")
        );
        assert_eq!(
            event.principal_id.as_deref(),
            Some("api_key:018fd1a0-0000-7000-8000-000000000101")
        );
        assert_eq!(event.attrs["keyset_name"], "primary keyset,name");
        assert_eq!(event.attrs["keyset_address"], "mm:example-address");
        assert_eq!(event.attrs["reason_code"], "exported");

        let serialized = serde_json::to_string(&event).expect("serialize audit event");
        assert!(!serialized.contains("passphrase=should-not-appear"));
        assert!(!serialized.contains("public_key"));
        assert!(!serialized.contains("private_key"));
        assert!(!serialized.contains("encrypted_private_key"));
        assert!(!serialized.contains("ciphertext"));
    }
}

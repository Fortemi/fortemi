//! PKE (Public Key Encryption) HTTP handlers.

use axum::{extract::Path, http::StatusCode, response::IntoResponse, Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};

use matric_crypto::pke::{
    decrypt_pke, encrypt_pke, get_pke_recipients, key_storage, Address, Keypair, PrivateKey,
    PublicKey,
};

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct PkeKeygenRequest {
    pub passphrase: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PkeKeygenResponse {
    pub public_key: String,            // base64
    pub encrypted_private_key: String, // base64 (encrypted with passphrase)
    pub address: String,               // mm:... address
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PkeAddressRequest {
    pub public_key: String, // base64 public key bytes
}

#[derive(Debug, Serialize)]
pub struct PkeAddressResponse {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct PkeEncryptRequest {
    pub plaintext: String,       // base64 encoded
    pub recipients: Vec<String>, // mm:... addresses or base64 public keys
    pub original_filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PkeEncryptResponse {
    pub ciphertext: String,      // base64 encoded MMPKE01 format
    pub recipients: Vec<String>, // mm:... addresses
}

#[derive(Debug, Deserialize)]
pub struct PkeDecryptRequest {
    pub ciphertext: String,            // base64 MMPKE01
    pub encrypted_private_key: String, // base64
    pub passphrase: String,
}

#[derive(Debug, Serialize)]
pub struct PkeDecryptResponse {
    pub plaintext: String, // base64
    pub original_filename: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PkeRecipientsRequest {
    pub ciphertext: String, // base64 MMPKE01
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

// =============================================================================
// HANDLERS
// =============================================================================

/// Generate a new PKE keypair.
///
/// POST /api/v1/pke/keygen
#[utoipa::path(post, path = "/api/v1/pke/keygen", tag = "PKE",
    request_body = PkeKeygenRequest,
    responses((status = 201, description = "Created")))]
pub async fn pke_keygen(
    Json(req): Json<PkeKeygenRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Encode public key as base64
    let public_key_b64 = BASE64.encode(keypair.public.as_bytes());

    // Encrypt private key with passphrase
    let encrypted_private =
        key_storage::encrypt_private_key(keypair.private.as_bytes(), &req.passphrase).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "keygen_failed",
                        "error_description": format!("Failed to encrypt private key: {}", e)
                    })),
                )
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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let public_key_bytes = BASE64.decode(&req.public_key).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_base64",
                "error_description": format!("Invalid base64 for public_key: {}", e)
            })),
        )
    })?;

    if public_key_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_public_key",
                "error_description": format!("Public key must be 32 bytes, got {}", public_key_bytes.len())
            })),
        ));
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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Size limit: 10MB
    let plaintext = BASE64.decode(&req.plaintext).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_base64",
                "error_description": format!("Invalid base64 for plaintext: {}", e)
            })),
        )
    })?;

    if plaintext.len() > 10 * 1024 * 1024 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": "payload_too_large",
                "error_description": "Plaintext exceeds 10MB limit"
            })),
        ));
    }

    // Parse recipient public keys (base64 encoded)
    // For now, we only support base64 public keys (not mm: addresses, which require lookup)
    let mut recipient_keys = Vec::new();
    let mut recipient_addresses = Vec::new();

    for r in &req.recipients {
        if r.starts_with("mm:") {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "address_lookup_not_supported",
                    "error_description": "Pass base64 public keys directly. Address-based lookup requires a key server."
                })),
            ));
        }
        let key_bytes = BASE64.decode(r).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_recipient",
                    "error_description": format!("Invalid base64 public key: {}", e)
                })),
            )
        })?;

        if key_bytes.len() != 32 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_recipient",
                    "error_description": format!("Public key must be 32 bytes, got {}", key_bytes.len())
                })),
            ));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);

        let pk = PublicKey::from_bytes(arr);
        recipient_addresses.push(pk.to_address().to_string());
        recipient_keys.push(pk);
    }

    let ciphertext =
        encrypt_pke(&plaintext, &recipient_keys, req.original_filename).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "encryption_failed",
                    "error_description": format!("Encryption failed: {}", e)
                })),
            )
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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let ciphertext = BASE64.decode(&req.ciphertext).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_base64",
                "error_description": format!("Invalid ciphertext base64: {}", e)
            })),
        )
    })?;

    let encrypted_key = BASE64.decode(&req.encrypted_private_key).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_base64",
                "error_description": format!("Invalid private key base64: {}", e)
            })),
        )
    })?;

    // Decrypt private key with passphrase
    let private_key_bytes = key_storage::decrypt_private_key(&encrypted_key, &req.passphrase)
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "decryption_failed",
                    "error_description": "Invalid passphrase or corrupted private key"
                })),
            )
        })?;

    let private_key = PrivateKey::from_bytes(private_key_bytes);

    let (plaintext, header) = decrypt_pke(&ciphertext, &private_key).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "decryption_failed",
                "error_description": format!("Decryption failed: {}", e)
            })),
        )
    })?;

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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let ciphertext = BASE64.decode(&req.ciphertext).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_base64",
                "error_description": format!("Invalid ciphertext base64: {}", e)
            })),
        )
    })?;

    let recipients = get_pke_recipients(&ciphertext).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_format",
                "error_description": format!("Not a valid MMPKE01 file: {}", e)
            })),
        )
    })?;

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

use crate::AppState;
use axum::extract::State;
use matric_db::{CreateKeysetRequest, ExportedKeyset};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateKeysetApiRequest {
    pub name: String,
    pub passphrase: String,
    pub label: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct ImportKeysetRequest {
    pub name: String,
    pub exported: ExportedKeyset,
}

/// List all PKE keysets.
///
/// GET /api/v1/pke/keysets
#[utoipa::path(get, path = "/api/v1/pke/keysets", tag = "PKE",
    responses((status = 200, description = "Success")))]
pub async fn list_keysets(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let keysets = state.db.pke_keysets.list().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "database_error",
                "error_description": format!("Failed to list keysets: {}", e)
            })),
        )
    })?;

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
    State(state): State<AppState>,
    Json(req): Json<CreateKeysetApiRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
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
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "encryption_failed",
                        "error_description": format!("Failed to encrypt private key: {}", e)
                    })),
                )
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
        .map_err(|e| {
            let (status, error) = match &e {
                matric_core::Error::InvalidInput(_) => (StatusCode::CONFLICT, "keyset_exists"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
            };
            (
                status,
                Json(serde_json::json!({
                    "error": error,
                    "error_description": format!("{}", e)
                })),
            )
        })?;

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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let keyset = state.db.pke_keysets.get_active().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "database_error",
                "error_description": format!("Failed to get active keyset: {}", e)
            })),
        )
    })?;

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
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Try to parse as UUID first, otherwise treat as name
    let keyset = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.get_by_id(uuid).await
    } else {
        state.db.pke_keysets.get_by_name(&name_or_id).await
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "database_error",
                "error_description": format!("Failed to get keyset: {}", e)
            })),
        )
    })?;

    let keyset = keyset.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "error_description": format!("Keyset '{}' not found", name_or_id)
            })),
        )
    })?;

    state
        .db
        .pke_keysets
        .set_active(keyset.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "database_error",
                    "error_description": format!("Failed to set active keyset: {}", e)
                })),
            )
        })?;

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
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Try to parse as UUID first, otherwise treat as name
    let deleted = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.delete(uuid).await
    } else {
        state.db.pke_keysets.delete_by_name(&name_or_id).await
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "database_error",
                "error_description": format!("Failed to delete keyset: {}", e)
            })),
        )
    })?;

    if deleted {
        Ok(Json(serde_json::json!({
            "success": true,
            "deleted": name_or_id
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "error_description": format!("Keyset '{}' not found", name_or_id)
            })),
        ))
    }
}

/// Export a PKE keyset by name or ID.
///
/// GET /api/v1/pke/keysets/:name_or_id/export
#[utoipa::path(get, path = "/api/v1/pke/keysets/{name_or_id}/export", tag = "PKE",
    params(("name_or_id" = String, Path, description = "Keyset name or UUID")),
    responses((status = 200, description = "Success")))]
pub async fn export_keyset(
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Try to parse as UUID first, otherwise treat as name
    let keyset = if let Ok(uuid) = Uuid::parse_str(&name_or_id) {
        state.db.pke_keysets.get_by_id(uuid).await
    } else {
        state.db.pke_keysets.get_by_name(&name_or_id).await
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "database_error",
                "error_description": format!("Failed to get keyset: {}", e)
            })),
        )
    })?;

    let keyset = keyset.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "error_description": format!("Keyset '{}' not found", name_or_id)
            })),
        )
    })?;

    let exported = state
        .db
        .pke_keysets
        .export(keyset.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "database_error",
                    "error_description": format!("Failed to export keyset: {}", e)
                })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "not_found",
                    "error_description": format!("Keyset '{}' not found", name_or_id)
                })),
            )
        })?;

    Ok(Json(exported))
}

/// Import a PKE keyset.
///
/// POST /api/v1/pke/keysets/import
#[utoipa::path(post, path = "/api/v1/pke/keysets/import", tag = "PKE",
    request_body = ImportKeysetRequest,
    responses((status = 201, description = "Created")))]
pub async fn import_keyset(
    State(state): State<AppState>,
    Json(req): Json<ImportKeysetRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let keyset = state
        .db
        .pke_keysets
        .import(req.name, req.exported)
        .await
        .map_err(|e| {
            let (status, error) = match &e {
                matric_core::Error::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
            };
            (
                status,
                Json(serde_json::json!({
                    "error": error,
                    "error_description": format!("{}", e)
                })),
            )
        })?;

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

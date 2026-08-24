//! Provider-neutral envelope encryption and managed-key boundary.
//!
//! The types in this module deliberately separate public key metadata from
//! plaintext key material. Plaintext DEKs are non-serializable, zeroize on
//! drop, and never appear in `Debug` output.

mod aws_kms;
mod env;

#[cfg(feature = "kms-aws")]
pub use aws_kms::AwsSdkKmsClient;
pub use aws_kms::{
    AwsKmsClient, AwsKmsClientError, AwsKmsDecryptOutput, AwsKmsFuture,
    AwsKmsGenerateDataKeyOutput, AwsKmsProvider, AwsKmsWrapOutput,
};
pub use env::{DeploymentMode, EnvKeyProvider};

use std::{collections::BTreeMap, fmt, future::Future, pin::Pin, str::FromStr};

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

const KEY_PURPOSE_VERSION: u16 = 1;
const KEY_CONTEXT_VERSION: u16 = 1;
const WRAPPED_KEY_FORMAT_VERSION: u16 = 1;
const ENCRYPTED_BLOB_FORMAT_VERSION: u16 = 1;
const AES_256_KEY_BYTES: usize = 32;
const AES_GCM_NONCE_BYTES: usize = 12;
const MAX_CONTEXT_VALUE_BYTES: usize = 256;
const MAX_KEK_REF_BYTES: usize = 1024;
const MAX_PROVIDER_METADATA_ENTRIES: usize = 32;
const MAX_PROVIDER_METADATA_BYTES: usize = 4096;
const MIN_DEK_BYTES: usize = 16;
const MAX_DEK_BYTES: usize = 4096;

/// Versioned semantic purpose for a managed key operation.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyPurpose {
    version: u16,
    kind: KeyPurposeKind,
}

impl KeyPurpose {
    /// User content encrypted at rest.
    pub const CONTENT_BLOB: Self = Self::v1(KeyPurposeKind::ContentBlob);
    /// Stored credentials, including BYO provider credentials.
    pub const USER_SECRET: Self = Self::v1(KeyPurposeKind::UserSecret);
    /// Stored OAuth refresh tokens.
    pub const OAUTH_REFRESH_TOKEN: Self = Self::v1(KeyPurposeKind::OAuthRefreshToken);
    /// Plugin JWT signing.
    pub const PLUGIN_JWT: Self = Self::v1(KeyPurposeKind::PluginJwt);
    /// Audit-chain signing.
    pub const AUDIT_CHAIN: Self = Self::v1(KeyPurposeKind::AuditChain);
    /// API-key validation MACs.
    pub const API_KEY_HMAC: Self = Self::v1(KeyPurposeKind::ApiKeyHmac);

    const fn v1(kind: KeyPurposeKind) -> Self {
        Self {
            version: KEY_PURPOSE_VERSION,
            kind,
        }
    }

    /// Construct a custom v1 purpose. Names are stable protocol identifiers,
    /// not display text.
    pub fn custom(name: impl Into<String>) -> Result<Self, KeyError> {
        let purpose = Self {
            version: KEY_PURPOSE_VERSION,
            kind: KeyPurposeKind::Custom(name.into()),
        };
        purpose.validate()?;
        Ok(purpose)
    }

    /// Purpose schema version.
    pub fn version(&self) -> u16 {
        self.version
    }

    /// Semantic purpose kind.
    pub fn kind(&self) -> &KeyPurposeKind {
        &self.kind
    }

    /// Stable snake-case protocol name.
    pub fn name(&self) -> &str {
        match &self.kind {
            KeyPurposeKind::ContentBlob => "content_blob",
            KeyPurposeKind::UserSecret => "user_secret",
            KeyPurposeKind::OAuthRefreshToken => "oauth_refresh_token",
            KeyPurposeKind::PluginJwt => "plugin_jwt",
            KeyPurposeKind::AuditChain => "audit_chain",
            KeyPurposeKind::ApiKeyHmac => "api_key_hmac",
            KeyPurposeKind::Custom(name) => name,
        }
    }

    fn validate(&self) -> Result<(), KeyError> {
        if self.version != KEY_PURPOSE_VERSION {
            return Err(KeyError::new(
                KeyOperation::ValidateContext,
                KeyFailureClass::UnsupportedVersion,
            ));
        }
        validate_protocol_value(self.name(), false).map_err(|_| {
            KeyError::new(
                KeyOperation::ValidateContext,
                KeyFailureClass::InvalidContext,
            )
        })
    }
}

impl fmt::Debug for KeyPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPurpose")
            .field("version", &self.version)
            .field("name", &self.name())
            .finish()
    }
}

/// Known purpose names. `Custom` is restricted to protocol-safe identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPurposeKind {
    ContentBlob,
    UserSecret,
    OAuthRefreshToken,
    PluginJwt,
    AuditChain,
    ApiKeyHmac,
    Custom(String),
}

/// Non-secret, versioned context bound to wrap and payload AEAD operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyContext {
    version: u16,
    purpose: KeyPurpose,
    tenant_id: Option<String>,
    user_id: Option<String>,
    resource_id: Option<String>,
    schema: String,
}

impl KeyContext {
    /// Construct a v1 context for a trusted storage schema.
    pub fn new(purpose: KeyPurpose, schema: impl Into<String>) -> Result<Self, KeyError> {
        let context = Self {
            version: KEY_CONTEXT_VERSION,
            purpose,
            tenant_id: None,
            user_id: None,
            resource_id: None,
            schema: schema.into(),
        };
        context.validate()?;
        Ok(context)
    }

    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Result<Self, KeyError> {
        self.tenant_id = Some(tenant_id.into());
        self.validate()?;
        Ok(self)
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Result<Self, KeyError> {
        self.user_id = Some(user_id.into());
        self.validate()?;
        Ok(self)
    }

    pub fn with_resource_id(mut self, resource_id: impl Into<String>) -> Result<Self, KeyError> {
        self.resource_id = Some(resource_id.into());
        self.validate()?;
        Ok(self)
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn purpose(&self) -> &KeyPurpose {
        &self.purpose
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }

    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// Validate values before using a deserialized context in a key operation.
    pub fn validate(&self) -> Result<(), KeyError> {
        if self.version != KEY_CONTEXT_VERSION {
            return Err(KeyError::new(
                KeyOperation::ValidateContext,
                KeyFailureClass::UnsupportedVersion,
            ));
        }
        self.purpose.validate()?;
        validate_protocol_value(&self.schema, false).map_err(invalid_context)?;
        for value in [
            self.tenant_id.as_deref(),
            self.user_id.as_deref(),
            self.resource_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_protocol_value(value, false).map_err(invalid_context)?;
        }
        Ok(())
    }

    fn canonical_bytes(
        &self,
        domain: &str,
        provider_kind: &KeyProviderKind,
        kek_ref: &str,
    ) -> Result<Vec<u8>, KeyError> {
        self.validate()?;
        provider_kind.validate()?;
        validate_kek_ref(kek_ref)?;

        let mut bytes = Vec::with_capacity(256);
        append_field(&mut bytes, "domain", domain.as_bytes());
        append_field(&mut bytes, "context_version", &self.version.to_be_bytes());
        append_field(
            &mut bytes,
            "purpose_version",
            &self.purpose.version.to_be_bytes(),
        );
        append_field(&mut bytes, "purpose", self.purpose.name().as_bytes());
        append_optional_field(&mut bytes, "tenant_id", self.tenant_id.as_deref());
        append_optional_field(&mut bytes, "user_id", self.user_id.as_deref());
        append_optional_field(&mut bytes, "resource_id", self.resource_id.as_deref());
        append_field(&mut bytes, "schema", self.schema.as_bytes());
        append_field(&mut bytes, "provider_kind", provider_kind.name().as_bytes());
        append_field(&mut bytes, "kek_ref", kek_ref.as_bytes());
        Ok(bytes)
    }

    fn payload_aad(&self) -> Result<Vec<u8>, KeyError> {
        self.canonical_bytes(
            "fortemi/encrypted-blob/aad/v1",
            &KeyProviderKind::Other("provider-neutral".to_string()),
            "provider-neutral",
        )
    }
}

fn invalid_context(_: ()) -> KeyError {
    KeyError::new(
        KeyOperation::ValidateContext,
        KeyFailureClass::InvalidContext,
    )
}

fn validate_protocol_value(value: &str, allow_empty: bool) -> Result<(), ()> {
    if (!allow_empty && value.is_empty())
        || value.len() > MAX_CONTEXT_VALUE_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/' | b'@')
        })
    {
        return Err(());
    }
    Ok(())
}

fn append_field(output: &mut Vec<u8>, name: &str, value: &[u8]) {
    output.extend_from_slice(&(name.len() as u16).to_be_bytes());
    output.extend_from_slice(name.as_bytes());
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn append_optional_field(output: &mut Vec<u8>, name: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            output.push(1);
            append_field(output, name, value.as_bytes());
        }
        None => {
            output.push(0);
            append_field(output, name, &[]);
        }
    }
}

/// Provider identifier persisted with wrapped keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyProviderKind {
    Env,
    AwsKms,
    VaultTransit,
    GcpKms,
    Other(String),
}

impl KeyProviderKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Env => "env",
            Self::AwsKms => "aws-kms",
            Self::VaultTransit => "vault-transit",
            Self::GcpKms => "gcp-kms",
            Self::Other(name) => name,
        }
    }

    fn validate(&self) -> Result<(), KeyError> {
        validate_protocol_value(self.name(), false).map_err(|_| {
            KeyError::new(
                KeyOperation::ValidateContext,
                KeyFailureClass::InvalidConfiguration,
            )
        })
    }
}

/// AEAD algorithm used for the payload ciphertext.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AeadAlgorithm {
    Aes256Gcm,
}

/// Provider-neutral wrapped DEK. Binary wrapping output is always redacted.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    format_version: u16,
    provider_kind: KeyProviderKind,
    kek_ref: String,
    purpose: KeyPurpose,
    context_version: u16,
    wrapped_dek: Vec<u8>,
    wrapping_nonce: Option<Vec<u8>>,
    provider_metadata: BTreeMap<String, String>,
    created_at: DateTime<Utc>,
    rewrapped_at: Option<DateTime<Utc>>,
}

impl WrappedKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_kind: KeyProviderKind,
        kek_ref: impl Into<String>,
        purpose: KeyPurpose,
        context_version: u16,
        wrapped_dek: Vec<u8>,
        wrapping_nonce: Option<Vec<u8>>,
        provider_metadata: BTreeMap<String, String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, KeyError> {
        let wrapped = Self {
            format_version: WRAPPED_KEY_FORMAT_VERSION,
            provider_kind,
            kek_ref: kek_ref.into(),
            purpose,
            context_version,
            wrapped_dek,
            wrapping_nonce,
            provider_metadata,
            created_at,
            rewrapped_at: None,
        };
        wrapped.validate()?;
        Ok(wrapped)
    }

    pub fn format_version(&self) -> u16 {
        self.format_version
    }

    pub fn provider_kind(&self) -> &KeyProviderKind {
        &self.provider_kind
    }

    pub fn kek_ref(&self) -> &str {
        &self.kek_ref
    }

    pub fn purpose(&self) -> &KeyPurpose {
        &self.purpose
    }

    pub fn context_version(&self) -> u16 {
        self.context_version
    }

    pub fn wrapped_dek(&self) -> &[u8] {
        &self.wrapped_dek
    }

    pub fn wrapping_nonce(&self) -> Option<&[u8]> {
        self.wrapping_nonce.as_deref()
    }

    pub fn provider_metadata(&self) -> &BTreeMap<String, String> {
        &self.provider_metadata
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn rewrapped_at(&self) -> Option<DateTime<Utc>> {
        self.rewrapped_at
    }

    pub fn validate(&self) -> Result<(), KeyError> {
        if self.format_version != WRAPPED_KEY_FORMAT_VERSION
            || self.context_version != KEY_CONTEXT_VERSION
        {
            return Err(KeyError::new(
                KeyOperation::UnwrapDek,
                KeyFailureClass::UnsupportedVersion,
            ));
        }
        self.provider_kind.validate()?;
        self.purpose.validate()?;
        validate_kek_ref(&self.kek_ref)?;
        if self.wrapped_dek.is_empty() {
            return Err(KeyError::new(
                KeyOperation::UnwrapDek,
                KeyFailureClass::InvalidCiphertext,
            ));
        }
        validate_provider_metadata(&self.provider_metadata)?;
        Ok(())
    }

    fn mark_rewrapped_from(&mut self, previous: &Self) {
        self.created_at = previous.created_at;
        self.rewrapped_at = Some(Utc::now());
    }
}

impl fmt::Debug for WrappedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WrappedKey")
            .field("format_version", &self.format_version)
            .field("provider_kind", &self.provider_kind)
            .field("kek_ref", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .field("context_version", &self.context_version)
            .field("wrapped_dek", &"[REDACTED]")
            .field("wrapping_nonce", &"[REDACTED]")
            .field("provider_metadata", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .field("rewrapped_at", &self.rewrapped_at)
            .finish()
    }
}

impl Drop for WrappedKey {
    fn drop(&mut self) {
        self.wrapped_dek.zeroize();
        if let Some(nonce) = &mut self.wrapping_nonce {
            nonce.zeroize();
        }
    }
}

/// Provider-neutral encrypted payload envelope.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedBlob {
    format_version: u16,
    aead_algorithm: AeadAlgorithm,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    wrapped_key: WrappedKey,
}

impl EncryptedBlob {
    pub fn new(
        nonce: Vec<u8>,
        ciphertext: Vec<u8>,
        wrapped_key: WrappedKey,
    ) -> Result<Self, KeyError> {
        let blob = Self {
            format_version: ENCRYPTED_BLOB_FORMAT_VERSION,
            aead_algorithm: AeadAlgorithm::Aes256Gcm,
            nonce,
            ciphertext,
            wrapped_key,
        };
        blob.validate()?;
        Ok(blob)
    }

    pub fn format_version(&self) -> u16 {
        self.format_version
    }

    pub fn aead_algorithm(&self) -> AeadAlgorithm {
        self.aead_algorithm
    }

    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn wrapped_key(&self) -> &WrappedKey {
        &self.wrapped_key
    }

    pub fn replace_wrapped_key(&mut self, wrapped_key: WrappedKey) -> Result<(), KeyError> {
        wrapped_key.validate()?;
        if wrapped_key.purpose() != self.wrapped_key.purpose()
            || wrapped_key.context_version() != self.wrapped_key.context_version()
        {
            return Err(KeyError::new(
                KeyOperation::RewrapDek,
                KeyFailureClass::InvalidContext,
            ));
        }
        self.wrapped_key = wrapped_key;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), KeyError> {
        if self.format_version != ENCRYPTED_BLOB_FORMAT_VERSION {
            return Err(KeyError::new(
                KeyOperation::DecryptBlob,
                KeyFailureClass::UnsupportedVersion,
            ));
        }
        if self.aead_algorithm != AeadAlgorithm::Aes256Gcm
            || self.nonce.len() != AES_GCM_NONCE_BYTES
            || self.ciphertext.len() < 16
        {
            return Err(KeyError::new(
                KeyOperation::DecryptBlob,
                KeyFailureClass::InvalidCiphertext,
            ));
        }
        self.wrapped_key.validate()
    }
}

impl fmt::Debug for EncryptedBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedBlob")
            .field("format_version", &self.format_version)
            .field("aead_algorithm", &self.aead_algorithm)
            .field("nonce", &"[REDACTED]")
            .field("ciphertext", &"[REDACTED]")
            .field("wrapped_key", &self.wrapped_key)
            .finish()
    }
}

impl Drop for EncryptedBlob {
    fn drop(&mut self) {
        self.nonce.zeroize();
        self.ciphertext.zeroize();
    }
}

/// Plaintext DEK with automatic zeroization and no serialization surface.
pub struct PlaintextDek(Zeroizing<Vec<u8>>);

impl PlaintextDek {
    pub fn new(bytes: Vec<u8>) -> Result<Self, KeyError> {
        validate_dek_len(bytes.len(), KeyOperation::GenerateDek)?;
        Ok(Self(Zeroizing::new(bytes)))
    }

    /// Borrow plaintext key material for immediate cryptographic use.
    pub fn expose_secret(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for PlaintextDek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlaintextDek")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

/// Fresh plaintext DEK and its persisted wrapped representation.
pub struct GeneratedDek {
    plaintext: PlaintextDek,
    wrapped_key: WrappedKey,
}

impl GeneratedDek {
    pub fn new(plaintext: PlaintextDek, wrapped_key: WrappedKey) -> Result<Self, KeyError> {
        wrapped_key.validate()?;
        Ok(Self {
            plaintext,
            wrapped_key,
        })
    }

    pub fn plaintext(&self) -> &PlaintextDek {
        &self.plaintext
    }

    pub fn wrapped_key(&self) -> &WrappedKey {
        &self.wrapped_key
    }

    pub fn into_parts(self) -> (PlaintextDek, WrappedKey) {
        (self.plaintext, self.wrapped_key)
    }
}

impl fmt::Debug for GeneratedDek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedDek")
            .field("plaintext", &self.plaintext)
            .field("wrapped_key", &self.wrapped_key)
            .finish()
    }
}

/// Opaque provider signature. Signature bytes are redacted from `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSignature {
    provider_kind: KeyProviderKind,
    algorithm: String,
    bytes: Vec<u8>,
    provider_metadata: BTreeMap<String, String>,
}

impl ProviderSignature {
    pub fn new(
        provider_kind: KeyProviderKind,
        algorithm: impl Into<String>,
        bytes: Vec<u8>,
        provider_metadata: BTreeMap<String, String>,
    ) -> Result<Self, KeyError> {
        let signature = Self {
            provider_kind,
            algorithm: algorithm.into(),
            bytes,
            provider_metadata,
        };
        signature.provider_kind.validate()?;
        validate_protocol_value(&signature.algorithm, false).map_err(|_| {
            KeyError::new(KeyOperation::Sign, KeyFailureClass::InvalidConfiguration)
        })?;
        validate_provider_metadata(&signature.provider_metadata)?;
        if signature.bytes.is_empty() {
            return Err(KeyError::new(
                KeyOperation::Sign,
                KeyFailureClass::ProviderFailure,
            ));
        }
        Ok(signature)
    }

    pub fn provider_kind(&self) -> &KeyProviderKind {
        &self.provider_kind
    }

    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for ProviderSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderSignature")
            .field("provider_kind", &self.provider_kind)
            .field("algorithm", &self.algorithm)
            .field("bytes", &"[REDACTED]")
            .field("provider_metadata", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ProviderSignature {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Stable provider-neutral failure classes. No backend response body is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyFailureClass {
    InvalidConfiguration,
    InvalidContext,
    UnsupportedVersion,
    UnsupportedOperation,
    ProviderUnavailable,
    AccessDenied,
    KeyDisabled,
    KeyVersionUnavailable,
    ContextMismatch,
    InvalidCiphertext,
    Throttled,
    ProviderFailure,
}

/// Explicit behavior for an operation after a provider failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedMode {
    /// Do not perform or bypass the requested cryptographic operation.
    FailClosed,
    /// Fail closed now; the caller may retry with bounded backoff.
    RetryableFailClosed,
    /// Existing key versions remain usable while rotation progresses.
    RotationInProgress,
}

/// Stable operation identifiers suitable for metadata-only audit events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyOperation {
    ValidateContext,
    WrapDek,
    UnwrapDek,
    GenerateDek,
    EncryptBlob,
    DecryptBlob,
    RewrapDek,
    Sign,
    Verify,
    Rotate,
    HealthCheck,
}

/// Redacted provider error. It intentionally has no arbitrary message/source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyError {
    operation: KeyOperation,
    class: KeyFailureClass,
    degraded_mode: DegradedMode,
}

impl KeyError {
    pub fn new(operation: KeyOperation, class: KeyFailureClass) -> Self {
        let degraded_mode = match class {
            KeyFailureClass::ProviderUnavailable | KeyFailureClass::Throttled => {
                DegradedMode::RetryableFailClosed
            }
            _ => DegradedMode::FailClosed,
        };
        Self {
            operation,
            class,
            degraded_mode,
        }
    }

    pub fn operation(&self) -> KeyOperation {
        self.operation
    }

    pub fn class(&self) -> KeyFailureClass {
        self.class
    }

    pub fn degraded_mode(&self) -> DegradedMode {
        self.degraded_mode
    }

    pub fn is_retryable(&self) -> bool {
        self.degraded_mode == DegradedMode::RetryableFailClosed
    }
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "key provider operation {:?} failed with {:?}",
            self.operation, self.class
        )
    }
}

impl std::error::Error for KeyError {}

/// Provider health result with explicit fail-closed semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum HealthStatus {
    Ready,
    Degraded {
        class: KeyFailureClass,
        mode: DegradedMode,
    },
    Unavailable {
        class: KeyFailureClass,
        mode: DegradedMode,
    },
}

impl HealthStatus {
    pub fn permits_key_operations(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Provider-neutral rotation receipt. Version identifiers are opaque metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationInfo {
    provider_kind: KeyProviderKind,
    purpose: KeyPurpose,
    previous_version: Option<String>,
    current_version: String,
    mode: DegradedMode,
}

impl RotationInfo {
    pub fn new(
        provider_kind: KeyProviderKind,
        purpose: KeyPurpose,
        previous_version: Option<String>,
        current_version: String,
    ) -> Result<Self, KeyError> {
        provider_kind.validate()?;
        purpose.validate()?;
        validate_protocol_value(&current_version, false)
            .map_err(|_| KeyError::new(KeyOperation::Rotate, KeyFailureClass::ProviderFailure))?;
        if let Some(previous) = &previous_version {
            validate_protocol_value(previous, false).map_err(|_| {
                KeyError::new(KeyOperation::Rotate, KeyFailureClass::ProviderFailure)
            })?;
        }
        Ok(Self {
            provider_kind,
            purpose,
            previous_version,
            current_version,
            mode: DegradedMode::RotationInProgress,
        })
    }

    pub fn provider_kind(&self) -> &KeyProviderKind {
        &self.provider_kind
    }

    pub fn purpose(&self) -> &KeyPurpose {
        &self.purpose
    }

    pub fn previous_version(&self) -> Option<&str> {
        self.previous_version.as_deref()
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub fn mode(&self) -> DegradedMode {
        self.mode
    }
}

/// Boxed provider future used to keep the async trait object-safe.
pub type KeyFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, KeyError>> + Send + 'a>>;

/// Async, object-safe managed-key provider boundary.
pub trait KeyProvider: Send + Sync {
    fn kind(&self) -> KeyProviderKind;

    fn wrap_dek<'a>(
        &'a self,
        plaintext_dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey>;

    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek>;

    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek>;

    fn rewrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            validate_provider_binding(self, wrapped, context, KeyOperation::RewrapDek)?;
            let plaintext = self.unwrap_dek(wrapped, context).await?;
            let mut next = self.wrap_dek(&plaintext, context).await?;
            validate_provider_binding(self, &next, context, KeyOperation::RewrapDek)?;
            next.mark_rewrapped_from(wrapped);
            Ok(next)
        })
    }

    fn sign<'a>(
        &'a self,
        _context: &'a KeyContext,
        _data: &'a [u8],
    ) -> KeyFuture<'a, ProviderSignature> {
        Box::pin(async {
            Err(KeyError::new(
                KeyOperation::Sign,
                KeyFailureClass::UnsupportedOperation,
            ))
        })
    }

    fn verify<'a>(
        &'a self,
        _context: &'a KeyContext,
        _data: &'a [u8],
        _signature: &'a ProviderSignature,
    ) -> KeyFuture<'a, bool> {
        Box::pin(async {
            Err(KeyError::new(
                KeyOperation::Verify,
                KeyFailureClass::UnsupportedOperation,
            ))
        })
    }

    fn rotate<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, RotationInfo>;

    /// Must exercise the provider with the supplied production-shaped context.
    fn health_check<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, HealthStatus>;
}

/// Encrypt a payload under a fresh provider-generated DEK.
pub async fn encrypt_blob(
    provider: &dyn KeyProvider,
    plaintext: &[u8],
    context: &KeyContext,
) -> Result<EncryptedBlob, KeyError> {
    context.validate()?;
    let generated = provider.generate_dek(context, AES_256_KEY_BYTES).await?;
    validate_provider_binding(
        provider,
        generated.wrapped_key(),
        context,
        KeyOperation::EncryptBlob,
    )?;
    let key = generated.plaintext().expose_secret();
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| KeyError::new(KeyOperation::EncryptBlob, KeyFailureClass::ProviderFailure))?;
    let mut nonce = vec![0u8; AES_GCM_NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut nonce);
    let aad = context.payload_aad()?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| KeyError::new(KeyOperation::EncryptBlob, KeyFailureClass::ProviderFailure))?;
    let (_, wrapped_key) = generated.into_parts();
    EncryptedBlob::new(nonce, ciphertext, wrapped_key)
}

/// Decrypt an envelope using context reconstructed from trusted application state.
pub async fn decrypt_blob(
    provider: &dyn KeyProvider,
    blob: &EncryptedBlob,
    context: &KeyContext,
) -> Result<Zeroizing<Vec<u8>>, KeyError> {
    context.validate()?;
    blob.validate()?;
    if blob.wrapped_key().purpose() != context.purpose()
        || blob.wrapped_key().context_version() != context.version()
    {
        return Err(KeyError::new(
            KeyOperation::DecryptBlob,
            KeyFailureClass::ContextMismatch,
        ));
    }
    validate_provider_binding(
        provider,
        blob.wrapped_key(),
        context,
        KeyOperation::DecryptBlob,
    )?;
    let plaintext_dek = provider.unwrap_dek(blob.wrapped_key(), context).await?;
    if plaintext_dek.len() != AES_256_KEY_BYTES {
        return Err(KeyError::new(
            KeyOperation::DecryptBlob,
            KeyFailureClass::KeyVersionUnavailable,
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(plaintext_dek.expose_secret())
        .map_err(|_| KeyError::new(KeyOperation::DecryptBlob, KeyFailureClass::ProviderFailure))?;
    let aad = context.payload_aad()?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(blob.nonce()),
            Payload {
                msg: blob.ciphertext(),
                aad: &aad,
            },
        )
        .map_err(|_| KeyError::new(KeyOperation::DecryptBlob, KeyFailureClass::ContextMismatch))?;
    Ok(Zeroizing::new(plaintext))
}

/// Rewrap a DEK between providers without replacing the DEK or payload ciphertext.
pub async fn rewrap_between(
    source: &dyn KeyProvider,
    target: &dyn KeyProvider,
    wrapped: &WrappedKey,
    context: &KeyContext,
) -> Result<WrappedKey, KeyError> {
    validate_provider_binding(source, wrapped, context, KeyOperation::RewrapDek)?;
    let plaintext = source.unwrap_dek(wrapped, context).await?;
    let mut next = target.wrap_dek(&plaintext, context).await?;
    validate_provider_binding(target, &next, context, KeyOperation::RewrapDek)?;
    next.mark_rewrapped_from(wrapped);
    Ok(next)
}

fn validate_provider_binding<P: KeyProvider + ?Sized>(
    provider: &P,
    wrapped: &WrappedKey,
    context: &KeyContext,
    operation: KeyOperation,
) -> Result<(), KeyError> {
    wrapped.validate()?;
    if wrapped.provider_kind() != &provider.kind()
        || wrapped.purpose() != context.purpose()
        || wrapped.context_version() != context.version()
    {
        return Err(KeyError::new(operation, KeyFailureClass::ContextMismatch));
    }
    Ok(())
}

fn validate_dek_len(bytes: usize, operation: KeyOperation) -> Result<(), KeyError> {
    if !(MIN_DEK_BYTES..=MAX_DEK_BYTES).contains(&bytes) {
        return Err(KeyError::new(
            operation,
            KeyFailureClass::InvalidConfiguration,
        ));
    }
    Ok(())
}

fn validate_kek_ref(kek_ref: &str) -> Result<(), KeyError> {
    if kek_ref.is_empty()
        || kek_ref.len() > MAX_KEK_REF_BYTES
        || kek_ref.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(KeyError::new(
            KeyOperation::ValidateContext,
            KeyFailureClass::InvalidConfiguration,
        ));
    }
    Ok(())
}

fn validate_provider_metadata(metadata: &BTreeMap<String, String>) -> Result<(), KeyError> {
    if metadata.len() > MAX_PROVIDER_METADATA_ENTRIES {
        return Err(KeyError::new(
            KeyOperation::ValidateContext,
            KeyFailureClass::InvalidConfiguration,
        ));
    }
    let total_bytes = metadata
        .iter()
        .map(|(key, value)| key.len().saturating_add(value.len()))
        .sum::<usize>();
    if total_bytes > MAX_PROVIDER_METADATA_BYTES
        || metadata.iter().any(|(key, value)| {
            validate_protocol_value(key, false).is_err()
                || value.bytes().any(|byte| byte.is_ascii_control())
        })
    {
        return Err(KeyError::new(
            KeyOperation::ValidateContext,
            KeyFailureClass::InvalidConfiguration,
        ));
    }
    Ok(())
}

fn metadata_version(wrapped: &WrappedKey) -> Result<u64, KeyError> {
    wrapped
        .provider_metadata()
        .get("key_version")
        .and_then(|value| u64::from_str(value).ok())
        .ok_or_else(|| {
            KeyError::new(
                KeyOperation::UnwrapDek,
                KeyFailureClass::KeyVersionUnavailable,
            )
        })
}

#[cfg(test)]
mod tests;

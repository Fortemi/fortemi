//! Explicit single-tenant/development provider backed by a process secret.

use std::{collections::BTreeMap, fmt};

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use chrono::Utc;
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

use super::{
    metadata_version, validate_dek_len, validate_kek_ref, DegradedMode, GeneratedDek, HealthStatus,
    KeyContext, KeyError, KeyFailureClass, KeyFuture, KeyOperation, KeyProvider, KeyProviderKind,
    PlaintextDek, RotationInfo, WrappedKey, AES_GCM_NONCE_BYTES,
};

const MASTER_KEY_BYTES: usize = 32;
const HKDF_SALT: &[u8] = b"fortemi/env-key-provider/extract/v1";
const WRAP_KDF_DOMAIN: &str = "fortemi/env-key-provider/wrap-kdf/v1";
const WRAP_AAD_DOMAIN: &str = "fortemi/wrapped-key/aad/v1";

/// Deployment posture supplied explicitly when constructing an env provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentMode {
    SingleTenant,
    Development,
    HostedMultiTenant,
}

/// Local master-key provider. It cannot be constructed for hosted multi-tenant use.
pub struct EnvKeyProvider {
    master_key: Zeroizing<[u8; MASTER_KEY_BYTES]>,
    kek_ref: String,
    key_version: u64,
    deployment_mode: DeploymentMode,
}

impl EnvKeyProvider {
    /// Construct from already-decoded high-entropy master material.
    pub fn new(
        master_key: Zeroizing<[u8; MASTER_KEY_BYTES]>,
        kek_ref: impl Into<String>,
        key_version: u64,
        deployment_mode: DeploymentMode,
    ) -> Result<Self, KeyError> {
        if deployment_mode == DeploymentMode::HostedMultiTenant || key_version == 0 {
            return Err(KeyError::new(
                KeyOperation::HealthCheck,
                KeyFailureClass::InvalidConfiguration,
            ));
        }
        let kek_ref = kek_ref.into();
        validate_kek_ref(&kek_ref)?;
        Ok(Self {
            master_key,
            kek_ref,
            key_version,
            deployment_mode,
        })
    }

    /// Load `FORTEMI_MASTER_KEY` as standard base64 and require an explicit,
    /// non-secret `FORTEMI_ENV_KEK_REF`. `FORTEMI_ENV_KEY_VERSION` defaults to 1.
    pub fn from_env(deployment_mode: DeploymentMode) -> Result<Self, KeyError> {
        if deployment_mode == DeploymentMode::HostedMultiTenant {
            return Err(KeyError::new(
                KeyOperation::HealthCheck,
                KeyFailureClass::InvalidConfiguration,
            ));
        }

        let encoded = std::env::var("FORTEMI_MASTER_KEY").map_err(|_| {
            KeyError::new(
                KeyOperation::HealthCheck,
                KeyFailureClass::InvalidConfiguration,
            )
        })?;
        let mut encoded = Zeroizing::new(encoded);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| {
                KeyError::new(
                    KeyOperation::HealthCheck,
                    KeyFailureClass::InvalidConfiguration,
                )
            })?;
        encoded.zeroize();
        let decoded = Zeroizing::new(decoded);
        if decoded.len() != MASTER_KEY_BYTES {
            return Err(KeyError::new(
                KeyOperation::HealthCheck,
                KeyFailureClass::InvalidConfiguration,
            ));
        }
        let mut master_key = Zeroizing::new([0u8; MASTER_KEY_BYTES]);
        master_key.copy_from_slice(decoded.as_slice());

        let kek_ref = std::env::var("FORTEMI_ENV_KEK_REF").map_err(|_| {
            KeyError::new(
                KeyOperation::HealthCheck,
                KeyFailureClass::InvalidConfiguration,
            )
        })?;
        let key_version = std::env::var("FORTEMI_ENV_KEY_VERSION")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| {
                KeyError::new(
                    KeyOperation::HealthCheck,
                    KeyFailureClass::InvalidConfiguration,
                )
            })?
            .unwrap_or(1);
        Self::new(master_key, kek_ref, key_version, deployment_mode)
    }

    pub fn deployment_mode(&self) -> DeploymentMode {
        self.deployment_mode
    }

    pub fn key_version(&self) -> u64 {
        self.key_version
    }

    fn derive_wrapping_key(&self, context: &KeyContext) -> Result<Zeroizing<[u8; 32]>, KeyError> {
        let info =
            context.canonical_bytes(WRAP_KDF_DOMAIN, &KeyProviderKind::Env, &self.kek_ref)?;
        let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), self.master_key.as_slice());
        let mut wrapping_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(&info, wrapping_key.as_mut())
            .map_err(|_| KeyError::new(KeyOperation::WrapDek, KeyFailureClass::ProviderFailure))?;
        Ok(wrapping_key)
    }

    fn wrapping_aad(&self, context: &KeyContext) -> Result<Vec<u8>, KeyError> {
        context.canonical_bytes(WRAP_AAD_DOMAIN, &KeyProviderKind::Env, &self.kek_ref)
    }
}

impl fmt::Debug for EnvKeyProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvKeyProvider")
            .field("master_key", &"[REDACTED]")
            .field("kek_ref", &"[REDACTED]")
            .field("key_version", &self.key_version)
            .field("deployment_mode", &self.deployment_mode)
            .finish()
    }
}

impl KeyProvider for EnvKeyProvider {
    fn kind(&self) -> KeyProviderKind {
        KeyProviderKind::Env
    }

    fn wrap_dek<'a>(
        &'a self,
        plaintext_dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            validate_dek_len(plaintext_dek.len(), KeyOperation::WrapDek)?;
            context.validate()?;
            let wrapping_key = self.derive_wrapping_key(context)?;
            let cipher = Aes256Gcm::new_from_slice(wrapping_key.as_slice()).map_err(|_| {
                KeyError::new(KeyOperation::WrapDek, KeyFailureClass::ProviderFailure)
            })?;
            let mut nonce = vec![0u8; AES_GCM_NONCE_BYTES];
            rand::thread_rng().fill_bytes(&mut nonce);
            let aad = self.wrapping_aad(context)?;
            let wrapped_dek = cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext_dek.expose_secret(),
                        aad: &aad,
                    },
                )
                .map_err(|_| {
                    KeyError::new(KeyOperation::WrapDek, KeyFailureClass::ProviderFailure)
                })?;
            let mut provider_metadata = BTreeMap::new();
            provider_metadata.insert("key_version".to_string(), self.key_version.to_string());
            WrappedKey::new(
                KeyProviderKind::Env,
                self.kek_ref.clone(),
                context.purpose().clone(),
                context.version(),
                wrapped_dek,
                Some(nonce),
                provider_metadata,
                Utc::now(),
            )
        })
    }

    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        Box::pin(async move {
            wrapped.validate()?;
            context.validate()?;
            if wrapped.provider_kind() != &KeyProviderKind::Env
                || wrapped.kek_ref() != self.kek_ref
                || wrapped.purpose() != context.purpose()
                || wrapped.context_version() != context.version()
            {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::ContextMismatch,
                ));
            }
            if metadata_version(wrapped)? != self.key_version {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::KeyVersionUnavailable,
                ));
            }
            let nonce = wrapped.wrapping_nonce().ok_or_else(|| {
                KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::InvalidCiphertext)
            })?;
            if nonce.len() != AES_GCM_NONCE_BYTES {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::InvalidCiphertext,
                ));
            }
            let wrapping_key = self.derive_wrapping_key(context)?;
            let cipher = Aes256Gcm::new_from_slice(wrapping_key.as_slice()).map_err(|_| {
                KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::ProviderFailure)
            })?;
            let aad = self.wrapping_aad(context)?;
            let plaintext = cipher
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: wrapped.wrapped_dek(),
                        aad: &aad,
                    },
                )
                .map_err(|_| {
                    KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::ContextMismatch)
                })?;
            PlaintextDek::new(plaintext)
        })
    }

    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek> {
        Box::pin(async move {
            validate_dek_len(bytes, KeyOperation::GenerateDek)?;
            context.validate()?;
            let mut plaintext = vec![0u8; bytes];
            rand::thread_rng().fill_bytes(&mut plaintext);
            let plaintext = PlaintextDek::new(plaintext)?;
            let wrapped_key = self.wrap_dek(&plaintext, context).await?;
            GeneratedDek::new(plaintext, wrapped_key)
        })
    }

    fn rotate<'a>(&'a self, _context: &'a KeyContext) -> KeyFuture<'a, RotationInfo> {
        Box::pin(async {
            Err(KeyError::new(
                KeyOperation::Rotate,
                KeyFailureClass::UnsupportedOperation,
            ))
        })
    }

    fn health_check<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, HealthStatus> {
        Box::pin(async move {
            context.validate()?;
            let canary = PlaintextDek::new(vec![0xA5; MASTER_KEY_BYTES])?;
            let wrapped = self.wrap_dek(&canary, context).await?;
            let unwrapped = self.unwrap_dek(&wrapped, context).await?;
            if unwrapped.expose_secret() != canary.expose_secret() {
                return Ok(HealthStatus::Unavailable {
                    class: KeyFailureClass::ProviderFailure,
                    mode: DegradedMode::FailClosed,
                });
            }
            Ok(HealthStatus::Ready)
        })
    }
}

use std::{collections::BTreeMap, fmt, future::Future, pin::Pin, sync::Arc};

use chrono::Utc;
use zeroize::{Zeroize, Zeroizing};

use super::{
    validate_dek_len, validate_kek_ref, GeneratedDek, HealthStatus, KeyContext, KeyError,
    KeyFailureClass, KeyFuture, KeyOperation, KeyProvider, KeyProviderKind, PlaintextDek,
    RotationInfo, WrappedKey,
};

const AWS_AES_256_DEK_BYTES: usize = 32;

/// Redacted error returned by the injectable AWS KMS transport boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AwsKmsClientError {
    class: KeyFailureClass,
}

impl AwsKmsClientError {
    pub fn new(class: KeyFailureClass) -> Self {
        Self { class }
    }

    pub fn class(self) -> KeyFailureClass {
        self.class
    }
}

impl fmt::Display for AwsKmsClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AWS KMS request failed with {:?}", self.class)
    }
}

impl std::error::Error for AwsKmsClientError {}

pub struct AwsKmsGenerateDataKeyOutput {
    pub plaintext: Zeroizing<Vec<u8>>,
    pub ciphertext: Vec<u8>,
    pub key_id: String,
    pub key_material_id: Option<String>,
}

pub struct AwsKmsWrapOutput {
    pub ciphertext: Vec<u8>,
    pub key_id: String,
}

pub struct AwsKmsDecryptOutput {
    pub plaintext: Zeroizing<Vec<u8>>,
    pub key_id: Option<String>,
    pub key_material_id: Option<String>,
}

pub type AwsKmsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AwsKmsClientError>> + Send + 'a>>;

/// Minimal KMS operation set. Tests and emulators can implement this without the AWS SDK.
pub trait AwsKmsClient: Send + Sync {
    fn generate_data_key<'a>(
        &'a self,
        key_id: &'a str,
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsGenerateDataKeyOutput>;

    fn encrypt<'a>(
        &'a self,
        key_id: &'a str,
        plaintext: &'a [u8],
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsWrapOutput>;

    fn decrypt<'a>(
        &'a self,
        key_id: &'a str,
        ciphertext: &'a [u8],
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsDecryptOutput>;
}

/// AWS KMS envelope provider. Construction is valid for hosted mode and never falls back to env.
pub struct AwsKmsProvider {
    client: Arc<dyn AwsKmsClient>,
    key_id: String,
}

impl AwsKmsProvider {
    pub fn new(client: Arc<dyn AwsKmsClient>, key_id: impl Into<String>) -> Result<Self, KeyError> {
        let key_id = key_id.into();
        validate_kek_ref(&key_id)?;
        Ok(Self { client, key_id })
    }

    /// Build the production SDK client from the standard AWS region and
    /// credential provider chains.
    #[cfg(feature = "kms-aws")]
    pub async fn from_environment(key_id: impl Into<String>) -> Result<Self, KeyError> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::v2026_01_12())
            .load()
            .await;
        let client = aws_sdk_kms::Client::new(&config);
        Self::new(Arc::new(AwsSdkKmsClient::new(client)), key_id)
    }

    fn encryption_context(
        &self,
        context: &KeyContext,
    ) -> Result<BTreeMap<String, String>, KeyError> {
        context.validate()?;
        let mut values = BTreeMap::new();
        values.insert(
            "fortemi_context_version".to_string(),
            context.version().to_string(),
        );
        values.insert("purpose".to_string(), context.purpose().name().to_string());
        values.insert("provider_kind".to_string(), "aws-kms".to_string());
        values.insert("kek_ref".to_string(), self.key_id.clone());
        values.insert("schema".to_string(), context.schema().to_string());
        if let Some(value) = context.tenant_id() {
            values.insert("tenant_id".to_string(), value.to_string());
        }
        if let Some(value) = context.user_id() {
            values.insert("user_id".to_string(), value.to_string());
        }
        if let Some(value) = context.resource_id() {
            values.insert("resource_id".to_string(), value.to_string());
        }
        Ok(values)
    }

    fn wrapped_key(
        &self,
        context: &KeyContext,
        ciphertext: Vec<u8>,
        returned_key_id: String,
        key_material_id: Option<String>,
        operation: KeyOperation,
    ) -> Result<WrappedKey, KeyError> {
        if returned_key_id.is_empty() {
            return Err(KeyError::new(operation, KeyFailureClass::ProviderFailure));
        }
        let mut metadata = BTreeMap::new();
        metadata.insert("key_id".to_string(), returned_key_id);
        if let Some(value) = key_material_id.filter(|value| !value.is_empty()) {
            metadata.insert("key_material_id".to_string(), value);
        }
        WrappedKey::new(
            KeyProviderKind::AwsKms,
            self.key_id.clone(),
            context.purpose().clone(),
            context.version(),
            ciphertext,
            None,
            metadata,
            Utc::now(),
        )
    }

    fn validate_binding(&self, wrapped: &WrappedKey, context: &KeyContext) -> Result<(), KeyError> {
        wrapped.validate()?;
        context.validate()?;
        if wrapped.provider_kind() != &KeyProviderKind::AwsKms
            || wrapped.kek_ref() != self.key_id
            || wrapped.purpose() != context.purpose()
            || wrapped.context_version() != context.version()
        {
            return Err(KeyError::new(
                KeyOperation::UnwrapDek,
                KeyFailureClass::ContextMismatch,
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for AwsKmsProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AwsKmsProvider")
            .field("client", &"[REDACTED]")
            .field("key_id", &"[REDACTED]")
            .finish()
    }
}

impl KeyProvider for AwsKmsProvider {
    fn kind(&self) -> KeyProviderKind {
        KeyProviderKind::AwsKms
    }

    fn wrap_dek<'a>(
        &'a self,
        plaintext_dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            validate_dek_len(plaintext_dek.len(), KeyOperation::WrapDek)?;
            let encryption_context = self.encryption_context(context)?;
            let output = self
                .client
                .encrypt(
                    &self.key_id,
                    plaintext_dek.expose_secret(),
                    &encryption_context,
                )
                .await
                .map_err(|error| KeyError::new(KeyOperation::WrapDek, error.class()))?;
            self.wrapped_key(
                context,
                output.ciphertext,
                output.key_id,
                None,
                KeyOperation::WrapDek,
            )
        })
    }

    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        Box::pin(async move {
            self.validate_binding(wrapped, context)?;
            let encryption_context = self.encryption_context(context)?;
            let output = self
                .client
                .decrypt(&self.key_id, wrapped.wrapped_dek(), &encryption_context)
                .await
                .map_err(|error| KeyError::new(KeyOperation::UnwrapDek, error.class()))?;
            if output
                .key_id
                .as_deref()
                .is_some_and(|value| value.is_empty())
            {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::ProviderFailure,
                ));
            }
            PlaintextDek::new(output.plaintext.to_vec())
        })
    }

    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek> {
        Box::pin(async move {
            validate_dek_len(bytes, KeyOperation::GenerateDek)?;
            if bytes != AWS_AES_256_DEK_BYTES {
                return Err(KeyError::new(
                    KeyOperation::GenerateDek,
                    KeyFailureClass::InvalidConfiguration,
                ));
            }
            let encryption_context = self.encryption_context(context)?;
            let mut output = self
                .client
                .generate_data_key(&self.key_id, &encryption_context)
                .await
                .map_err(|error| KeyError::new(KeyOperation::GenerateDek, error.class()))?;
            if output.plaintext.len() != bytes {
                output.plaintext.zeroize();
                return Err(KeyError::new(
                    KeyOperation::GenerateDek,
                    KeyFailureClass::ProviderFailure,
                ));
            }
            let plaintext = PlaintextDek::new(output.plaintext.to_vec())?;
            output.plaintext.zeroize();
            let wrapped_key = self.wrapped_key(
                context,
                output.ciphertext,
                output.key_id,
                output.key_material_id,
                KeyOperation::GenerateDek,
            )?;
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
            let generated = self.generate_dek(context, AWS_AES_256_DEK_BYTES).await?;
            let unwrapped = self.unwrap_dek(generated.wrapped_key(), context).await?;
            if generated.plaintext().expose_secret() != unwrapped.expose_secret() {
                return Ok(HealthStatus::Unavailable {
                    class: KeyFailureClass::ProviderFailure,
                    mode: super::DegradedMode::FailClosed,
                });
            }
            Ok(HealthStatus::Ready)
        })
    }
}

#[cfg(feature = "kms-aws")]
mod sdk {
    use std::collections::HashMap;

    use aws_sdk_kms::{
        error::ProvideErrorMetadata,
        primitives::Blob,
        types::{DataKeySpec, EncryptionAlgorithmSpec},
        Client,
    };

    use super::*;

    #[derive(Clone, Debug)]
    pub struct AwsSdkKmsClient {
        client: Client,
    }

    impl AwsSdkKmsClient {
        pub fn new(client: Client) -> Self {
            Self { client }
        }
    }

    fn context(values: &BTreeMap<String, String>) -> HashMap<String, String> {
        values.clone().into_iter().collect()
    }

    fn classify_code(code: Option<&str>) -> KeyFailureClass {
        match code {
            Some("AccessDeniedException") => KeyFailureClass::AccessDenied,
            Some("DisabledException" | "KMSInvalidStateException") => KeyFailureClass::KeyDisabled,
            Some("InvalidCiphertextException" | "IncorrectKeyException") => {
                KeyFailureClass::ContextMismatch
            }
            Some("NotFoundException") => KeyFailureClass::KeyVersionUnavailable,
            Some("ThrottlingException") => KeyFailureClass::Throttled,
            Some("DependencyTimeoutException" | "KeyUnavailableException") => {
                KeyFailureClass::ProviderUnavailable
            }
            _ => KeyFailureClass::ProviderFailure,
        }
    }

    fn classify_error<E, R>(error: &aws_sdk_kms::error::SdkError<E, R>) -> KeyFailureClass
    where
        E: ProvideErrorMetadata,
    {
        match error {
            aws_sdk_kms::error::SdkError::TimeoutError(_)
            | aws_sdk_kms::error::SdkError::DispatchFailure(_) => {
                KeyFailureClass::ProviderUnavailable
            }
            aws_sdk_kms::error::SdkError::ConstructionFailure(_) => {
                KeyFailureClass::InvalidConfiguration
            }
            aws_sdk_kms::error::SdkError::ServiceError(_) => classify_code(error.code()),
            aws_sdk_kms::error::SdkError::ResponseError(_) => KeyFailureClass::ProviderFailure,
            _ => KeyFailureClass::ProviderFailure,
        }
    }

    impl AwsKmsClient for AwsSdkKmsClient {
        fn generate_data_key<'a>(
            &'a self,
            key_id: &'a str,
            encryption_context: &'a BTreeMap<String, String>,
        ) -> AwsKmsFuture<'a, AwsKmsGenerateDataKeyOutput> {
            Box::pin(async move {
                let output = self
                    .client
                    .generate_data_key()
                    .key_id(key_id)
                    .key_spec(DataKeySpec::Aes256)
                    .set_encryption_context(Some(context(encryption_context)))
                    .send()
                    .await
                    .map_err(|error| AwsKmsClientError::new(classify_error(&error)))?;
                let plaintext = output
                    .plaintext
                    .ok_or_else(|| AwsKmsClientError::new(KeyFailureClass::ProviderFailure))?;
                let ciphertext = output
                    .ciphertext_blob
                    .ok_or_else(|| AwsKmsClientError::new(KeyFailureClass::ProviderFailure))?;
                Ok(AwsKmsGenerateDataKeyOutput {
                    plaintext: Zeroizing::new(plaintext.into_inner()),
                    ciphertext: ciphertext.into_inner(),
                    key_id: output
                        .key_id
                        .ok_or_else(|| AwsKmsClientError::new(KeyFailureClass::ProviderFailure))?,
                    key_material_id: output.key_material_id,
                })
            })
        }

        fn encrypt<'a>(
            &'a self,
            key_id: &'a str,
            plaintext: &'a [u8],
            encryption_context: &'a BTreeMap<String, String>,
        ) -> AwsKmsFuture<'a, AwsKmsWrapOutput> {
            Box::pin(async move {
                let output = self
                    .client
                    .encrypt()
                    .key_id(key_id)
                    .plaintext(Blob::new(plaintext))
                    .encryption_algorithm(EncryptionAlgorithmSpec::SymmetricDefault)
                    .set_encryption_context(Some(context(encryption_context)))
                    .send()
                    .await
                    .map_err(|error| AwsKmsClientError::new(classify_error(&error)))?;
                Ok(AwsKmsWrapOutput {
                    ciphertext: output
                        .ciphertext_blob
                        .ok_or_else(|| AwsKmsClientError::new(KeyFailureClass::ProviderFailure))?
                        .into_inner(),
                    key_id: output
                        .key_id
                        .ok_or_else(|| AwsKmsClientError::new(KeyFailureClass::ProviderFailure))?,
                })
            })
        }

        fn decrypt<'a>(
            &'a self,
            key_id: &'a str,
            ciphertext: &'a [u8],
            encryption_context: &'a BTreeMap<String, String>,
        ) -> AwsKmsFuture<'a, AwsKmsDecryptOutput> {
            Box::pin(async move {
                let output = self
                    .client
                    .decrypt()
                    .key_id(key_id)
                    .ciphertext_blob(Blob::new(ciphertext))
                    .encryption_algorithm(EncryptionAlgorithmSpec::SymmetricDefault)
                    .set_encryption_context(Some(context(encryption_context)))
                    .send()
                    .await
                    .map_err(|error| AwsKmsClientError::new(classify_error(&error)))?;
                Ok(AwsKmsDecryptOutput {
                    plaintext: Zeroizing::new(
                        output
                            .plaintext
                            .ok_or_else(|| {
                                AwsKmsClientError::new(KeyFailureClass::ProviderFailure)
                            })?
                            .into_inner(),
                    ),
                    key_id: output.key_id,
                    key_material_id: output.key_material_id,
                })
            })
        }
    }
}

#[cfg(feature = "kms-aws")]
pub use sdk::AwsSdkKmsClient;

#[cfg(test)]
mod tests;

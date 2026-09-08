//! OpenBao Transit envelope encryption. Runtime credentials cannot create or rotate keys.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Method, Url,
};
use serde::de::DeserializeOwned;
use std::{fs::OpenOptions, io::Read, path::PathBuf, time::Duration};

const MAX_RESPONSE: usize = 64 * 1024;
fn fail(op: KeyOperation, class: KeyFailureClass) -> KeyError {
    KeyError::new(op, class)
}
fn config_error() -> KeyError {
    fail(
        KeyOperation::HealthCheck,
        KeyFailureClass::InvalidConfiguration,
    )
}
fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Validated, non-secret Transit configuration. Tokens are read from the protected file per request.
#[derive(Clone)]
pub struct VaultTransitConfig {
    address: Url,
    namespace: Option<String>,
    mount: String,
    key: String,
    per_purpose: bool,
    token_file: PathBuf,
    ca_bundle: Option<PathBuf>,
    timeout: Duration,
}
impl fmt::Debug for VaultTransitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultTransitConfig").finish_non_exhaustive()
    }
}
impl VaultTransitConfig {
    pub fn from_env() -> Result<Self, KeyError> {
        let mut invalid = false;
        let result = Self::from_lookup(|name| match std::env::var(name) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                invalid = true;
                None
            }
        });
        if invalid {
            return Err(config_error());
        }
        result
    }
    /// Construct without changing process-global environment (also useful to embedders).
    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, KeyError> {
        let address = lookup("FORTEMI_VAULT_ADDR").ok_or_else(config_error)?;
        let address = Url::parse(&address).map_err(|_| config_error())?;
        if address.scheme() != "https"
            || address.host_str().is_none()
            || !address.username().is_empty()
            || address.password().is_some()
            || address.query().is_some()
            || address.fragment().is_some()
            || address.path() != "/"
        {
            return Err(config_error());
        }
        let mount = lookup("FORTEMI_VAULT_TRANSIT_MOUNT").unwrap_or_else(|| "transit".into());
        let key = lookup("FORTEMI_VAULT_TRANSIT_KEY").ok_or_else(config_error)?;
        let namespace = lookup("FORTEMI_VAULT_NAMESPACE");
        if !safe_segment(&mount)
            || !safe_segment(&key)
            || namespace
                .as_ref()
                .is_some_and(|n| n.len() > 256 || !n.split('/').all(safe_segment))
        {
            return Err(config_error());
        }
        let per_purpose = match lookup("FORTEMI_KEY_STRATEGY")
            .as_deref()
            .unwrap_or("per-purpose")
        {
            "per-purpose" => true,
            "shared-with-context" => false,
            _ => return Err(config_error()),
        };
        if lookup("FORTEMI_KEY_CONTEXT_VERSION")
            .as_deref()
            .unwrap_or("1")
            != "1"
            || lookup("FORTEMI_VAULT_AUTH_METHOD").as_deref() != Some("token-file")
        {
            return Err(config_error());
        }
        let token_file =
            PathBuf::from(lookup("FORTEMI_VAULT_TOKEN_FILE").ok_or_else(config_error)?);
        if !token_file.is_absolute() {
            return Err(config_error());
        }
        let ca_bundle = lookup("FORTEMI_VAULT_CA_BUNDLE").map(PathBuf::from);
        if ca_bundle.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err(config_error());
        }
        let timeout = lookup("FORTEMI_VAULT_TIMEOUT_SECONDS")
            .unwrap_or_else(|| "10".into())
            .parse::<u64>()
            .map_err(|_| config_error())?;
        if !(1..=120).contains(&timeout) {
            return Err(config_error());
        }
        Ok(Self {
            address,
            namespace,
            mount,
            key,
            per_purpose,
            token_file,
            ca_bundle,
            timeout: Duration::from_secs(timeout),
        })
    }
    fn token(&self) -> Result<Zeroizing<String>, KeyError> {
        #[cfg(unix)]
        {
            use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
            // Open every component relative to an already opened directory: neither the final
            // sink nor a parent symlink may redirect credential reads.
            use std::os::fd::{AsRawFd, FromRawFd};
            let mut directory = std::fs::File::open("/").map_err(|_| config_error())?;
            let parts: Vec<_> = self.token_file.components().collect();
            let mut file = None;
            for (index, component) in parts.iter().enumerate().skip(1) {
                let std::path::Component::Normal(name) = component else {
                    return Err(config_error());
                };
                let name = std::ffi::CString::new(name.as_bytes()).map_err(|_| config_error())?;
                let last = index == parts.len() - 1;
                let flags = libc::O_RDONLY
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | if last { 0 } else { libc::O_DIRECTORY };
                // SAFETY: directory and C string remain alive; successful fd is uniquely owned.
                let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
                if fd < 0 {
                    return Err(config_error());
                }
                let opened = unsafe { std::fs::File::from_raw_fd(fd) };
                if last {
                    file = Some(opened);
                } else {
                    directory = opened;
                }
            }
            let file = file.ok_or_else(config_error)?;
            let metadata = file.metadata().map_err(|_| config_error())?;
            if !metadata.is_file()
                || metadata.mode() & 0o7177 != 0
                || metadata.nlink() != 1
                || (metadata.uid() != 0 && metadata.uid() != unsafe { libc::geteuid() })
            {
                return Err(config_error());
            }
            let mut bytes = Zeroizing::new(Vec::new());
            file.take(8193)
                .read_to_end(&mut bytes)
                .map_err(|_| config_error())?;
            if bytes.len() > 8192 {
                return Err(config_error());
            }
            let value = std::str::from_utf8(&bytes)
                .map_err(|_| config_error())?
                .trim_end_matches(['\r', '\n']);
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(config_error());
            }
            Ok(Zeroizing::new(value.to_owned()))
        }
        #[cfg(not(unix))]
        {
            Err(config_error())
        }
    }
}

/// HTTPS Transit provider with no DEK cache and independent additive CA trust.
pub struct VaultTransitProvider {
    config: VaultTransitConfig,
    client: Client,
}
impl fmt::Debug for VaultTransitProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultTransitProvider")
            .finish_non_exhaustive()
    }
}
impl VaultTransitProvider {
    pub fn new(config: VaultTransitConfig) -> Result<Self, KeyError> {
        let _ = config.token()?;
        let mut builder = Client::builder()
            .use_rustls_tls()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(config.timeout)
            .connect_timeout(config.timeout)
            .no_proxy();
        if let Some(path) = &config.ca_bundle {
            let mut bytes = Vec::new();
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC);
            }
            let file = options.open(path).map_err(|_| config_error())?;
            if !file.metadata().map_err(|_| config_error())?.is_file() {
                return Err(config_error());
            }
            file.take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| config_error())?;
            if bytes.len() > 1024 * 1024 {
                return Err(config_error());
            }
            for cert in parse_ca_bundle(&bytes)? {
                builder = builder.add_root_certificate(cert);
            }
        }
        Ok(Self {
            config,
            client: builder.build().map_err(|_| config_error())?,
        })
    }
    fn key(&self, context: &KeyContext) -> Result<String, KeyError> {
        context.validate()?;
        let key = if self.config.per_purpose {
            format!("{}-{}", self.config.key, context.purpose().name())
        } else {
            self.config.key.clone()
        };
        if !safe_segment(&key) {
            return Err(config_error());
        }
        Ok(key)
    }
    fn reference(&self, key: &str) -> String {
        format!(
            "{}|{}|{}/{}",
            self.config.address,
            self.config.namespace.as_deref().unwrap_or(""),
            self.config.mount,
            key
        )
    }
    fn binding(&self, context: &KeyContext, key: &str) -> Result<String, KeyError> {
        Ok(STANDARD.encode(context.canonical_bytes(
            "fortemi/vault-transit/wrap/v1",
            &KeyProviderKind::VaultTransit,
            &self.reference(key),
        )?))
    }
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&TransitRequest<'_>>,
        op: KeyOperation,
    ) -> Result<T, KeyError> {
        let token = self
            .config
            .token()
            .map_err(|_| fail(op, KeyFailureClass::AccessDenied))?;
        let mut header =
            HeaderValue::from_str(&token).map_err(|_| fail(op, KeyFailureClass::AccessDenied))?;
        header.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-Vault-Token", header);
        if let Some(namespace) = &self.config.namespace {
            headers.insert(
                "X-Vault-Namespace",
                HeaderValue::from_str(namespace).map_err(|_| config_error())?,
            );
        }
        let url = self
            .config
            .address
            .join(&format!("v1/{}/{path}", self.config.mount))
            .map_err(|_| config_error())?;
        let mut request = self.client.request(method, url).headers(headers);
        if let Some(body) = body {
            let serialized = Zeroizing::new(
                serde_json::to_vec(body).map_err(|_| fail(op, KeyFailureClass::ProviderFailure))?,
            );
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(bytes::Bytes::from_owner(serialized));
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| fail(op, KeyFailureClass::ProviderUnavailable))?;
        if !response.status().is_success() {
            return Err(fail(op, classify_status(response.status().as_u16())));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(fail(op, KeyFailureClass::ProviderFailure));
        }
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| fail(op, KeyFailureClass::ProviderUnavailable))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(fail(op, KeyFailureClass::ProviderFailure));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| fail(op, KeyFailureClass::ProviderFailure))
    }
    async fn validate_key(&self, key: &str, op: KeyOperation) -> Result<(), KeyError> {
        let response: Reply<KeyInfo> = self
            .request(Method::GET, &format!("keys/{key}"), None, op)
            .await?;
        let k = response.data;
        if k.kind != "aes256-gcm96"
            || !k.derived
            || k.convergent_encryption
            || k.exportable
            || k.allow_plaintext_backup
            || k.deletion_allowed
            || k.latest_version < 1
        {
            return Err(fail(op, KeyFailureClass::InvalidConfiguration));
        }
        Ok(())
    }
    fn wrapped(
        &self,
        ciphertext: String,
        key: &str,
        context: &KeyContext,
        op: KeyOperation,
    ) -> Result<WrappedKey, KeyError> {
        let version = ciphertext_version(&ciphertext)
            .ok_or_else(|| fail(op, KeyFailureClass::ProviderFailure))?;
        WrappedKey::new(
            self.kind(),
            self.reference(key),
            context.purpose().clone(),
            context.version(),
            ciphertext.into_bytes(),
            None,
            BTreeMap::from([
                ("key_version".into(), version.to_string()),
                ("binding_version".into(), "1".into()),
            ]),
            Utc::now(),
        )
    }
}
#[derive(Serialize)]
struct TransitRequest<'a> {
    context: &'a str,
    associated_data: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    plaintext: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ciphertext: Option<&'a str>,
}
#[derive(Deserialize)]
struct Reply<T> {
    data: T,
}
#[derive(Deserialize)]
struct KeyInfo {
    #[serde(rename = "type")]
    kind: String,
    derived: bool,
    convergent_encryption: bool,
    exportable: bool,
    allow_plaintext_backup: bool,
    deletion_allowed: bool,
    latest_version: u64,
}
#[derive(Deserialize)]
struct DataReply {
    #[serde(default, deserialize_with = "deserialize_secret")]
    plaintext: Zeroizing<String>,
    #[serde(default)]
    ciphertext: String,
}
fn deserialize_secret<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(deserializer).map(Zeroizing::new)
}
impl Drop for DataReply {
    fn drop(&mut self) {
        self.plaintext.zeroize();
        self.ciphertext.zeroize();
    }
}
fn ciphertext_version(value: &str) -> Option<u64> {
    let rest = value.strip_prefix("vault:v")?;
    let (version, data) = rest.split_once(':')?;
    let n = version.parse::<u64>().ok()?;
    if n == 0
        || n.to_string() != version
        || data.is_empty()
        || value.len() > 16384
        || STANDARD.decode(data).ok()?.len() < 28
    {
        return None;
    }
    Some(n)
}
fn classify_status(status: u16) -> KeyFailureClass {
    match status {
        401 | 403 => KeyFailureClass::AccessDenied,
        404 => KeyFailureClass::KeyVersionUnavailable,
        429 => KeyFailureClass::Throttled,
        400 => KeyFailureClass::InvalidCiphertext,
        500..=599 => KeyFailureClass::ProviderUnavailable,
        _ => KeyFailureClass::ProviderFailure,
    }
}
fn parse_ca_bundle(bytes: &[u8]) -> Result<Vec<reqwest::Certificate>, KeyError> {
    let text = std::str::from_utf8(bytes).map_err(|_| config_error())?;
    let mut rest = text.trim();
    let mut certs = Vec::new();
    while !rest.is_empty() {
        if !rest.starts_with("-----BEGIN CERTIFICATE-----") {
            return Err(config_error());
        }
        let end = rest
            .find("-----END CERTIFICATE-----")
            .ok_or_else(config_error)?
            + "-----END CERTIFICATE-----".len();
        let block = &rest[..end];
        let encoded: String = block
            ["-----BEGIN CERTIFICATE-----".len()..block.len() - "-----END CERTIFICATE-----".len()]
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let der = STANDARD.decode(encoded).map_err(|_| config_error())?;
        let (remaining, _) =
            x509_parser::parse_x509_certificate(&der).map_err(|_| config_error())?;
        if !remaining.is_empty() {
            return Err(config_error());
        }
        certs.push(reqwest::Certificate::from_der(&der).map_err(|_| config_error())?);
        rest = rest[end..].trim();
    }
    if certs.is_empty() {
        return Err(config_error());
    }
    Ok(certs)
}
impl KeyProvider for VaultTransitProvider {
    fn kind(&self) -> KeyProviderKind {
        KeyProviderKind::VaultTransit
    }
    fn wrap_dek<'a>(
        &'a self,
        dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            let op = KeyOperation::WrapDek;
            if dek.len() != 32 {
                return Err(fail(op, KeyFailureClass::InvalidConfiguration));
            }
            let key = self.key(context)?;
            self.validate_key(&key, op).await?;
            let binding = self.binding(context, &key)?;
            let plaintext = Zeroizing::new(STANDARD.encode(dek.expose_secret()));
            let mut reply: Reply<DataReply> = self
                .request(
                    Method::POST,
                    &format!("encrypt/{key}"),
                    Some(&TransitRequest {
                        context: &binding,
                        associated_data: &binding,
                        plaintext: Some(&plaintext),
                        ciphertext: None,
                    }),
                    op,
                )
                .await?;
            self.wrapped(
                std::mem::take(&mut reply.data.ciphertext),
                &key,
                context,
                op,
            )
        })
    }
    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        Box::pin(async move {
            let op = KeyOperation::UnwrapDek;
            validate_provider_binding(self, wrapped, context, op)?;
            let key = self.key(context)?;
            let ciphertext = std::str::from_utf8(wrapped.wrapped_dek())
                .map_err(|_| fail(op, KeyFailureClass::InvalidCiphertext))?;
            let version = ciphertext_version(ciphertext)
                .ok_or_else(|| fail(op, KeyFailureClass::InvalidCiphertext))?;
            if wrapped.kek_ref() != self.reference(&key)
                || wrapped.wrapping_nonce().is_some()
                || wrapped.provider_metadata().len() != 2
                || wrapped
                    .provider_metadata()
                    .get("binding_version")
                    .map(String::as_str)
                    != Some("1")
                || wrapped.provider_metadata().get("key_version") != Some(&version.to_string())
            {
                return Err(fail(op, KeyFailureClass::ContextMismatch));
            }
            self.validate_key(&key, op).await?;
            let binding = self.binding(context, &key)?;
            let reply: Reply<DataReply> = self
                .request(
                    Method::POST,
                    &format!("decrypt/{key}"),
                    Some(&TransitRequest {
                        context: &binding,
                        associated_data: &binding,
                        plaintext: None,
                        ciphertext: Some(ciphertext),
                    }),
                    op,
                )
                .await?;
            decode_dek(&reply.data.plaintext, op)
        })
    }
    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek> {
        Box::pin(async move {
            let op = KeyOperation::GenerateDek;
            if bytes != 32 {
                return Err(fail(op, KeyFailureClass::InvalidConfiguration));
            }
            // OpenBao 2.3.x datakey does not bind associated_data. Generate locally
            // with the OS CSPRNG and use encrypt so derivation context AND AEAD AAD
            // remain identical across every wrapping operation.
            context.validate()?;
            let mut bytes = Zeroizing::new(vec![0u8; 32]);
            rand::rngs::OsRng
                .try_fill_bytes(&mut bytes)
                .map_err(|_| fail(op, KeyFailureClass::ProviderFailure))?;
            let plaintext = PlaintextDek::new(std::mem::take(&mut *bytes))?;
            let wrapped = self
                .wrap_dek(&plaintext, context)
                .await
                .map_err(|error| fail(op, error.class()))?;
            GeneratedDek::new(plaintext, wrapped)
        })
    }
    fn rotate<'a>(&'a self, _context: &'a KeyContext) -> KeyFuture<'a, RotationInfo> {
        Box::pin(async {
            Err(fail(
                KeyOperation::Rotate,
                KeyFailureClass::UnsupportedOperation,
            ))
        })
    }
    fn health_check<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, HealthStatus> {
        Box::pin(async move {
            let generated = self.generate_dek(context, 32).await?;
            let unwrapped = self.unwrap_dek(generated.wrapped_key(), context).await?;
            if generated.plaintext().expose_secret() != unwrapped.expose_secret() {
                return Err(fail(
                    KeyOperation::HealthCheck,
                    KeyFailureClass::ProviderFailure,
                ));
            }
            Ok(HealthStatus::Ready)
        })
    }
}
fn decode_dek(value: &str, op: KeyOperation) -> Result<PlaintextDek, KeyError> {
    let mut bytes = Zeroizing::new(
        STANDARD
            .decode(value)
            .map_err(|_| fail(op, KeyFailureClass::ProviderFailure))?,
    );
    if bytes.len() != 32 {
        return Err(fail(op, KeyFailureClass::ProviderFailure));
    }
    PlaintextDek::new(std::mem::take(&mut *bytes))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn values() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "FORTEMI_VAULT_ADDR".into(),
                "https://bao.example.test".into(),
            ),
            ("FORTEMI_VAULT_TRANSIT_KEY".into(), "fortemi".into()),
            ("FORTEMI_VAULT_AUTH_METHOD".into(), "token-file".into()),
            (
                "FORTEMI_VAULT_TOKEN_FILE".into(),
                "/run/fortemi/token".into(),
            ),
        ])
    }
    fn config(values: &BTreeMap<String, String>) -> Result<VaultTransitConfig, KeyError> {
        VaultTransitConfig::from_lookup(|n| values.get(n).cloned())
    }
    #[test]
    fn configuration_rejects_authority_path_strategy_and_version_confusion() {
        assert!(config(&values()).is_ok());
        for (name, value) in [
            ("FORTEMI_VAULT_ADDR", "http://bao.example.test"),
            ("FORTEMI_VAULT_ADDR", "https://user:secret@bao.example.test"),
            ("FORTEMI_VAULT_ADDR", "https://bao.example.test/v1/transit"),
            (
                "FORTEMI_VAULT_ADDR",
                "https://bao.example.test/?token=secret",
            ),
            ("FORTEMI_VAULT_TRANSIT_MOUNT", "../sys"),
            ("FORTEMI_VAULT_TRANSIT_KEY", "a/b"),
            ("FORTEMI_VAULT_NAMESPACE", "ns/../admin"),
            ("FORTEMI_KEY_STRATEGY", "per-tenant"),
            ("FORTEMI_KEY_CONTEXT_VERSION", "2"),
            ("FORTEMI_VAULT_AUTH_METHOD", "token"),
            ("FORTEMI_VAULT_TOKEN_FILE", "relative"),
            ("FORTEMI_VAULT_TIMEOUT_SECONDS", "0"),
            ("FORTEMI_VAULT_TIMEOUT_SECONDS", "121"),
        ] {
            let mut v = values();
            v.insert(name.into(), value.into());
            assert!(config(&v).is_err(), "{name}");
        }
    }
    #[test]
    fn token_is_reloaded_and_unsafe_files_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        std::fs::write(&path, "first-token\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut v = values();
        v.insert(
            "FORTEMI_VAULT_TOKEN_FILE".into(),
            path.display().to_string(),
        );
        let c = config(&v).unwrap();
        assert_eq!(c.token().unwrap().as_str(), "first-token");
        let next = directory.path().join("next");
        std::fs::write(&next, "replacement-token").unwrap();
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o400)).unwrap();
        std::fs::rename(&next, &path).unwrap();
        assert_eq!(c.token().unwrap().as_str(), "replacement-token");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(c.token().is_err());
        std::fs::remove_file(&path).unwrap();
        symlink("missing", &path).unwrap();
        assert!(c.token().is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(c.token().is_err());
    }
    #[test]
    fn canonical_context_separates_all_security_dimensions() {
        let c = config(&values()).unwrap();
        let provider = VaultTransitProvider {
            config: c,
            client: Client::new(),
        };
        let base = KeyContext::new(KeyPurpose::USER_SECRET, "user-secrets-v1")
            .unwrap()
            .with_tenant_id("tenant-a")
            .unwrap()
            .with_user_id("alice")
            .unwrap()
            .with_resource_id("row-a")
            .unwrap();
        let key = provider.key(&base).unwrap();
        assert_eq!(key, "fortemi-user_secret");
        let binding = provider.binding(&base, &key).unwrap();
        for changed in [
            base.clone().with_tenant_id("tenant-b").unwrap(),
            base.clone().with_user_id("bob").unwrap(),
            base.clone().with_resource_id("row-b").unwrap(),
            KeyContext::new(KeyPurpose::CONTENT_BLOB, "user-secrets-v1").unwrap(),
            KeyContext::new(KeyPurpose::USER_SECRET, "different-schema").unwrap(),
        ] {
            assert_ne!(binding, provider.binding(&changed, &key).unwrap());
        }
        assert_ne!(binding, provider.binding(&base, "another-key").unwrap());
        let mut v = values();
        v.insert("FORTEMI_VAULT_NAMESPACE".into(), "other".into());
        let other = VaultTransitProvider {
            config: config(&v).unwrap(),
            client: Client::new(),
        };
        assert_ne!(binding, other.binding(&base, &key).unwrap());
    }
    #[test]
    fn malformed_versions_and_response_classes_fail_closed() {
        let payload = STANDARD.encode([0u8; 28]);
        assert_eq!(ciphertext_version(&format!("vault:v7:{payload}")), Some(7));
        for value in [
            format!("vault:v0:{payload}"),
            format!("vault:v01:{payload}"),
            "vault:v1:AAAA".into(),
            "vault:v1:%%%".into(),
            "aws:v1:AAAA".into(),
        ] {
            assert!(ciphertext_version(&value).is_none());
        }
        assert_eq!(classify_status(403), KeyFailureClass::AccessDenied);
        assert_eq!(classify_status(429), KeyFailureClass::Throttled);
        assert_eq!(classify_status(503), KeyFailureClass::ProviderUnavailable);
        assert_eq!(classify_status(307), KeyFailureClass::ProviderFailure);
        assert!(decode_dek(&STANDARD.encode([0u8; 31]), KeyOperation::UnwrapDek).is_err());
    }
    #[test]
    fn ca_bundle_rejects_empty_private_key_and_non_certificate_data() {
        for bytes in [
            b"".as_slice(),
            b"\n",
            b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----",
            b"junk",
            b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----",
            b"-----BEGIN CERTIFICATE-----\n%%%\n-----END CERTIFICATE-----",
        ] {
            assert!(parse_ca_bundle(bytes).is_err());
        }
    }
    #[test]
    fn certificate_only_bundle_accepts_multiple_roots_and_rejects_junk() {
        let cert = include_str!("../../../../ci/trust/integro-labs-root-ca-g2.crt");
        assert_eq!(parse_ca_bundle(cert.as_bytes()).unwrap().len(), 1);
        assert_eq!(
            parse_ca_bundle(format!("{cert}\n{cert}").as_bytes())
                .unwrap()
                .len(),
            2
        );
        for bad in [
            format!("junk\n{cert}"),
            format!("{cert}junk"),
            format!("{cert}-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----"),
        ] {
            assert!(parse_ca_bundle(bad.as_bytes()).is_err());
        }
    }
    #[tokio::test]
    async fn envelope_metadata_substitution_fails_before_network() {
        let provider = VaultTransitProvider {
            config: config(&values()).unwrap(),
            client: Client::new(),
        };
        let context = KeyContext::new(KeyPurpose::USER_SECRET, "test").unwrap();
        let key = provider.key(&context).unwrap();
        let wrapped = provider
            .wrapped(
                format!("vault:v1:{}", STANDARD.encode([0u8; 28])),
                &key,
                &context,
                KeyOperation::WrapDek,
            )
            .unwrap();
        let serialized = serde_json::to_value(&wrapped).unwrap();
        for (field, value) in [
            ("kek_ref", serde_json::json!("other")),
            ("wrapping_nonce", serde_json::json!([1])),
            (
                "provider_metadata",
                serde_json::json!({"key_version":"2","binding_version":"1"}),
            ),
            (
                "provider_metadata",
                serde_json::json!({"key_version":"1","binding_version":"2"}),
            ),
            (
                "provider_metadata",
                serde_json::json!({"key_version":"1","binding_version":"1","extra":"1"}),
            ),
        ] {
            let mut altered = serialized.clone();
            altered[field] = value;
            let altered: WrappedKey = serde_json::from_value(altered).unwrap();
            assert_eq!(
                provider
                    .unwrap_dek(&altered, &context)
                    .await
                    .unwrap_err()
                    .class(),
                KeyFailureClass::ContextMismatch
            );
        }
    }
}

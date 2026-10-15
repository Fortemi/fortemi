//! Disposable HTTPS discovery/JWKS server and signed synthetic access tokens.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::{
    process::{Child, Command, Stdio},
    sync::{Mutex, Weak},
};

pub(super) struct Issuer {
    directory: tempfile::TempDir,
    server: Mutex<Child>,
    pub url: String,
}
impl Drop for Issuer {
    fn drop(&mut self) {
        let child = self.server.get_mut().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
static ISSUER: Mutex<Weak<Issuer>> = Mutex::new(Weak::new());

pub(super) fn issuer() -> Arc<Issuer> {
    let mut slot = ISSUER.lock().unwrap();
    if let Some(issuer) = slot.upgrade() {
        return issuer;
    }
    let directory = tempfile::tempdir().unwrap();
    let openssl = |args: &[&str]| {
        let output = Command::new("openssl")
            .args(args)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    openssl(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "key.pem",
        "-out",
        "cert.pem",
        "-days",
        "1",
        "-subj",
        "/CN=localhost",
        "-addext",
        "subjectAltName=DNS:localhost",
        "-addext",
        "basicConstraints=critical,CA:FALSE",
    ]);
    let modulus = openssl(&["rsa", "-in", "key.pem", "-noout", "-modulus"]);
    let modulus = hex::decode(
        String::from_utf8(modulus)
            .unwrap()
            .trim()
            .strip_prefix("Modulus=")
            .unwrap(),
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("https://localhost:{port}");
    std::fs::create_dir(directory.path().join(".well-known")).unwrap();
    std::fs::write(directory.path().join(".well-known/openid-configuration"),serde_json::json!({"issuer":url,"jwks_uri":format!("{url}/jwks.json"),"authorization_endpoint":format!("{url}/authorize"),"token_endpoint":format!("{url}/token")}).to_string()).unwrap();
    std::fs::write(directory.path().join("jwks.json"),serde_json::json!({"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"synthetic","n":URL_SAFE_NO_PAD.encode(modulus),"e":"AQAB"}]}).to_string()).unwrap();
    std::fs::write(directory.path().join("policy.json"),serde_json::json!({"scope_mapping":{"claim":"groups","rules":[{"id":"read","value":"reader","scopes":["read"]},{"id":"write","value":"writer","scopes":["read","write"]},{"id":"admin","value":"admin","scopes":["read","write","admin","mcp"]}]}}).to_string()).unwrap();
    let server = Command::new("openssl")
        .args([
            "s_server",
            "-accept",
            &format!("127.0.0.1:{port}"),
            "-cert",
            "cert.pem",
            "-key",
            "key.pem",
            "-WWW",
            "-quiet",
        ])
        .current_dir(directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let value = Arc::new(Issuer {
        directory,
        server: Mutex::new(server),
        url,
    });
    *slot = Arc::downgrade(&value);
    value
}

impl Issuer {
    pub fn token(&self, overrides: serde_json::Value) -> String {
        let now = Utc::now().timestamp();
        let mut claims = serde_json::json!({"iss":self.url,"sub":"external-user","aud":"https://api.example","iat":now,"nbf":now-1,"exp":now+300,"token_use":"access","groups":["admin"]});
        claims
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"at+jwt","kid":"synthetic"}"#);
        let signing = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let input = tempfile::NamedTempFile::new_in(self.directory.path()).unwrap();
        std::fs::write(input.path(), signing.as_bytes()).unwrap();
        let result = Command::new("openssl")
            .args(["dgst", "-sha256", "-sign"])
            .arg(self.directory.path().join("key.pem"))
            .arg(input.path())
            .output()
            .unwrap();
        assert!(result.status.success());
        format!("{signing}.{}", URL_SAFE_NO_PAD.encode(result.stdout))
    }
    pub fn config(&self, tenant: Uuid) -> HostedAuthConfig {
        HostedAuthConfig::from_env(|name| match name {
            "FORTEMI_AUTH_ISSUER" => Some(self.url.clone()),
            "FORTEMI_AUTH_AUDIENCE" => Some("https://api.example".into()),
            "FORTEMI_AUTH_DEFAULT_TENANT" => Some(tenant.to_string()),
            "FORTEMI_AUTH_CLOCK_SKEW_SECONDS" => Some("0".into()),
            "FORTEMI_AUTH_CA_BUNDLE" => Some(
                self.directory
                    .path()
                    .join("cert.pem")
                    .to_str()
                    .unwrap()
                    .into(),
            ),
            "FORTEMI_AUTH_CLAIM_POLICY_FILE" => Some(
                self.directory
                    .path()
                    .join("policy.json")
                    .to_str()
                    .unwrap()
                    .into(),
            ),
            _ => None,
        })
        .unwrap()
    }
}

/// Retain the disposable issuer for the lifetime of the production verifier.
struct SignedProvider {
    provider: Arc<dyn HostedAuthenticator>,
    _issuer: Arc<Issuer>,
}
#[async_trait::async_trait]
impl HostedAuthenticator for SignedProvider {
    async fn authenticate(
        &self,
        token: &str,
    ) -> Result<fortemi_auth_core::AuthContext, fortemi_auth_core::AuthError> {
        self.provider.authenticate(token).await
    }
}
pub(super) fn authenticator(pool: sqlx::PgPool, tenant: Uuid) -> Arc<dyn HostedAuthenticator> {
    let issuer = issuer();
    let options = matric_api::hosted_claim_policy::ClaimPolicyLoadOptions {
        required: true,
        allow_token_scopes: false,
        forbidden_audiences: &Default::default(),
    };
    let provider = matric_api::hosted_auth::build_clerk_authenticator_with_claim_policy_options(
        &issuer.config(tenant),
        pool,
        &options,
    )
    .unwrap();
    Arc::new(SignedProvider {
        provider,
        _issuer: issuer,
    })
}
pub(super) fn bearer() -> String {
    format!("Bearer {}", issuer().token(serde_json::json!({})))
}

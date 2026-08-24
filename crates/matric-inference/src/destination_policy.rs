//! Shared outbound destination policy for inference providers.

use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::{Client, Url};
use thiserror::Error;

/// Trust source for an inference destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationSource {
    BuiltInDefault,
    OperatorConfiguration,
    CallerRequest,
}

/// Policy profile selected for an approved destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationProfile {
    PublicProviderApi,
    OperatorLocalProvider,
    TenantConfiguredProvider,
}

/// Stable, non-enumerating destination-policy errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DestinationPolicyError {
    #[error("destination URL is invalid")]
    InvalidUrl,
    #[error("destination URL contains prohibited components")]
    ProhibitedUrlComponent,
    #[error("destination scheme is denied")]
    SchemeDenied,
    #[error("caller-controlled destinations are denied")]
    CallerDestinationDenied,
    #[error("destination host or port is not approved")]
    DestinationDenied,
    #[error("destination address is denied")]
    AddressDenied,
    #[error("destination resolution failed")]
    ResolutionFailed,
    #[error("destination TLS policy is denied")]
    TlsPolicyDenied,
    #[error("outbound HTTP client construction failed")]
    ClientBuildFailed,
    #[error("destination allowlist configuration is invalid")]
    InvalidAllowlist,
}

impl DestinationPolicyError {
    /// Stable reason suitable for metadata-only logs and audit events.
    pub const fn reason_code(self) -> &'static str {
        match self {
            Self::InvalidUrl => "invalid_url",
            Self::ProhibitedUrlComponent => "prohibited_url_component",
            Self::SchemeDenied => "scheme_denied",
            Self::CallerDestinationDenied => "caller_destination_denied",
            Self::DestinationDenied => "destination_denied",
            Self::AddressDenied => "address_denied",
            Self::ResolutionFailed => "resolution_failed",
            Self::TlsPolicyDenied => "tls_policy_denied",
            Self::ClientBuildFailed => "client_build_failed",
            Self::InvalidAllowlist => "invalid_allowlist",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddressClass {
    Public,
    PrivateOrLoopback,
    AlwaysDenied,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AllowedDestination {
    host: String,
    port: u16,
}

/// Deployment-aware policy for inference/provider destinations.
#[derive(Clone)]
pub struct OutboundDestinationPolicy {
    hosted: bool,
    allowed_custom: HashSet<AllowedDestination>,
}

impl fmt::Debug for OutboundDestinationPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutboundDestinationPolicy")
            .field("hosted", &self.hosted)
            .field("allowed_custom_count", &self.allowed_custom.len())
            .finish()
    }
}

impl OutboundDestinationPolicy {
    /// Construct policy from deployment mode and the operator allowlist.
    ///
    /// `FORTEMI_INFERENCE_ALLOWED_DESTINATIONS` is a comma-separated list of
    /// exact `host` or `host:port` entries. Hosted custom destinations still
    /// require HTTPS and public DNS answers.
    pub fn from_env(hosted: bool) -> Result<Self, DestinationPolicyError> {
        Self::new(
            hosted,
            std::env::var("FORTEMI_INFERENCE_ALLOWED_DESTINATIONS")
                .ok()
                .as_deref(),
        )
    }

    pub fn new(hosted: bool, allowlist: Option<&str>) -> Result<Self, DestinationPolicyError> {
        let mut allowed_custom = HashSet::new();
        if let Some(entries) = allowlist {
            for raw in entries.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                allowed_custom.insert(parse_allowlist_entry(raw)?);
            }
        }
        Ok(Self {
            hosted,
            allowed_custom,
        })
    }

    pub const fn is_hosted(&self) -> bool {
        self.hosted
    }

    /// Resolve and authorize a destination before any credential is attached.
    pub async fn authorize(
        &self,
        provider: &str,
        raw_url: &str,
        source: DestinationSource,
    ) -> Result<ApprovedDestination, DestinationPolicyError> {
        let candidate = self.validate(provider, raw_url, source)?;
        let port = candidate
            .url
            .port_or_known_default()
            .ok_or(DestinationPolicyError::InvalidUrl)?;

        let resolved = if let Ok(ip) = candidate.host.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, port)]
        } else {
            tokio::net::lookup_host((candidate.host.as_str(), port))
                .await
                .map_err(|_| DestinationPolicyError::ResolutionFailed)?
                .collect::<Vec<_>>()
        };
        self.approve_resolved(candidate, resolved)
    }

    fn validate(
        &self,
        provider: &str,
        raw_url: &str,
        source: DestinationSource,
    ) -> Result<CandidateDestination, DestinationPolicyError> {
        if self.hosted && source == DestinationSource::CallerRequest {
            return Err(DestinationPolicyError::CallerDestinationDenied);
        }

        let mut url = Url::parse(raw_url).map_err(|_| DestinationPolicyError::InvalidUrl)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(DestinationPolicyError::ProhibitedUrlComponent);
        }

        let host = url
            .host_str()
            .ok_or(DestinationPolicyError::InvalidUrl)?
            .trim_matches(['[', ']'])
            .to_ascii_lowercase();
        let port = url
            .port_or_known_default()
            .ok_or(DestinationPolicyError::InvalidUrl)?;
        url.set_fragment(None);

        let provider = provider.to_ascii_lowercase();
        let official = official_destination(&provider, &host, port);
        let custom = self.allowed_custom.contains(&AllowedDestination {
            host: host.clone(),
            port,
        });

        let profile = if self.hosted {
            if url.scheme() != "https" {
                return Err(DestinationPolicyError::SchemeDenied);
            }
            if official {
                DestinationProfile::PublicProviderApi
            } else if custom {
                DestinationProfile::TenantConfiguredProvider
            } else {
                return Err(DestinationPolicyError::DestinationDenied);
            }
        } else {
            if url.scheme() != "http" && url.scheme() != "https" {
                return Err(DestinationPolicyError::SchemeDenied);
            }
            if official {
                DestinationProfile::PublicProviderApi
            } else if source == DestinationSource::CallerRequest && !custom {
                return Err(DestinationPolicyError::DestinationDenied);
            } else {
                DestinationProfile::OperatorLocalProvider
            }
        };

        Ok(CandidateDestination {
            url,
            host,
            provider,
            profile,
            source,
        })
    }

    fn approve_resolved(
        &self,
        candidate: CandidateDestination,
        mut resolved: Vec<SocketAddr>,
    ) -> Result<ApprovedDestination, DestinationPolicyError> {
        resolved.sort_unstable();
        resolved.dedup();
        if resolved.is_empty() {
            return Err(DestinationPolicyError::ResolutionFailed);
        }

        let addresses_allowed = resolved.iter().all(|addr| match candidate.profile {
            DestinationProfile::PublicProviderApi
            | DestinationProfile::TenantConfiguredProvider => {
                classify_address(addr.ip()) == AddressClass::Public
            }
            DestinationProfile::OperatorLocalProvider => {
                classify_address(addr.ip()) != AddressClass::AlwaysDenied
            }
        });
        if !addresses_allowed {
            return Err(DestinationPolicyError::AddressDenied);
        }

        Ok(ApprovedDestination {
            url: candidate.url,
            host: candidate.host,
            provider: candidate.provider,
            profile: candidate.profile,
            source: candidate.source,
            resolved,
            hosted: self.hosted,
        })
    }
}

struct CandidateDestination {
    url: Url,
    host: String,
    provider: String,
    profile: DestinationProfile,
    source: DestinationSource,
}

impl fmt::Debug for CandidateDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CandidateDestination")
            .field("provider", &self.provider)
            .field("profile", &self.profile)
            .field("source", &self.source)
            .finish()
    }
}

/// A normalized destination plus the exact DNS answers approved for use.
pub struct ApprovedDestination {
    url: Url,
    host: String,
    provider: String,
    profile: DestinationProfile,
    source: DestinationSource,
    resolved: Vec<SocketAddr>,
    hosted: bool,
}

impl fmt::Debug for ApprovedDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApprovedDestination")
            .field("provider", &self.provider)
            .field("profile", &self.profile)
            .field("source", &self.source)
            .field("resolved_count", &self.resolved.len())
            .field("hosted", &self.hosted)
            .finish()
    }
}

impl ApprovedDestination {
    pub fn base_url(&self) -> &str {
        self.url.as_str()
    }

    pub const fn profile(&self) -> DestinationProfile {
        self.profile
    }

    pub const fn source(&self) -> DestinationSource {
        self.source
    }

    /// Build a client that cannot follow redirects or inherit process proxies.
    /// Domain destinations are pinned to the addresses approved above.
    pub fn build_client(
        &self,
        timeout: Duration,
        allow_invalid_tls: bool,
    ) -> Result<Client, DestinationPolicyError> {
        if self.hosted && allow_invalid_tls {
            return Err(DestinationPolicyError::TlsPolicyDenied);
        }

        let mut builder = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy();
        if allow_invalid_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if self.host.parse::<IpAddr>().is_err() {
            builder = builder.resolve_to_addrs(&self.host, &self.resolved);
        }
        builder
            .build()
            .map_err(|_| DestinationPolicyError::ClientBuildFailed)
    }
}

fn parse_allowlist_entry(raw: &str) -> Result<AllowedDestination, DestinationPolicyError> {
    if raw.contains("//") || raw.contains('/') || raw.contains('?') || raw.contains('#') {
        return Err(DestinationPolicyError::InvalidAllowlist);
    }
    let url = Url::parse(&format!("https://{raw}/"))
        .map_err(|_| DestinationPolicyError::InvalidAllowlist)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(DestinationPolicyError::InvalidAllowlist);
    }
    Ok(AllowedDestination {
        host: url
            .host_str()
            .ok_or(DestinationPolicyError::InvalidAllowlist)?
            .trim_matches(['[', ']'])
            .to_ascii_lowercase(),
        port: url.port_or_known_default().unwrap_or(443),
    })
}

fn official_destination(provider: &str, host: &str, port: u16) -> bool {
    port == 443
        && matches!(
            (provider, host),
            ("openai", "api.openai.com") | ("openrouter", "openrouter.ai")
        )
}

fn classify_address(address: IpAddr) -> AddressClass {
    match address {
        IpAddr::V4(address) => classify_v4(address),
        IpAddr::V6(address) => classify_v6(address),
    }
}

fn classify_v4(address: Ipv4Addr) -> AddressClass {
    let octets = address.octets();
    if address == Ipv4Addr::new(169, 254, 169, 254)
        || address.is_unspecified()
        || address.is_link_local()
        || address.is_multicast()
        || address == Ipv4Addr::BROADCAST
        || octets[0] == 0
        || octets[0] >= 224
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
    {
        AddressClass::AlwaysDenied
    } else if address.is_private() || address.is_loopback() {
        AddressClass::PrivateOrLoopback
    } else {
        AddressClass::Public
    }
}

fn classify_v6(address: Ipv6Addr) -> AddressClass {
    if let Some(embedded) = address.to_ipv4() {
        return classify_v4(embedded);
    }
    let segments = address.segments();
    if address
        == "fd00:ec2::254"
            .parse::<Ipv6Addr>()
            .expect("valid metadata IP")
        || address.is_unspecified()
        || address.is_multicast()
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
    {
        AddressClass::AlwaysDenied
    } else if address.is_loopback() || (segments[0] & 0xfe00) == 0xfc00 {
        AddressClass::PrivateOrLoopback
    } else {
        AddressClass::Public
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(raw: &str) -> Vec<SocketAddr> {
        raw.split(',').map(|value| value.parse().unwrap()).collect()
    }

    #[test]
    fn hosted_denies_caller_controlled_destination_before_parsing() {
        let policy = OutboundDestinationPolicy::new(true, None).unwrap();
        assert_eq!(
            policy
                .validate(
                    "openai",
                    "https://api.openai.com/v1",
                    DestinationSource::CallerRequest
                )
                .unwrap_err(),
            DestinationPolicyError::CallerDestinationDenied
        );
    }

    #[test]
    fn hosted_official_provider_requires_public_dns_answers() {
        let policy = OutboundDestinationPolicy::new(true, None).unwrap();
        let candidate = policy
            .validate(
                "openai",
                "https://api.openai.com/v1",
                DestinationSource::BuiltInDefault,
            )
            .unwrap();
        let approved = policy
            .approve_resolved(candidate, resolved("104.18.7.192:443"))
            .unwrap();
        assert_eq!(approved.profile(), DestinationProfile::PublicProviderApi);
        assert!(!format!("{approved:?}").contains("api.openai.com"));
    }

    #[test]
    fn hosted_rejects_mixed_public_and_private_dns_answers() {
        let policy = OutboundDestinationPolicy::new(true, None).unwrap();
        let candidate = policy
            .validate(
                "openrouter",
                "https://openrouter.ai/api/v1",
                DestinationSource::OperatorConfiguration,
            )
            .unwrap();
        assert_eq!(
            policy
                .approve_resolved(candidate, resolved("104.18.1.1:443,10.0.0.4:443"))
                .unwrap_err(),
            DestinationPolicyError::AddressDenied
        );
    }

    #[test]
    fn metadata_and_link_local_addresses_are_always_denied() {
        for address in [
            "169.254.169.254".parse().unwrap(),
            "fd00:ec2::254".parse().unwrap(),
            "fe80::1".parse().unwrap(),
        ] {
            assert_eq!(classify_address(address), AddressClass::AlwaysDenied);
        }
    }

    #[test]
    fn mapped_ipv4_addresses_keep_the_ipv4_classification() {
        assert_eq!(
            classify_address("::ffff:127.0.0.1".parse().unwrap()),
            AddressClass::PrivateOrLoopback
        );
        assert_eq!(
            classify_address("::ffff:169.254.169.254".parse().unwrap()),
            AddressClass::AlwaysDenied
        );
        assert_eq!(
            classify_address("::127.0.0.1".parse().unwrap()),
            AddressClass::PrivateOrLoopback
        );
    }

    #[test]
    fn community_local_profile_allows_private_but_not_metadata() {
        let policy = OutboundDestinationPolicy::new(false, Some("localhost:11434")).unwrap();
        let candidate = policy
            .validate(
                "ollama",
                "http://localhost:11434",
                DestinationSource::CallerRequest,
            )
            .unwrap();
        assert!(policy
            .approve_resolved(candidate, resolved("127.0.0.1:11434"))
            .is_ok());

        let candidate = policy
            .validate(
                "ollama",
                "http://169.254.169.254:80",
                DestinationSource::OperatorConfiguration,
            )
            .unwrap();
        assert_eq!(
            policy
                .approve_resolved(candidate, resolved("169.254.169.254:80"))
                .unwrap_err(),
            DestinationPolicyError::AddressDenied
        );
    }

    #[test]
    fn community_caller_custom_destination_requires_exact_allowlist() {
        let policy = OutboundDestinationPolicy::new(false, None).unwrap();
        assert_eq!(
            policy
                .validate(
                    "ollama",
                    "http://localhost:11434",
                    DestinationSource::CallerRequest,
                )
                .unwrap_err(),
            DestinationPolicyError::DestinationDenied
        );
    }

    #[test]
    fn hosted_custom_allowlist_is_exact_and_https_only() {
        let policy = OutboundDestinationPolicy::new(true, Some("models.example.com:8443")).unwrap();
        let candidate = policy
            .validate(
                "llamacpp",
                "https://models.example.com:8443/v1",
                DestinationSource::OperatorConfiguration,
            )
            .unwrap();
        assert_eq!(
            candidate.profile,
            DestinationProfile::TenantConfiguredProvider
        );
        assert_eq!(
            policy
                .validate(
                    "llamacpp",
                    "http://models.example.com:8443/v1",
                    DestinationSource::OperatorConfiguration,
                )
                .unwrap_err(),
            DestinationPolicyError::SchemeDenied
        );
    }

    #[test]
    fn url_credentials_query_and_fragment_are_rejected() {
        let policy = OutboundDestinationPolicy::new(false, None).unwrap();
        for raw in [
            "https://user:pass@example.com/v1",
            "https://example.com/v1?token=secret",
            "https://example.com/v1#fragment",
        ] {
            assert_eq!(
                policy
                    .validate("openai", raw, DestinationSource::OperatorConfiguration)
                    .unwrap_err(),
                DestinationPolicyError::ProhibitedUrlComponent
            );
        }
    }

    #[test]
    fn alternate_ipv4_spellings_normalize_before_policy() {
        let policy = OutboundDestinationPolicy::new(true, Some("127.0.0.1")).unwrap();
        for raw in [
            "https://2130706433/v1",
            "https://0177.0.0.1/v1",
            "https://0x7f000001/v1",
        ] {
            let candidate = policy
                .validate("llamacpp", raw, DestinationSource::OperatorConfiguration)
                .unwrap();
            assert_eq!(candidate.host, "127.0.0.1");
            assert_eq!(
                policy
                    .approve_resolved(candidate, resolved("127.0.0.1:443"))
                    .unwrap_err(),
                DestinationPolicyError::AddressDenied
            );
        }
    }

    #[tokio::test]
    async fn approved_client_does_not_follow_redirects() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let policy = OutboundDestinationPolicy::new(false, None).unwrap();
        let approved = policy
            .authorize(
                "ollama",
                &format!("http://127.0.0.1:{}", address.port()),
                DestinationSource::OperatorConfiguration,
            )
            .await
            .unwrap();
        let response = approved
            .build_client(Duration::from_secs(2), false)
            .unwrap()
            .get(approved.base_url())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    }
}

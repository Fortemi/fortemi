use std::convert::Infallible;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::{request::Parts, uri::Authority, HeaderMap, Uri};
use ipnet::IpNet;

const MAX_TRUSTED_PROXY_CIDRS: usize = 64;
const FORWARDED_HEADER_NAMES: [&str; 7] = [
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-port",
    "x-forwarded-proto",
    "x-forwarded-protocol",
    "x-real-ip",
];

pub(crate) struct SocketPeer(pub(crate) Option<SocketAddr>);

impl<S> FromRequestParts<S> for SocketPeer
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|connect_info| connect_info.0),
        ))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TrustedProxyConfig {
    cidrs: Arc<[IpNet]>,
}

impl TrustedProxyConfig {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        Self::from_value(std::env::var("FORTEMI_TRUSTED_PROXY_CIDRS").ok().as_deref())
    }

    pub(crate) fn from_value(raw: Option<&str>) -> anyhow::Result<Self> {
        let Some(raw) = raw else {
            return Ok(Self::default());
        };
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }

        let values = raw.split(',').collect::<Vec<_>>();
        if values.len() > MAX_TRUSTED_PROXY_CIDRS {
            anyhow::bail!(
                "FORTEMI_TRUSTED_PROXY_CIDRS supports at most {MAX_TRUSTED_PROXY_CIDRS} entries"
            );
        }

        let mut cidrs = Vec::with_capacity(values.len());
        for value in values {
            let value = value.trim();
            if value.is_empty() {
                anyhow::bail!(
                    "FORTEMI_TRUSTED_PROXY_CIDRS contains an empty entry; use numeric CIDRs"
                );
            }
            let cidr = value.parse::<IpNet>().map_err(|_| {
                anyhow::anyhow!("FORTEMI_TRUSTED_PROXY_CIDRS contains an invalid numeric CIDR")
            })?;
            if cidr.prefix_len() == 0 {
                anyhow::bail!("FORTEMI_TRUSTED_PROXY_CIDRS must not trust a universal network");
            }
            if cidrs.contains(&cidr) {
                anyhow::bail!("FORTEMI_TRUSTED_PROXY_CIDRS contains a duplicate CIDR");
            }
            cidrs.push(cidr);
        }

        Ok(Self {
            cidrs: cidrs.into(),
        })
    }

    pub(crate) fn trusted_source_count(&self) -> usize {
        self.cidrs.len()
    }

    fn trusts(&self, address: IpAddr) -> bool {
        self.cidrs.iter().any(|cidr| cidr.contains(&address))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardedHeaderDisposition {
    Absent,
    Suppressed,
    Trusted,
}

impl ForwardedHeaderDisposition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Suppressed => "suppressed",
            Self::Trusted => "trusted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalRequestContext {
    socket_peer: Option<IpAddr>,
    client_ip: Option<IpAddr>,
    scheme: String,
    authority: String,
    proxy_trusted: bool,
    forwarded: ForwardedHeaderDisposition,
}

impl ExternalRequestContext {
    pub(crate) fn from_request(
        config: &TrustedProxyConfig,
        socket_peer: Option<SocketAddr>,
        headers: &HeaderMap,
        uri: &Uri,
    ) -> Result<Self, ForwardedHeaderError> {
        let socket_peer_ip = socket_peer.map(|peer| peer.ip());
        let proxy_trusted = socket_peer_ip
            .map(|address| config.trusts(address))
            .unwrap_or(false);
        let forwarded_present = FORWARDED_HEADER_NAMES
            .iter()
            .any(|name| headers.contains_key(*name));
        let forwarded = match (forwarded_present, proxy_trusted) {
            (false, _) => ForwardedHeaderDisposition::Absent,
            (true, false) => ForwardedHeaderDisposition::Suppressed,
            (true, true) => ForwardedHeaderDisposition::Trusted,
        };

        if proxy_trusted && headers.contains_key("forwarded") {
            return Err(ForwardedHeaderError::new(
                "RFC 7239 Forwarded is not accepted; configure the edge to emit canonical X-Forwarded-* fields",
            ));
        }

        let scheme = if proxy_trusted {
            trusted_forwarded_scheme(headers)?
                .map(str::to_owned)
                .or_else(|| uri.scheme_str().map(str::to_owned))
                .unwrap_or_else(|| "https".to_string())
        } else {
            uri.scheme_str().unwrap_or("https").to_string()
        };

        let forwarded_port = if proxy_trusted {
            single_header(headers, "x-forwarded-port")?
                .map(parse_port)
                .transpose()?
        } else {
            None
        };
        let authority = if proxy_trusted {
            match single_header(headers, "x-forwarded-host")? {
                Some(value) => normalize_authority(value, forwarded_port, &scheme)?,
                None => request_authority(headers, uri, forwarded_port, &scheme)?,
            }
        } else {
            request_authority(headers, uri, None, &scheme)?
        };

        let client_ip = if proxy_trusted {
            trusted_client_ip(config, socket_peer_ip, headers)?
        } else {
            socket_peer_ip
        };

        Ok(Self {
            socket_peer: socket_peer_ip,
            client_ip,
            scheme,
            authority,
            proxy_trusted,
            forwarded,
        })
    }

    pub(crate) fn external_url(&self, uri: &Uri) -> String {
        let path_and_query = uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or("/");
        format!("{}://{}{}", self.scheme, self.authority, path_and_query)
    }

    pub(crate) fn socket_peer_present(&self) -> bool {
        self.socket_peer.is_some()
    }

    pub(crate) fn client_ip_present(&self) -> bool {
        self.client_ip.is_some()
    }

    pub(crate) fn proxy_trusted(&self) -> bool {
        self.proxy_trusted
    }

    pub(crate) fn forwarded_disposition(&self) -> ForwardedHeaderDisposition {
        self.forwarded
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ForwardedHeaderError {
    detail: &'static str,
}

impl ForwardedHeaderError {
    fn new(detail: &'static str) -> Self {
        Self { detail }
    }
}

impl fmt::Display for ForwardedHeaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for ForwardedHeaderError {}

fn trusted_forwarded_scheme(headers: &HeaderMap) -> Result<Option<&str>, ForwardedHeaderError> {
    let proto = single_header(headers, "x-forwarded-proto")?;
    let legacy = single_header(headers, "x-forwarded-protocol")?;
    if proto.is_some() && legacy.is_some() {
        return Err(ForwardedHeaderError::new(
            "conflicting forwarded protocol headers",
        ));
    }
    let scheme = proto.or(legacy);
    match scheme {
        Some("http" | "https") | None => Ok(scheme),
        Some(_) => Err(ForwardedHeaderError::new(
            "forwarded protocol must be http or https",
        )),
    }
}

fn request_authority(
    headers: &HeaderMap,
    uri: &Uri,
    forwarded_port: Option<u16>,
    scheme: &str,
) -> Result<String, ForwardedHeaderError> {
    if let Some(authority) = uri.authority() {
        return normalize_authority(authority.as_str(), forwarded_port, scheme);
    }
    let host = single_header(headers, "host")?
        .ok_or_else(|| ForwardedHeaderError::new("request host is missing"))?;
    normalize_authority(host, forwarded_port, scheme)
}

fn normalize_authority(
    raw: &str,
    forwarded_port: Option<u16>,
    scheme: &str,
) -> Result<String, ForwardedHeaderError> {
    let authority = raw
        .parse::<Authority>()
        .map_err(|_| ForwardedHeaderError::new("forwarded host is invalid"))?;
    let host = authority.host();
    if host.is_empty() {
        return Err(ForwardedHeaderError::new("forwarded host is empty"));
    }
    if authority.port_u16().is_some() && forwarded_port.is_some() {
        return Err(ForwardedHeaderError::new(
            "forwarded host and port are ambiguous",
        ));
    }

    let host = host.to_ascii_lowercase();
    let normalized_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    let port = forwarded_port.or_else(|| authority.port_u16());
    let port = match (scheme, port) {
        ("http", Some(80)) | ("https", Some(443)) => None,
        (_, port) => port,
    };
    Ok(match port {
        Some(port) => format!("{normalized_host}:{port}"),
        None => normalized_host,
    })
}

fn parse_port(raw: &str) -> Result<u16, ForwardedHeaderError> {
    raw.parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| ForwardedHeaderError::new("forwarded port is invalid"))
}

fn trusted_client_ip(
    config: &TrustedProxyConfig,
    socket_peer: Option<IpAddr>,
    headers: &HeaderMap,
) -> Result<Option<IpAddr>, ForwardedHeaderError> {
    let Some(socket_peer) = socket_peer else {
        return Ok(None);
    };
    if let Some(raw) = single_header_allow_commas(headers, "x-forwarded-for")? {
        let mut chain = Vec::new();
        for value in raw.split(',') {
            let value = value.trim();
            if value.is_empty() {
                return Err(ForwardedHeaderError::new(
                    "forwarded client chain contains an empty hop",
                ));
            }
            let address = value.parse::<IpAddr>().map_err(|_| {
                ForwardedHeaderError::new("forwarded client chain contains an invalid IP address")
            })?;
            chain.push(address);
        }
        chain.push(socket_peer);
        return Ok(chain
            .into_iter()
            .rev()
            .find(|address| !config.trusts(*address)));
    }

    match single_header(headers, "x-real-ip")? {
        Some(raw) => raw
            .parse::<IpAddr>()
            .map(Some)
            .map_err(|_| ForwardedHeaderError::new("forwarded client IP is invalid")),
        None => Ok(Some(socket_peer)),
    }
}

fn single_header<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<Option<&'a str>, ForwardedHeaderError> {
    let value = single_header_allow_commas(headers, name)?;
    if value.is_some_and(|value| value.contains(',')) {
        return Err(ForwardedHeaderError::new(
            "forwarded metadata contains ambiguous multiple values",
        ));
    }
    Ok(value)
}

fn single_header_allow_commas<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<Option<&'a str>, ForwardedHeaderError> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(ForwardedHeaderError::new(
            "forwarded metadata contains multiple header fields",
        ));
    }
    let value = value
        .to_str()
        .map_err(|_| ForwardedHeaderError::new("forwarded metadata is not valid text"))?
        .trim();
    if value.is_empty() {
        return Err(ForwardedHeaderError::new("forwarded metadata is empty"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(value: Option<&str>) -> TrustedProxyConfig {
        TrustedProxyConfig::from_value(value).unwrap()
    }

    fn request_context(
        config: &TrustedProxyConfig,
        peer: &str,
        headers: HeaderMap,
    ) -> Result<ExternalRequestContext, ForwardedHeaderError> {
        ExternalRequestContext::from_request(
            config,
            Some(peer.parse().unwrap()),
            &headers,
            &"/api/v1/webhooks/incoming/twilio".parse().unwrap(),
        )
    }

    #[test]
    fn config_defaults_to_trusting_no_proxy_and_rejects_invalid_entries() {
        assert_eq!(config(None).trusted_source_count(), 0);
        assert_eq!(config(Some("")).trusted_source_count(), 0);
        assert_eq!(
            config(Some("127.0.0.1/32,10.0.0.0/8")).trusted_source_count(),
            2
        );

        for invalid in [
            "proxy.internal",
            "10.0.0.0/8,",
            "10.0.0.0/8,10.0.0.0/8",
            "0.0.0.0/0",
            "::/0",
        ] {
            assert!(TrustedProxyConfig::from_value(Some(invalid)).is_err());
        }
    }

    #[test]
    fn untrusted_peer_forwarded_values_are_suppressed() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "direct.example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "http".parse().unwrap());
        headers.insert("x-forwarded-host", "attacker.example".parse().unwrap());
        headers.insert("x-forwarded-for", "203.0.113.8".parse().unwrap());

        let context =
            request_context(&config(Some("10.0.0.0/8")), "192.0.2.4:45000", headers).unwrap();

        assert_eq!(
            context.forwarded_disposition(),
            ForwardedHeaderDisposition::Suppressed
        );
        assert!(!context.proxy_trusted());
        assert_eq!(context.client_ip, Some("192.0.2.4".parse().unwrap()));
        assert_eq!(
            context.external_url(&"/hook?retry=1".parse().unwrap()),
            "https://direct.example.com/hook?retry=1"
        );
    }

    #[test]
    fn trusted_peer_uses_normalized_external_url_and_first_untrusted_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "internal.local:3000".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-host", "VOICE.EXAMPLE.COM".parse().unwrap());
        headers.insert("x-forwarded-port", "8443".parse().unwrap());
        headers.insert(
            "x-forwarded-for",
            "198.51.100.12, 10.2.3.4".parse().unwrap(),
        );

        let context =
            request_context(&config(Some("10.0.0.0/8")), "10.9.8.7:45000", headers).unwrap();

        assert_eq!(
            context.forwarded_disposition(),
            ForwardedHeaderDisposition::Trusted
        );
        assert!(context.proxy_trusted());
        assert_eq!(context.client_ip, Some("198.51.100.12".parse().unwrap()));
        assert_eq!(
            context.external_url(&"/hook?retry=1".parse().unwrap()),
            "https://voice.example.com:8443/hook?retry=1"
        );
    }

    #[test]
    fn trusted_peer_omits_default_external_port() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "internal.local:3000".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-host", "voice.example.com".parse().unwrap());
        headers.insert("x-forwarded-port", "443".parse().unwrap());

        let context =
            request_context(&config(Some("10.0.0.0/8")), "10.9.8.7:45000", headers).unwrap();
        assert_eq!(
            context.external_url(&"/hook".parse().unwrap()),
            "https://voice.example.com/hook"
        );
    }

    #[test]
    fn trusted_peer_rejects_malformed_or_ambiguous_metadata() {
        let trusted = config(Some("127.0.0.1/32"));
        for (name, value) in [
            ("x-forwarded-proto", "ftp"),
            ("x-forwarded-host", "one.example, two.example"),
            ("x-forwarded-port", "0"),
            ("x-forwarded-for", "not-an-ip"),
            ("forwarded", "for=192.0.2.1;proto=https"),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("host", "direct.example.com".parse().unwrap());
            headers.insert(name, value.parse().unwrap());
            assert!(
                request_context(&trusted, "127.0.0.1:45000", headers).is_err(),
                "{name}={value} should fail"
            );
        }
    }

    #[test]
    fn trusted_peer_mismatch_suppresses_even_malformed_forwarded_values() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "direct.example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "ftp".parse().unwrap());
        headers.insert(
            "x-forwarded-host",
            "one.example, two.example".parse().unwrap(),
        );

        let context =
            request_context(&config(Some("10.0.0.0/8")), "127.0.0.1:45000", headers).unwrap();
        assert_eq!(
            context.forwarded_disposition(),
            ForwardedHeaderDisposition::Suppressed
        );
        assert_eq!(
            context.external_url(&"/hook".parse().unwrap()),
            "https://direct.example.com/hook"
        );
    }
}

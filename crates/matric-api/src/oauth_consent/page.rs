//! HTML for the authorization endpoint: consent form and local error page (#943).
//!
//! Every response carries frame-blocking and no-store headers. The pages contain no
//! script, and no client-supplied value is ever used as a link or redirect target.

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use super::transaction::{ValidatedRequest, TRANSACTION_TTL};

const CSP: &str =
    "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'";
const STYLE: &str =
    "body{font-family:system-ui,sans-serif;background:#f4f5f8;margin:0;padding:24px}\
main{max-width:440px;margin:40px auto;background:#fff;border-radius:12px;padding:24px;\
box-shadow:0 4px 18px rgba(0,0,0,.12)}h1{font-size:20px;margin:0 0 12px}\
code{font-size:12px;color:#555}.scope{display:inline-block;background:#eef;border-radius:4px;\
padding:2px 8px;margin:2px}.notice{background:#fff3cd;padding:8px;border-radius:6px}\
label{display:block;margin:12px 0 4px}input[type=password]{width:100%;padding:8px}\
.actions{display:flex;gap:12px;margin-top:16px}button{flex:1;padding:10px;border-radius:6px}";

/// Apply the authorization endpoint's browser security headers.
pub(crate) fn secure(mut response: Response) -> Response {
    let headers = response.headers_mut();
    let pairs = [
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::CACHE_CONTROL, "no-store"),
        (header::PRAGMA, "no-cache"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ];
    for (name, value) in pairs {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

pub(crate) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn document(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{title}</title><style>{STYLE}</style></head><body><main>{body}</main></body></html>",
        title = escape(title),
    )
}

fn html(status: StatusCode, body: String) -> Response {
    secure(
        (
            status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
    )
}

/// A local error page. Used whenever redirecting would be unsafe or impossible.
pub(crate) fn error_page(status: StatusCode, message: &str) -> Response {
    let body = format!(
        "<h1>Authorization failed</h1><p>{}</p><p>Return to the application and start again.</p>",
        escape(message)
    );
    html(status, document("Authorization failed", &body))
}

/// What the consent page shows and how the owner proves who they are.
pub(crate) struct ConsentView<'a> {
    pub(crate) request: &'a ValidatedRequest,
    pub(crate) transaction_id: &'a str,
    pub(crate) csrf_token: &'a str,
    pub(crate) owner_subject: Option<&'a str>,
    pub(crate) ask_credential: bool,
    pub(crate) notice: Option<&'a str>,
}

fn redirect_host(redirect_uri: &str) -> String {
    let rest = redirect_uri
        .split_once("://")
        .map_or(redirect_uri, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .to_string()
}

pub(crate) fn consent_page(status: StatusCode, view: &ConsentView<'_>) -> Response {
    let request = view.request;
    let scopes: String = request
        .scope
        .split_whitespace()
        .map(|scope| format!("<span class=\"scope\">{}</span>", escape(scope)))
        .collect();
    let notice = view
        .notice
        .map(|text| format!("<p class=\"notice\">{}</p>", escape(text)))
        .unwrap_or_default();
    let who = match view.owner_subject {
        Some(subject) => format!("<p>Signed in as <code>{}</code>.</p>", escape(subject)),
        None => String::new(),
    };
    let credential = if view.ask_credential {
        "<label for=\"credential\">Fortemi API key or access token (required to approve)</label>\
<input type=\"password\" id=\"credential\" name=\"credential\" autocomplete=\"off\">"
    } else {
        ""
    };
    let body = format!(
        "<h1>Authorize {name}</h1>{notice}<p><code>{client_id}</code></p>\
<p>Requested access:</p><p>{scopes}</p><p>After you decide, you return to <code>{host}</code>.</p>{who}\
<form method=\"post\" action=\"authorize\">\
<input type=\"hidden\" name=\"transaction\" value=\"{tx}\">\
<input type=\"hidden\" name=\"csrf_token\" value=\"{csrf}\">{credential}\
<div class=\"actions\"><button type=\"submit\" name=\"action\" value=\"deny\">Deny</button>\
<button type=\"submit\" name=\"action\" value=\"approve\">Approve</button></div></form>",
        name = escape(&request.client_name),
        client_id = escape(&request.client_id),
        host = escape(&redirect_host(&request.redirect_uri)),
        tx = escape(view.transaction_id),
        csrf = escape(view.csrf_token),
    );
    html(status, document("Authorize application", &body))
}

/// Cookie name tied to one transaction, so parallel authorizations do not collide.
pub(crate) fn binding_cookie_name(transaction_id: &str) -> String {
    let tag: String = transaction_id.chars().take(16).collect();
    format!("fortemi_oauth_{tag}")
}

pub(crate) fn binding_cookie(transaction_id: &str, binding: &str, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    let value = format!(
        "{}={binding}; HttpOnly; SameSite=Strict; Max-Age={}{secure}",
        binding_cookie_name(transaction_id),
        TRANSACTION_TTL.as_secs()
    );
    HeaderValue::from_str(&value).expect("cookie value is ASCII")
}

/// Read the binding cookie for a transaction from the request.
pub(crate) fn read_binding(headers: &HeaderMap, transaction_id: &str) -> Option<String> {
    let name = binding_cookie_name(transaction_id);
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ValidatedRequest {
        ValidatedRequest {
            client_id: "mm_<client>".to_string(),
            client_name: "<script>alert(1)</script>".to_string(),
            redirect_uri: "https://app.example:8443/cb?x=1".to_string(),
            scope: "read write".to_string(),
            state: None,
            code_challenge: None,
            code_challenge_method: None,
        }
    }

    #[test]
    fn pages_carry_frame_and_cache_protection() {
        for response in [
            error_page(StatusCode::BAD_REQUEST, "bad"),
            consent_page(
                StatusCode::OK,
                &ConsentView {
                    request: &request(),
                    transaction_id: "tx",
                    csrf_token: "csrf",
                    owner_subject: None,
                    ask_credential: true,
                    notice: None,
                },
            ),
        ] {
            let headers = response.headers();
            assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
            assert!(headers[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"));
            assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        }
    }

    #[test]
    fn consent_page_escapes_client_values_and_hides_the_redirect() {
        let response = consent_page(
            StatusCode::OK,
            &ConsentView {
                request: &request(),
                transaction_id: "tx",
                csrf_token: "csrf",
                owner_subject: Some("proxy:a@b.example"),
                ask_credential: false,
                notice: None,
            },
        );
        let body =
            futures::executor::block_on(axum::body::to_bytes(response.into_body(), usize::MAX))
                .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("<script>"));
        assert!(body.contains("app.example:8443"));
        assert!(
            !body.contains("/cb?x=1"),
            "the redirect URI is not embedded"
        );
        assert!(!body.contains("credential"));
        assert!(!body.contains("redirect_uri"));
    }

    #[test]
    fn binding_cookie_round_trips() {
        let cookie = binding_cookie("abcdef0123456789zz", "secret", true);
        let text = cookie.to_str().unwrap();
        assert!(
            text.contains("HttpOnly")
                && text.contains("SameSite=Strict")
                && text.contains("Secure")
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=b; fortemi_oauth_abcdef0123456789=secret"),
        );
        assert_eq!(
            read_binding(&headers, "abcdef0123456789zz").as_deref(),
            Some("secret")
        );
        assert_eq!(read_binding(&headers, "ffffffffffffffff"), None);
    }
}

//! `/oauth/authorize` GET and POST flows (#943).
//!
//! GET validates the request, stores it as a server-side transaction and renders the
//! consent page. POST verifies the transaction, CSRF token and cookie binding, then
//! authenticates the resource owner before any code is issued. Denial goes only to the
//! redirect URI stored with the transaction.

use std::net::SocketAddr;

use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};

use super::config::OwnerAuthMethod;
use super::owner::{self, Owner};
use super::page::{self, ConsentView};
use super::request::{self, Rejection};
use super::transaction::{PendingAuthorization, ValidatedRequest, MAX_OWNER_ATTEMPTS};
use crate::{AppState, AuthorizationForm, AuthorizationRequest};

fn redirect(url: String) -> Response {
    page::secure(Redirect::to(&url).into_response())
}

fn error_redirect(
    state: &AppState,
    uri: &str,
    error: &str,
    description: &str,
    st: Option<&str>,
) -> Response {
    let mut params = vec![("error", error), ("error_description", description)];
    if let Some(value) = st {
        params.push(("state", value));
    }
    params.push(("iss", state.issuer.as_str()));
    redirect(request::redirect_with(uri, &params))
}

fn rejection_response(state: &AppState, rejection: Rejection) -> Response {
    match rejection {
        Rejection::Local(status, message) => page::error_page(status, message),
        Rejection::Redirect {
            redirect_uri,
            error,
            description,
            state: st,
        } => error_redirect(state, &redirect_uri, error, description, st.as_deref()),
    }
}

pub(crate) async fn authorize_get(
    state: &AppState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    req: &AuthorizationRequest,
) -> Response {
    let request = match request::validate(&state.db, req).await {
        Ok(request) => request,
        Err(rejection) => return rejection_response(state, rejection),
    };
    let runtime = &state.oauth_authorize;
    if runtime.config.is_disabled() {
        return error_redirect(
            state,
            &request.redirect_uri,
            "access_denied",
            "Browser authorization is disabled on this server",
            request.state.as_deref(),
        );
    }
    let owner =
        owner::from_trusted_header(&runtime.config, &state.trusted_proxy_config, peer, headers);
    let ask_credential = owner.is_none() && runtime.config.allows(OwnerAuthMethod::ApiKey);
    if owner.is_none() && !ask_credential {
        return page::error_page(
            StatusCode::UNAUTHORIZED,
            "Sign in through the configured identity proxy before authorizing applications.",
        );
    }
    let pending = PendingAuthorization::new(request);
    let Ok(id) = runtime.store.insert(pending.clone()) else {
        return page::error_page(
            StatusCode::SERVICE_UNAVAILABLE,
            "Too many pending authorizations.",
        );
    };
    let view = ConsentView {
        request: &pending.request,
        transaction_id: &id,
        csrf_token: &pending.csrf_token,
        owner_subject: owner.as_ref().map(|owner| owner.subject.as_str()),
        ask_credential,
        notice: None,
    };
    let mut response = page::consent_page(StatusCode::OK, &view);
    let cookie = page::binding_cookie(&id, &pending.binding, state.issuer.starts_with("https://"));
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

/// Fetch Metadata and Origin checks: the form may only be posted from this server.
fn cross_site(headers: &HeaderMap, issuer: &str) -> bool {
    match headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        Some("same-origin" | "none") => return false,
        Some(_) => return true,
        None => {}
    }
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let issuer_origin = issuer
        .split_once("://")
        .map(|(scheme, rest)| format!("{scheme}://{}", rest.split('/').next().unwrap_or(rest)))
        .unwrap_or_default();
    origin != issuer_origin
}

pub(crate) async fn authorize_post(
    state: &AppState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    form: &AuthorizationForm,
) -> Response {
    if cross_site(headers, &state.issuer) {
        return page::error_page(
            StatusCode::FORBIDDEN,
            "Cross-site authorization requests are refused.",
        );
    }
    let runtime = &state.oauth_authorize;
    let Some(mut pending) = runtime.store.take(&form.transaction) else {
        return page::error_page(
            StatusCode::BAD_REQUEST,
            "This authorization request expired or was already used.",
        );
    };
    let binding = page::read_binding(headers, &form.transaction);
    if !pending.matches(&form.csrf_token, binding.as_deref()) {
        return page::error_page(
            StatusCode::FORBIDDEN,
            "The authorization form could not be verified.",
        );
    }
    let request = pending.request.clone();
    match form.action.as_str() {
        "deny" => {
            return error_redirect(
                state,
                &request.redirect_uri,
                "access_denied",
                "The resource owner denied the request",
                request.state.as_deref(),
            )
        }
        "approve" => {}
        _ => return page::error_page(StatusCode::BAD_REQUEST, "Unknown authorization action."),
    }
    match resolve_owner(state, peer, headers, form.credential.as_deref()).await {
        Some(owner) if owner.covers(&request.scope) => issue_code(state, &request, &owner).await,
        other => {
            pending.owner_attempts += 1;
            retry_or_fail(state, form.transaction.clone(), pending, other.is_some())
        }
    }
}

/// The POST re-authenticates the owner: proxy identity first, then a typed credential.
async fn resolve_owner(
    state: &AppState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    credential: Option<&str>,
) -> Option<Owner> {
    let config = &state.oauth_authorize.config;
    if let Some(owner) =
        owner::from_trusted_header(config, &state.trusted_proxy_config, peer, headers)
    {
        return Some(owner);
    }
    owner::from_credential(state, credential?).await
}

fn retry_or_fail(
    state: &AppState,
    id: String,
    pending: PendingAuthorization,
    authenticated: bool,
) -> Response {
    if pending.owner_attempts >= MAX_OWNER_ATTEMPTS {
        return page::error_page(
            StatusCode::UNAUTHORIZED,
            "Too many failed sign-in attempts.",
        );
    }
    let notice = if authenticated {
        "This credential does not hold every requested scope."
    } else {
        "A valid credential is required to approve."
    };
    let ask_credential = state.oauth_authorize.config.allows(OwnerAuthMethod::ApiKey);
    let response = page::consent_page(
        StatusCode::UNAUTHORIZED,
        &ConsentView {
            request: &pending.request,
            transaction_id: &id,
            csrf_token: &pending.csrf_token,
            owner_subject: None,
            ask_credential,
            notice: Some(notice),
        },
    );
    state.oauth_authorize.store.restore(id, pending);
    response
}

async fn issue_code(state: &AppState, request: &ValidatedRequest, owner: &Owner) -> Response {
    match state.db.oauth.get_client(&request.client_id).await {
        Ok(Some(client)) if client.is_active => {}
        _ => {
            return page::error_page(
                StatusCode::BAD_REQUEST,
                "The client is unknown or inactive.",
            )
        }
    }
    let code = state
        .db
        .oauth
        .create_authorization_code(
            &request.client_id,
            &request.redirect_uri,
            &request.scope,
            request.state.as_deref(),
            request.code_challenge.as_deref(),
            request.code_challenge_method.as_deref(),
            Some(&owner.subject),
        )
        .await;
    let Ok(code) = code else {
        return page::error_page(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The authorization code could not be issued.",
        );
    };
    tracing::info!(
        target: "fortemi.security",
        client_id_len = request.client_id.len(),
        scope_count = request.scope.split_whitespace().count(),
        "oauth authorization approved by an authenticated resource owner"
    );
    let mut params = vec![("code", code.as_str())];
    if let Some(value) = request.state.as_deref() {
        params.push(("state", value));
    }
    params.push(("iss", state.issuer.as_str()));
    redirect(request::redirect_with(&request.redirect_uri, &params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn cross_site_posts_are_detected() {
        let issuer = "https://memory.example.com";
        let mut headers = HeaderMap::new();
        assert!(!cross_site(&headers, issuer));
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(cross_site(&headers, issuer));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(!cross_site(&headers, issuer));
        let mut origin_only = HeaderMap::new();
        origin_only.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(cross_site(&origin_only, issuer));
        origin_only.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://memory.example.com"),
        );
        assert!(!cross_site(&origin_only, issuer));
    }
}

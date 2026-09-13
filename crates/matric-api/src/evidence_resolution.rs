//! Candidate read-only evidence resolution; no retained-history or capability claim.

use super::*;
use matric_core::search_evidence::{SearchEvidenceError, SearchEvidenceLocator};
use matric_db::search_candidates::SearchCandidateScope;
use serde_json::{json, Value};
use std::sync::LazyLock;

const MAX_REQUEST_BYTES: usize = 65536;

fn invalid() -> ApiError {
    ApiError::BadRequest(SearchEvidenceError::Invalid.to_string())
}

fn unavailable() -> ApiError {
    ApiError::NotFound(SearchEvidenceError::Unavailable.to_string())
}

async fn authorize_note(
    state: &AppState,
    scope: Option<&TenantRequestScope>,
    archive: &ArchiveContext,
    auth: Option<&Auth>,
    locator: &SearchEvidenceLocator,
) -> Result<(), ApiError> {
    let note = Uuid::parse_str(locator.note_id()).map_err(|_| unavailable())?;
    if note.to_string() != locator.note_id() {
        return Err(unavailable());
    }
    let tenant = scope.map(|scope| scope.tenant());
    if state.multi_tenant && tenant.is_none() {
        return Err(ApiError::OperationFailed {
            operation: "Search",
            detail: "hosted evidence transaction unavailable".into(),
        });
    }
    if (state.multi_tenant || state.require_auth) && auth.is_none() {
        return Err(ApiError::Unauthorized("Authentication required.".into()));
    }
    // Use the same backing-note normalization, policy action and audit gate as
    // GET note. Route-level search permission alone does not authorize this ID.
    let input = route_policy::authorization_input_for_request(
        &Method::GET,
        &format!("/api/v1/notes/{note}"),
        None,
    )
    .expect("registered note read route");
    let input = apply_verified_tenant_to_policy_input(input, tenant);
    let input =
        normalize_route_policy_input_for_authorization(state, input, Some(archive), scope).await;
    let anonymous = Auth {
        principal: AuthPrincipal::Anonymous,
    };
    authorize_policy_input(
        state.authorization_policy.as_ref(),
        state.audit_sink.as_ref(),
        auth.unwrap_or(&anonymous),
        &input,
    )
    .await
    .map_err(|response| {
        if response.status() == StatusCode::FORBIDDEN {
            unavailable()
        } else {
            ApiError::OperationFailed {
                operation: "Search",
                detail: "evidence authorization unavailable".into(),
            }
        }
    })
}

fn parse_request(bytes: &[u8]) -> Result<(SearchEvidenceLocator, SearchCandidateScope), ApiError> {
    static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
        jsonschema::options()
            .should_validate_formats(true)
            .build(&json!({
                "$schema":"https://json-schema.org/draft/2020-12/schema",
                "$ref":"#/components/schemas/SearchEvidenceResolveRequest",
                "components":{"schemas":search_contract::components()}
            }))
            .expect("embedded resolution request schema")
    });
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(invalid());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !VALIDATOR.is_valid(&value) {
        return Err(invalid());
    }
    let locator =
        SearchEvidenceLocator::try_from(value["locator"].clone()).map_err(|_| invalid())?;
    let metadata = value
        .get("metadata_predicates")
        .map(|value| {
            matric_core::metadata_search::MetadataPredicates::try_from(value.clone())
                .map_err(|_| invalid())
        })
        .transpose()?;
    Ok((
        locator,
        SearchCandidateScope {
            metadata,
            exclude_archived: value["include_archived"] != true,
            ..Default::default()
        },
    ))
}

pub(super) fn route() -> axum::routing::MethodRouter<AppState> {
    post(resolve_search_evidence).layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
}

#[utoipa::path(post, path = "/api/v1/search/evidence/resolve", tag = "Search",
    request_body = Value,
    responses((status = 200, description = "Exact authorized current-storage range"),
        (status = 400, description = "SEARCH_EVIDENCE_INVALID"),
        (status = 404, description = "SEARCH_EVIDENCE_UNAVAILABLE")))]
pub(super) async fn resolve_search_evidence(
    State(state): State<AppState>,
    scope: Option<Extension<TenantRequestScope>>,
    auth: Option<Extension<Auth>>,
    Extension(archive): Extension<ArchiveContext>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    async fn resolve(
        state: AppState,
        scope: Option<Extension<TenantRequestScope>>,
        auth: Option<Extension<Auth>>,
        archive: ArchiveContext,
        body: Result<Bytes, axum::extract::rejection::BytesRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let bytes = body.map_err(|_| invalid())?;
        let (locator, candidate) = parse_request(&bytes)?;
        authorize_note(
            &state,
            scope.as_ref().map(|Extension(scope)| scope),
            &archive,
            auth.as_ref().map(|Extension(auth)| auth),
            &locator,
        )
        .await?;
        let text = with_request_schema(
            &state,
            scope.map(|Extension(scope)| scope),
            archive.schema,
            move |connection| {
                Box::pin(async move {
                    matric_db::search_evidence_resolution::resolve_on_connection(
                        connection, &locator, &candidate,
                    )
                    .await
                })
            },
        )
        .await
        .map_err(|error| match error {
            matric_core::Error::Search(ref code) if code == "SEARCH_EVIDENCE_UNAVAILABLE" => {
                ApiError::NotFound(SearchEvidenceError::Unavailable.to_string())
            }
            other => search_operation_failed("resolve evidence", other),
        })?;
        Ok(Json(json!({"text":text})))
    }
    let json_content = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/json")
        });
    let mut response = if json_content {
        resolve(state, scope, auth, archive, body)
            .await
            .into_response()
    } else {
        invalid().into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Value {
        json!({"locator":{"version":"1.0.0","note_id":"native","unit":{"kind":"current","id":"native","index":0},
            "content_digest":format!("sha256:{}","0".repeat(64)),"span":{"unit":"utf8-bytes","start":0,"end":0}}})
    }

    #[test]
    fn shared_request_vectors_match_the_runtime_parser() {
        let vectors: Value = serde_json::from_str(include_str!(
            "../../../contracts/metadata-search/candidate/1.0.0/evidence-resolution-vectors.json"
        ))
        .unwrap();
        for case in vectors["requests"].as_array().unwrap() {
            assert_eq!(
                parse_request(case["value"].to_string().as_bytes()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["id"]
            );
        }
    }

    #[test]
    fn request_is_strict_and_validates_before_storage() {
        let value = request();
        assert!(parse_request(value.to_string().as_bytes()).is_ok());
        for pointer in [
            "/tenant_id",
            "/archive_id",
            "/locator/private",
            "/metadata_predicates",
            "/include_archived",
        ] {
            let mut bad = value.clone();
            if pointer == "/locator/private" {
                bad["locator"]["private"] = json!("private-marker");
            } else {
                bad[pointer.trim_start_matches('/')] = json!("private-marker");
            }
            assert!(parse_request(bad.to_string().as_bytes()).is_err());
        }
        assert!(parse_request(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
        assert!(parse_request(b"{bad-json").is_err());
        let mut bad = value.clone();
        bad["locator"]["span"]["start"] = json!(1);
        assert!(parse_request(bad.to_string().as_bytes()).is_err());
        bad = value;
        bad["metadata_predicates"] = json!([{"path":"model","op":"range","gte":4,"lte":1}]);
        assert!(parse_request(bad.to_string().as_bytes()).is_err());
    }
}

//! Job payload selectors are checked at the enqueue/read boundary (#1201).
use super::*;

pub async fn authorize_submission(
    state: &AppState,
    caller: &archive_access::Caller,
    payload: Option<&serde_json::Value>,
) -> Result<ArchiveContext, ApiError> {
    if payload.is_some_and(has_internal_key) {
        return Err(ApiError::BadRequest(
            "Job payload keys beginning with '_' are reserved for server use.".into(),
        ));
    }
    // All publicly submittable workers consume payload.schema, falling back to
    // public. Keep that historical fallback, independent of the request header.
    let schema = payload
        .and_then(|p| p.get("schema"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("public");
    let target = caller.resolve_schema(state, schema).await?;
    // Also guard named selectors in forwarded/legacy payloads; they cannot
    // substitute for authorizing the worker's effective schema above.
    if let Some(name) = payload
        .and_then(|p| p.get("memory"))
        .and_then(serde_json::Value::as_str)
    {
        let archive = state
            .db
            .archives
            .get_archive_by_name(name)
            .await?
            .ok_or_else(archive_access::not_found)?;
        caller.require(state, &archive).await?;
    }
    Ok(target)
}

pub async fn visible(state: &AppState, caller: &archive_access::Caller, job: &Job) -> bool {
    if let Some(schema) = job
        .payload
        .as_ref()
        .and_then(|p| p.get("schema"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        caller.resolve_schema(state, schema).await.is_ok()
    } else {
        archive_access::complete_catalog(&state.db, caller, state.authorization_policy.as_ref())
            .await
    }
}

/// Unscoped detail requires complete catalog before routing can synchronize an
/// unrelated selected archive. The handler repeats visibility at the point of use.
pub async fn admit_detail(
    state: &AppState,
    caller: &archive_access::Caller,
    id: Uuid,
) -> Result<(), ApiError> {
    let job = state
        .db
        .jobs
        .get(id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Job not found".into()))?;
    if !visible(state, caller, &job).await {
        return Err(ApiError::NotFound("Job not found".into()));
    }
    Ok(())
}

pub async fn list_scope(
    state: &AppState,
    caller: &archive_access::Caller,
) -> Result<Option<Vec<String>>, ApiError> {
    if archive_access::complete_catalog(&state.db, caller, state.authorization_policy.as_ref())
        .await
    {
        return Ok(None);
    }
    let mut schemas: Vec<_> = archive_access::visible(state, caller, None)
        .await?
        .into_iter()
        .map(|a| a.schema_name)
        .collect();
    if !schemas.iter().any(|s| s == "public")
        && caller.resolve_schema(state, "public").await.is_ok()
    {
        schemas.push("public".into());
    }
    Ok(Some(schemas))
}

/// Internal adapter controls may occur anywhere in forwarded JSON.
fn has_internal_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object
            .iter()
            .any(|(key, value)| key.starts_with('_') || has_internal_key(value)),
        serde_json::Value::Array(values) => values.iter().any(has_internal_key),
        _ => false,
    }
}

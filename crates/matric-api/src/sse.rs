//! Admission and archive isolation shared by live SSE delivery and replay.

use super::*;

pub(super) fn stable_user_id(identity: &ValidatedBearerIdentity) -> Option<Uuid> {
    #[cfg(feature = "hosted-auth")]
    if let Some(principal) = identity.request_principal.as_ref() {
        return principal.user_id;
    }
    identity
        .stream_identity
        .as_ref()
        .and_then(|identity| identity.user_id)
}

/// Query credentials delegate only the intersection of captured and current
/// authority. Missing records or storage failures deny access without writes.
pub(super) async fn revalidate_query_identity(
    state: &AppState,
    mut identity: matric_api::services::ingest_token_store::EventStreamIdentity,
) -> Option<matric_api::services::ingest_token_store::EventStreamIdentity> {
    use matric_api::services::ingest_token_store::EventStreamParent;
    if identity
        .expires_at
        .is_some_and(|expiry| expiry <= Utc::now())
    {
        return None;
    }
    let current_scope = match &identity.parent_credential {
        EventStreamParent::OAuthToken(id) => {
            let (scope, client_id, user_id) =
                sqlx::query_as::<_, (String, String, Option<String>)>(
                    "SELECT scope, client_id, user_id FROM public.oauth_token
                 WHERE id=$1 AND NOT revoked AND access_token_expires_at > now()",
                )
                .bind(id)
                .fetch_optional(&state.db.pool)
                .await
                .ok()??;
            if !matches!(&identity.principal, AuthPrincipal::OAuthClient { client_id: captured_client, user_id: captured_user, .. }
                if *captured_client == client_id && *captured_user == user_id)
            {
                return None;
            }
            scope
        }
        EventStreamParent::ApiKey(id) => {
            let key = state.db.oauth.get_api_key(*id).await.ok()??;
            if !key.is_active || key.expires_at.is_some_and(|expiry| expiry <= Utc::now()) {
                return None;
            }
            if !matches!(&identity.principal, AuthPrincipal::ApiKey { key_id, .. } if key_id == id)
            {
                return None;
            }
            key.scope
        }
        EventStreamParent::PersonalAccessToken(_) | EventStreamParent::Oidc(_) => {
            #[cfg(feature = "hosted-auth")]
            {
                let tenant = identity.tenant_id?;
                let user = identity.user_id?;
                let mut tx = state.db.pool.begin().await.ok()?;
                sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
                    .bind(tenant.to_string())
                    .execute(&mut *tx)
                    .await
                    .ok()?;
                let (scopes, kind, last_oidc) =
                    sqlx::query_as::<_, (Vec<String>, String, DateTime<Utc>)>(
                        "SELECT u.current_scopes, u.kind, u.last_oidc_at
                     FROM public.app_user u JOIN public.tenant_registry t ON t.id=u.tenant_id
                     WHERE u.tenant_id=$1 AND u.id=$2 AND u.status='active' AND t.status='active'",
                    )
                    .bind(tenant)
                    .bind(user)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()??;
                identity.human &= kind == "user";
                let scopes = if let EventStreamParent::PersonalAccessToken(id) =
                    &identity.parent_credential
                {
                    if last_oidc < Utc::now() - chrono::Duration::days(state.pat_revalidate_days) {
                        return None;
                    }
                    let parent_scopes = sqlx::query_scalar::<_, Vec<String>>(
                        "SELECT scopes FROM public.personal_access_token
                         WHERE tenant_id=$1 AND user_id=$2 AND id=$3 AND status='active'
                           AND revoked_at IS NULL AND expires_at > now()",
                    )
                    .bind(tenant)
                    .bind(user)
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()??;
                    parent_scopes
                        .into_iter()
                        .filter(|scope| scopes.contains(scope))
                        .collect::<Vec<_>>()
                } else {
                    scopes
                };
                tx.rollback().await.ok()?;
                scopes.join(" ")
            }
            #[cfg(not(feature = "hosted-auth"))]
            return None;
        }
    };
    #[cfg(feature = "hosted-auth")]
    let current_scope = if matches!(
        &identity.parent_credential,
        EventStreamParent::OAuthToken(_) | EventStreamParent::ApiKey(_)
    ) {
        if let Some(config) = state
            .external_oidc
            .as_ref()
            .filter(|_| state.auth_mode.external_oidc())
        {
            if !config.allow_legacy_tokens {
                return None;
            }
            ceil_legacy_scope(&current_scope, &config.legacy_scope_ceiling)
        } else {
            current_scope
        }
    } else {
        current_scope
    };
    let effective = identity
        .principal
        .scope_str()
        .split_whitespace()
        .filter(|scope| {
            current_scope
                .split_whitespace()
                .any(|current| current == *scope)
        })
        .collect::<Vec<_>>()
        .join(" ");
    if effective.is_empty() {
        return None;
    }
    match &mut identity.principal {
        AuthPrincipal::OAuthClient { scope, .. } | AuthPrincipal::ApiKey { scope, .. } => {
            *scope = effective
        }
        AuthPrincipal::Anonymous => return None,
    }
    Some(identity)
}

pub(super) async fn query_access_unchanged(
    state: &AppState,
    token: Option<&str>,
    admitted: &AuthPrincipal,
    human: bool,
) -> bool {
    match token {
        Some(token) => validate_event_stream_query_principal(state, token.trim())
            .await
            .is_some_and(|(current, _)| {
                current.principal == *admitted && identity_is_human(&current) == human
            }),
        None => true,
    }
}

pub(super) fn identity_is_human(identity: &ValidatedBearerIdentity) -> bool {
    if let Some(verified) = &identity.stream_identity {
        return verified.human;
    }
    #[cfg(feature = "hosted-auth")]
    {
        use matric_api::external_oidc::CredentialClass;
        if let Some(principal) = identity.request_principal.as_ref() {
            return principal.kind == fortemi_auth_core::PrincipalKind::Human
                && matches!(
                    principal.credential_class,
                    CredentialClass::Oidc | CredentialClass::Pat
                );
        }
        if let Some(context) = identity.canonical_context.as_ref() {
            return context.principal_kind == fortemi_auth_core::PrincipalKind::Human;
        }
    }
    // Local authorization-code credentials have a user; API keys and
    // client-credentials grants do not. External subjects alone prove no kind.
    matches!(&identity.principal, AuthPrincipal::OAuthClient { client_id, user_id: Some(_), .. }
        if client_id != "external-oidc" && client_id != "hosted-oidc" && client_id != "fortemi-pat")
}

pub(super) fn authorization_input(
    principal: &AuthPrincipal,
    human: bool,
    tenant: Option<Uuid>,
) -> route_policy::RoutePolicyInput {
    let tenant = tenant.map(|id| id.to_string());
    let input = route_policy::authorization_input_for_request(
        &Method::GET,
        "/api/v1/events",
        tenant.as_deref(),
    )
    .expect("SSE route is inventoried");
    route_policy::apply_event_stream_admission(input, principal, human)
}

pub(super) fn authorization_error(response: Response) -> ApiError {
    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
        ApiError::ServiceUnavailable("Authorization audit storage is unavailable.".into())
    } else {
        ApiError::Forbidden("SSE stream authorization denied".into())
    }
}

pub(super) async fn authorize_memory_subscription(
    state: &AppState,
    auth: &Auth,
    human: bool,
    tenant: Option<Uuid>,
    name: &str,
    user_id: Option<Uuid>,
) -> Result<(), ApiError> {
    let tenant_text = tenant.map(|id| id.to_string());
    let path = format!("/api/v1/archives/{name}");
    let mut input =
        route_policy::authorization_input_for_request(&Method::GET, &path, tenant_text.as_deref())
            .expect("archive route is inventoried");
    input = normalize_archive_route_policy_input(input, |archive_name| async move {
        state.db.archives.get_archive_by_name(&archive_name).await
    })
    .await;
    if let Some(user_id) = user_id {
        input
            .context
            .environment
            .insert("app_user_id".into(), serde_json::json!(user_id));
    }
    // Archive selection shares the stream's human-read or MCP admission.
    input.action = authorization_input(&auth.principal, human, tenant).action;
    authorize_policy_input(
        state.authorization_policy.as_ref(),
        state.audit_sink.as_ref(),
        auth,
        &input,
    )
    .await
    .map_err(authorization_error)
}

/// Archive access seam for #1201. Names/schemas must come from a trusted
/// archive resolver. The default policy permits every archive in the principal's
/// tenant; a selected enterprise policy can narrow that visibility.
pub(super) async fn may_select_archive(
    policy: &dyn AuthorizationPolicy,
    user_id: Option<Uuid>,
    principal: &AuthPrincipal,
    principal_tenant: Option<Uuid>,
    archive_tenant: Option<Uuid>,
    archive: Option<(&str, &str)>,
    multi_tenant: bool,
) -> bool {
    // Community subscribers can receive the reserved local owner's events;
    // authenticated tenant subscribers still require an exact ownership match.
    (principal_tenant == archive_tenant
        || (!multi_tenant && principal_tenant.is_none() && archive_tenant == Some(Uuid::nil())))
        && match archive {
            Some((name, schema)) => {
                let (resource, context) =
                    archive_access::policy_input(principal_tenant, user_id, name, schema);
                policy
                    .may_select_archive(principal, &resource, &context)
                    .await
                    .unwrap_or(false)
            }
            // Global queue/configuration data can reveal activity across archives.
            // Preserve personal operator monitoring; hosted tenants never see it.
            None => {
                !multi_tenant && (principal.has_scope("admin") || !principal.is_authenticated())
            }
        }
}

fn is_system_event(event: &ServerEvent) -> bool {
    matches!(
        event,
        ServerEvent::QueueStatus { .. }
            | ServerEvent::JobsPaused { .. }
            | ServerEvent::JobsResumed { .. }
            | ServerEvent::InferenceConfigChanged { .. }
            | ServerEvent::InferenceAvailabilityChanged { .. }
    )
}

#[derive(Clone)]
pub(super) struct Subscription {
    pub policy: Arc<dyn AuthorizationPolicy>,
    pub user_id: Option<Uuid>,
    pub principal: AuthPrincipal,
    pub tenant_id: Option<Uuid>,
    pub multi_tenant: bool,
    /// Trusted archive request name and canonical schema.
    pub archive: Option<(String, String)>,
}

impl Subscription {
    /// Resolve attribution, check selection, and enrich identically for both paths.
    pub async fn prepare(
        &self,
        db: &Database,
        mut envelope: EventEnvelope,
    ) -> Option<EventEnvelope> {
        if self.multi_tenant {
            if !sse_scope_matches(
                &envelope,
                self.tenant_id,
                self.archive.as_ref().map(|(_, schema)| schema.as_str()),
            ) {
                return None;
            }
        } else if let Some(tenant) = self.tenant_id {
            // External single-tenant publishers carry verified request or
            // persisted job ownership. Missing ownership never grants visibility.
            if !sse_tenant_matches(&envelope, Some(tenant)) {
                return None;
            }
        }

        // System/queue payloads can carry cross-archive counts even when the
        // publisher attributes a copy to a particular archive.
        if is_system_event(&envelope.payload)
            && !archive_access::complete_catalog(
                db,
                &archive_access::Caller {
                    principal: self.principal.clone(),
                    tenant_id: self.tenant_id,
                    user_id: self.user_id,
                },
                self.policy.as_ref(),
            )
            .await
        {
            return None;
        }

        let archive = if let Some((name, schema)) = &self.archive {
            let event_schema = envelope
                .archive_schema
                .as_deref()
                .or(envelope.memory.as_deref());
            if event_schema != Some(schema)
                && !(envelope.archive_schema.is_none()
                    && !self.multi_tenant
                    && envelope.memory.as_deref() == Some(name))
            {
                return None;
            }
            (name.clone(), schema.clone())
        } else if let (Some(name), Some(schema)) =
            (&envelope.archive_name, &envelope.archive_schema)
        {
            (name.clone(), schema.clone())
        } else if let Some(memory) = envelope
            .archive_schema
            .as_deref()
            .or(envelope.memory.as_deref())
        {
            if memory == "public" {
                ("public".to_string(), "public".to_string())
            } else {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT name, schema_name FROM public.archive_registry WHERE name = $1 OR schema_name = $1
                     ORDER BY CASE WHEN schema_name = $1 THEN 0 ELSE 1 END LIMIT 1",
                ).bind(memory).fetch_optional(&db.pool).await.ok()??
            }
        } else {
            if !is_system_event(&envelope.payload) {
                return None;
            }
            let event_tenant = envelope
                .tenant_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .ok()?;
            if !may_select_archive(
                self.policy.as_ref(),
                self.user_id,
                &self.principal,
                self.tenant_id,
                event_tenant,
                None,
                self.multi_tenant,
            )
            .await
            {
                return None;
            }
            return Some(envelope);
        };

        let archive_tenant = envelope
            .tenant_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .ok()?;
        if !may_select_archive(
            self.policy.as_ref(),
            self.user_id,
            &self.principal,
            self.tenant_id,
            archive_tenant,
            Some((&archive.0, &archive.1)),
            self.multi_tenant,
        )
        .await
        {
            return None;
        }
        envelope.memory = Some(if self.multi_tenant {
            archive.1.clone()
        } else {
            archive.0.clone()
        });
        envelope.archive_name = Some(archive.0);
        envelope.archive_schema = Some(archive.1);
        Some(envelope)
    }
}

pub(super) fn frame(envelope: &EventEnvelope) -> Option<Event> {
    serde_json::to_string(envelope).ok().map(|json| {
        Event::default()
            .event(envelope.event_type.clone())
            .id(envelope.event_id.to_string())
            .data(json)
    })
}

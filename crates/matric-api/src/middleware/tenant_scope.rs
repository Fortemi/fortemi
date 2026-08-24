//! Request-scoped tenant transaction coordination for hosted Axum routes.
//!
//! Authentication must insert [`VerifiedRequestTenant`] before this middleware
//! runs. The middleware owns the transaction and gives handlers a cloneable
//! [`TenantRequestScope`] that can execute work only through the bound
//! connection. Streaming responses are rejected because their body can outlive
//! the request transaction.

use std::{any::Any, future::Future, pin::Pin};

use axum::{
    body::HttpBody,
    extract::{Request, State},
    http::{header, Extensions, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use matric_core::{Error, Result};
use matric_db::TenantScopedConn;
use serde_json::json;
use sqlx::postgres::{PgConnection, PgPool};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const COMMAND_CAPACITY: usize = 1;

/// Tenant identity admitted by canonical authentication and active-tenant
/// validation.
///
/// This type does not authenticate a tenant. Callers must construct it only
/// after the identity and active tenant have been verified. The nil UUID is
/// rejected because it cannot identify a hosted tenant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VerifiedRequestTenant(Uuid);

impl VerifiedRequestTenant {
    /// Mark an already authenticated and admitted tenant as verified.
    pub fn from_verified(tenant_id: Uuid) -> Result<Self> {
        if tenant_id.is_nil() {
            return Err(Error::InvalidInput(
                "a verified request tenant must not use the nil UUID".to_string(),
            ));
        }
        Ok(Self(tenant_id))
    }

    pub fn tenant_id(self) -> Uuid {
        self.0
    }
}

/// Boxed operation future accepted by [`TenantRequestScope::with_connection`].
pub type TenantConnectionFuture<'connection, T> =
    Pin<Box<dyn Future<Output = Result<T>> + Send + 'connection>>;

type ErasedValue = Box<dyn Any + Send>;
type ErasedConnectionFuture<'connection> =
    Pin<Box<dyn Future<Output = Result<ErasedValue>> + Send + 'connection>>;
type ConnectionOperation = Box<
    dyn for<'connection> FnOnce(
            &'connection mut PgConnection,
        ) -> ErasedConnectionFuture<'connection>
        + Send,
>;

struct ConnectionCommand {
    operation: ConnectionOperation,
    result: oneshot::Sender<Result<ErasedValue>>,
}

/// Cloneable request extension for work on the bound tenant transaction.
///
/// Operations are serialized on the transaction owner task. The handle never
/// exposes the source pool, and a clone cannot extend the transaction beyond
/// request finalization.
#[derive(Clone)]
pub struct TenantRequestScope {
    tenant: VerifiedRequestTenant,
    commands: mpsc::Sender<ConnectionCommand>,
}

impl std::fmt::Debug for TenantRequestScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TenantRequestScope")
            .field("tenant_id_present", &true)
            .field("available", &!self.commands.is_closed())
            .finish()
    }
}

impl TenantRequestScope {
    pub fn tenant(&self) -> VerifiedRequestTenant {
        self.tenant
    }

    /// Execute one operation against the transaction's tenant-bound
    /// connection.
    ///
    /// The boxed future makes the connection borrow explicit and prevents it
    /// from escaping the operation. Handlers must await all operations before
    /// returning their response.
    pub async fn with_connection<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: for<'connection> FnOnce(
                &'connection mut PgConnection,
            ) -> TenantConnectionFuture<'connection, T>
            + Send
            + 'static,
    {
        let operation: ConnectionOperation = Box::new(move |connection| {
            let future = operation(connection);
            Box::pin(async move {
                future
                    .await
                    .map(|value| Box::new(value) as Box<dyn Any + Send>)
            })
        });
        let (result_tx, result_rx) = oneshot::channel();

        self.commands
            .send(ConnectionCommand {
                operation,
                result: result_tx,
            })
            .await
            .map_err(|_| Error::Internal("tenant request scope is unavailable".to_string()))?;

        let value = result_rx.await.map_err(|_| {
            Error::Internal("tenant request scope operation was canceled".to_string())
        })??;
        value.downcast::<T>().map(|value| *value).map_err(|_| {
            Error::Internal("tenant request scope returned an invalid operation result".to_string())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestScopeRejection {
    MissingTenant,
    ReusedScope,
}

fn tenant_for_request(
    extensions: &Extensions,
) -> std::result::Result<VerifiedRequestTenant, RequestScopeRejection> {
    if extensions.get::<TenantRequestScope>().is_some() {
        return Err(RequestScopeRejection::ReusedScope);
    }
    extensions
        .get::<VerifiedRequestTenant>()
        .copied()
        .ok_or(RequestScopeRejection::MissingTenant)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishAction {
    Commit,
    Rollback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResponseDisposition {
    Finish(FinishAction),
    RejectStreaming,
}

fn response_disposition(response: &Response) -> ResponseDisposition {
    let content_type_is_stream = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value.starts_with("text/event-stream") || value.starts_with("multipart/x-mixed-replace")
        });
    let is_upgrade = response.status() == StatusCode::SWITCHING_PROTOCOLS
        || response.headers().contains_key(header::UPGRADE);
    let has_fixed_body = response.body().size_hint().exact().is_some();

    if is_upgrade || (response.status().is_success() && (content_type_is_stream || !has_fixed_body))
    {
        ResponseDisposition::RejectStreaming
    } else if response.status().is_success() {
        ResponseDisposition::Finish(FinishAction::Commit)
    } else {
        ResponseDisposition::Finish(FinishAction::Rollback)
    }
}

#[allow(async_fn_in_trait)]
trait ScopedTransaction: Send {
    fn executor(&mut self) -> &mut PgConnection;
    async fn commit(self) -> Result<()>;
    async fn rollback(self) -> Result<()>;
}

impl ScopedTransaction for TenantScopedConn<'_> {
    fn executor(&mut self) -> &mut PgConnection {
        TenantScopedConn::executor(self)
    }

    async fn commit(self) -> Result<()> {
        TenantScopedConn::commit(self).await
    }

    async fn rollback(self) -> Result<()> {
        TenantScopedConn::rollback(self).await
    }
}

async fn finish_transaction<T>(transaction: T, action: FinishAction) -> Result<()>
where
    T: ScopedTransaction,
{
    match action {
        FinishAction::Commit => transaction.commit().await,
        FinishAction::Rollback => transaction.rollback().await,
    }
}

async fn run_transaction_owner<T>(
    mut transaction: T,
    mut commands: mpsc::Receiver<ConnectionCommand>,
    mut finish: oneshot::Receiver<FinishAction>,
    completed: oneshot::Sender<Result<()>>,
) where
    T: ScopedTransaction,
{
    loop {
        tokio::select! {
            biased;
            action = &mut finish => {
                let action = action.unwrap_or(FinishAction::Rollback);
                let _ = completed.send(finish_transaction(transaction, action).await);
                return;
            }
            command = commands.recv() => {
                let Some(command) = command else {
                    let action = finish.await.unwrap_or(FinishAction::Rollback);
                    let _ = completed
                        .send(finish_transaction(transaction, action).await);
                    return;
                };

                let outcome = {
                    let operation = (command.operation)(transaction.executor());
                    tokio::pin!(operation);
                    tokio::select! {
                        biased;
                        action = &mut finish => Err(action.unwrap_or(FinishAction::Rollback)),
                        result = &mut operation => Ok(result),
                    }
                };
                match outcome {
                    Ok(result) => {
                        let _ = command.result.send(result);
                    }
                    Err(action) => {
                        let _ = completed.send(finish_transaction(transaction, action).await);
                        return;
                    }
                }
            }
        }
    }
}

fn problem_response(
    status: StatusCode,
    type_suffix: &'static str,
    title: &'static str,
    detail: &'static str,
) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(json!({
            "type": format!("https://fortemi.com/problems/{type_suffix}"),
            "title": title,
            "status": status.as_u16(),
            "detail": detail,
        })),
    )
        .into_response()
}

fn internal_scope_response(type_suffix: &'static str, detail: &'static str) -> Response {
    problem_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        type_suffix,
        "Tenant request scope failed",
        detail,
    )
}

/// Own one tenant-bound transaction for an authenticated Axum request.
///
/// Apply this middleware only to authenticated, non-streaming hosted routes and
/// place it inside the authentication layer so [`VerifiedRequestTenant`] is
/// present before this function runs.
pub async fn tenant_scope_middleware(
    State(pool): State<PgPool>,
    mut request: Request,
    next: Next,
) -> Response {
    let tenant = match tenant_for_request(request.extensions()) {
        Ok(tenant) => tenant,
        Err(RequestScopeRejection::MissingTenant) => {
            return problem_response(
                StatusCode::UNAUTHORIZED,
                "tenant-context-required",
                "Tenant context required",
                "A verified tenant context is required for this route.",
            );
        }
        Err(RequestScopeRejection::ReusedScope) => {
            return internal_scope_response(
                "tenant-scope-reused",
                "A tenant transaction is already attached to this request.",
            );
        }
    };

    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (ready_tx, ready_rx) = oneshot::channel();
    let (finish_tx, finish_rx) = oneshot::channel();
    let (completed_tx, completed_rx) = oneshot::channel();
    let tenant_id = tenant.tenant_id();

    tokio::spawn(async move {
        let transaction = match TenantScopedConn::begin(&pool, tenant_id).await {
            Ok(transaction) => transaction,
            Err(error) => {
                let _ = ready_tx.send(Err(error));
                return;
            }
        };

        if ready_tx.send(Ok(())).is_err() {
            let _ = transaction.rollback().await;
            return;
        }
        run_transaction_owner(transaction, commands_rx, finish_rx, completed_tx).await;
    });

    match ready_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(error = %error, "failed to establish tenant request scope");
            return problem_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "tenant-scope-unavailable",
                "Tenant request scope unavailable",
                "The tenant database scope could not be established.",
            );
        }
        Err(_) => {
            return internal_scope_response(
                "tenant-scope-coordinator-failed",
                "The tenant transaction coordinator stopped before handler access.",
            );
        }
    }

    request.extensions_mut().insert(TenantRequestScope {
        tenant,
        commands: commands_tx,
    });

    let response = next.run(request).await;
    let disposition = response_disposition(&response);
    let action = match disposition {
        ResponseDisposition::Finish(action) => action,
        ResponseDisposition::RejectStreaming => FinishAction::Rollback,
    };

    if finish_tx.send(action).is_err() {
        return internal_scope_response(
            "tenant-scope-coordinator-failed",
            "The tenant transaction coordinator stopped before finalization.",
        );
    }

    match completed_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(error = %error, ?action, "failed to finalize tenant request scope");
            return internal_scope_response(
                "tenant-scope-finalization-failed",
                "The tenant transaction could not be finalized.",
            );
        }
        Err(_) => {
            return internal_scope_response(
                "tenant-scope-coordinator-failed",
                "The tenant transaction coordinator stopped during finalization.",
            );
        }
    }

    if disposition == ResponseDisposition::RejectStreaming {
        return problem_response(
            StatusCode::CONFLICT,
            "tenant-scope-streaming-unsupported",
            "Streaming response is incompatible with request transaction scope",
            "Use a route-specific streaming policy that does not retain a request transaction.",
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Body;

    use super::*;

    struct MockTransaction {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl ScopedTransaction for MockTransaction {
        fn executor(&mut self) -> &mut PgConnection {
            panic!("mock transaction does not execute connection operations")
        }

        async fn commit(self) -> Result<()> {
            self.events.lock().unwrap().push("commit");
            Ok(())
        }

        async fn rollback(self) -> Result<()> {
            self.events.lock().unwrap().push("rollback");
            Ok(())
        }
    }

    fn verified_tenant() -> VerifiedRequestTenant {
        VerifiedRequestTenant::from_verified(Uuid::from_u128(1)).unwrap()
    }

    fn test_scope() -> TenantRequestScope {
        let (commands, _receiver) = mpsc::channel(COMMAND_CAPACITY);
        TenantRequestScope {
            tenant: verified_tenant(),
            commands,
        }
    }

    async fn owner_action(
        action: Option<FinishAction>,
        close_commands_before_finish: bool,
    ) -> Vec<&'static str> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let transaction = MockTransaction {
            events: Arc::clone(&events),
        };
        let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (finish_tx, finish_rx) = oneshot::channel();
        let (completed_tx, completed_rx) = oneshot::channel();
        let owner = tokio::spawn(run_transaction_owner(
            transaction,
            commands_rx,
            finish_rx,
            completed_tx,
        ));

        let commands_guard = if close_commands_before_finish {
            drop(commands_tx);
            tokio::task::yield_now().await;
            None
        } else {
            Some(commands_tx)
        };
        if let Some(action) = action {
            finish_tx.send(action).unwrap();
        } else {
            drop(finish_tx);
        }

        completed_rx.await.unwrap().unwrap();
        owner.await.unwrap();
        drop(commands_guard);
        let recorded = events.lock().unwrap().clone();
        recorded
    }

    #[test]
    fn missing_verified_context_is_rejected() {
        assert_eq!(
            tenant_for_request(&Extensions::new()),
            Err(RequestScopeRejection::MissingTenant)
        );
    }

    #[test]
    fn reused_scope_is_rejected() {
        let mut extensions = Extensions::new();
        extensions.insert(verified_tenant());
        extensions.insert(test_scope());

        assert_eq!(
            tenant_for_request(&extensions),
            Err(RequestScopeRejection::ReusedScope)
        );
    }

    #[tokio::test]
    async fn successful_non_streaming_response_commits() {
        let response = Response::new(Body::from("ok"));
        assert_eq!(
            response_disposition(&response),
            ResponseDisposition::Finish(FinishAction::Commit)
        );
        assert_eq!(
            owner_action(Some(FinishAction::Commit), false).await,
            ["commit"]
        );
    }

    #[tokio::test]
    async fn error_response_rolls_back() {
        let response = Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("bad request"))
            .unwrap();
        assert_eq!(
            response_disposition(&response),
            ResponseDisposition::Finish(FinishAction::Rollback)
        );
        assert_eq!(
            owner_action(Some(FinishAction::Rollback), false).await,
            ["rollback"]
        );
    }

    #[tokio::test]
    async fn dropped_finalizer_rolls_back() {
        assert_eq!(owner_action(None, false).await, ["rollback"]);
    }

    #[tokio::test]
    async fn command_channel_close_does_not_race_successful_commit() {
        assert_eq!(
            owner_action(Some(FinishAction::Commit), true).await,
            ["commit"]
        );
    }

    #[test]
    fn streaming_response_is_rejected_by_explicit_policy() {
        let stream = futures::stream::pending::<std::result::Result<&'static [u8], Error>>();
        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(stream))
            .unwrap();

        assert_eq!(
            response_disposition(&response),
            ResponseDisposition::RejectStreaming
        );
    }

    #[test]
    fn unknown_length_body_is_rejected_even_without_stream_content_type() {
        let stream = futures::stream::pending::<std::result::Result<&'static [u8], Error>>();
        let response = Response::new(Body::from_stream(stream));

        assert_eq!(
            response_disposition(&response),
            ResponseDisposition::RejectStreaming
        );
    }

    #[test]
    fn protocol_upgrade_is_rejected() {
        let response = Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            response_disposition(&response),
            ResponseDisposition::RejectStreaming
        );
    }
}

//! Selection, probing and recovery for the RLS-safe lexical admission function.
//!
//! Under FORCE ROW LEVEL SECURITY the runtime role cannot use the GIN indexes
//! for `@@`/`to_tsvector` (neither is leakproof), so candidate admission can
//! run through `public.fts_admit_note_ids`, a SECURITY DEFINER function that
//! enforces the current tenant and a registered schema itself (migration
//! `20261120010000`).
//!
//! The function path is only activated for deployments that are single-tenant
//! for lexical search (community mode, or external-OIDC with its fixed
//! single-tenant pool), because in a shared multi-tenant schema the GIN work
//! spans every tenant and its latency is a timing/existence channel. Hosted
//! multi-tenant deployments keep the direct RLS statements unless an operator
//! explicitly accepts that channel. When the function is missing, not
//! executable by the runtime role, or its owner cannot bypass RLS (community
//! bundles), the direct statements remain in use.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use sqlx::{PgConnection, Pool, Postgres};
use tracing::{info, warn};

/// Result cap applied inside the function: a runaway-memory guard, not a
/// pagination bound. Exceeding it raises (never truncates) and the caller retries
/// that request on the direct path. The 200k-note scale check shows a word that
/// matches every note is served faster through the function (1.2 s vs 2.6 s), so
/// the cap sits at the function's hard clamp instead of a low value.
pub const ADMISSION_CAP: i32 = 1_000_000;

/// Exact signature, for GRANT statements.
pub const ADMISSION_FUNCTION_SIGNATURE: &str =
    "public.fts_admit_note_ids(tsquery, text, text, text[], integer)";

/// Function name as it appears in server error messages.
const FUNCTION_NAME: &str = "fts_admit_note_ids";

/// Operator opt-in (value `accept`) to run the function path in a hosted
/// multi-tenant deployment, accepting the shared-schema timing/existence channel.
pub const SHARED_TENANT_OPT_IN_ENV: &str = "FTS_ADMISSION_SHARED_TENANT";

/// A downgraded process re-probes at most this often (seconds).
const DEFAULT_RECOVERY_SECS: u64 = 300;

/// Data-independent probe: an unsatisfiable tsquery cannot match any row, so
/// user-writable data can never change the probe's result.
const PROBE_CALL: &str = "SELECT count(*) FROM public.fts_admit_note_ids('fortemiprobe & !fortemiprobe'::tsquery, 'public.matric_english', 'public', ARRAY['title','content','attachment']::text[], 1)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionPath {
    /// SECURITY DEFINER function, index-backed under forced RLS.
    Function,
    /// Direct table statements (RLS-filtered; correct, but not index-backed
    /// for non-bypass roles).
    Direct,
}

impl AdmissionPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "security_definer_function",
            Self::Direct => "direct_statements",
        }
    }
}

/// Whether this deployment may use the function path at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// Single-tenant for lexical search (community, or the external-OIDC fixed
    /// single-tenant pool); the string says which.
    SingleTenant(&'static str),
    /// Hosted multi-tenant with the operator's explicit opt-in.
    SharedTenantAccepted,
    /// Not activated; the string says why.
    Disabled(&'static str),
}

// The default is the conservative current behaviour until a probe succeeds.
static ACTIVE: AtomicU8 = AtomicU8::new(0);
static ELIGIBLE: AtomicBool = AtomicBool::new(true);
/// 0 = not downgraded; otherwise the (1-based) second of the downgrade.
static DOWNGRADED_AT: AtomicU64 = AtomicU64::new(0);
static LAST_RECOVERY: AtomicU64 = AtomicU64::new(0);
static RECOVERY_SECS: AtomicU64 = AtomicU64::new(DEFAULT_RECOVERY_SECS);
static START: OnceLock<Instant> = OnceLock::new();

fn now_secs() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_secs() + 1
}

pub fn active_path() -> AdmissionPath {
    if ACTIVE.load(Ordering::Relaxed) == 1 {
        AdmissionPath::Function
    } else {
        AdmissionPath::Direct
    }
}

pub fn set_active_path(path: AdmissionPath) {
    ACTIVE.store(u8::from(path == AdmissionPath::Function), Ordering::Relaxed);
    if path == AdmissionPath::Function {
        DOWNGRADED_AT.store(0, Ordering::Relaxed);
    }
}

static FUNCTION_TIMEOUT_MS: AtomicU64 = AtomicU64::new(10_000);

/// Test hook: the statement timeout applied around function-path statements.
#[doc(hidden)]
pub fn set_function_timeout_ms(milliseconds: u64) {
    FUNCTION_TIMEOUT_MS.store(milliseconds, Ordering::Relaxed);
}

/// Bound the function-path statement: set a transaction-local `statement_timeout`
/// (never loosening a stricter existing one) and return the previous value to
/// restore. A function-local `SET statement_timeout` would not interrupt the
/// statements the function runs, so the bound is applied by the caller on its
/// savepoint. Call only inside a savepoint.
pub(crate) async fn bound_statement_timeout(connection: &mut PgConnection) -> Option<String> {
    let previous: String = sqlx::query_scalar("SELECT current_setting('statement_timeout')")
        .fetch_one(&mut *connection)
        .await
        .ok()?;
    sqlx::query(
        "SELECT set_config('statement_timeout', CASE WHEN $2::interval = interval '0' OR $2::interval > ($1::double precision / 1000.0) * interval '1 second' THEN $1::text || 'ms' ELSE $2 END, true)",
    )
    .bind(FUNCTION_TIMEOUT_MS.load(Ordering::Relaxed) as i64)
    .bind(&previous)
    .execute(&mut *connection)
    .await
    .ok()?;
    Some(previous)
}

pub(crate) async fn restore_statement_timeout(
    connection: &mut PgConnection,
    previous: Option<String>,
) {
    if let Some(previous) = previous {
        let _ = sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(previous)
            .execute(connection)
            .await;
    }
}

/// Test hook: how often a downgraded process may re-probe.
#[doc(hidden)]
pub fn set_recovery_interval_secs(seconds: u64) {
    RECOVERY_SECS.store(seconds, Ordering::Relaxed);
}

/// Classification of a server error raised on the function path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorClass {
    /// The function itself is missing or not executable by this role.
    Definitive,
    /// Cancellation, deadlock, resource, cap overflow or connection trouble:
    /// says nothing about whether the function works.
    Inconclusive,
    /// Anything else (including the function's own tenant/schema refusals and an
    /// owner that lost BYPASSRLS): not a reason to change global state.
    Other,
}

pub fn classify_error(error: &sqlx::Error) -> ErrorClass {
    match error {
        sqlx::Error::Database(db) => {
            let code = db.code();
            let code = code.as_deref().unwrap_or("");
            let message = db.message();
            if (code == "42883" && message.contains(FUNCTION_NAME))
                || (code == "42501"
                    && message.contains(&format!("permission denied for function {FUNCTION_NAME}")))
            {
                ErrorClass::Definitive
            } else if matches!(code, "57014" | "40P01" | "40001" | "54000")
                || code.starts_with("53")
                || code.starts_with("08")
                || code.starts_with("57P")
            {
                ErrorClass::Inconclusive
            } else {
                ErrorClass::Other
            }
        }
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::PoolTimedOut
        | sqlx::Error::PoolClosed
        | sqlx::Error::WorkerCrashed => ErrorClass::Inconclusive,
        _ => ErrorClass::Other,
    }
}

/// A "function does not exist" failure for this function (for example from the
/// startup GRANT when the migration is recorded but the function was dropped).
pub fn is_missing_function_error(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db)
        if db.code().as_deref() == Some("42883") && db.message().contains(FUNCTION_NAME))
}

/// Outcome of probing the function on a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    Works,
    Definitive,
    Inconclusive,
}

/// Probe the function on a request's own connection (inside a savepoint that is
/// always rolled back, so neither the tenant GUC nor any other state of the
/// request changes). Only a definitive missing-function/EXECUTE failure reports
/// `Definitive`; everything else that fails is `Inconclusive`.
#[doc(hidden)]
pub async fn probe_on_connection(connection: &mut PgConnection) -> ProbeOutcome {
    let Ok(mut savepoint) = sqlx::Connection::begin(&mut *connection).await else {
        return ProbeOutcome::Inconclusive;
    };
    if sqlx::query(
        "SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', true)",
    )
    .execute(&mut *savepoint)
    .await
    .is_err()
    {
        let _ = savepoint.rollback().await;
        return ProbeOutcome::Inconclusive;
    }
    let outcome = sqlx::query_scalar::<_, i64>(PROBE_CALL)
        .fetch_one(&mut *savepoint)
        .await;
    let _ = savepoint.rollback().await;
    match outcome {
        Ok(_) => ProbeOutcome::Works,
        Err(error) if classify_error(&error) == ErrorClass::Definitive => ProbeOutcome::Definitive,
        Err(_) => ProbeOutcome::Inconclusive,
    }
}

/// Record a runtime downgrade (recoverable after the re-probe interval).
pub(crate) fn note_downgrade() {
    set_active_path(AdmissionPath::Direct);
    let now = now_secs();
    DOWNGRADED_AT.store(now, Ordering::Relaxed);
    LAST_RECOVERY.store(now, Ordering::Relaxed);
}

/// Bounded recovery: a downgraded, still-eligible process re-probes at most every
/// recovery interval (one caller wins each window) and returns to the function
/// path when the probe passes.
pub(crate) async fn maybe_recover(connection: &mut PgConnection) {
    if active_path() == AdmissionPath::Function
        || !ELIGIBLE.load(Ordering::Relaxed)
        || DOWNGRADED_AT.load(Ordering::Relaxed) == 0
    {
        return;
    }
    let now = now_secs();
    let last = LAST_RECOVERY.load(Ordering::Relaxed);
    if now.saturating_sub(last) < RECOVERY_SECS.load(Ordering::Relaxed)
        || LAST_RECOVERY
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    if probe_on_connection(connection).await == ProbeOutcome::Works {
        set_active_path(AdmissionPath::Function);
        info!("lexical admission function recovered; returning to the function path");
    }
}

/// Decide whether this deployment may use the function path, then probe and
/// select the path as the runtime role behind `pool`. Always logs the active
/// path and why. Never errors.
pub async fn configure(pool: &Pool<Postgres>, eligibility: Eligibility) -> AdmissionPath {
    match eligibility {
        Eligibility::Disabled(reason) => {
            ELIGIBLE.store(false, Ordering::Relaxed);
            set_active_path(AdmissionPath::Direct);
            info!(
                path = AdmissionPath::Direct.as_str(),
                reason, "lexical candidate admission path selected (function path not activated)"
            );
            AdmissionPath::Direct
        }
        Eligibility::SharedTenantAccepted => {
            warn!(
                env = SHARED_TENANT_OPT_IN_ENV,
                "ACCEPTED RISK: the lexical admission function is enabled for a shared \
                 multi-tenant schema. Search latency can scale with a term's frequency \
                 across tenants (a timing/existence channel). No other tenant's rows are \
                 returned."
            );
            ELIGIBLE.store(true, Ordering::Relaxed);
            probe_and_select(pool).await
        }
        Eligibility::SingleTenant(why) => {
            info!(
                why,
                "lexical admission function eligible (single-tenant deployment)"
            );
            ELIGIBLE.store(true, Ordering::Relaxed);
            probe_and_select(pool).await
        }
    }
}

/// Probe the function as the runtime role behind `pool`: it must execute (the
/// owner bypasses RLS and the role holds EXECUTE) and must fail closed without
/// a tenant. Records and returns the active path; never errors.
pub async fn probe_and_select(pool: &Pool<Postgres>) -> AdmissionPath {
    let path = match probe(pool).await {
        Ok(()) => AdmissionPath::Function,
        Err(reason) => {
            warn!(
                reason = %reason,
                "lexical admission function unavailable; using direct statements \
                 (not index-backed under forced RLS for non-bypass roles)"
            );
            note_downgrade();
            AdmissionPath::Direct
        }
    };
    if path == AdmissionPath::Function {
        set_active_path(path);
    }
    info!(
        path = path.as_str(),
        "lexical candidate admission path selected"
    );
    path
}

async fn probe(pool: &Pool<Postgres>) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', true)",
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_scalar::<_, i64>(PROBE_CALL)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("function probe failed: {}", short(&e)))?;
    tx.rollback().await.map_err(|e| e.to_string())?;
    // Fail-closed self-check: an unset tenant must raise, never widen.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SELECT set_config('app.current_tenant', '', true)")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let unscoped = sqlx::query_scalar::<_, i64>(PROBE_CALL)
        .fetch_one(&mut *tx)
        .await;
    let _ = tx.rollback().await;
    match unscoped {
        Err(_) => Ok(()),
        Ok(_) => Err("function did not fail closed without a tenant".into()),
    }
}

fn short(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(db) => format!(
            "{} {}",
            db.code().as_deref().unwrap_or("?"),
            db.message().chars().take(120).collect::<String>()
        ),
        other => other.to_string().chars().take(120).collect(),
    }
}

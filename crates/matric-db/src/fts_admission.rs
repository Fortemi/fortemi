//! Selection and startup probe for the RLS-safe lexical admission function.
//!
//! Under FORCE ROW LEVEL SECURITY the runtime role cannot use the GIN indexes
//! for `@@`/`to_tsvector` (neither is leakproof), so candidate admission can
//! run through `public.fts_admit_note_ids`, a SECURITY DEFINER function that
//! enforces the current tenant and a registered schema itself (migration
//! `20261120010000`). When the function is missing, not executable by the
//! runtime role, or its owner cannot bypass RLS (community bundles), the
//! direct statements remain in use.

use std::sync::atomic::{AtomicU8, Ordering};

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

// The default is the conservative current behaviour until a probe succeeds.
static ACTIVE: AtomicU8 = AtomicU8::new(0);

pub fn active_path() -> AdmissionPath {
    if ACTIVE.load(Ordering::Relaxed) == 1 {
        AdmissionPath::Function
    } else {
        AdmissionPath::Direct
    }
}

pub fn set_active_path(path: AdmissionPath) {
    ACTIVE.store(u8::from(path == AdmissionPath::Function), Ordering::Relaxed);
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
            AdmissionPath::Direct
        }
    };
    set_active_path(path);
    info!(
        path = path.as_str(),
        "lexical candidate admission path selected"
    );
    path
}

/// Re-run the function probe on a request's own connection (inside a savepoint
/// that is always rolled back, so neither the tenant GUC nor any other state of
/// the request changes). Returns `false` only when the server definitively
/// rejects the function call as the current role; infrastructure errors and
/// anything inconclusive return `true` so a process-wide switch is never flipped
/// on weak evidence.
pub(crate) async fn function_works_on_connection(connection: &mut PgConnection) -> bool {
    let Ok(mut savepoint) = sqlx::Connection::begin(&mut *connection).await else {
        return true;
    };
    if sqlx::query(
        "SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', true)",
    )
    .execute(&mut *savepoint)
    .await
    .is_err()
    {
        let _ = savepoint.rollback().await;
        return true;
    }
    let outcome = sqlx::query_scalar::<_, i64>(PROBE_CALL)
        .fetch_one(&mut *savepoint)
        .await;
    let _ = savepoint.rollback().await;
    !matches!(outcome, Err(sqlx::Error::Database(_)))
}

const PROBE_CALL: &str = "SELECT count(*) FROM public.fts_admit_note_ids('fortemiprobe'::tsquery, 'public.matric_english', 'public', ARRAY['title','content','attachment']::text[], 1)";

async fn probe(pool: &Pool<Postgres>) -> Result<(), String> {
    const CALL: &str = PROBE_CALL;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', true)",
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_scalar::<_, i64>(CALL)
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
    let unscoped = sqlx::query_scalar::<_, i64>(CALL).fetch_one(&mut *tx).await;
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
